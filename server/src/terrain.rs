//! Terrain files (`war3map.w3e`) for the editor, which shows their header
//! as text (`w3e-rs`'s text form: tilesets, size, position), the points
//! left out. Saving puts the header back onto the file as it is now, on
//! disk or inside the map, its points kept, and fails with the reason when
//! the points don't allow the change. While the text is edited, it's a BNI
//! document like any other, and `analysis` reports what would stop it
//! from saving ([`diagnostics`]).

use std::path::Path;

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a terrain file, by its extension?
pub fn is_terrain_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("w3e"))
}

/// Converts for the editor: the bytes of the terrain file `name` to its
/// text form (`TO_TEXT`), or a text form to the bytes of that file with
/// the new header (`TO_BYTES`). The error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let path = Path::new(name);
    let file = path.file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            let terrain = w3e_rs::parse(data).map_err(|e| format!("Can't read {file}: {e}"))?;
            Ok(w3e_rs::text::to_text(&terrain).into_bytes())
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let parsed = w3e_rs::text::from_text(text);
            let Some(header) = parsed.header else {
                let e = &parsed.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
            // The points: the file's as it is now.
            let bytes = if path.is_file() { std::fs::read(path).ok() } else { crate::map::bytes(path) };
            let bytes = bytes.ok_or_else(|| format!("Can't save {file}: can't read its points from the file"))?;
            let mut terrain =
                w3e_rs::parse(&bytes).map_err(|e| format!("Can't save {file}: can't read its points: {e}"))?;
            header.apply(&mut terrain).map_err(|e| format!("Can't save {file}: {e}"))?;
            terrain.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form of the terrain file at `path` from
/// saving, past its BNI errors (which the caller has); what only the
/// points can say, saving says. Nothing for other BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document) -> Vec<parser_rs::ParseError> {
    if !path.is_some_and(is_terrain_file) || !w3e_rs::text::is_terrain_text(doc) {
        return Vec::new();
    }
    let mut errors = w3e_rs::text::from_bni(doc).errors;
    errors.drain(..doc.errors.len());
    errors
}

#[cfg(test)]
#[path = "terrain.test.rs"]
mod tests;
