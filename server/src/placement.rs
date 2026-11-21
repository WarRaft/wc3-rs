//! What a map places, for the editor: `war3map.doo` (doodads and
//! destructables) and `war3mapUnits.doo` (units, buildings, items, start
//! locations), shown as their BNI text forms (`doo-rs`, a section per
//! thing placed) and saved back to the file's bytes.
//!
//! Since 1.32 every thing placed has a skin: which game the map is for is
//! in the `war3map.w3i` beside the file ([`layout`]). Without one, a file
//! opens in whichever layout it reads in, and text saves in the layout its
//! `skin=` lines say.

use std::path::Path;

use doo_rs::Layout;

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a placement file (`.doo`), by its extension?
pub fn is_placement_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("doo"))
}

/// The layout the map's `war3map.w3i` beside `path` says (on disk, or in
/// the same map); `None` without one.
pub fn layout(path: &Path) -> Option<Layout> {
    let w3i = path.parent()?.join("war3map.w3i");
    let bytes = if w3i.is_file() { std::fs::read(&w3i).ok() } else { crate::map::bytes(&w3i) };
    bytes.as_deref().and_then(Layout::from_w3i)
}

/// The layout for text without a `war3map.w3i`: the one its `skin=`
/// lines say.
fn text_layout(text: &str) -> Layout {
    if text.lines().any(|l| l.trim_start().starts_with("skin=")) { Layout::Skins } else { Layout::Classic }
}

/// Converts for the editor: the bytes of the file `name` to its text form
/// (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`). The error is a
/// message to show.
pub fn convert(op: u8, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let path = Path::new(name);
    let file = path.file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            let layout = match layout(path) {
                Some(l) => l,
                None => match doo_rs::FileKind::of(name) {
                    doo_rs::FileKind::Doodads => doo_rs::parse_doodads_any(data).map(|f| f.layout).unwrap_or_default(),
                    doo_rs::FileKind::Units => doo_rs::parse_units_any(data).map(|f| f.layout).unwrap_or_default(),
                },
            };
            doo_rs::text::to_text(name, data, layout)
                .map(String::into_bytes)
                .map_err(|e| format!("Can't read {file}: {e} (the layout for {})", layout.describe()))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let layout = layout(path).unwrap_or_else(|| text_layout(text));
            doo_rs::text::from_text(name, text, layout).map_err(|errors| {
                let e = &errors[0];
                format!("Can't save {file}: line {}: {}", e.line, e.message)
            })
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form at `path` from saving, past its BNI
/// errors. Nothing for other BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document, text: &str) -> Vec<parser_rs::ParseError> {
    let Some(path) = path.filter(|p| is_placement_file(p)) else { return Vec::new() };
    if !doo_rs::text::is_placement_text(doc) {
        return Vec::new();
    }
    let layout = layout(path).unwrap_or_else(|| text_layout(text));
    let bni = &doc.errors;
    doo_rs::text::errors(&path.to_string_lossy(), text, layout).into_iter().filter(|e| !bni.contains(e)).collect()
}

#[cfg(test)]
#[path = "placement.test.rs"]
mod tests;
