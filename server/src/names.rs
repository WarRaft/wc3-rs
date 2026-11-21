//! Names World Editor shows for values the files store by id: weather
//! effects (`RAhr`: Ashenvale Rain (Heavy)) and tilesets (`L`: Lordaeron
//! Summer). Nothing is built in: every name comes from the game's files,
//! looked up through a project's layers (`game::Lookup`: its game
//! folders, then its map's roots, the last that has a file wins), so a
//! game folder or a map that replaces them replaces the names too.
//!
//! - weather: `TerrainArt\Weather.slk`, its `effectID` and `name`
//!   columns;
//! - tilesets: `UI\WorldEditData.txt`, `[TileSets]`, a letter's first
//!   value;
//! - a name that is a `WESTRING_...` key is looked up in
//!   `UI\WorldEditStrings.txt` and `UI\WorldEditGameStrings.txt`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::game::{Game, Lookup};

/// The files names come from, for [`Cache::disk_changed`].
pub const FILES: [&str; 4] = [
    "TerrainArt\\Weather.slk",
    "UI\\WorldEditData.txt",
    "UI\\WorldEditStrings.txt",
    "UI\\WorldEditGameStrings.txt",
];

/// The names a project's layers give.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Names {
    /// By weather effect id (`RAhr`).
    pub weather: HashMap<String, String>,
    /// By tileset letter (`L`).
    pub tilesets: HashMap<char, String>,
}

impl Names {
    /// Reads them through `game`'s layers. What a file doesn't give stays
    /// out.
    pub fn load(game: &Game) -> Names {
        let read = |name: &str| game.read(name).map(|(bytes, _)| crate::document::decode(bytes));
        let mut strings: HashMap<String, String> = HashMap::new();
        for file in ["UI\\WorldEditStrings.txt", "UI\\WorldEditGameStrings.txt"] {
            let Some(text) = read(file) else { continue };
            for section in bni_rs::parse(&text).sections {
                for item in section.items {
                    if let Some(key) = &item.key {
                        strings.insert(key.text.trim().to_string(), item.value_text().trim().to_string());
                    }
                }
            }
        }
        let resolve = |s: &str| strings.get(s).cloned().unwrap_or_else(|| s.to_string());

        let mut weather = HashMap::new();
        if let Some(text) = read("TerrainArt\\Weather.slk") {
            let table = slk_rs::Table::new(&slk_rs::parse(&text));
            let column = |name: &str| (1..=table.cols()).find(|&c| table.text(1, c).trim() == name);
            if let (Some(id), Some(name)) = (column("effectID"), column("name")) {
                for row in 2..=table.rows() {
                    let code = table.text(row, id).trim().to_string();
                    let label = table.text(row, name);
                    if !code.is_empty() && !label.trim().is_empty() {
                        weather.insert(code, resolve(label.trim()));
                    }
                }
            }
        }

        let mut tilesets = HashMap::new();
        if let Some(text) = read("UI\\WorldEditData.txt") {
            let doc = bni_rs::parse(&text);
            for item in doc.section("TileSets").map(|s| s.items.as_slice()).unwrap_or_default() {
                let (Some(key), Some(first)) = (&item.key, item.fields.first()) else { continue };
                let mut letters = key.text.trim().chars();
                if let (Some(letter), None) = (letters.next(), letters.next()) {
                    tilesets.insert(letter, resolve(first.text().trim()));
                }
            }
        }
        Names { weather, tilesets }
    }

    /// The name of weather effect `id`.
    pub fn weather(&self, id: &str) -> Option<&str> {
        self.weather.get(id).map(String::as_str)
    }

    /// The name of tileset `letter`, in any case.
    pub fn tileset(&self, letter: char) -> Option<&str> {
        self.tilesets
            .get(&letter)
            .or_else(|| self.tilesets.get(&letter.to_ascii_uppercase()))
            .map(String::as_str)
    }
}

/// The names of a lookup, opening its layers.
pub fn names(lookup: &Lookup) -> Names {
    if lookup.is_empty() {
        return Names::default();
    }
    Names::load(&lookup.open().0)
}

/// Names read so far, by lookup: hints are asked for on every scroll.
#[derive(Default)]
pub struct Cache {
    read: HashMap<Lookup, Arc<Names>>,
}

impl Cache {
    pub fn get(&mut self, lookup: &Lookup) -> Arc<Names> {
        self.read
            .entry(lookup.clone())
            .or_insert_with(|| Arc::new(names(lookup)))
            .clone()
    }

    /// Drops what changes with `paths`; whether anything was dropped.
    pub fn disk_changed(&mut self, paths: &[PathBuf]) -> bool {
        let before = self.read.len();
        let in_game = |folder: &std::path::Path, path: &std::path::Path| {
            crate::game::is_archive_name(path) && path.parent() == Some(folder)
                || FILES.iter().any(|f| {
                    let rel = path.strip_prefix(folder).map(|r| r.to_string_lossy().replace('/', "\\"));
                    rel.is_ok_and(|r| r.eq_ignore_ascii_case(f))
                })
        };
        self.read.retain(|lookup, _| !paths.iter().any(|p| lookup.affected_by(p, in_game)));
        self.read.len() != before
    }
}

#[cfg(test)]
#[path = "names.test.rs"]
mod tests;
