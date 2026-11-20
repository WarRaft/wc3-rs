//! Object data (`.w3a`, `.w3b`, `.w3d`, `.w3h`, `.w3q`, `.w3t`, `.w3u`)
//! for the editor, which shows it as text: its BNI text form
//! (`w3abdhqtu-rs`), and back to the file's bytes when that's saved. While
//! the text is edited, it's a BNI document like any other, and
//! `analysis` reports its errors.

use std::path::Path;

use w3abdhqtu_rs::{Kind, from_text, parse};

/// `ClientMsg::Objects`' ops.
pub const TO_TEXT: u8 = 0;
pub const TO_BYTES: u8 = 1;

/// Converts for the editor: the bytes of the file `name` to its text form
/// (`TO_TEXT`), or a text form to the bytes of `name` (`TO_BYTES`). The
/// error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let file = Path::new(name)
        .file_name()
        .map_or(name.into(), |n| n.to_string_lossy());
    let kind = Kind::from_path(name).ok_or(format!(
        "{file} isn't object data (.w3a .w3b .w3d .w3h .w3q .w3t .w3u)"
    ))?;
    match op {
        TO_TEXT => {
            let data = parse(data, kind).map_err(|err| format!("Can't read {file}: {err}"))?;
            Ok(data.to_text().into_bytes())
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            to_bytes(kind, text).map_err(|err| format!("Can't save {file}: {err}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

fn to_bytes(kind: Kind, text: &str) -> Result<Vec<u8>, String> {
    let parsed = from_text(text);
    let Some(data) = parsed.data else {
        let n = parsed.errors.len();
        let first = &parsed.errors[0];
        let more = if n > 1 {
            format!(" (and {} more)", n - 1)
        } else {
            String::new()
        };
        return Err(format!("line {}: {}{more}", first.line, first.message));
    };
    if data.kind != kind {
        return Err(format!(
            "the text holds .{} data, not .{}",
            data.kind.extension(),
            kind.extension()
        ));
    }
    data.to_bytes().map_err(|err| err.message)
}

/// The errors of a BNI document that is an object-data text form, past
/// the BNI ones (which the caller has); none for other BNI.
pub fn text_errors(doc: &bni_rs::Document) -> Vec<parser_rs::ParseError> {
    if !w3abdhqtu_rs::is_object_text(doc) {
        return Vec::new();
    }
    let mut errors = w3abdhqtu_rs::from_bni(doc).errors;
    errors.drain(..doc.errors.len());
    errors
}

#[cfg(test)]
#[path = "objects.test.rs"]
mod tests;
