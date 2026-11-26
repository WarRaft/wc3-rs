//! The object editor's data: a map's object data (`war3map.w3u` units,
//! `.w3t` items, `.w3b` destructables, `.w3d` doodads, `.w3a` abilities,
//! `.w3h` buffs, `.w3q` upgrades) over the game's standard objects. The
//! game's values come from its tables (`Units\UnitData.slk` ...) and its
//! profile files (`Units\HumanUnitFunc.txt`, `...Strings.txt`), read as
//! the game reads them (one table of sections by id, a later file's over
//! an earlier one's), a field found by its metadata's key (`HP`,
//! `DataA` with its level: `DataA1`; a list's item by its `index` or
//! its level). What the map changes is its object data's; a field set
//! adds a change (a standard object the map didn't change gets one), a
//! field set empty takes it out: the game's again.

use std::collections::HashMap;

use parser_rs::Rawcode;
use w3abdhqtu_rs::{FieldMeta, Kind, Metadata, Mod, Object, ObjectData, Set, Storage, Value};
use wc3_server::game::Game;

/// The kinds, in World Editor's order of its Object Editor's tabs.
pub const KINDS: [Kind; 7] = [Kind::Unit, Kind::Item, Kind::Destructable, Kind::Doodad, Kind::Ability, Kind::Buff, Kind::Upgrade];

/// The map's file of `kind`'s object data: `war3map.w3u` ...
pub fn file_name(kind: Kind) -> String {
    format!("war3map.{}", kind.extension())
}

/// `kind`'s bit in `edit::Dirty::objects`.
pub fn bit(kind: Kind) -> u8 {
    1 << KINDS.iter().position(|k| *k == kind).unwrap_or(0)
}

/// The kind of a map's object data file `name`.
pub fn kind_of(name: &str) -> Option<Kind> {
    KINDS.iter().copied().find(|k| file_name(*k) == name)
}

/// Each kind's tables, a row per object, lower ones first.
fn tables(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Unit => &["Units\\UnitData.slk", "Units\\UnitBalance.slk", "Units\\UnitUI.slk", "Units\\UnitAbilities.slk", "Units\\UnitWeapons.slk"],
        Kind::Item => &["Units\\ItemData.slk"],
        Kind::Destructable => &["Units\\DestructableData.slk"],
        Kind::Doodad => &["Doodads\\Doodads.slk"],
        Kind::Ability => &["Units\\AbilityData.slk"],
        Kind::Buff => &["Units\\AbilityBuffData.slk"],
        Kind::Upgrade => &["Units\\UpgradeData.slk"],
    }
}

/// The profile files, as the game reads them all.
fn profile_files() -> Vec<String> {
    let mut out = Vec::new();
    for race in ["Campaign", "Common", "Human", "Item", "Neutral", "NightElf", "Orc", "Undead"] {
        for what in ["UnitFunc", "UnitStrings", "AbilityFunc", "AbilityStrings", "UpgradeFunc", "UpgradeStrings"] {
            out.push(format!("Units\\{race}{what}.txt"));
        }
    }
    out.extend(["Units\\ItemFunc.txt", "Units\\ItemStrings.txt", "Units\\CommandFunc.txt", "Units\\CommandStrings.txt"].map(String::from));
    out
}

/// A kind's standard objects as the game has them: their ids in its
/// table's order, each one's fields by key in lower case, its tables'
/// and its profile's apart (`UnitUI.slk`'s `name` is `footman`, the
/// profile's `Name` `Footman`).
#[derive(Debug, Clone, Default)]
pub struct Standard {
    pub ids: Vec<String>,
    rows: HashMap<String, HashMap<String, String>>,
    profile: HashMap<String, HashMap<String, String>>,
}

impl Standard {
    /// `kind`'s standard objects through `game`, its profile `profile`'s.
    pub fn load(game: &Game, kind: Kind, profile: &HashMap<String, HashMap<String, String>>) -> Standard {
        let mut out = Standard::default();
        for name in tables(kind) {
            let Some(slk) = crate::tileset::Slk::read(game, name) else { continue };
            let mut rows: Vec<_> = slk.rows().collect();
            rows.sort_by(|a, b| a.0.cmp(b.0));
            for (id, row) in rows {
                if !out.rows.contains_key(id) {
                    out.ids.push(id.clone());
                }
                let fields = out.rows.entry(id.clone()).or_default();
                for (k, v) in row {
                    if !v.is_empty() && v != "_" && v != "-" {
                        fields.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        for id in &out.ids {
            if let Some(section) = profile.get(id) {
                out.profile.insert(id.clone(), section.clone());
            }
        }
        out
    }

    /// The profile files through `game`: their sections by id, each
    /// field's key in lower case, a list's items joined by commas.
    pub fn profile(game: &Game) -> HashMap<String, HashMap<String, String>> {
        let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
        for name in profile_files() {
            let Some((bytes, _)) = game.read(&name) else { continue };
            let text = String::from_utf8_lossy(&bytes);
            for section in bni_rs::parse(&text).sections {
                let fields = out.entry(section.name.text.trim().to_string()).or_default();
                for item in section.items {
                    if let Some(k) = &item.key
                        && !item.fields.is_empty()
                    {
                        let v: Vec<String> = item.fields.iter().map(|f| f.text().trim().trim_matches('"').to_string()).collect();
                        fields.insert(k.text.trim().to_ascii_lowercase(), v.join(","));
                    }
                }
            }
        }
        out
    }

    /// Whether `id` is a standard object.
    pub fn has(&self, id: &str) -> bool {
        self.rows.contains_key(id)
    }

    /// Field `key` (any case) of `id`, as the game has it: its profile's
    /// (the text files'), else its tables'.
    pub fn get(&self, id: &str, key: &str) -> Option<&str> {
        self.from(id, key, true)
    }

    /// Field `key` of `id`: its profile's first, or its tables'.
    fn from(&self, id: &str, key: &str, profile_first: bool) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        let profile = || self.profile.get(id)?.get(&key).map(String::as_str);
        let table = || self.rows.get(id)?.get(&key).map(String::as_str);
        if profile_first { profile().or_else(table) } else { table().or_else(profile) }
    }

    /// Field `f` of `id` at `level` (from 1; 0 for a field without
    /// levels), as the game has it: its key with the level (`DataA1`),
    /// else the key's list's item by its level or `index`.
    pub fn value(&self, id: &str, f: &FieldMeta, level: u32) -> Option<String> {
        if f.key.is_empty() {
            return None;
        }
        let item = |v: &str, n: usize| v.split(',').nth(n).map(|s| s.trim().to_string());
        // Where the metadata says it's read from first.
        let profile = f.table.eq_ignore_ascii_case("Profile");
        let get = |key: &str| self.from(id, key, profile);
        if f.levels && level >= 1 {
            if let Some(v) = get(&format!("{}{level}", f.key)) {
                return Some(v.to_string());
            }
            let v = get(&f.key)?;
            return item(v, level as usize - 1).or_else(|| v.split(',').next_back().map(|s| s.trim().to_string()));
        }
        let v = get(&f.key)?;
        match f.index {
            Some(i) => item(v, i as usize),
            None => Some(v.to_string()),
        }
    }
}

/// An object's id as the list names it: its own, or a standard one's.
pub fn id_of(o: &Object) -> Rawcode {
    if o.id == Rawcode::NONE { o.base } else { o.id }
}

/// Where object `id` is in `data`: (custom?, index).
pub fn find(data: &ObjectData, id: Rawcode) -> Option<(bool, usize)> {
    if let Some(i) = data.customs.iter().position(|o| o.id == id) {
        return Some((true, i));
    }
    data.originals.iter().position(|o| o.base == id).map(|i| (false, i))
}

/// A value as the panel shows it.
pub fn shown(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::Real(r) | Value::Unreal(r) => {
            let s = format!("{r}");
            s.strip_suffix(".0").map(String::from).unwrap_or(s)
        }
        Value::Str(s) => s.clone(),
    }
}

/// What `text` stores in field `f` (`None`: no metadata, by how it's
/// written), in World Editor's limits.
pub fn to_value(text: &str, f: Option<&FieldMeta>) -> Result<Value, String> {
    let storage = f.and_then(|f| f.storage);
    if storage == Some(Storage::Str) || (storage.is_none() && text.trim().parse::<f64>().is_err()) {
        return Ok(Value::Str(text.to_string()));
    }
    let n: f64 = text.trim().parse().map_err(|_| format!("a number, not {text:?}"))?;
    if !n.is_finite() {
        return Err(format!("a number, not {text:?}"));
    }
    if let Some(f) = f {
        let (min, max) = (f.min.unwrap_or(f64::MIN), f.max.unwrap_or(f64::MAX));
        // A bool's limits aren't in the table: 0 or 1.
        let (min, max) = if f.ty == "bool" { (0.0, 1.0) } else { (min, max) };
        if n < min || n > max {
            return Err(format!("{} to {}, not {n}", fmt(min), fmt(max)));
        }
    }
    Ok(match storage {
        Some(Storage::Real) => Value::Real(n as f32),
        Some(Storage::Unreal) => Value::Unreal(n as f32),
        _ if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => Value::Int(n as i32),
        Some(Storage::Int) => return Err(format!("a whole number, not {n}")),
        _ => Value::Real(n as f32),
    })
}

fn fmt(n: f64) -> String {
    if n == f64::MIN || n == f64::MAX { "any".into() } else { format!("{n}") }
}

/// Field `field` (at `level`, in data `column`) of object `id` set to
/// `value`, or (`None`) taken out: the game's again. A standard object
/// the map didn't change gets an entry; one whose last change is taken
/// out loses it.
pub fn set(data: &mut ObjectData, id: Rawcode, field: Rawcode, level: u32, column: u32, value: Option<Value>) {
    let at = |m: &Mod| m.field == field && m.level == level && m.column == column;
    let i = match find(data, id) {
        Some((true, i)) => {
            set_in(&mut data.customs[i], &at, field, level, column, value);
            return;
        }
        Some((false, i)) => i,
        None => {
            if value.is_none() {
                return;
            }
            data.originals.push(Object { base: id, id: Rawcode::NONE, sets: Vec::new() });
            data.originals.len() - 1
        }
    };
    set_in(&mut data.originals[i], &at, field, level, column, value);
    if data.originals[i].mods().next().is_none() {
        data.originals.remove(i);
    }
}

fn set_in(o: &mut Object, at: &impl Fn(&Mod) -> bool, field: Rawcode, level: u32, column: u32, value: Option<Value>) {
    match value {
        Some(value) => {
            if let Some(m) = o.sets.iter_mut().flat_map(|s| s.mods.iter_mut()).find(|m| at(m)) {
                m.value = value;
                return;
            }
            if o.sets.is_empty() {
                o.sets.push(Set { flags: 0, mods: Vec::new() });
            }
            o.sets[0].mods.push(Mod { field, level, column, value, end: Rawcode::NONE });
        }
        None => {
            for s in &mut o.sets {
                s.mods.retain(|m| !at(m));
            }
        }
    }
}

/// A new custom object's id, as World Editor makes them: a letter (a
/// unit's base's first, so a hero's a capital and a race's its own; `I`
/// items, `B` destructables and buffs, `D` doodads, `A` abilities, `R`
/// upgrades) and three of `0-9A-Z` counting up, the first no object of
/// the map's or the game's has.
pub fn new_id(kind: Kind, base: Rawcode, taken: impl Fn(Rawcode) -> bool) -> Option<Rawcode> {
    let first = match kind {
        Kind::Unit => base.0[0],
        Kind::Item => b'I',
        Kind::Destructable | Kind::Buff => b'B',
        Kind::Doodad => b'D',
        Kind::Ability => b'A',
        Kind::Upgrade => b'R',
    };
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    (0..36 * 36 * 36).map(|n| Rawcode([first, DIGITS[n / 1296], DIGITS[n / 36 % 36], DIGITS[n % 36]])).find(|id| !taken(*id))
}

/// The name field of a kind's objects, by key.
pub fn name_key(kind: Kind) -> &'static str {
    if kind == Kind::Buff { "EditorName" } else { "Name" }
}

/// What groups a kind's objects in the list: a unit's race, an item's
/// class, a doodad's or destructable's category, an ability's, buff's
/// or upgrade's race.
pub fn group_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Item => "class",
        Kind::Doodad | Kind::Destructable => "category",
        _ => "race",
    }
}

/// The field that says how many levels (a doodad's variations) an object
/// has, by key in lower case.
pub fn levels_key(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Ability => Some("levels"),
        Kind::Upgrade => Some("maxlevel"),
        Kind::Doodad => Some("numvar"),
        _ => None,
    }
}

/// The metadata of every kind through `game`.
pub fn metadata(game: &Game) -> HashMap<Kind, Metadata> {
    let read = |name: &str| match wc3_server::names::PATCHED.contains(&name) {
        true => game.read_patched(name),
        false => game.read(name).map(|(bytes, _)| String::from_utf8_lossy(&bytes).into_owned()),
    };
    KINDS.iter().map(|k| (*k, Metadata::load(*k, read))).collect()
}

/// The game's tables the object editor reads, each once, when first
/// asked: every kind's metadata, the profile, each kind's standard
/// objects.
#[derive(Debug, Default)]
pub struct Tables {
    pub meta: HashMap<Kind, Metadata>,
    profile: Option<HashMap<String, HashMap<String, String>>>,
    pub standard: HashMap<Kind, Standard>,
}

impl Tables {
    /// `kind`'s tables read, if they aren't.
    pub fn load(&mut self, game: &Game, kind: Kind) {
        if self.meta.is_empty() {
            self.meta = metadata(game);
        }
        if !self.standard.contains_key(&kind) {
            let profile = self.profile.get_or_insert_with(|| Standard::profile(game));
            self.standard.insert(kind, Standard::load(game, kind, profile));
        }
    }
}

#[cfg(test)]
#[path = "objdata.test.rs"]
mod tests;
