//! Hovers over `TRIGSTR_<n>`, in any file: the text of string `n` from the
//! map's string table, `war3map.wts` next to the file (in the same folder,
//! or inside the same map). A table the editor has open, or the project
//! has loaded, is read as it is now, unsaved edits and all; else from disk
//! or the map.

use std::path::Path;

use crate::document::{LineIndex, Position};
use crate::workspace::{Workspace, path_id};

/// The hover for a `TRIGSTR_<n>` at `pos` in file `id`, as Markdown (with
/// the colors as HTML spans); `None` off one, or without a string table.
pub fn hover(ws: &Workspace, id: &str, pos: Position) -> Option<String> {
    let (doc, _) = ws.file(id)?;
    let lines = LineIndex::new(&doc.text);
    let line = lines.line(&doc.text, pos.line as usize);
    let col = lines.column(&doc.text, pos) - 1;
    let (reference, number) = reference_at(line, col)?;
    let wts = doc.path.as_deref()?.parent()?.join("war3map.wts");
    let table = wts_rs::parse(&table_text(ws, &wts)?);
    let Some(entry) = table.get(number) else {
        return Some(format!("**{reference}**\n\nNot in `war3map.wts`."));
    };
    let mut out = format!("**{reference}** · `war3map.wts`, line {}\n\n{}", entry.line, colored(&entry.text));
    if !entry.comments.is_empty() {
        let comments: Vec<String> = entry.comments.iter().map(|c| format!("*{}*", escape(c))).collect();
        out += &format!("\n\n---\n\n{}", comments.join("  \n"));
    }
    Some(out)
}

/// The `TRIGSTR_<digits>` that character `col` (0-based) of `line` is in,
/// and its number.
fn reference_at(line: &str, col: usize) -> Option<(String, u32)> {
    let chars: Vec<char> = line.chars().collect();
    let text: String = chars.iter().collect();
    let mut from = 0;
    while let Some(at) = text[from..].find("TRIGSTR_").map(|i| from + i) {
        let start = text[..at].chars().count();
        let digits = chars[start + 8..].iter().take_while(|c| c.is_ascii_digit()).count();
        let end = start + 8 + digits;
        if digits > 0 && (start..=end).contains(&col) {
            let reference: String = chars[start..end].iter().collect();
            return Some((reference.clone(), reference[8..].parse().ok()?));
        }
        from = at + 8;
    }
    None
}

/// The string table's text: the editor's or the project's copy, else the
/// file on disk or in the map.
fn table_text(ws: &Workspace, path: &Path) -> Option<String> {
    if let Some((doc, _)) = ws.file(&path_id(path)) {
        return Some(doc.text.clone());
    }
    if path.is_file() {
        return std::fs::read(path).ok().map(crate::document::decode);
    }
    crate::map::text(path)
}

/// Markdown with nothing in `text` read as Markdown or HTML.
fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out += "&amp;",
            '<' => out += "&lt;",
            '>' => out += "&gt;",
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// A string's text as the game shows it: `|cAARRGGBB ... |r` in that color
/// (an HTML span the editor's hover draws), `|n` and line breaks as line
/// breaks.
fn colored(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut plain = String::new();
    let mut open = 0;
    let mut i = 0;
    let flush = |plain: &mut String, out: &mut String| {
        *out += &escape(plain);
        plain.clear();
    };
    while i < chars.len() {
        let code = |n: usize| chars.get(i + 1).map(char::to_ascii_lowercase) == Some(if n == 0 { 'c' } else { 'r' });
        if chars[i] == '|'
            && code(0)
            && chars.len() >= i + 10
            && chars[i + 2..i + 10].iter().all(char::is_ascii_hexdigit)
        {
            flush(&mut plain, &mut out);
            let rgb: String = chars[i + 4..i + 10].iter().collect();
            out += &format!("<span style=\"color:#{rgb};\">");
            open += 1;
            i += 10;
        } else if chars[i] == '|' && code(1) {
            flush(&mut plain, &mut out);
            if open > 0 {
                out += "</span>";
                open -= 1;
            }
            i += 2;
        } else if chars[i] == '|' && chars.get(i + 1).map(char::to_ascii_lowercase) == Some('n') || chars[i] == '\n' {
            flush(&mut plain, &mut out);
            out += "<br>";
            i += if chars[i] == '\n' { 1 } else { 2 };
        } else {
            plain.push(chars[i]);
            i += 1;
        }
    }
    flush(&mut plain, &mut out);
    out + &"</span>".repeat(open)
}

#[cfg(test)]
#[path = "trigstr.test.rs"]
mod tests;
