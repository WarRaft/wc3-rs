//! Names World Editor shows for values the files store by id or number:
//! weather effects (`RAhr`: Ashenvale Rain (Heavy)), tilesets (`L`:
//! Lordaeron Summer), sounds' EAX settings (`SpellsEAX`: Spells) and
//! channels (`5`: Combat), and the editor's words for the values formats
//! define (a race, a flag: [`Names::string`]). They come from the game's
//! files, in the game's language, looked up through a project's layers
//! (`game::Lookup`: its game folders, then its map's roots, the last that
//! has a file wins), so a game folder or a map that replaces them
//! replaces the names too.
//!
//! - weather: `TerrainArt\Weather.slk`, its `effectID` and `name`
//!   columns;
//! - ground and cliff textures: `TerrainArt\Terrain.slk` (`tileID`,
//!   `name`) and `TerrainArt\CliffTypes.slk` (`cliffID`, `name`);
//! - tilesets: `UI\WorldEditData.txt`, `[TileSets]`, a letter's first
//!   value;
//! - sound channels: `UI\WorldEditData.txt`, `[SoundChannels]`, a
//!   number's second value;
//! - EAX settings: `UI\SoundInfo\EAXDefs.slk`, its `EAXLabel` and
//!   `DisplayText` columns;
//! - the editor's strings: `UI\WorldEditStrings.txt` and
//!   `UI\WorldEditGameStrings.txt`, `WESTRING_...` keys. A name that is
//!   a key is looked up there, and so is a value that is a key in turn.
//! - the trigger editor's functions: `UI\TriggerData.txt` (how many
//!   parameters each takes, its category, the presets) and
//!   `UI\TriggerStrings.txt` (their phrases), for triggers' text and
//!   view.
//!
//! What the formats say a value means (the sound channels' numbers, a
//! flag's bit) is built into the crates, with English names; the hints
//! show the game's words for them where it has some, and the built-in
//! ones without the game's files.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::game::{Game, Lookup};

/// The files names come from, for [`Cache::disk_changed`].
pub const FILES: [&str; 9] = [
    "TerrainArt\\Weather.slk",
    "TerrainArt\\Terrain.slk",
    "TerrainArt\\CliffTypes.slk",
    "UI\\SoundInfo\\EAXDefs.slk",
    "UI\\WorldEditData.txt",
    "UI\\WorldEditStrings.txt",
    "UI\\WorldEditGameStrings.txt",
    "UI\\TriggerData.txt",
    "UI\\TriggerStrings.txt",
];

/// The names a project's layers give.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Names {
    /// By weather effect id (`RAhr`).
    pub weather: HashMap<String, String>,
    /// By tileset letter (`L`).
    pub tilesets: HashMap<char, String>,
    /// By EAX label (`SpellsEAX`).
    pub eax: HashMap<String, String>,
    /// Ground textures by id (`Ldrt`).
    pub ground: HashMap<String, String>,
    /// Cliff textures by id (`CLdi`).
    pub cliffs: HashMap<String, String>,
    /// By sound channel number.
    pub channels: HashMap<i32, String>,
    /// The editor's strings, by key (`WESTRING_RACE_HUMAN`), as written.
    pub strings: HashMap<String, String>,
    /// How many parameters the trigger editor's functions take
    /// (`UI\TriggerData.txt`), to read `war3map.wtg` with.
    pub triggers: wtg_rs::TriggerData,
    /// What the trigger editor shows for a trigger: its categories,
    /// presets and phrases (`UI\TriggerData.txt`, `UI\TriggerStrings.txt`,
    /// with the editor's strings), for the view of a `.trg`.
    pub trigger_ui: wtg_rs::ui::Ui,
}

impl Names {
    /// Reads them through `game`'s layers. What a file doesn't give stays
    /// out.
    pub fn load(game: &Game) -> Names {
        let read = |name: &str| game.read(name).map(|(bytes, _)| crate::document::decode(bytes));
        let mut names = Names::default();
        for file in ["UI\\WorldEditStrings.txt", "UI\\WorldEditGameStrings.txt"] {
            let Some(text) = read(file) else { continue };
            for section in bni_rs::parse(&text).sections {
                for item in section.items {
                    if let Some(key) = &item.key {
                        names.strings.insert(key.text.trim().to_string(), item.value_text().trim().to_string());
                    }
                }
            }
        }

        // A table's two columns, by their names in its first row: id to
        // name, the name resolved.
        let table = |file: &str, id: &str, name: &str| -> HashMap<String, String> {
            let mut out = HashMap::new();
            let Some(text) = read(file) else { return out };
            let table = slk_rs::Table::new(&slk_rs::parse(&text));
            let column = |n: &str| (1..=table.cols()).find(|&c| table.text(1, c).trim() == n);
            if let (Some(id), Some(name)) = (column(id), column(name)) {
                for row in 2..=table.rows() {
                    let code = table.text(row, id).trim().to_string();
                    let label = table.text(row, name);
                    if !code.is_empty() && !label.trim().is_empty() {
                        out.insert(code, names.resolve(label.trim()));
                    }
                }
            }
            out
        };
        let weather = table("TerrainArt\\Weather.slk", "effectID", "name");
        let eax = table("UI\\SoundInfo\\EAXDefs.slk", "EAXLabel", "DisplayText");
        let ground = table("TerrainArt\\Terrain.slk", "tileID", "name");
        let cliffs = table("TerrainArt\\CliffTypes.slk", "cliffID", "name");

        let mut tilesets = HashMap::new();
        let mut channels = HashMap::new();
        if let Some(text) = read("UI\\WorldEditData.txt") {
            let doc = bni_rs::parse(&text);
            let items = |name: &str| doc.section(name).map(|s| s.items.as_slice()).unwrap_or_default();
            for item in items("TileSets") {
                let (Some(key), Some(first)) = (&item.key, item.fields.first()) else { continue };
                let mut letters = key.text.trim().chars();
                if let (Some(letter), None) = (letters.next(), letters.next()) {
                    tilesets.insert(letter, names.resolve(first.text().trim()));
                }
            }
            // `00=1,WESTRING_SOUNDCHANNEL_GENERAL`: can it be 3D, then its
            // name.
            for item in items("SoundChannels") {
                let (Some(key), Some(name)) = (&item.key, item.fields.get(1)) else { continue };
                if let Ok(n) = key.text.trim().parse::<i32>() {
                    channels.insert(n, names.resolve(name.text().trim()));
                }
            }
        }
        let data = read("UI\\TriggerData.txt");
        let triggers = data.as_deref().map(wtg_rs::TriggerData::parse).unwrap_or_default();
        let trigger_ui = match (&data, read("UI\\TriggerStrings.txt")) {
            (Some(data), strings) => wtg_rs::ui::Ui::parse(data, strings.as_deref().unwrap_or(""), &|k| names.resolve(k)),
            (None, _) => wtg_rs::ui::Ui::default(),
        };
        Names { weather, tilesets, eax, ground, cliffs, channels, triggers, trigger_ui, ..names }
    }

    /// `text` with a key in it replaced by the string it names, and so on
    /// while that is a key too (a few steps at most), the `&` that marks a
    /// menu's access key dropped (`&Looping`: Looping). Anything else as
    /// it is.
    pub fn resolve(&self, text: &str) -> String {
        let mut out = text;
        for _ in 0..8 {
            match self.strings.get(out) {
                Some(next) => out = next,
                None => break,
            }
        }
        out.replace("&&", "\u{0}").replace('&', "").replace('\u{0}', "&")
    }

    /// The editor's string `key`, resolved ([`resolve`](Self::resolve)).
    pub fn string(&self, key: &str) -> Option<String> {
        self.strings.contains_key(key).then(|| self.resolve(key))
    }

    /// The editor's string `key`, or `english` without one.
    pub fn say(&self, key: &str, english: &str) -> String {
        self.string(key).unwrap_or_else(|| english.to_string())
    }

    /// The name of weather effect `id`.
    pub fn weather(&self, id: &str) -> Option<&str> {
        self.weather.get(id).map(String::as_str)
    }

    /// The name of EAX setting `label`.
    pub fn eax(&self, label: &str) -> Option<&str> {
        self.eax.get(label).map(String::as_str)
    }

    /// The name of sound channel `n`.
    pub fn channel(&self, n: i32) -> Option<&str> {
        self.channels.get(&n).map(String::as_str)
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
