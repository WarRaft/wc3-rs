//! Inlay hints for the text forms of regions (`.w3r`), terrain (`.w3e`)
//! and map info (`.w3i`): the name World Editor shows after a value
//! stored by id, from the game's files (`names`). Nothing is built in:
//! without the files (no project, no game folders) there are no hints but
//! for the values that mean none.
//!
//! - a region's `weather`: the effect's name, or `none` for
//!   `0x00000000`;
//! - `[w3e] tileset`, `[w3i] tileset` and `light_tileset`: the tileset's
//!   name; `light_tileset=0` is the map's own light.

use std::path::Path;

use bni_rs::ast::Item;

use crate::document::LineIndex;
use crate::names::Names;

/// The file types whose text forms have such hints, by extension.
const TYPES: [&str; 3] = ["w3r", "w3e", "w3i"];

/// Does the text form of the file at `path` have names to show?
pub fn has_names(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| TYPES.iter().any(|t| e.eq_ignore_ascii_case(t)))
}

/// The value of an item: its one field's text, trimmed.
fn value(item: &Item) -> Option<String> {
    match item.fields.as_slice() {
        [f] => Some(f.text().trim().to_string()),
        _ => None,
    }
}

/// What a region's `weather` shows.
fn weather(value: &str, names: &Names) -> Option<String> {
    let code = parser_rs::bin::Rawcode::parse(value)?;
    if code.is_none() {
        return Some("none".into());
    }
    match names.weather(&code.to_string()) {
        Some(name) => Some(name.to_string()),
        None if !names.weather.is_empty() => Some("not in TerrainArt\\Weather.slk".into()),
        None => None,
    }
}

/// What a tileset letter shows; `own` is what `0` shows, when it means
/// something.
fn tileset(value: &str, names: &Names, own: Option<&str>) -> Option<String> {
    if value == "0" {
        return own.map(String::from);
    }
    let mut chars = value.chars();
    let (Some(letter), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match names.tileset(letter) {
        Some(name) => Some(name.to_string()),
        None if !names.tilesets.is_empty() => Some("not in UI\\WorldEditData.txt [TileSets]".into()),
        None => None,
    }
}

/// The hints of lines `first..=last` (0-based) of the text form `text` of
/// the file at `path`, as JSON `[[line, character, "label"], ...]` in
/// editor coordinates: after the value each goes with.
pub fn hints(text: &str, path: &Path, names: &Names, first: u32, last: u32) -> String {
    let doc = bni_rs::parse(text);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let lines = LineIndex::new(text);
    let range = first as usize + 1..=last as usize + 1;
    let mut out = Vec::new();
    let Some((head, rest)) = doc.sections.split_first() else {
        return "[]".into();
    };
    if head.name.text != ext {
        return "[]".into();
    }
    let mut add = |item: &Item, label: Option<String>| {
        let Some(label) = label else { return };
        let end = item.fields.last().map_or(item.eq, |f| f.span);
        let at = lines.range(text, end.end_line, end.end_col, end.end_col).start;
        out.push(format!("[{},{},{}]", at.line, at.character, crate::objects::json_string(&label)));
    };
    let key = |item: &Item| item.key.as_ref().map(|k| k.text.trim().to_string());
    match ext.as_str() {
        "w3r" => {
            for item in rest.iter().flat_map(|s| &s.items).filter(|i| range.contains(&i.line)) {
                if key(item).as_deref() == Some("weather") {
                    add(item, value(item).and_then(|v| weather(&v, names)));
                }
            }
        }
        _ => {
            for item in head.items.iter().filter(|i| range.contains(&i.line)) {
                match key(item).as_deref() {
                    Some("tileset") => add(item, value(item).and_then(|v| tileset(&v, names, None))),
                    Some("light_tileset") if ext == "w3i" => {
                        add(item, value(item).and_then(|v| tileset(&v, names, Some("the map's own"))))
                    }
                    _ => {}
                }
            }
        }
    }
    format!("[{}]", out.join(","))
}

#[cfg(test)]
#[path = "hints.test.rs"]
mod tests;
