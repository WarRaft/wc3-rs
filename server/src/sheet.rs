//! SYLK tables (`.slk`) for the editor's table view: the sheet as JSON
//! ([`table`]), and the text edits that write a cell ([`set_cell`]). The
//! text stays the document; the table view only shows it and asks for
//! edits, which the editor applies like any other (so undo, save and the
//! text view keep working). Both read the same `slk-rs` parse the
//! highlighting does.

use slk_rs::{Table, Value};

use crate::document::LineIndex;

/// The sheet of `text` as JSON:
///
/// ```json
/// {"rows": 3, "cols": 4, "errors": 0,
///  "cells": [[1, 1, "s", "unitID", null, null], [2, 3, "n", "420", "a comment", "R[-1]C*2"], ...]}
/// ```
///
/// A cell is row, column (from 1), its kind (`n` number, `s` string, `b`
/// `TRUE`/`FALSE`, `e` an error value, `x` other text, `""` none), the
/// text it shows, its comment and its formula.
pub fn table(text: &str) -> String {
    let doc = slk_rs::parse(text);
    let table = Table::new(&doc);
    let mut out = format!(
        "{{\"rows\": {}, \"cols\": {}, \"errors\": {}, \"cells\": [",
        table.rows(),
        table.cols(),
        doc.errors.len()
    );
    for (i, ((row, col), cell)) in table.cells().enumerate() {
        let kind = match cell.value {
            Value::Number(_) => "n",
            Value::Str(_) => "s",
            Value::Bool(_) => "b",
            Value::Error(_) => "e",
            Value::Empty => "",
            _ => "x",
        };
        let opt = |s: &Option<String>| s.as_deref().map_or("null".to_string(), json_str);
        if i > 0 {
            out.push(',');
        }
        out += &format!(
            "[{row},{col},\"{kind}\",{},{},{}]",
            json_str(&cell.value.display()),
            opt(&cell.comment),
            opt(&cell.formula)
        );
    }
    out + "]}"
}

/// The edits that write `input` into the cell at `row`, `col` (from 1) of
/// `text`, as JSON: `[[startLine, startChar, endLine, endChar, "text"],
/// ...]` in editor coordinates (0-based lines, UTF-16 characters). Empty
/// input clears the cell; how input becomes a value is
/// `slk_rs::parse_input` (the cell's kind is kept for text). `[]` when
/// nothing changes or the cell is out of range.
pub fn set_cell(text: &str, row: u32, col: u32, input: &str) -> String {
    if row == 0 || col == 0 {
        return "[]".into();
    }
    let doc = slk_rs::parse(text);
    let table = Table::new(&doc);
    let value = slk_rs::parse_input(input, table.get(row, col).map(|c| &c.value));
    let lines = LineIndex::new(text);
    let edits: Vec<String> = slk_rs::set_cell(&doc, row, col, value.as_ref())
        .into_iter()
        .map(|e| {
            let r = lines.span(text, e.span);
            format!(
                "[{},{},{},{},{}]",
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                json_str(&e.text)
            )
        })
        .collect();
    format!("[{}]", edits.join(","))
}

/// A JSON string literal.
fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            '\t' => out += "\\t",
            c if (c as u32) < 0x20 => out += &format!("\\u{:04x}", c as u32),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "sheet.test.rs"]
mod tests;
