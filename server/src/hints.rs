//! Inlay hints for the text forms of regions (`.w3r`), sounds (`.w3s`),
//! terrain (`.w3e`) and map info (`.w3i`): what a value means, after it.
//! The texts carry none of it, only the values.
//!
//! Every word shown is the game's where the game has one, in its
//! language, from its files through the project's layers (`names`):
//!
//! - names the game's files give by id: a region's weather, a sound's
//!   EAX setting (an empty one is `DefaultEAXON`'s, as the game takes it)
//!   and channel, a tileset, the terrain's ground and cliff textures
//!   (with how many of the file's points use each, read from the file);
//! - the meaning of a format's value (a race, a flag, a fog style): the
//!   crates' tables (`w3i_rs::RACES`, `w3s_rs::FLAGS` ...) give the key of
//!   World Editor's string for it (`Names::string`), and their English
//!   name when the game's files don't have it.
//!
//! What only the format says (the order of a list's numbers, which
//! players a mask has) is in English. Without the game's files the
//! built-in English names show, and nothing for an id only the files
//! name.

use std::path::Path;

use bni_rs::ast::Item;

use crate::document::LineIndex;
use crate::names::Names;

/// The file types whose text forms have hints, by extension.
const TYPES: [&str; 4] = ["w3r", "w3s", "w3e", "w3i"];

/// Does the text form of the file at `path` have hints?
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

/// A whole number, decimal or `0x` and hex.
fn number(value: &str) -> Option<i64> {
    match value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => value.parse().ok(),
    }
}

/// The game's word for a meaning (its string), else the English name.
fn word(names: &Names, english: &str, key: &str) -> String {
    if key.is_empty() { english.to_string() } else { names.say(key, english) }
}

/// What a value of `table` shows.
fn meaning<T: PartialEq + Copy + TryFrom<i64>>(table: &[(T, &str, &str)], value: &str, names: &Names) -> Option<String> {
    let n = T::try_from(number(value)?).ok()?;
    table.iter().find(|(v, _, _)| *v == n).map(|(_, english, key)| word(names, english, key))
}

/// What flags of `table` show: the words for the bits set, and what's
/// left in hex.
fn bits(table: &[(u32, &str, &str)], value: &str, names: &Names) -> Option<String> {
    let n = u32::try_from(number(value)?).ok()?;
    let mut out: Vec<String> = table
        .iter()
        .filter(|(b, _, _)| n & b != 0)
        .map(|(_, english, key)| word(names, english, key))
        .collect();
    let rest = table.iter().fold(n, |n, (b, _, _)| n & !b);
    if rest != 0 {
        out.push(format!("0x{rest:X}"));
    }
    (!out.is_empty()).then(|| out.join(", "))
}

/// The players of a mask, counting from 1.
fn players(value: &str) -> Option<String> {
    let mask = u32::try_from(number(value)?).ok()?;
    let list: Vec<String> = (0..32).filter(|i| mask & (1 << i) != 0).map(|i| (i + 1).to_string()).collect();
    (!list.is_empty()).then(|| format!("{} {}", word_players(list.len()), list.join(", ")))
}

fn word_players(n: usize) -> &'static str {
    if n == 1 { "player" } else { "players" }
}

/// The values of a list field, each named by `name` (`[unit, item]`).
fn each(value: &str, name: impl Fn(&str) -> Option<String>) -> Option<String> {
    let names: Option<Vec<String>> = value.split(',').map(|v| name(v.trim())).collect();
    names.filter(|n| !n.is_empty()).map(|n| n.join(", "))
}

/// What a region's `weather` shows.
fn weather(value: &str, names: &Names) -> Option<String> {
    let code = parser_rs::bin::Rawcode::parse(value)?;
    if code.is_none() {
        return Some(names.say("WESTRING_NONE", "none"));
    }
    match names.weather(&code.to_string()) {
        Some(name) => Some(name.to_string()),
        None if !names.weather.is_empty() => Some("not in TerrainArt\\Weather.slk".into()),
        None => None,
    }
}

/// What a sound's `eax` shows: an empty one is the default setting.
fn eax(value: &str, names: &Names) -> Option<String> {
    let label = if value.is_empty() { "DefaultEAXON" } else { value };
    match names.eax(label) {
        Some(name) => Some(name.to_string()),
        None if !names.eax.is_empty() => Some("not in UI\\SoundInfo\\EAXDefs.slk".into()),
        None => None,
    }
}

/// What a sound's `channel` shows: the game's name for it, or the
/// built-in one.
fn channel(value: &str, names: &Names) -> Option<String> {
    let n = if value == "default" { w3s_rs::DEFAULT_INT } else { value.parse().ok()? };
    if let Some(name) = names.channel(n) {
        return Some(name.to_string());
    }
    Some(w3s_rs::channel_name(n).unwrap_or("not one of the game's channels (0 to 14)").to_string())
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

/// What an item of a map info file's section `section` shows.
fn map_info(section: &str, key: &str, v: &str, names: &Names) -> Option<String> {
    use w3i_rs::{AVAILABILITY, COLUMNS, FLAGS, FOG_STYLES, FORCE_FLAGS, MODES, PLAYER_TYPES, RACES, SCRIPTS};
    let color = || Some("red, green, blue, alpha".to_string());
    match (section, key) {
        ("w3i", "version") => v.parse().ok().map(|n| w3i_rs::describe(n).to_string()),
        ("w3i", "game_version") => Some("major, minor, patch, build".into()),
        ("w3i", "camera_bounds") => Some("x, y of bottom left, top right, top left, bottom right".into()),
        ("w3i", "camera_complements") => Some("left, right, bottom, top".into()),
        ("w3i", "flags") => bits(&FLAGS, v, names),
        ("w3i", "tileset") => tileset(v, names, None),
        ("w3i", "light_tileset") => tileset(v, names, Some("the map's own")),
        ("w3i", "fog_style") => meaning(&FOG_STYLES, v, names),
        ("w3i", "fog_color" | "water_color") => color(),
        ("w3i", "weather") => weather(v, names),
        ("w3i", "script") => meaning(&SCRIPTS, v, names),
        ("w3i", "supported_modes") => meaning(&MODES, v, names),
        ("player", "number") => number(v).map(|n| format!("{} {}", names.say("WESTRING_PLAYER", "Player"), n + 1)),
        ("player", "type") => meaning(&PLAYER_TYPES, v, names),
        ("player", "race") => meaning(&RACES, v, names),
        ("player", "ally_low" | "ally_high" | "enemy_low" | "enemy_high") => players(v),
        ("force", "flags") => bits(&FORCE_FLAGS, v, names),
        ("force" | "upgrade" | "tech", "players") => players(v),
        ("upgrade", "availability") => meaning(&AVAILABILITY, v, names),
        ("unit_table", "columns") => each(v, |c| meaning(&COLUMNS, c, names)),
        _ => None,
    }
}

/// The hints of lines `first..=last` (0-based) of the text form `text` of
/// the file at `path`, as JSON `[[line, character, "label"], ...]` in
/// editor coordinates: after the value each goes with.
/// `usage` is what a terrain file's points say (`w3e_rs::text::Usage`).
pub fn hints(
    text: &str,
    path: &Path,
    names: &Names,
    usage: Option<&w3e_rs::text::Usage>,
    first: u32,
    last: u32,
) -> String {
    let doc = bni_rs::parse(text);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let lines = LineIndex::new(text);
    let range = first as usize + 1..=last as usize + 1;
    if doc.sections.first().is_none_or(|h| h.name.text != ext) {
        return "[]".into();
    }
    let mut out = Vec::new();
    for (i, section) in doc.sections.iter().enumerate() {
        for item in section.items.iter().filter(|i| range.contains(&i.line)) {
            let Some(key) = item.key.as_ref().map(|k| k.text.trim()) else { continue };
            // The whole value: a list's fields joined; an empty one is
            // empty (`eax=`).
            let v = if item.fields.len() > 1 { item.value_text().trim().to_string() } else { value(item).unwrap_or_default() };
            let label = match (ext.as_str(), i) {
                ("w3r", 1..) if key == "weather" => weather(&v, names),
                ("w3s", 1..) => match key {
                    "eax" => eax(&v, names),
                    "channel" => channel(&v, names),
                    "flags" => bits(&w3s_rs::FLAGS, &v, names),
                    _ => None,
                },
                ("w3e", 0) => match key {
                    "tileset" => tileset(&v, names, None),
                    _ => {
                        let header = |k: &str| {
                            section.items.iter().find(|i| i.key.as_ref().is_some_and(|n| n.text.trim() == k)).and_then(value)
                        };
                        let named = |ground: bool, code: &str| {
                            if ground { names.ground.get(code) } else { names.cliffs.get(code) }.cloned()
                        };
                        w3e_rs::text::hint(key, &v, &header, usage, &named)
                    }
                },
                ("w3i", _) => map_info(&section.name.text, key, &v, names),
                _ => None,
            };
            let Some(label) = label else { continue };
            let end = item.fields.last().map_or(item.eq, |f| f.span);
            let at = lines.range(text, end.end_line, end.end_col, end.end_col).start;
            out.push(format!("[{},{},{}]", at.line, at.character, crate::objects::json_string(&label)));
        }
    }
    format!("[{}]", out.join(","))
}

#[cfg(test)]
#[path = "hints.test.rs"]
mod tests;
