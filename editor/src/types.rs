//! The fields of the types of what's placed (a doodad's model, a unit's
//! scale ...), as the game finds them: its tables and profile files
//! through the project's lookup, then the map's object data over them
//! (`war3map.w3d` doodads, `.w3b` destructables, `.w3u` units, `.w3t`
//! items), each change put under its field's key by the kind's metadata
//! (`umdl` is a unit's `file`), a custom object starting from its
//! standard one. Keys in any case: `file`, `numVar`, `modelScale`.

use std::collections::HashMap;

use w3abdhqtu_rs::{Kind, Metadata, Value};
use wc3_server::game::Game;

use crate::tileset::Slk;

/// The kinds of what's placed.
pub const PLACED: [Kind; 4] = [Kind::Doodad, Kind::Destructable, Kind::Unit, Kind::Item];

/// Each kind's tables: SLKs (a row per id) and profile files (a section
/// per id), lower ones first.
fn files(kind: Kind) -> (&'static [&'static str], &'static [&'static str]) {
    match kind {
        Kind::Doodad => (&["Doodads\\Doodads.slk"], &[]),
        Kind::Destructable => (&["Units\\DestructableData.slk"], &[]),
        Kind::Unit => (&["Units\\UnitData.slk", "Units\\UnitBalance.slk", "Units\\UnitUI.slk"], &[]),
        Kind::Item => (&["Units\\ItemData.slk"], &["Units\\ItemFunc.txt"]),
        _ => (&[], &[]),
    }
}

/// Every type's fields, by kind.
#[derive(Debug, Default)]
pub struct Types {
    of: HashMap<Kind, HashMap<String, HashMap<String, String>>>,
}

impl Types {
    pub fn load(game: &Game) -> Types {
        let text = |name: &str| game.read(name).map(|(b, _)| String::from_utf8_lossy(&b).into_owned());
        let mut out = Types::default();
        for kind in PLACED {
            let objects = out.of.entry(kind).or_default();
            let (slks, profiles) = files(kind);
            for name in slks {
                let Some(slk) = Slk::read(game, name) else { continue };
                for (id, row) in slk.rows() {
                    let fields = objects.entry(id.clone()).or_default();
                    for (k, v) in row {
                        if !v.is_empty() {
                            fields.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            for name in profiles {
                let Some(t) = text(name) else { continue };
                for section in bni_rs::parse(&t).sections {
                    let id = section.name.text.trim().to_string();
                    let fields = objects.entry(id).or_default();
                    for item in section.items {
                        if let (Some(k), Some(v)) = (&item.key, item.fields.first()) {
                            fields.insert(k.text.trim().to_ascii_lowercase(), v.text().trim().trim_matches('"').to_string());
                        }
                    }
                }
            }
            // The map's object data over them.
            let Some((bytes, _)) = game.read(&format!("war3map.{}", kind.extension())) else { continue };
            let Ok(data) = w3abdhqtu_rs::parse(&bytes, kind) else { continue };
            let meta = Metadata::load(kind, text);
            for (o, custom) in data.originals.iter().map(|o| (o, false)).chain(data.customs.iter().map(|o| (o, true))) {
                let id = if custom { o.id } else { o.base }.to_string();
                let mut fields = objects.get(&o.base.to_string()).cloned().unwrap_or_default();
                if !custom {
                    fields = objects.get(&id).cloned().unwrap_or(fields);
                }
                for m in o.sets.iter().flat_map(|s| &s.mods) {
                    // A doodad's per variation fields: its first's.
                    if m.level > 1 {
                        continue;
                    }
                    let Some(key) = meta.get(m.field).map(|f| f.key.as_str()).filter(|k| !k.is_empty()) else { continue };
                    let value = match &m.value {
                        Value::Int(i) => i.to_string(),
                        Value::Real(f) | Value::Unreal(f) => f.to_string(),
                        Value::Str(s) => s.clone(),
                    };
                    fields.insert(key.to_ascii_lowercase(), value);
                }
                objects.insert(id, fields);
            }
        }
        out
    }

    /// Field `key` (any case) of type `id` of `kind`; `None` for no value,
    /// or `_` (the tables' empty).
    pub fn get(&self, kind: Kind, id: &str, key: &str) -> Option<&str> {
        self.of.get(&kind)?.get(id)?.get(&key.to_ascii_lowercase()).map(String::as_str).filter(|v| !v.is_empty() && *v != "_")
    }

    /// [`get`](Self::get) as a number.
    pub fn number(&self, kind: Kind, id: &str, key: &str) -> Option<f32> {
        self.get(kind, id, key)?.trim().parse().ok()
    }

    /// Is `id` a type of `kind`?
    pub fn has(&self, kind: Kind, id: &str) -> bool {
        self.of.get(&kind).is_some_and(|o| o.contains_key(id))
    }
}

#[cfg(test)]
#[path = "types.test.rs"]
mod tests;
