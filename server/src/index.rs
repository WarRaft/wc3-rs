//! The symbol index: answers about names, from the symbols every file of
//! the model already carries (`Workspace`). Nothing is precomputed or
//! cached beyond them: a query scans the occurrences it needs, which is
//! fast enough for any real map (a whole-project scan is a few
//! milliseconds), and can never go stale.
//!
//! Names resolve by scope (see `parser_rs::symbols`): a local name within
//! its own file and scope; a struct member (in its own namespace) across
//! every file of the model; a project-wide name across every file too, in
//! the namespace it resolves to (AngelScript's `A::B::name`, looked up
//! from where it's used outwards); always by exact (case-sensitive) name.

use std::collections::{HashMap, HashSet};

use parser_rs::{Occurrence, Scope, SymbolKind};

use crate::document::{Document, LineIndex, Position, Range};
use crate::workspace::Workspace;

/// What the editor asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// Where the name under the cursor is declared.
    Definition,
    /// Every occurrence of that symbol, its declarations included.
    References,
    /// A description of that symbol: its declaration lines.
    Hover,
    /// The declarations of one file, for the outline: everything but
    /// parameters and locals.
    DocumentSymbols,
    /// The declarations (but parameters and locals) whose name contains
    /// the query text, in every file.
    WorkspaceSymbols,
    /// Can the name under the cursor be renamed? Its location, or why not.
    PrepareRename,
    /// Every occurrence to replace to rename the symbol to the query
    /// text, or why it can't be.
    Rename,
    /// The text of a file, as the model has it: for the editor to show a
    /// file it can't read itself, one inside a map.
    Content,
    /// In a `wc3.bni`: the entries completing the path at the cursor
    /// (see `paths`). Each location's `name` is an entry, its `id` the
    /// entry's path, its `range` what the entry replaces, its `container`
    /// `folder` or `file`.
    PathCompletion,
    /// In a `wc3.bni`: every folder along every path, a location whose
    /// `range` is the path part and `id` the folder.
    PathLinks,
    /// In a `.slk`: its sheet, as JSON in the text (`sheet::table`).
    SlkTable,
    /// In a `.slk`: the edits that write the query text into the cell at
    /// row `line`, column `character` (both from 1), as JSON in the text
    /// (`sheet::set_cell`).
    SlkSetCell,
    /// In the text form of object data: the inlay hints (fields' names and
    /// types) of lines `line` to `character` (0-based), as JSON in the
    /// text (`objects::hints`).
    InlayHints,
}

/// Path completion and links in a manifest; nothing for any other file.
fn manifest_paths(ws: &Workspace, kind: QueryKind, id: &str, pos: Position) -> Answer {
    let Some((doc, _)) = ws.file(id) else {
        return Answer::default();
    };
    let Some(root) = doc.path.as_deref().filter(|p| crate::project::is_manifest(p)).and_then(|p| p.parent()) else {
        return Answer::default();
    };
    let text = doc.text.as_str();
    let lines = LineIndex::new(text);
    let location = |id: &std::path::Path, range: Range, name: String, container: &str| Location {
        id: id.to_string_lossy().into_owned(),
        range,
        name,
        kind: SymbolKind::Namespace,
        container: container.to_string(),
    };
    let locations = if kind == QueryKind::PathLinks {
        crate::paths::links(root, text)
            .into_iter()
            .map(|l| location(&l.target, lines.range(text, l.line, l.col, l.end_col), String::new(), "folder"))
            .collect()
    } else {
        let line = pos.line as usize + 1;
        let col = lines.column(text, pos);
        match crate::paths::complete(root, text, line, col) {
            Some(c) => {
                let range = lines.range(text, c.line, c.col, c.end_col);
                c.entries
                    .into_iter()
                    .map(|e| location(&e.path, range, e.name, if e.is_dir { "folder" } else { "file" }))
                    .collect()
            }
            None => Vec::new(),
        }
    };
    Answer { locations, text: String::new() }
}

/// One place in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub id: String,
    pub range: Range,
    pub name: String,
    pub kind: SymbolKind,
    /// The namespace a declaration lives in (`A::B`); empty for the
    /// global one and for anything that isn't a declaration.
    pub container: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Answer {
    pub locations: Vec<Location>,
    /// Markdown for a hover; for a rename, why it was refused (empty when
    /// it's fine).
    pub text: String,
}

/// The most symbols a workspace search returns.
const MAX_SEARCH: usize = 1000;

/// The most declarations a hover shows.
const MAX_HOVER: usize = 5;

pub fn answer(ws: &Workspace, kind: QueryKind, id: &str, pos: Position, query: &str) -> Answer {
    // A `TRIGSTR_<n>`, in any file: its text from the map's string table.
    if kind == QueryKind::Hover
        && let Some(text) = crate::trigstr::hover(ws, id, pos)
    {
        return Answer { locations: Vec::new(), text };
    }
    let mut out = Out::default();
    match kind {
        QueryKind::PrepareRename => return crate::rename::prepare(ws, id, pos),
        QueryKind::Rename => return crate::rename::rename(ws, id, pos, query),
        QueryKind::Content => {
            let text = match ws.file(id) {
                Some((doc, _)) => doc.text.clone(),
                None => crate::map::text(std::path::Path::new(id)).unwrap_or_default(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::PathCompletion | QueryKind::PathLinks => return manifest_paths(ws, kind, id, pos),
        QueryKind::SlkTable | QueryKind::SlkSetCell => {
            let Some((doc, _)) = ws.file(id).filter(|(d, _)| d.language == crate::document::Language::Slk) else {
                return Answer::default();
            };
            let text = match kind {
                QueryKind::SlkTable => crate::sheet::table(&doc.text),
                _ => crate::sheet::set_cell(&doc.text, pos.line, pos.character, query),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::InlayHints => {
            let text = match ws.object_text(id) {
                Some((doc, meta)) => crate::objects::hints(&doc.text, pos.line, pos.character, meta),
                None => "[]".into(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Hover if ws.object_text(id).is_some() => {
            let (doc, meta) = ws.object_text(id).expect("just checked");
            let text = crate::objects::hover(&doc.text, pos, meta).unwrap_or_default();
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Definition | QueryKind::References | QueryKind::Hover => {
            ws.note_context(id);
            let Some(target) = at(ws, id, pos) else {
                return Answer::default();
            };
            let mut found = matches(ws, id, target);
            if kind != QueryKind::References {
                found.retain(|(_, _, o)| o.declaration);
            }
            if kind == QueryKind::Hover {
                return Answer { locations: Vec::new(), text: hover(target, &found) };
            }
            for (id, doc, o) in found {
                out.push(id, doc, o);
            }
        }
        QueryKind::DocumentSymbols => {
            if let Some((doc, symbols)) = ws.file(id) {
                for o in symbols.iter().filter(|o| o.declaration && !o.kind.is_local()) {
                    out.push(id, doc, o);
                }
            }
        }
        QueryKind::WorkspaceSymbols => {
            let query = query.to_lowercase();
            let mut found: Vec<(&str, &Document, &Occurrence)> = ws
                .all_files()
                .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
                .filter(|(_, _, o)| {
                    o.declaration && !o.kind.is_local() && o.name.to_lowercase().contains(&query)
                })
                .collect();
            found.sort_by(|a, b| (&a.2.name, a.0, a.2.line).cmp(&(&b.2.name, b.0, b.2.line)));
            for (id, doc, o) in found.into_iter().take(MAX_SEARCH) {
                out.push(id, doc, o);
            }
        }
    }
    out.finish()
}

/// The occurrence under the cursor.
pub(crate) fn at<'a>(ws: &'a Workspace, id: &str, pos: Position) -> Option<&'a Occurrence> {
    let (doc, symbols) = ws.file(id)?;
    let col = LineIndex::new(&doc.text).column(&doc.text, pos);
    let line = pos.line as usize + 1;
    // A cursor between two names (`a+b` at `+`) is on both: prefer the one
    // it's inside.
    symbols
        .iter()
        .filter(|o| o.covers(line, col))
        .min_by_key(|o| (o.end_col == col) as u8)
}

/// Every occurrence of `target`'s symbol that `id` sees (see
/// `Workspace::scope`), sorted by file and position.
pub(crate) fn matches<'a>(ws: &'a Workspace, id: &'a str, target: &Occurrence) -> Vec<(&'a str, &'a Document, &'a Occurrence)> {
    let mut found: Vec<(&str, &Document, &Occurrence)> = match target.scope {
        Scope::Local(_) => ws
            .file(id)
            .into_iter()
            .flat_map(|(doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
            .filter(|(_, _, o)| o.same_symbol(target))
            .collect(),
        Scope::Member => ws
            .scope(id)
            .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
            .filter(|(_, _, o)| o.same_symbol(target))
            .collect(),
        Scope::Project => {
            // The namespaces that declare the name, to resolve every
            // occurrence of it against.
            let declared: HashSet<&str> = ws
                .scope(id)
                .flat_map(|(_, _, symbols)| symbols)
                .filter(|o| o.declaration && o.same_symbol(target))
                .map(|o| o.namespace.as_str())
                .collect();
            let declares = |ns: &str| declared.contains(ns);
            let namespace = target.resolve(declares);
            ws.scope(id)
                .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
                .filter(|(_, _, o)| o.same_symbol(target) && o.resolve(declares) == namespace)
                .collect()
        }
    };
    found.sort_by_key(|(id, _, o)| (*id, o.line, o.col));
    found
}

/// The declaration lines, as code in the file's language, each with the
/// file and line it's from. A name with no declaration in the project
/// (a native from a `common.j` that isn't part of it, say) says so.
fn hover(target: &Occurrence, decls: &[(&str, &Document, &Occurrence)]) -> String {
    if decls.is_empty() {
        return format!(
            "{} `{}`\n\n*not declared in the project*",
            target.kind.as_str(),
            target.name
        );
    }
    let mut parts = Vec::new();
    for (id, doc, o) in decls.iter().take(MAX_HOVER) {
        let lines = LineIndex::new(&doc.text);
        let line = lines.line(&doc.text, o.line - 1).trim();
        let file = std::path::Path::new(id).file_name().map_or(*id, |n| n.to_str().unwrap_or(id));
        let place = if o.namespace.is_empty() {
            format!("{file}:{}", o.line)
        } else {
            format!("{file}:{}, in `{}`", o.line, o.namespace)
        };
        parts.push(format!("```{}\n{line}\n```\n*{place}*", doc.language.id()));
    }
    if decls.len() > MAX_HOVER {
        parts.push(format!("*and {} more declarations*", decls.len() - MAX_HOVER));
    }
    parts.join("\n\n---\n\n")
}

/// Builds locations, indexing each file's lines once.
#[derive(Default)]
pub(crate) struct Out<'a> {
    lines: HashMap<&'a str, LineIndex>,
    locations: Vec<Location>,
}

impl<'a> Out<'a> {
    pub(crate) fn push(&mut self, id: &'a str, doc: &Document, o: &Occurrence) {
        let lines = self.lines.entry(id).or_insert_with(|| LineIndex::new(&doc.text));
        self.locations.push(Location {
            id: id.to_string(),
            range: lines.range(&doc.text, o.line, o.col, o.end_col),
            name: o.name.clone(),
            kind: o.kind,
            container: if o.declaration { o.namespace.clone() } else { String::new() },
        });
    }

    pub(crate) fn finish(self) -> Answer {
        Answer { locations: self.locations, text: String::new() }
    }
}

#[cfg(test)]
#[path = "index.test.rs"]
mod tests;
