//! Triggers (`war3map.wtg`) for the editor: shown as their BNI text form
//! (`wtg-rs`: categories, variables, and each trigger's events,
//! conditions and actions as lines of calls) and saved back to bytes.
//!
//! How many parameters each function takes isn't in the file: it's World
//! Editor's `UI\TriggerData.txt`, read through the project's lookup (its
//! game folders, then its map's roots, the last found winning: a modded
//! editor's data in a root replaces the game's). Opening needs it; saving
//! checks every function against it. While typing, the text is checked
//! without it (its syntax and fields). A value is the rest of its line as
//! it is (strings as JASS writes them), highlighted by what it is
//! (`wtg_rs::text::semantic_tokens`).

use std::path::Path;

use wtg_rs::TriggerData;

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a trigger file (`.wtg`), by its extension?
pub fn is_trigger_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wtg"))
}

const NO_DATA: &str = "no UI\\TriggerData.txt in the project's game folders or map roots: it says how many parameters each function takes, which the file doesn't";

/// Converts for the editor: the bytes of the file `name` to its text form
/// (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`), with `data`.
/// The error is a message to show.
pub fn convert(op: u8, name: &str, bytes: &[u8], data: &TriggerData) -> Result<Vec<u8>, String> {
    let file = Path::new(name).file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            if data.is_empty() {
                return Err(format!("Can't read {file}: {NO_DATA}"));
            }
            let f = wtg_rs::parse_any(bytes, data).map_err(|e| format!("Can't read {file}: {e}"))?;
            let wtg_rs::File::Classic(f) = f else {
                return Err(format!("Can't show {file}: the layout of 1.31 and later has no text form yet"));
            };
            wtg_rs::text::to_text(&f).map(String::into_bytes).map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(bytes).map_err(|_| "the text isn't UTF-8".to_string())?;
            let p = wtg_rs::text::from_text(text, (!data.is_empty()).then_some(data));
            let Some(f) = p.file else {
                let e = &p.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
            f.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form at `path` from saving, past its BNI
/// errors and past what `UI\TriggerData.txt` says. Nothing for other BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document, text: &str) -> Vec<parser_rs::ParseError> {
    if !path.is_some_and(is_trigger_file) || !wtg_rs::text::is_trigger_text(doc) {
        return Vec::new();
    }
    let bni = &doc.errors;
    wtg_rs::text::from_bni(doc, text, None).errors.into_iter().filter(|e| !bni.contains(e)).collect()
}

#[cfg(test)]
#[path = "triggers.test.rs"]
mod tests;
