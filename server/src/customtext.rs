//! A map's custom script, `war3map.wct`: the code the trigger editor
//! keeps as text (the map's custom script, the custom text triggers).
//! It's code, so it's part of the project's model like any script: its
//! text form (`wct-rs`, the pieces of code one after another, after
//! `// @wct ...` marker lines) is a JASS document, or Lua in a Lua map,
//! analyzed, indexed and seen by the project's other files. A
//! `war3map.wct` in a root (a root folder, or a root map) is in the
//! project as any file of a root is.
//!
//! The editor opens the file as that text too (`convert`, under the
//! `wc3obj:` scheme); while it's open, its text stands for the file in
//! the model (`Workspace::open`). Saving turns the text back into the
//! file's bytes, and fails with the reason when the markers are wrong.

use std::path::Path;

use wct_rs::text::Script;

use crate::document::{Document, Language};
use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a custom script file, by its extension?
pub fn is_custom_text_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wct"))
}

/// The script language of the map the custom script at `path` is for:
/// Lua when the map's `war3map.w3i` next to it (on disk or in the same
/// map) says so (`script=1`, since 1.31), JASS otherwise.
pub fn script(path: &Path) -> Script {
    let Some(w3i) = path.parent().map(|p| p.join("war3map.w3i")) else {
        return Script::Jass;
    };
    let bytes = if w3i.is_file() { std::fs::read(&w3i).ok() } else { crate::map::bytes(&w3i) };
    match bytes.and_then(|b| w3i_rs::parse(&b).ok()) {
        Some(info) if info.script == 1 => Script::Lua,
        _ => Script::Jass,
    }
}

/// The language the text form of `path` is in.
pub fn language(path: &Path) -> Language {
    match script(path) {
        Script::Lua => Language::Lua,
        Script::Jass => Language::Jass,
    }
}

/// The text form of the custom script at `path` with bytes `bytes`, as
/// a document of the model. `None` when the file can't be read.
pub fn document(path: &Path, bytes: &[u8]) -> Option<Document> {
    let file = wct_rs::parse(bytes).ok()?;
    let script = script(path);
    let text = wct_rs::text::to_text(&file, script).ok()?;
    let language = match script {
        Script::Lua => Language::Lua,
        Script::Jass => Language::Jass,
    };
    Some(Document { language, version: 0, text, path: Some(path.to_path_buf()) })
}

/// Converts for the editor: the bytes of the custom script `name` to its
/// text form (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`). The
/// error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let path = Path::new(name);
    let file = path.file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            let parsed = wct_rs::parse(data).map_err(|e| format!("Can't read {file}: {e}"))?;
            wct_rs::text::to_text(&parsed, script(path))
                .map(String::into_bytes)
                .map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let parsed = wct_rs::text::from_text(text).map_err(|errors| {
                let e = &errors[0];
                format!("Can't save {file}: line {}: {}", e.line, e.message)
            })?;
            parsed.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form of the custom script at `path` from
/// saving: its markers' errors. Nothing for other files.
pub fn diagnostics(path: Option<&Path>, text: &str) -> Vec<parser_rs::ParseError> {
    if !path.is_some_and(is_custom_text_file) {
        return Vec::new();
    }
    let Err(errors) = wct_rs::text::from_text(text) else {
        return Vec::new();
    };
    errors
        .into_iter()
        .map(|e| {
            let len = text.lines().nth(e.line - 1).map_or(1, |l| l.chars().count().max(1));
            parser_rs::ParseError { message: e.message, line: e.line, col: 1, end_col: len + 1 }
        })
        .collect()
}

/// The inlay hints of lines `first..=last` (0-based) of the text form
/// `text`: after each `trigger` marker, which trigger it is, counting
/// from 1 (the trigger editor's order), as JSON `[[line, character,
/// "label"], ...]`.
pub fn hints(text: &str, first: u32, last: u32) -> String {
    let mut out = Vec::new();
    let mut n = 0;
    for (i, line) in text.lines().enumerate() {
        let trigger = wct_rs::text::is_marker(line) && line.split_whitespace().nth(2) == Some("trigger");
        if !trigger {
            continue;
        }
        n += 1;
        if (first as usize..=last as usize).contains(&i) {
            let end = line.encode_utf16().count();
            out.push(format!("[{i},{end},\"trigger {n}\"]"));
        }
    }
    format!("[{}]", out.join(","))
}

/// The pieces of a custom script's text form, by its markers: the map's
/// custom script (`header`) and each trigger's, in the trigger editor's
/// order. Lines from 0; a piece's code runs from the line after its
/// marker to the line before the next.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pieces {
    /// The `header` marker's line and the end of its code.
    pub header: Option<(usize, usize)>,
    /// Each trigger's: its marker's line, the end of its code, and
    /// whether it stores none (`trigger none`).
    pub triggers: Vec<(usize, usize, bool)>,
    /// The markers' comment (`//`, `--`).
    pub comment: &'static str,
    /// 1.31's layout (`magic`): a piece for each trigger that isn't a
    /// comment only.
    pub magic: bool,
}

/// A trigger's piece: its marker's line, the end of its code, whether it
/// stores none.
pub type Piece = (usize, usize, bool);

impl Pieces {
    /// Each trigger's piece, the triggers' being comments `comments` (a
    /// comment has none in 1.31's layout); an error when there are more or
    /// fewer pieces than they have.
    pub fn of_triggers(&self, comments: &[bool]) -> Result<Vec<Option<Piece>>, String> {
        let having = if self.magic { comments.iter().filter(|c| !**c).count() } else { comments.len() };
        if self.triggers.len() != having {
            let whose = if self.magic { " that aren't comments" } else { "" };
            return Err(format!(
                "war3map.wct has the code of {} triggers, war3map.wtg has {having}{whose}: the code of custom text can't be told whose",
                self.triggers.len()
            ));
        }
        let mut next = self.triggers.iter();
        Ok(comments.iter().map(|&c| if self.magic && c { None } else { next.next().copied() }).collect())
    }

    /// `order` (the triggers after a change, as [`reorder`] takes it) as
    /// the pieces': without the comments' in 1.31's layout. `comments`
    /// are the triggers' before; a new one is no comment.
    pub fn order(&self, order: &[Option<usize>], comments: &[bool]) -> Vec<Option<usize>> {
        if !self.magic {
            return order.to_vec();
        }
        let piece = |k: usize| comments.get(k).is_some_and(|c| !c).then(|| comments[..k].iter().filter(|c| !**c).count());
        order.iter().filter_map(|o| match o {
            Some(k) => piece(*k).map(Some),
            None => Some(None),
        }).collect()
    }
}

/// The pieces of the text form `text`.
pub fn pieces(text: &str) -> Pieces {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let markers: Vec<(usize, Vec<&str>)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| wct_rs::text::is_marker(l))
        .map(|(i, l)| (i, l.split_whitespace().skip(2).collect()))
        .collect();
    let mut out = Pieces { comment: if markers.first().is_some_and(|(i, _)| lines[*i].trim_start().starts_with("--")) { "--" } else { "//" }, ..Default::default() };
    for (k, (i, words)) in markers.iter().enumerate() {
        let end = markers.get(k + 1).map_or(lines.len(), |(j, _)| *j);
        match words.first().copied() {
            Some("header") => out.header = Some((*i, end)),
            Some("magic") => out.magic = true,
            Some("trigger") => out.triggers.push((*i, end, words.get(1) == Some(&"none"))),
            _ => {}
        }
    }
    out
}

/// The text form `text` with its triggers' pieces in a new order: `order`
/// is the triggers after a change, each the old one it is (from 0) or a
/// new one (`None`: a trigger of World Editor's, which stores no code).
/// The text before the first trigger's piece stays. `None` when the text
/// has no piece for some trigger `order` names.
pub fn reorder(text: &str, order: &[Option<usize>]) -> Option<String> {
    let p = pieces(text);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start = p.triggers.first().map_or(lines.len(), |t| t.0);
    let mut out: String = lines[..start].concat();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for o in order {
        match o {
            Some(k) => {
                let (from, to, _) = *p.triggers.get(*k)?;
                let piece = lines[from..to].concat();
                out.push_str(&piece);
                if !piece.ends_with('\n') {
                    out.push('\n');
                }
            }
            None => out.push_str(&format!("{} @wct trigger none\n", p.comment)),
        }
    }
    // The text's own end: no line break added where it had none.
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

#[cfg(test)]
#[path = "customtext.test.rs"]
mod tests;
