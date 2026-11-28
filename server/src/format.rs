//! Formatting code (JASS and vJASS, Zinc, AngelScript): every line moved
//! left or right to its level, never a line added, removed or joined, so
//! every line number still means the same line (see
//! [`parser_rs::format`]). A JASS file is read in its dialect
//! ([`analysis::jass_dialect`]), and a vJASS file's `//! zinc` blocks as
//! Zinc.

use parser_rs::{Edit, SymbolKind};

use crate::analysis;
use crate::document::{Document, Language, Position, Range};
use crate::index::{Answer, Location};
use crate::project::Project;
use crate::workspace::Workspace;

/// The edits that indent lines `pos.line` to `pos.character` (0-based,
/// inclusive) of file `id`, each level `unit` (the editor's: four spaces,
/// a tab): a location each, its `range` the line's indentation and its
/// `name` the new one. None for a file of another language.
pub fn format(ws: &Workspace, id: &str, pos: Position, unit: &str) -> Answer {
    let Some((doc, _)) = ws.file(id) else { return Answer::default() };
    let lines = pos.line as usize + 1..=(pos.character as usize).saturating_add(1);
    let locations = edits(doc, ws.settings(id), unit, lines)
        .into_iter()
        .map(|e| Location {
            id: id.to_string(),
            range: range(&e),
            name: e.text,
            kind: SymbolKind::Local,
            container: String::new(),
        })
        .collect();
    Answer { locations, text: String::new() }
}

/// `doc`'s edits, read with its `project`'s settings.
pub fn edits(doc: &Document, project: Option<&Project>, unit: &str, lines: std::ops::RangeInclusive<usize>) -> Vec<Edit> {
    let unit = if unit.is_empty() || !unit.chars().all(|c| c == ' ' || c == '\t') { "    " } else { unit };
    match doc.language {
        Language::Jass => jass_rs::format::format(&doc.text, analysis::jass_dialect(doc, project), unit, lines),
        Language::Zinc => zinc_rs::format::format(&doc.text, unit, lines),
        Language::AngelScript => as_rs::format::format(&doc.text, unit, lines),
        _ => Vec::new(),
    }
}

/// An edit's span in the editor's terms. It only ever covers a line's
/// spaces and tabs, after a byte-order mark at most, each one UTF-16
/// character as one codepoint.
fn range(e: &Edit) -> Range {
    let at = |line: usize, col: usize| Position::new(line as u32 - 1, col as u32 - 1);
    Range::new(at(e.span.start_line, e.span.start_col), at(e.span.end_line, e.span.end_col))
}

#[cfg(test)]
#[path = "format.test.rs"]
mod tests;
