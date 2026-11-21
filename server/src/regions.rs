//! Region files (`war3map.w3r`) for the editor, which shows them as text:
//! their BNI text form (`w3r-rs`, a section per region named after it),
//! and back to the file's bytes when that's saved. While the text is
//! edited, it's a BNI document like any other, and `analysis` reports what
//! would stop it from saving ([`diagnostics`]).

use std::path::Path;

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a region file, by its extension?
pub fn is_region_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("w3r"))
}

/// Converts for the editor: the bytes of the region file `name` to its
/// text form (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`). The
/// error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let path = Path::new(name);
    let file = path.file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            let regions = w3r_rs::parse(data).map_err(|e| format!("Can't read {file}: {e}"))?;
            w3r_rs::text::to_text(&regions).map(String::into_bytes).map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let parsed = w3r_rs::text::from_text(text);
            let Some(regions) = parsed.file else {
                let e = &parsed.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
            regions.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form of the region file at `path` from
/// saving, past its BNI errors (which the caller has). Nothing for other
/// BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document) -> Vec<parser_rs::ParseError> {
    if !path.is_some_and(is_region_file) || !w3r_rs::text::is_region_text(doc) {
        return Vec::new();
    }
    let mut errors = w3r_rs::text::from_bni(doc).errors;
    errors.drain(..doc.errors.len());
    errors
}

/// The colors of a region file's text form, for the editor's color
/// picker (see `hints::colors`).
pub fn colors(text: &str) -> String {
    crate::hints::colors(text, Path::new("war3map.w3r"))
}

#[cfg(test)]
#[path = "regions.test.rs"]
mod tests;
