//! The map an editor has open: its project's (`wc3.bni`), each file read
//! through the project's lookup (`game::Lookup`: the game's folders, then
//! the map's roots, the last that has it wins), as the build reads it.

use std::path::{Path, PathBuf};

use wc3_server::game::{Game, Lookup};
use wc3_server::names::Names;
use wc3_server::project::{self, Project};

use crate::protocol::{CliffsMsg, GroundMsg, MapInfo, ServerMsg, WaterMsg};
use crate::terrain::{Draw, Grounds};
use crate::tileset::Tileset;

/// A project's map, open.
pub struct Session {
    pub project: Project,
    pub game: Game,
    pub terrain: w3e_rs::Terrain,
    pub info: Option<w3i_rs::Info>,
    /// The map's strings (`war3map.wts`), for its `TRIGSTR_`s.
    pub strings: Option<wts_rs::Document>,
    pub names: Names,
    pub tileset: Tileset,
    /// What was read with a problem, gone past.
    pub notes: Vec<String>,
}

impl Session {
    /// Opens the map of the project at `root`. The error says why it
    /// can't be: no `wc3.bni`, no `[map] root=`, no terrain.
    pub fn open(root: &Path) -> Result<Session, String> {
        let manifest = root.join("wc3.bni");
        let src = std::fs::read_to_string(&manifest).map_err(|e| format!("Can't read {}: {e}", manifest.display()))?;
        let (project, issues) = project::load(root, &src);
        let mut notes: Vec<String> = issues
            .iter()
            .filter(|i| i.level == parser_rs::Level::Error)
            .map(|i| format!("wc3.bni:{}: {}", i.line + 1, i.message))
            .collect();
        if project.roots.is_empty() {
            return Err("The project has no map: its wc3.bni names no [map] root=.".into());
        }
        if let Some(missing) = project.roots.iter().find(|r| !r.exists()) {
            notes.push(format!("The root {} doesn't exist.", missing.display()));
        }
        let (game, errors) = Lookup::of(&project).open();
        notes.extend(errors);

        let read = |name: &str| game.read(name).map(|(bytes, _)| bytes);
        let terrain = read("war3map.w3e").ok_or("The map has no terrain: no war3map.w3e in its roots.")?;
        let terrain = w3e_rs::parse(&terrain).map_err(|e| format!("Can't read war3map.w3e: {e}"))?;
        let info = match read("war3map.w3i").map(|b| w3i_rs::parse(&b)) {
            Some(Ok(info)) => Some(info),
            Some(Err(e)) => {
                notes.push(format!("Can't read war3map.w3i: {e}"));
                None
            }
            None => {
                notes.push("The map has no war3map.w3i.".into());
                None
            }
        };
        let strings = read("war3map.wts").map(|b| wts_rs::parse(&String::from_utf8_lossy(&b)));
        let names = Names::load(&game);
        let tileset = Tileset::load(&game, terrain.tileset as char);
        notes.extend(tileset.notes.iter().cloned());
        Ok(Session { project, game, terrain, info, strings, names, tileset, notes })
    }

    /// `text` with its `TRIGSTR_`s replaced by the map's strings.
    pub fn text(&self, text: &str) -> String {
        match &self.strings {
            Some(strings) => strings.resolve(text),
            None => text.to_string(),
        }
    }

    pub fn root(&self) -> &PathBuf {
        &self.project.root
    }

    /// The terrain as the page draws it: its ground textures, its water,
    /// then the terrain, whose layers index those textures.
    pub fn draw(&self) -> Vec<ServerMsg> {
        let grounds = Grounds::of(&self.terrain, &self.tileset);
        let paths: Vec<(String, Option<String>)> = grounds
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let path = if i == grounds.blight { self.tileset.blight.clone() } else { self.tileset.ground_path(id) };
                (id.clone(), path)
            })
            .collect();
        let (textures, layers) = crate::textures::ground(&self.game, &paths);
        let water = match &self.tileset.water {
            Some(w) => {
                let (frames, error) = crate::textures::water(&self.game, &w.texture, w.count);
                WaterMsg { rate: w.rate, colors: w.colors, frames, error }
            }
            None => WaterMsg { error: "No water for this tileset in TerrainArt\\Water.slk.".into(), ..Default::default() },
        };
        let cliffs = crate::cliffs::Cliffs::of(&self.game, &self.terrain, &self.tileset);
        let cliff_paths: Vec<String> = self
            .terrain
            .cliffs
            .iter()
            .map(|c| {
                let t = self.tileset.cliffs.as_ref();
                let id = c.to_string();
                match (t.and_then(|t| t.get(&id, "texDir")), t.and_then(|t| t.get(&id, "texFile"))) {
                    (Some(dir), Some(file)) => format!("{dir}\\{file}.blp"),
                    _ => format!("{id}: not in TerrainArt\\CliffTypes.slk"),
                }
            })
            .collect();
        let (cliff_layers, cliff_errors) = crate::textures::whole(&self.game, &cliff_paths);
        let cliffs_msg = CliffsMsg {
            textures: cliff_layers,
            texture_paths: cliff_paths.into_iter().zip(cliff_errors).collect(),
            meshes: crate::cliffs::meshes(&cliffs),
            notes: cliffs.notes.clone(),
        };
        let extended = |t: usize| textures.get(t).is_some_and(|g| g.extended);
        let height = self.tileset.water.as_ref().map_or(0.0, |w| w.height);
        let draw = Draw::new(&self.terrain, &grounds, cliffs.tile_flags, &extended, height);
        vec![
            ServerMsg::Ground(GroundMsg { textures, layers }),
            ServerMsg::Water(water),
            ServerMsg::Cliffs(cliffs_msg),
            ServerMsg::Terrain(draw),
        ]
    }

    /// What the page is told of the map.
    pub fn map_msg(&self) -> ServerMsg {
        let letter = self.terrain.tileset as char;
        let (name, author) = self.info.as_ref().map_or_else(Default::default, |i| (self.text(&i.name), self.text(&i.author)));
        ServerMsg::Map(MapInfo {
            project: self.root().display().to_string(),
            name,
            author,
            width: self.terrain.width,
            height: self.terrain.height,
            tileset: letter.to_string(),
            tileset_name: self.names.tileset(letter).unwrap_or_default().to_string(),
            notes: self.notes.clone(),
        })
    }
}

#[cfg(test)]
#[path = "session.test.rs"]
mod tests;
