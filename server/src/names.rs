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
//!   view, and its icons (each category's, and `UI\WorldEditData.txt`'s
//!   `[WorldEditArt]` for the rest), decoded once.
//! - objects (units, abilities, items ...): their names, World Editor's
//!   (`w3abdhqtu_rs::ObjectNames`: each kind's table and the profile),
//!   the map's object data over them, for the trigger editor to show an
//!   id by its name and offer a type's objects ([`OBJECT_TYPES`]).
//!
//! The editor's data and strings (`UI\TriggerData.txt` and the rest of
//! [`PATCHED`]) are read with their patches (`UI\TriggerData.patch.txt`
//! in any layer changes the file by meaning: `game::Game::read_patched`),
//! so a map made with a modified editor needs only its additions.
//!
//! What the formats say a value means (the sound channels' numbers, a
//! flag's bit) is built into the crates, with English names; the hints
//! show the game's words for them where it has some, and the built-in
//! ones without the game's files.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::game::{Game, Lookup};

/// The files names come from, and the patches to those read with them
/// ([`PATCHED`]), for [`Cache::disk_changed`].
pub const FILES: [&str; 14] = [
    "TerrainArt\\Weather.slk",
    "TerrainArt\\Terrain.slk",
    "TerrainArt\\CliffTypes.slk",
    "UI\\SoundInfo\\EAXDefs.slk",
    "UI\\WorldEditData.txt",
    "UI\\WorldEditStrings.txt",
    "UI\\WorldEditGameStrings.txt",
    "UI\\TriggerData.txt",
    "UI\\TriggerStrings.txt",
    "UI\\WorldEditData.patch.txt",
    "UI\\WorldEditStrings.patch.txt",
    "UI\\WorldEditGameStrings.patch.txt",
    "UI\\TriggerData.patch.txt",
    "UI\\TriggerStrings.patch.txt",
];

/// The files read with their patches (`game::Game::read_patched`:
/// `UI\TriggerData.patch.txt` changes `UI\TriggerData.txt` by meaning):
/// the editor's data and strings, the BNI files a modified editor
/// extends (new functions, their phrases, categories and their names).
pub const PATCHED: [&str; 5] = [
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
    /// The trigger editor's icons, as `data:` URLs (PNG): each category's
    /// by its path as `UI\TriggerData.txt` says it
    /// (`ReplaceableTextures\WorldEditUI\Actions-Unit`), and the
    /// editor's own by their keys in `UI\WorldEditData.txt`'s
    /// `[WorldEditArt]` ([`TRIGGER_ART`]).
    pub trigger_icons: HashMap<String, String>,
    /// Each kind's objects with their names: the game's, the map's object
    /// data over them (`w3abdhqtu_rs::ObjectNames`). Read only with the
    /// trigger editor's data, which shows them.
    pub objects: w3abdhqtu_rs::ObjectNames,
    /// What the types are as a build tells them (an item's, a building's,
    /// a unit's: `wc3_build::objects::Objects`), the game's and the map's
    /// object data's; read with the objects' names.
    pub types: Option<wc3_build::objects::Objects>,
}

/// The trigger editor's types whose values are objects, and the kinds of
/// objects they are, as World Editor offers them: a hero's skill is an
/// ability heroes learn; a tech one is a unit or an upgrade.
pub const OBJECT_TYPES: [(&str, &[w3abdhqtu_rs::Kind]); 9] = {
    use w3abdhqtu_rs::Kind::*;
    [
        ("unitcode", &[Unit]),
        ("itemcode", &[Item]),
        ("abilcode", &[Ability]),
        ("heroskillcode", &[Ability]),
        ("buffcode", &[Buff]),
        ("destructablecode", &[Destructable]),
        ("doodadcode", &[Doodad]),
        ("upgradecode", &[Upgrade]),
        ("techcode", &[Unit, Upgrade]),
    ]
};

/// The objects of the trigger editor's type `ty` (see [`OBJECT_TYPES`]),
/// by id, with their names.
fn objects_of(objects: &w3abdhqtu_rs::ObjectNames, ty: &str, kinds: &[w3abdhqtu_rs::Kind]) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for kind in kinds {
        for (id, o) in objects.of.get(kind).into_iter().flatten() {
            if ty != "heroskillcode" || o.hero {
                out.insert(id.clone(), plain(&o.shown()));
            }
        }
    }
    out
}

/// The game's text without its markup: colors (`|cffRRGGBB`, `|r`) out,
/// a line break (`|n`) a space.
pub fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('|') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let code = after.chars().next().map(|c| c.to_ascii_lowercase());
        rest = match code {
            Some('c') if after.len() >= 9 && after.as_bytes()[1..9].iter().all(u8::is_ascii_hexdigit) => &after[9..],
            Some('r') => &after[1..],
            Some('n') => {
                out.push(' ');
                &after[1..]
            }
            _ => {
                out.push('|');
                after
            }
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Every file the objects' names come from: the game's tables and
/// profile, a map's object data and strings.
pub fn object_files() -> Vec<String> {
    let mut out: Vec<String> = w3abdhqtu_rs::PROFILE_FILES.iter().map(|f| f.to_string()).collect();
    for kind in w3abdhqtu_rs::Kind::ALL {
        out.push(kind.table_file().into());
        out.push(kind.metadata_file().into());
        out.push(format!("war3map.{}", kind.extension()));
    }
    out.push("war3map.wts".into());
    out
}

/// The trigger editor's own icons, keys of `UI\WorldEditData.txt`'s
/// `[WorldEditArt]`: the headings and blocks (events, conditions,
/// actions), a function (one the editor's files don't have), a trigger,
/// a category and the variables.
pub const TRIGGER_ART: [&str; 10] = [
    "SEIcon_Event",
    "SEIcon_Condition",
    "SEIcon_Action",
    "SEIcon_Function",
    "SEIcon_FunctionDisabled",
    "SEIcon_Trigger",
    "SEIcon_TriggerDisabled",
    "SEIcon_TriggerCategory",
    "SEIcon_TriggerCategoryOpen",
    "ToolBarIcon_SE_Variables",
];

impl Names {
    /// Reads them through `game`'s layers. What a file doesn't give stays
    /// out.
    pub fn load(game: &Game) -> Names {
        let read = |name: &str| match PATCHED.contains(&name) {
            true => game.read_patched(name),
            false => game.read(name).map(|(bytes, _)| crate::document::decode(bytes)),
        };
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
        let mut art: Vec<(String, String)> = Vec::new();
        if let Some(text) = read("UI\\WorldEditData.txt") {
            let doc = bni_rs::parse(&text);
            for item in doc.section("WorldEditArt").map(|s| s.items.as_slice()).unwrap_or_default() {
                if let Some(key) = item.key.as_ref().map(|k| k.text.trim()).filter(|k| TRIGGER_ART.contains(k)) {
                    art.push((key.to_string(), item.value_text().trim().to_string()));
                }
            }
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
        // The icons, once: a few dozen small textures, only for a project
        // with the trigger editor's data.
        let mut trigger_icons = HashMap::new();
        if !trigger_ui.is_empty() {
            let icon = |path: &str| -> Option<String> {
                let has_ext = std::path::Path::new(&path.replace('\\', "/")).extension().is_some();
                let file = if has_ext { path.to_string() } else { format!("{path}.blp") };
                crate::texture::png_data_url(&game.read(&file)?.0)
            };
            for c in trigger_ui.categories.values() {
                if !c.icon.is_empty() && !trigger_icons.contains_key(&c.icon)
                    && let Some(url) = icon(&c.icon)
                {
                    trigger_icons.insert(c.icon.clone(), url);
                }
            }
            for (key, path) in &art {
                if let Some(url) = icon(path) {
                    trigger_icons.insert(key.clone(), url);
                }
            }
        }
        Names { weather, tilesets, eax, ground, cliffs, channels, triggers, trigger_ui, trigger_icons, ..names }
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
    let (game, _) = lookup.open();
    let mut names = Names::load(&game);
    // The objects, with the trigger editor's data only (it shows them).
    if !names.trigger_ui.is_empty() {
        add_objects(&mut names, lookup, &game);
    }
    names
}

/// [`names`], with the objects' names whether or not the trigger editor's
/// data is there: for what shows what's placed (the World Editor, MCP's
/// `doo_list`).
pub fn with_objects(lookup: &Lookup) -> Names {
    if lookup.is_empty() {
        return Names::default();
    }
    let (game, _) = lookup.open();
    let mut names = Names::load(&game);
    add_objects(&mut names, lookup, &game);
    names
}

/// The game's part of the objects' names: its objects named, and each
/// kind's metadata (to read a map's object data with).
struct GameObjects {
    names: w3abdhqtu_rs::ObjectNames,
    types: wc3_build::objects::Objects,
    meta: Vec<(w3abdhqtu_rs::Kind, w3abdhqtu_rs::Metadata)>,
}

/// The game's part, by game folders: read once (a map's every save drops
/// the names, and reading the game's tables from its archives takes
/// seconds), dropped when they change ([`Cache::disk_changed`]).
static GAME_OBJECTS: std::sync::LazyLock<std::sync::Mutex<HashMap<Vec<PathBuf>, Arc<GameObjects>>>> = std::sync::LazyLock::new(Default::default);

/// The files of the game's part: its tables, profile, metadata and the
/// editor's strings.
fn game_object_files() -> Vec<String> {
    object_files().into_iter().filter(|f| !f.starts_with("war3map.")).chain(PATCHED.iter().chain(FILES.iter()).filter(|f| f.contains("Strings")).map(|f| f.to_string())).collect()
}

/// The game's part read through `game`, its strings resolved by `names`.
fn read_game_objects(game: &Game, names: &Names) -> GameObjects {
    let read = |name: &str| match PATCHED.contains(&name) {
        true => game.read_patched(name),
        false => game.read(name).map(|(bytes, _)| crate::document::decode(bytes)),
    };
    GameObjects {
        names: w3abdhqtu_rs::ObjectNames::load(read, |k| names.resolve(k)),
        types: wc3_build::objects::Objects::of_game(&read),
        meta: w3abdhqtu_rs::Kind::ALL.into_iter().map(|k| (k, w3abdhqtu_rs::Metadata::load(k, read))).collect(),
    }
}

/// `names`' objects: the game's (read once per game folders, unless the
/// map's roots replace one of their files: then through `game`, the whole
/// lookup), the map's object data over them.
fn add_objects(names: &mut Names, lookup: &Lookup, game: &Game) {
    let roots = Lookup { game: Vec::new(), roots: lookup.roots.clone(), ..Default::default() }.open().0;
    let replaced = !lookup.roots.is_empty() && game_object_files().iter().any(|f| roots.read(f).is_some());
    let base = if replaced {
        Arc::new(read_game_objects(game, names))
    } else {
        let cached = GAME_OBJECTS.lock().ok().and_then(|c| c.get(&lookup.game).cloned());
        cached.unwrap_or_else(|| {
            let read = Arc::new(read_game_objects(&Game::open(&lookup.game).0, names));
            if let Ok(mut c) = GAME_OBJECTS.lock() {
                c.insert(lookup.game.clone(), read.clone());
            }
            read
        })
    };
    let mut objects = base.names.clone();
    let mut types = base.types.clone();
    let wts = game.read("war3map.wts").map(|(b, _)| wts_rs::parse(&crate::document::decode(b)));
    let text = |s: &str| wts.as_ref().map_or(s.to_string(), |w| w.resolve(s));
    for (kind, meta) in &base.meta {
        let Some(data) = game.read(&format!("war3map.{}", kind.extension())).and_then(|(b, _)| w3abdhqtu_rs::parse(&b, *kind).ok()) else { continue };
        objects.add_object_data(&data, meta, text);
        types.add_object_data(&data);
    }
    for (ty, kinds) in OBJECT_TYPES {
        names.trigger_ui.objects.insert(ty.to_string(), objects_of(&objects, ty, kinds));
    }
    names.objects = objects;
    names.types = Some(types);
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
                || FILES.iter().map(|f| f.to_string()).chain(object_files()).any(|f| {
                    let rel = path.strip_prefix(folder).map(|r| r.to_string_lossy().replace('/', "\\"));
                    rel.is_ok_and(|r| r.eq_ignore_ascii_case(&f))
                })
        };
        self.read.retain(|lookup, _| !paths.iter().any(|p| lookup.affected_by(p, in_game)));
        // The game's part of the objects: only the game folders' changes.
        if let Ok(mut c) = GAME_OBJECTS.lock() {
            c.retain(|folders, _| !paths.iter().any(|p| folders.iter().any(|f| in_game(f, p))));
        }
        self.read.len() != before
    }
}

#[cfg(test)]
#[path = "names.test.rs"]
mod tests;
