//! Paths in a project manifest (`wc3.bni`): the values of `[project]`
//! `game=`, `sources=` and `exclude=`. The editor gets
//!
//! - **links**: every folder along a path is a link to that folder, so
//!   in `game=/Games/Warcraft III/Maps` each of `Games`, `Warcraft III`
//!   and `Maps` leads to its own folder;
//! - **completion**: at the cursor in such a value, the entries of the
//!   folder typed so far: folders for `game` (it names folders), folders
//!   and files for `sources` and `exclude`. Hidden entries (`.git`) only
//!   once a `.` is typed. In a mask, the parts before the first wildcard
//!   link and complete the same way.
//!
//! Paths resolve like the settings themselves (`project::load`): against
//! the project root, an absolute path as written, `/` or `\` between
//! parts.

use std::path::{Path, PathBuf};

use bni_rs::ast::{Field, Item, ValueKind};

use crate::project::resolve;

/// One part of a path, linked to its folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// 1-based line and codepoint columns, end-exclusive.
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
    pub target: PathBuf,
}

/// What completes the path at the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// The columns the chosen entry replaces: the path part the cursor is
    /// in (1-based, end-exclusive).
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// The keys of section `section` whose values are paths, and whether
/// files count too: `[project] game`, `[code] sources`, `exclude`, `[map] root`
/// (a folder or a map).
fn path_key(section: &str, item: &Item) -> Option<bool> {
    match (section, item.key.as_ref()?.text.as_str()) {
        ("project", "game") => Some(false),
        ("code", "sources" | "exclude") | ("map", "root") => Some(true),
        _ => None,
    }
}

/// The items whose values are paths, and whether files count.
fn path_items(doc: &bni_rs::ast::Document) -> impl Iterator<Item = (&Item, bool)> {
    doc.sections
        .iter()
        .flat_map(|s| s.items.iter().map(move |i| (s.name.text.as_str(), i)))
        .filter_map(|(s, i)| path_key(s, i).map(|files| (i, files)))
}

/// A field's characters with their columns, outer whitespace dropped.
/// Quoted parts count without their quotes.
fn chars(field: &Field) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    for part in &field.parts {
        let first = match part.kind {
            ValueKind::Quoted { .. } => part.span.start_col + 1,
            _ => part.span.start_col,
        };
        out.extend(part.text.chars().enumerate().map(|(i, c)| (c, first + i)));
    }
    let start = out.iter().position(|(c, _)| !c.is_whitespace()).unwrap_or(out.len());
    let end = out.iter().rposition(|(c, _)| !c.is_whitespace()).map_or(start, |e| e + 1);
    out[start..end].to_vec()
}

fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Every folder along every path in the manifest `src` of the project
/// rooted at `root`, as links. Parts that name nothing on disk (yet)
/// aren't links.
pub fn links(root: &Path, src: &str) -> Vec<Link> {
    let doc = bni_rs::parse(src);
    let mut out = Vec::new();
    for (item, _) in path_items(&doc) {
        for field in &item.fields {
            let cs = chars(field);
            let text: String = cs.iter().map(|(c, _)| c).collect();
            let mut i = 0;
            while i < cs.len() {
                if is_sep(cs[i].0) {
                    i += 1;
                    continue;
                }
                let end = (i..cs.len()).find(|&j| is_sep(cs[j].0)).unwrap_or(cs.len());
                let prefix: String = text.chars().take(end).collect();
                let target = resolve(root, &prefix);
                if target.is_dir() {
                    out.push(Link {
                        line: field.span.start_line,
                        col: cs[i].1,
                        end_col: cs[end - 1].1 + 1,
                        target,
                    });
                }
                i = end;
            }
        }
    }
    out
}

/// The entries that complete the path at `line`, codepoint column `col`
/// (1-based), if the cursor is in a path value of the manifest `src`.
pub fn complete(root: &Path, src: &str, line: usize, col: usize) -> Option<Completion> {
    let doc = bni_rs::parse(src);
    let (item, files) = path_items(&doc).find(|(i, _)| i.line == line && col > i.eq.start_col)?;
    // The field the cursor is in; past the last one, or with none yet
    // (`game=`), a new one starting at the cursor.
    let field = item
        .fields
        .iter()
        .find(|f| f.span.start_col <= col && col <= f.span.end_col);
    let cs = field.map(chars).unwrap_or_default();
    // The path typed before the cursor, and the part the cursor is in.
    let before = cs.iter().take_while(|(_, c)| *c < col).count();
    let start = cs[..before].iter().rposition(|(c, _)| is_sep(*c)).map_or(0, |p| p + 1);
    let end = (before..cs.len()).find(|&j| is_sep(cs[j].0)).unwrap_or(cs.len());
    let dir_text: String = cs[..start].iter().map(|(c, _)| c).collect();
    let typed: String = cs[start..before].iter().map(|(c, _)| c).collect();
    let (from, to) = if start < cs.len() && start < end {
        (cs[start].1, cs[end - 1].1 + 1)
    } else {
        (col, col)
    };
    let dir = if dir_text.is_empty() { root.to_path_buf() } else { resolve(root, &dir_text) };

    let mut entries: Vec<Entry> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let path = e.path();
            let is_dir = path.is_dir();
            let hidden = name.starts_with('.') && !typed.starts_with('.');
            (!hidden && (is_dir || files)).then_some(Entry { name, path, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Some(Completion { line, col: from, end_col: to, entries })
}

#[cfg(test)]
#[path = "paths.test.rs"]
mod tests;
