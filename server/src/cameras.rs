//! Camera files (`war3map.w3c`) for the editor, which shows them as text:
//! their BNI text form (`w3c-rs`, one section per camera), and back to
//! the file's bytes when that's saved. While the text is edited, it's a
//! BNI document like any other, and `analysis` reports what would stop it
//! from saving ([`diagnostics`]).
//!
//! The file doesn't say its layout (1.31 added a local pitch, yaw and roll
//! to every camera): the map's `war3map.w3i` next to it does, its first
//! four bytes being its format version. Without one, it's 1.27a's
//! ([`layout`]).

use std::path::Path;

use w3c_rs::Layout;

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a camera file, by its extension?
pub fn is_camera_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("w3c"))
}

/// The layout of the camera file at `path`: from `war3map.w3i` in the same
/// folder, on disk or inside the same map; 1.27a's without one.
pub fn layout(path: &Path) -> Layout {
    let Some(w3i) = path.parent().map(|p| p.join("war3map.w3i")) else {
        return Layout::default();
    };
    let bytes = if w3i.is_file() {
        // Only the version: the first four bytes.
        std::fs::File::open(&w3i).ok().and_then(|mut f| {
            let mut head = [0; 4];
            std::io::Read::read_exact(&mut f, &mut head).ok().map(|()| head.to_vec())
        })
    } else {
        crate::map::bytes(&w3i)
    };
    bytes.as_deref().and_then(Layout::from_w3i).unwrap_or_default()
}

/// Converts for the editor: the bytes of the camera file `name` to its
/// text form (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`). The
/// error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let path = Path::new(name);
    let file = path.file_name().map_or(name.into(), |n| n.to_string_lossy());
    let layout = layout(path);
    let why = |err: String| format!("{err} (the layout for {}, from the map's war3map.w3i or 1.27a's without it)", layout.describe());
    match op {
        TO_TEXT => {
            let cameras = w3c_rs::parse(data, layout).map_err(|e| why(format!("Can't read {file}: {e}")))?;
            w3c_rs::text::to_text(&cameras).map(String::into_bytes).map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let parsed = w3c_rs::text::from_text(text, layout);
            let Some(cameras) = parsed.file else {
                let e = &parsed.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
            cameras.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form of the camera file at `path` from
/// saving, past its BNI errors (which the caller has). Nothing for other
/// BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document) -> Vec<parser_rs::ParseError> {
    if !path.is_some_and(is_camera_file) || !w3c_rs::text::is_camera_text(doc) {
        return Vec::new();
    }
    let mut errors = w3c_rs::text::from_bni(doc, layout(path.expect("checked"))).errors;
    errors.drain(..doc.errors.len());
    errors
}

#[cfg(test)]
#[path = "cameras.test.rs"]
mod tests;
