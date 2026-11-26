//! The map an editor has open: its project's (`wc3.bni`), each file read
//! through the project's lookup (`game::Lookup`: the game's folders, then
//! the map's roots, the last that has it wins), as the build reads it.

use std::path::{Path, PathBuf};

use wc3_server::game::{Game, Lookup};
use wc3_server::names::Names;
use wc3_server::project::{self, Project};

use crate::protocol::{CliffsMsg, GroundMsg, MapInfo, ObjectEntry, ObjectsMsg, PaletteEntry, PaletteMsg, PickedMsg, ServerMsg, StateMsg, WaterMsg};
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
    /// What's placed, once drawn ([`draw`](Self::draw)).
    pub objects: Option<crate::placed::Objects>,
    /// The terrain as drawn, and what's over it, once drawn.
    pub drawn: Option<Draw>,
    pub overlays: Option<crate::overlays::Overlays>,
    /// The changes made, undo and redo.
    pub edits: crate::edit::Edits,
    /// The terrain's ground textures for its list (the list, the
    /// textures' ids, their pictures).
    ground_cache: Option<(Vec<parser_rs::bin::Rawcode>, Grounds, Vec<crate::textures::GroundTexture>, crate::textures::Layers)>,
    /// Files read (the cliffs' models), by path in lower case.
    files: std::collections::HashMap<String, Option<Vec<u8>>>,
    /// What's placed, as its files have it (and the edits change it).
    pub placed: crate::placed::Files,
    /// The types of what's placed, once read.
    types: Option<crate::types::Types>,
    /// The models and textures the page has (the ones after are new);
    /// `None` before it has any.
    sent: Option<(usize, usize)>,
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
        // With the objects' names (the game's and the map's), for what
        // a pick says.
        let names = wc3_server::names::with_objects(&Lookup::of(&project));
        let tileset = Tileset::load(&game, terrain.tileset as char);
        notes.extend(tileset.notes.iter().cloned());
        Ok(Session {
            project,
            game,
            terrain,
            info,
            strings,
            names,
            tileset,
            objects: None,
            drawn: None,
            overlays: None,
            edits: Default::default(),
            ground_cache: None,
            files: Default::default(),
            placed: Default::default(),
            types: None,
            sent: None,
            notes,
        })
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

    /// The ground textures of the terrain's list, read again only when
    /// the list changes; whether they were.
    fn grounds(&mut self) -> bool {
        if self.ground_cache.as_ref().is_some_and(|c| c.0 == self.terrain.ground) {
            return false;
        }
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
        self.ground_cache = Some((self.terrain.ground.clone(), grounds, textures, layers));
        true
    }

    /// The cliffs and ramps, their models read once.
    fn cliffs(&mut self) -> crate::cliffs::Cliffs {
        let (game, files) = (&self.game, &mut self.files);
        let mut read = |path: &str| files.entry(path.to_ascii_lowercase()).or_insert_with(|| game.read(path).map(|(b, _)| b)).clone();
        crate::cliffs::Cliffs::with(&mut read, &self.terrain, &self.tileset)
    }

    /// The cliff types' textures, and each one's path and error.
    fn cliff_textures(&self) -> (crate::textures::Layers, Vec<(String, String)>) {
        let paths: Vec<String> = self
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
        let (layers, errors) = crate::textures::whole(&self.game, &paths);
        (layers, paths.into_iter().zip(errors).collect())
    }

    /// The terrain worked out again: its ground textures (`Ground`, when
    /// they changed or `all`), its cliffs (their textures with `all`),
    /// the terrain.
    fn terrain_msgs(&mut self, all: bool) -> Vec<ServerMsg> {
        let changed = self.grounds() || all;
        let cliffs = self.cliffs();
        let (textures, texture_paths) = if all { self.cliff_textures() } else { Default::default() };
        let cliffs_msg = CliffsMsg { textures, texture_paths, meshes: crate::cliffs::meshes(&cliffs), notes: cliffs.notes.clone() };
        let (_, grounds, ground_textures, _) = self.ground_cache.as_ref().expect("read just now");
        let extended = |t: usize| ground_textures.get(t).is_some_and(|g| g.extended);
        let height = self.tileset.water.as_ref().map_or(0.0, |w| w.height);
        let draw = Draw::new(&self.terrain, grounds, cliffs.tile_flags, &extended, height);
        self.drawn = Some(draw.clone());
        let mut out = Vec::new();
        if changed {
            let (_, _, textures, layers) = self.ground_cache.as_ref().unwrap();
            out.push(ServerMsg::Ground(GroundMsg { textures: textures.clone(), layers: layers.clone() }));
        }
        out.push(ServerMsg::Cliffs(cliffs_msg));
        out.push(ServerMsg::Terrain(draw));
        out
    }

    /// The terrain as the page draws it: its ground textures, its water,
    /// its cliffs, then the terrain, whose layers index those textures;
    /// what's placed; the overlays; the palette and the state.
    pub fn draw(&mut self) -> Vec<ServerMsg> {
        let water = match &self.tileset.water {
            Some(w) => {
                let (frames, error) = crate::textures::water(&self.game, &w.texture, w.count);
                WaterMsg { rate: w.rate, colors: w.colors, frames, error }
            }
            None => WaterMsg { error: "No water for this tileset in TerrainArt\\Water.slk.".into(), ..Default::default() },
        };
        let mut terrain = self.terrain_msgs(true);
        // Water before the cliffs and the terrain, after the ground.
        terrain.insert(1, ServerMsg::Water(water));
        let draw = self.drawn.clone().expect("drawn just now");
        if self.types.is_none() {
            let (files, notes) = crate::placed::Files::read(&self.game);
            self.placed = files;
            self.notes.extend(notes);
            self.types = Some(crate::types::Types::load(&self.game));
        }
        // All of it again: the page (re)loaded.
        self.objects = None;
        self.sent = None;
        let objects_msg = self.place_objects();
        let objects = self.objects.as_ref().unwrap();
        let (_, grounds, textures, layers) = self.ground_cache.as_ref().unwrap();
        let minimap = crate::picture::paint(&self.terrain, grounds, textures, layers, &draw, self.tileset.water.as_ref(), 1);
        let overlays = crate::overlays::Overlays::load(&self.game, &self.terrain, &draw, objects, minimap);
        self.overlays = Some(overlays.clone());
        terrain.extend(objects_msg);
        terrain.push(ServerMsg::Overlays(overlays));
        terrain.push(ServerMsg::Palette(self.palette()));
        terrain.push(ServerMsg::ObjectPalette(self.object_palette()));
        terrain.push(ServerMsg::State(self.state(String::new())));
        terrain
    }

    /// Everything placed, placed again on the terrain as it is: the
    /// models and textures the page hasn't (`Objects`), then where each
    /// stands (`Instances`).
    fn place_objects(&mut self) -> Vec<ServerMsg> {
        let draw = self.drawn.clone().expect("the terrain drawn first");
        let types = self.types.get_or_insert_with(|| crate::types::Types::load(&self.game));
        let objects = self.objects.get_or_insert_with(Default::default);
        objects.place(&self.game, types, &draw, &self.placed);
        let (models, textures) = self.sent.unwrap_or((0, 0));
        let mut out = Vec::new();
        if objects.models.len() > models || objects.textures.len() > textures || self.sent.is_none() {
            out.push(ServerMsg::Objects(ObjectsMsg {
                first_texture: textures as u32,
                first_model: models as u32,
                textures: objects.textures[textures..].to_vec(),
                models: objects.models[models..].iter().map(|m| crate::placed::Model { instances: Vec::new(), ..m.clone() }).collect(),
                notes: objects.notes.clone(),
            }));
            self.sent = Some((objects.models.len(), objects.textures.len()));
        }
        out.push(ServerMsg::Instances(objects.models.iter().map(|m| m.instances.clone()).collect()));
        out
    }

    /// What the terrain palette offers: the tileset's ground textures
    /// (`UI\\WorldEditData.txt` `[TileSets]`, after its name and blight)
    /// and the terrain's own, each with a picture (its full cell, 32
    /// pixels); the terrain's cliff types.
    pub fn palette(&self) -> PaletteMsg {
        let mut ids: Vec<String> = self.tileset.tiles.clone();
        for g in &self.terrain.ground {
            let id = g.to_string();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let paths: Vec<(String, Option<String>)> = ids.iter().map(|id| (id.clone(), self.tileset.ground_path(id))).collect();
        let (textures, layers) = crate::textures::ground(&self.game, &paths);
        let grounds = textures
            .iter()
            .map(|g| {
                let full = g.first + 15.min(g.count.saturating_sub(1));
                PaletteEntry { id: g.id.clone(), name: self.names.ground.get(&g.id).cloned().unwrap_or_else(|| g.id.clone()), icon: crate::textures::icon(&layers, full, 32) }
            })
            .collect();
        let cliffs = self
            .terrain
            .cliffs
            .iter()
            .map(|c| {
                let id = c.to_string();
                PaletteEntry { name: self.names.cliffs.get(&id).cloned().unwrap_or_else(|| id.clone()), id, icon: Default::default() }
            })
            .collect();
        PaletteMsg { grounds, cliffs }
    }

    /// The edits' state, with `message` (what a save or a brush said).
    pub fn state(&self, message: String) -> StateMsg {
        StateMsg { dirty: self.edits.dirty.any(), undo: self.edits.can_undo(), redo: self.edits.can_redo(), message }
    }

    /// A brush laid where the ray meets the ground: the terrain worked
    /// out again, and the state.
    pub fn brush(&mut self, origin: [f32; 3], dir: [f32; 3], brush: &crate::edit::Brush, start: bool) -> Vec<ServerMsg> {
        let Some([x, y, _]) = self.drawn.as_ref().and_then(|d| crate::overlays::ground_hit(d, origin, dir)) else {
            return vec![ServerMsg::State(self.state(String::new()))];
        };
        match self.edits.apply(&mut self.terrain, x, y, brush, start) {
            Ok(()) => {
                let mut out = self.terrain_msgs(false);
                // What stands on the ground stands on it as it is now.
                out.extend(self.place_objects());
                out.push(ServerMsg::State(self.state(String::new())));
                out
            }
            Err(message) => vec![ServerMsg::State(self.state(message))],
        }
    }

    pub fn stroke_end(&mut self) -> Vec<ServerMsg> {
        self.edits.end();
        vec![ServerMsg::State(self.state(String::new()))]
    }

    /// Undo (or redo): the terrain as it was, worked out again.
    pub fn undo(&mut self, redo: bool) -> Vec<ServerMsg> {
        let target = crate::edit::Target { terrain: &mut self.terrain, files: &mut self.placed };
        let done = if redo { self.edits.redo(target) } else { self.edits.undo(target) };
        let mut out = Vec::new();
        if let Some(what) = done {
            if matches!(what, crate::edit::Snapshot::Terrain(..)) {
                out.extend(self.terrain_msgs(false));
            }
            out.extend(self.place_objects());
        }
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// Writes the terrain where it came from: the root folder's file, or
    /// into the root map; a terrain only the game has, into the top root.
    /// Where it went, or why it didn't.
    pub fn save(&mut self) -> Vec<ServerMsg> {
        let dirty = self.edits.dirty;
        let mut saved = Vec::new();
        let mut failed = Vec::new();
        let files: [FileToSave; 3] = [
            (dirty.terrain, "war3map.w3e", self.terrain.to_bytes().map_err(|e| e.to_string())),
            (dirty.doodads, "war3map.doo", self.placed.doodads.as_ref().map_or(Ok(Vec::new()), |d| d.to_bytes().map_err(|e| e.to_string()))),
            (dirty.units, "war3mapUnits.doo", self.placed.units.as_ref().map_or(Ok(Vec::new()), |u| u.to_bytes().map_err(|e| e.to_string()))),
        ];
        for (i, (changed, name, bytes)) in files.into_iter().enumerate() {
            if !changed {
                continue;
            }
            match bytes.and_then(|b| self.write_file(name, b)) {
                Ok(to) => {
                    saved.push(to.display().to_string());
                    match i {
                        0 => self.edits.dirty.terrain = false,
                        1 => self.edits.dirty.doodads = false,
                        _ => self.edits.dirty.units = false,
                    }
                }
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        let message = match (saved.is_empty(), failed.is_empty()) {
            (_, false) => format!("Not saved: {}", failed.join("; ")),
            (true, true) => "Nothing to save.".to_string(),
            (false, true) => format!("Saved {}", saved.join(", ")),
        };
        vec![ServerMsg::State(self.state(message))]
    }

    /// Writes `bytes` as the map's file `name` where it came from: a root
    /// folder's file, or into the root map; one only the game has (or
    /// none has), into the top root. Where it went.
    fn write_file(&self, name: &str, bytes: Vec<u8>) -> Result<PathBuf, String> {
        let roots: Vec<PathBuf> = self.project.roots.iter().map(|r| project::normalize(r)).collect();
        let source = self.game.read(name).map(|(_, s)| s);
        let into_map = |map: &Path| -> Result<PathBuf, String> {
            let path = map.join(name);
            wc3_server::mapfs::MapFs::default().write(&path, bytes.clone(), true, true).map_err(|e| format!("{}: {e:?}", path.display()))?;
            Ok(path)
        };
        let into_root = |root: &Path| -> Result<PathBuf, String> {
            if wc3_server::map::is_map_name(root) {
                into_map(root)
            } else {
                let path = root.join(name);
                std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(path)
            }
        };
        match source {
            Some(wc3_server::game::Source::File(file)) if roots.iter().any(|r| project::normalize(&file).starts_with(r)) => {
                std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
                Ok(file)
            }
            Some(wc3_server::game::Source::Archive(map)) if roots.contains(&project::normalize(&map)) => into_map(&map),
            _ => into_root(roots.last().ok_or("the project has no root")?),
        }
    }

    /// What the object palette offers: every type of doodad, destructable,
    /// unit and item, the game's and the map's (its object data), with
    /// its name and group (a unit's race, a doodad's or destructable's
    /// category, an item's class); those World Editor hides left out.
    pub fn object_palette(&self) -> Vec<ObjectEntry> {
        let types = self.types.as_ref();
        let mut out = Vec::new();
        for (code, kind) in crate::placed::PLACED_CODES.iter().enumerate() {
            let Some(objects) = self.names.objects.of.get(kind) else { continue };
            for (id, named) in objects {
                let get = |k: &str| types.and_then(|t| t.get(*kind, id, k)).unwrap_or_default().to_string();
                if get("hiddenInEditor") == "1" || get("inEditor") == "0" {
                    continue;
                }
                let group = match kind {
                    w3abdhqtu_rs::Kind::Unit => get("race"),
                    w3abdhqtu_rs::Kind::Item => get("class"),
                    _ => get("category"),
                };
                out.push(ObjectEntry { kind: code as u8, id: id.clone(), name: named.shown(), group, custom: named.custom });
            }
        }
        out
    }

    /// The ground's point a ray meets, in the game's units.
    fn ground_point(&self, origin: [f32; 3], dir: [f32; 3]) -> Option<[f32; 3]> {
        let d = self.drawn.as_ref()?;
        let [x, y, z] = crate::overlays::ground_hit(d, origin, dir)?;
        Some([d.offset[0] + x * 128.0, d.offset[1] + y * 128.0, z * 128.0])
    }

    /// An edit of what's placed in file `file` (0 `war3map.doo`, 1
    /// `war3mapUnits.doo`): its state recorded for undo, `change` made,
    /// everything placed again.
    fn things_edit(&mut self, file: u8, change: impl FnOnce(&mut crate::placed::Files) -> Result<(), String>) -> Vec<ServerMsg> {
        let layout = self.placed.layout;
        let before = if file == 0 {
            crate::edit::Snapshot::Doodads(self.placed.doodads.get_or_insert_with(|| crate::edit::new_doodads(layout)).doodads.clone())
        } else {
            crate::edit::Snapshot::Units(self.placed.units.get_or_insert_with(|| crate::edit::new_units(layout)).units.clone())
        };
        let mut probe = self.placed.clone();
        if let Err(message) = change(&mut probe) {
            return vec![ServerMsg::State(self.state(message))];
        }
        self.edits.record(before);
        self.placed = probe;
        let mut out = self.place_objects();
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// A thing of type `id` (`kind`: 0 doodad, 1 destructable, 2 unit, 3
    /// item) placed where the ray meets the ground, facing 270° as World
    /// Editor places them; a unit owned by `player`; a doodad a variation
    /// of its type's, by chance.
    pub fn place(&mut self, kind: u8, id: &str, player: u32, origin: [f32; 3], dir: [f32; 3]) -> Vec<ServerMsg> {
        let Some([x, y, z]) = self.ground_point(origin, dir) else { return vec![ServerMsg::State(self.state(String::new()))] };
        let Some(code) = parser_rs::bin::Rawcode::parse(id) else { return vec![ServerMsg::State(self.state(format!("{id} isn't a type id")))] };
        let file = if kind < 2 { 0 } else { 1 };
        let k = crate::placed::PLACED_CODES.get(kind as usize).copied().unwrap_or(w3abdhqtu_rs::Kind::Doodad);
        let variations = self.types.as_ref().and_then(|t| t.number(k, id, "numVar")).unwrap_or(1.0).max(1.0) as u32;
        let variation = ((x.abs() as u32).wrapping_mul(31).wrapping_add(y.abs() as u32)) % variations;
        let angle = 270f32.to_radians();
        self.things_edit(file, |files| {
            if file == 0 {
                let list = &mut files.doodads.as_mut().unwrap().doodads;
                let mut placement = doo_rs::Placement::new(code, x, y);
                (placement.z, placement.angle, placement.variation) = (z, angle, variation);
                let number = list.iter().map(|d| d.number).max().unwrap_or(-1) + 1;
                list.push(doo_rs::Doodad { placement, flags: 2, life: 100, item_table: -1, item_sets: Vec::new(), number });
            } else {
                let list = &mut files.units.as_mut().unwrap().units;
                let mut unit = doo_rs::Unit::new(code, x, y, if kind == 3 { 15 } else { player });
                (unit.placement.z, unit.placement.angle) = (z, angle);
                unit.number = list.iter().map(|u| u.number).max().unwrap_or(-1) + 1;
                list.push(unit);
            }
            Ok(())
        })
    }

    /// Thing `index` of `file` moved to where the ray meets the ground;
    /// `start`: a drag's first, which is what undo goes back to (the
    /// steps after it change what the first recorded).
    pub fn move_thing(&mut self, file: u8, index: u32, origin: [f32; 3], dir: [f32; 3], start: bool) -> Vec<ServerMsg> {
        let Some([x, y, z]) = self.ground_point(origin, dir) else { return vec![ServerMsg::State(self.state(String::new()))] };
        let set = move |files: &mut crate::placed::Files| -> Result<(), String> {
            let p = placement(files, file, index)?;
            (p.x, p.y) = (x, y);
            if file == 0 {
                p.z = z;
            }
            Ok(())
        };
        if start {
            return self.things_edit(file, set);
        }
        if let Err(message) = set(&mut self.placed) {
            return vec![ServerMsg::State(self.state(message))];
        }
        let mut out = self.place_objects();
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// Thing `index` of `file` turned by `delta` radians.
    pub fn rotate(&mut self, file: u8, index: u32, delta: f32) -> Vec<ServerMsg> {
        self.things_edit(file, |files| {
            let p = placement(files, file, index)?;
            p.angle = (p.angle + delta).rem_euclid(std::f32::consts::TAU);
            Ok(())
        })
    }

    /// Thing `index` of `file` taken away: the ones after it move up.
    pub fn delete(&mut self, file: u8, index: u32) -> Vec<ServerMsg> {
        self.things_edit(file, |files| {
            let i = index as usize;
            let len = if file == 0 { files.doodads.as_ref().map_or(0, |d| d.doodads.len()) } else { files.units.as_ref().map_or(0, |u| u.units.len()) };
            if i >= len {
                return Err(format!("no thing {index} to delete"));
            }
            if file == 0 {
                files.doodads.as_mut().unwrap().doodads.remove(i);
            } else {
                files.units.as_mut().unwrap().units.remove(i);
            }
            Ok(())
        })
    }

    /// What a ray from the camera meets first, said.
    pub fn pick(&self, origin: [f32; 3], dir: [f32; 3]) -> PickedMsg {
        let ground = self.drawn.as_ref().and_then(|d| crate::overlays::ground_hit(d, origin, dir));
        // ` · in Region A, Region B`, the regions over a point.
        let in_regions = |x: f32, y: f32| {
            let regions = self.overlays.as_ref().map(|o| o.regions_at(x, y)).unwrap_or_default();
            if regions.is_empty() { String::new() } else { format!(" · in {}", regions.join(", ")) }
        };
        let Some(t) = self.objects.as_ref().and_then(|o| o.pick(origin, dir)) else {
            // The ground: its point in the game's units, its regions.
            let Some(([x, y, z], d)) = ground.zip(self.drawn.as_ref()) else { return PickedMsg::default() };
            let (wx, wy, wz) = (d.offset[0] + x * 128.0, d.offset[1] + y * 128.0, z * 128.0);
            return PickedMsg { text: format!("({wx:.0}, {wy:.0}, {wz:.0}){}", in_regions(wx, wy)), ground, ..Default::default() };
        };
        let id = t.id.to_string();
        let name = self.names.objects.get(t.kind, &id).map(|n| n.shown()).unwrap_or_else(|| id.clone());
        let what = match t.kind {
            w3abdhqtu_rs::Kind::Doodad => "doodad".to_string(),
            w3abdhqtu_rs::Kind::Destructable => "destructable".to_string(),
            w3abdhqtu_rs::Kind::Item => "item".to_string(),
            _ => t.player.map_or("unit".to_string(), |p| format!("Player {}", p + 1)),
        };
        let file = if matches!(t.kind, w3abdhqtu_rs::Kind::Doodad | w3abdhqtu_rs::Kind::Destructable) { "war3map.doo" } else { "war3mapUnits.doo" };
        PickedMsg {
            found: true,
            model: t.model as u32,
            instance: t.instance as u32,
            file: if matches!(t.kind, w3abdhqtu_rs::Kind::Doodad | w3abdhqtu_rs::Kind::Destructable) { 0 } else { 1 },
            index: t.index as u32,
            text: format!(
                "{name} ({id}) · {what} · ({:.0}, {:.0}, {:.0}) · {file} #{}{}",
                t.world[0],
                t.world[1],
                t.world[2],
                t.index,
                in_regions(t.world[0], t.world[1])
            ),
            ground,
        }
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

/// A file maybe to save: changed, its name, its bytes.
type FileToSave<'a> = (bool, &'a str, Result<Vec<u8>, String>);

/// Thing `index`'s placement in `file` (0 `war3map.doo`, 1
/// `war3mapUnits.doo`).
fn placement(files: &mut crate::placed::Files, file: u8, index: u32) -> Result<&mut doo_rs::Placement, String> {
    let i = index as usize;
    let p = if file == 0 {
        files.doodads.as_mut().and_then(|d| d.doodads.get_mut(i)).map(|d| &mut d.placement)
    } else {
        files.units.as_mut().and_then(|u| u.units.get_mut(i)).map(|u| &mut u.placement)
    };
    p.ok_or_else(|| format!("no thing {index} in {}", if file == 0 { "war3map.doo" } else { "war3mapUnits.doo" }))
}
