//! The map an editor has open: its project's (`wc3.bni`), each file read
//! through the project's lookup (`game::Lookup`: the game's folders, then
//! the map's roots, the last that has it wins), as the build reads it.

use std::path::{Path, PathBuf};

use wc3_server::game::{Game, Lookup};
use wc3_server::names::Names;
use wc3_server::project::{self, Project};

use crate::protocol::{CliffsMsg, GroundMsg, IconEntry, LightMsg, MapInfo, MapSettingsMsg, TileEntry, ObjectEntry, ObjectsMsg, PaletteEntry, PaletteMsg, PickedMsg, ServerMsg, StateMsg, ThingMsg, WaterMsg};
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
    /// A region's drag: the region (`None` until a new one is made) and
    /// its bounds when it started.
    region_drag: Option<(Option<usize>, [f32; 4])>,
    /// The pieces with a text form open in VS Code: each one's path and
    /// the text last taken from it or given to it (what comes back the
    /// same is an echo).
    docs: std::collections::HashMap<&'static str, (String, String)>,
    /// Each piece's bytes as the editor last read or wrote them, hashed:
    /// a change on disk is one that isn't the editor's.
    disk: std::collections::HashMap<&'static str, Option<u64>>,
    /// The pathing the files made when the map's `war3map.wpm` (and the
    /// brush's paint since) was last in step with them: what a pathing
    /// brush's paint is measured against.
    pathing_made: Option<wpm_rs::Pathing>,
    /// A terrain brush's stroke is going on (its end works out the
    /// overlays again).
    terrain_stroke: bool,
    /// Icons read, by path in lower case (`None`: not there).
    icon_cache: std::collections::HashMap<String, Option<crate::textures::Icon>>,
    /// The models and textures the page has (the ones after are new);
    /// `None` before it has any.
    sent: Option<(usize, usize)>,
    /// The map's shadows (`war3map.shd`), when it has them at the
    /// terrain's size.
    pub shadows: Option<shd_rs::Shadows>,
    /// What was read with a problem, gone past.
    pub notes: Vec<String>,
    /// Whether the page has the object editor's field types' values
    /// (`ObjectEnums`), since it (re)loaded.
    enums_sent: bool,
    /// The game's tables the object editor reads, once asked.
    object_tables: crate::objdata::Tables,
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
        let disk = Self::disk_hashes(&game);
        notes.extend(tileset.notes.iter().cloned());
        let (sw, sh) = shd_rs::size_for_terrain(terrain.width, terrain.height);
        let shadows = match read("war3map.shd").map(|b| shd_rs::parse(&b, sw, sh)) {
            Some(Ok(s)) => Some(s),
            Some(Err(e)) => {
                notes.push(format!("Can't read war3map.shd: {e}"));
                None
            }
            None => None,
        };
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
            icon_cache: Default::default(),
            region_drag: None,
            terrain_stroke: false,
            pathing_made: None,
            disk,
            docs: Default::default(),
            shadows,
            notes,
            object_tables: Default::default(),
            enums_sent: false,
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
        let water = self.water();
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
        if self.pathing_made.is_none() {
            self.pathing_made = Some(self.made_pathing());
        }
        // All of it again: the page (re)loaded.
        self.enums_sent = false;
        self.objects = None;
        self.sent = None;
        let objects_msg = self.place_objects();
        let objects = self.objects.as_ref().unwrap();
        let (_, grounds, textures, layers) = self.ground_cache.as_ref().unwrap();
        let minimap = crate::picture::paint(&self.terrain, grounds, textures, layers, &draw, self.tileset.water.as_ref(), 1);
        let mut overlays = crate::overlays::Overlays::load(&self.terrain, &draw, objects, &self.placed, self.info.as_ref(), minimap);
        overlays.destructables = self.destructables_pathing();
        self.overlays = Some(overlays.clone());
        terrain.extend(objects_msg);
        terrain.push(ServerMsg::Overlays(overlays));
        terrain.push(ServerMsg::Palette(self.palette()));
        terrain.push(ServerMsg::ObjectPalette(self.object_palette()));
        terrain.push(self.sounds_msg());
        terrain.push(ServerMsg::Follow(self.follow()));
        terrain.push(ServerMsg::Light(self.light()));
        terrain.push(ServerMsg::State(self.state(String::new())));
        terrain
    }

    /// The letter of the tileset whose tables are read.
    fn tileset_letter(&self) -> u8 {
        self.tileset.letter
    }

    /// The tileset's water.
    fn water(&self) -> WaterMsg {
        match &self.tileset.water {
            Some(w) => {
                let (frames, error) = crate::textures::water(&self.game, &w.texture, w.count);
                WaterMsg { rate: w.rate, colors: w.colors, frames, error }
            }
            None => WaterMsg { error: "No water for this tileset in TerrainArt\\Water.slk.".into(), ..Default::default() },
        }
    }

    /// All of the map drawn again on the page as it is (its size or
    /// tiles changed), the models the page has kept: the map, the
    /// terrain (the water too when the tileset changed), what's placed,
    /// the overlays, the palette, the light, the map's settings, the
    /// state with `message`.
    fn redraw(&mut self, water: bool, message: String) -> Vec<ServerMsg> {
        let mut out = vec![self.map_msg()];
        out.extend(self.terrain_msgs(true));
        if water {
            out.insert(2, ServerMsg::Water(self.water()));
        }
        out.extend(self.place_objects());
        out.push(ServerMsg::Overlays(self.overlays()));
        out.push(ServerMsg::Palette(self.palette()));
        out.push(ServerMsg::Light(self.light()));
        out.push(ServerMsg::MapSettings(self.map_settings()));
        out.push(ServerMsg::State(self.state(message)));
        out
    }

    /// What the map's settings show: its size and playable area, its
    /// tiles, the game's tilesets and tiles.
    pub fn map_settings(&self) -> MapSettingsMsg {
        let mut tilesets: Vec<(String, String)> = self.names.tilesets.iter().map(|(l, n)| (l.to_string(), n.clone())).collect();
        tilesets.sort();
        let tiles = crate::tiles::all(&self.tileset)
            .into_iter()
            .map(|t| TileEntry { name: self.names.ground.get(&t.id).cloned().unwrap_or_else(|| t.id.clone()), cliff: t.cliff.is_some(), pathing: t.pathing, id: t.id })
            .collect();
        MapSettingsMsg {
            width: self.terrain.width,
            height: self.terrain.height,
            info: self.info.is_some(),
            margins: self.info.as_ref().map_or([0; 4], |i| i.camera_complements),
            tileset: (self.terrain.tileset as char).to_string(),
            max_tiles: self.terrain.flags_version().textures() as u32,
            tilesets,
            map_tiles: self.terrain.ground.iter().map(|g| g.to_string()).collect(),
            tiles,
        }
    }

    /// The map made bigger or smaller by `sides`, its playable area set
    /// to `margins` (or kept): the terrain, the pathing, the shadows,
    /// `war3map.w3i`, what's placed off it removed; nothing to undo
    /// after it (as World Editor's).
    pub fn resize(&mut self, sides: crate::resize::Sides, margins: Option<[i32; 4]>) -> Vec<ServerMsg> {
        let mut terrain = self.terrain.clone();
        let mut seed = self.terrain.points.len() as u32;
        if let Err(e) = crate::resize::terrain(&mut terrain, sides, &mut seed) {
            return vec![ServerMsg::State(self.state(e))];
        }
        let mut info = self.info.clone();
        if let Some(i) = info.as_mut() {
            crate::resize::info(i, &terrain, sides);
        }
        if let Some(m) = margins
            && let Err(e) = crate::resize::set_playable(&mut terrain, info.as_mut(), m)
        {
            return vec![ServerMsg::State(self.state(e))];
        }
        let resized = sides != crate::resize::Sides::default();
        let removed = crate::resize::placed(&mut self.placed, &terrain, sides);
        for p in [self.placed.pathing.as_mut(), self.pathing_made.as_mut()].into_iter().flatten() {
            crate::resize::pathing(p, sides);
        }
        if let Some(sh) = self.shadows.as_mut() {
            crate::resize::shadows(sh, sides);
        }
        self.terrain = terrain;
        self.info = info;
        self.edits.forget();
        let d = &mut self.edits.dirty;
        d.terrain = true;
        d.info |= self.info.is_some();
        d.pathing |= resized && self.placed.pathing.is_some();
        d.shadows |= resized && self.shadows.is_some();
        // The doodads: some removed, or the special ones moved.
        d.doodads |= (removed.things > 0 || sides.left != 0 || sides.bottom != 0) && self.placed.doodads.is_some();
        d.units |= removed.things > 0 && self.placed.units.is_some();
        d.regions |= removed.regions > 0;
        d.cameras |= removed.cameras > 0;
        let message = format!(
            "The map is {} by {} cells{}.",
            self.terrain.width - 1,
            self.terrain.height - 1,
            if removed.things + removed.regions + removed.cameras > 0 {
                format!("; off it, removed: {} things, {} regions, {} cameras", removed.things, removed.regions, removed.cameras)
            } else {
                String::new()
            }
        );
        self.redraw(false, message)
    }

    /// The tile setter's: the terrain's tiles `ids`, each old tile's
    /// points to `to[old]`, the tileset `letter`'s (its tables read
    /// again when it changes); undone as one.
    pub fn set_tiles(&mut self, ids: &[String], to: &[u8], letter: u8) -> Vec<ServerMsg> {
        let mut terrain = self.terrain.clone();
        if let Err(e) = crate::tiles::set(&mut terrain, &self.tileset, ids, to, letter) {
            return vec![ServerMsg::State(self.state(e))];
        }
        let before = std::mem::replace(&mut self.terrain, terrain);
        let changed = before.tileset != letter;
        self.edits.record(crate::edit::Snapshot::Whole(Box::new(before)));
        self.retileset(changed);
        self.redraw(changed, format!("The map's tiles: {}.", ids.join(", ")))
    }

    /// The tileset's tables read again (`changed`) after the terrain's
    /// tileset did, and `war3map.w3i`'s with it.
    fn retileset(&mut self, changed: bool) {
        if !changed {
            return;
        }
        let letter = self.terrain.tileset;
        self.tileset = Tileset::load(&self.game, letter as char);
        self.ground_cache = None;
        if let Some(i) = self.info.as_mut()
            && i.tileset != letter
        {
            i.tileset = letter;
            self.edits.dirty.info = true;
        }
    }

    /// The tile pather's: ground tile `index` of the terrain's list
    /// forbids `forbids` (walking, flying, building) where it lies; the
    /// rest of the pathing as it is. Undone as a pathing brush is.
    pub fn tile_pathing(&mut self, index: u8, forbids: u8) -> Vec<ServerMsg> {
        if index as usize >= self.terrain.ground.len() {
            return vec![ServerMsg::State(self.state(format!("The terrain has no tile {index}.")))];
        }
        if self.placed.pathing.is_none() {
            return vec![ServerMsg::State(self.state("The map has no war3map.wpm to change.".into()))];
        }
        let types = self.types.get_or_insert_with(|| crate::types::Types::load(&self.game));
        let draw = self.drawn.as_ref().expect("drawn");
        let made = crate::pathing::compute_with(&self.game, &self.terrain, draw, &self.tileset, types, &self.placed, Some((index as usize, forbids)));
        let cells = crate::tiles::cells_of(&self.terrain, draw, index as usize);
        let p = self.placed.pathing.as_mut().expect("checked");
        if p.cells.len() != made.cells.len() {
            return vec![ServerMsg::State(self.state("war3map.wpm isn't the terrain's size.".into()))];
        }
        self.edits.record(crate::edit::Snapshot::Pathing(p.cells.clone()));
        crate::tiles::repath(p, &made, &cells);
        let id = self.terrain.ground[index as usize].to_string();
        vec![ServerMsg::Overlays(self.overlays()), ServerMsg::State(self.state(format!("{id}'s pathing set on {} cells.", cells.len())))]
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

    /// The light through the day (the tileset's whose light the map uses:
    /// `war3map.w3i`'s, else the terrain's), the shadows
    /// (`war3map.shd`), the fog (when the map's options use it, the
    /// colour stored blue, green, red, alpha).
    pub fn light(&self) -> LightMsg {
        let letter = self.info.as_ref().map(|i| i.light_tileset).filter(|l| *l != 0).unwrap_or(self.terrain.tileset) as char;
        let (terrain, units, notes) = crate::light::load(&self.game, letter);
        let shadows = self.shadows.as_ref().map(|s| crate::overlays::Grid { width: s.width, height: s.height, data: s.cells.clone() }).unwrap_or_default();
        let fog = self.info.as_ref().filter(|i| i.flags & 0x2000 != 0 && i.fog.style != 0).map(|i| {
            let [b, g, r, a] = i.fog.color;
            (i.fog.style, i.fog.start_z, i.fog.end_z, i.fog.density, [r, g, b, a])
        });
        LightMsg { terrain: terrain.samples, units: units.samples, shadows, fog, notes }
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
        if matches!(brush.tool, crate::edit::Tool::PathingOn | crate::edit::Tool::PathingOff) {
            let Some(pathing) = self.placed.pathing.as_mut() else {
                return vec![ServerMsg::State(self.state("The map has no war3map.wpm to paint.".into()))];
            };
            self.edits.pathing(pathing, x, y, brush, start);
            return vec![ServerMsg::Overlays(self.overlays()), ServerMsg::State(self.state(String::new()))];
        }
        match self.edits.apply(&mut self.terrain, x, y, brush, start) {
            Ok(()) => {
                self.terrain_stroke = true;
                let mut out = self.terrain_msgs(false);
                // What stands on the ground stands on it as it is now.
                out.extend(self.place_objects());
                out.push(ServerMsg::State(self.state(String::new())));
                out
            }
            Err(message) => vec![ServerMsg::State(self.state(message))],
        }
    }

    /// A stroke done: what follows the terrain once it's done (the
    /// bounds' lines, the minimap) worked out again after a terrain
    /// brush's.
    pub fn stroke_end(&mut self) -> Vec<ServerMsg> {
        self.edits.end();
        let mut out = Vec::new();
        if std::mem::take(&mut self.terrain_stroke) {
            out.push(ServerMsg::Overlays(self.overlays()));
        }
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// Undo (or redo): the terrain as it was, worked out again.
    pub fn undo(&mut self, redo: bool) -> Vec<ServerMsg> {
        let target = crate::edit::Target { terrain: &mut self.terrain, files: &mut self.placed };
        let done = if redo { self.edits.redo(target) } else { self.edits.undo(target) };
        let mut out = Vec::new();
        if let Some(what) = done
            && let Some(all) = self.undone(&what, &mut out)
        {
            return all;
        }
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// What the page is told after `what` was undone or redone, into
    /// `out`; or all of it (the whole terrain: the map drawn again).
    fn undone(&mut self, what: &crate::edit::Snapshot, out: &mut Vec<ServerMsg>) -> Option<Vec<ServerMsg>> {
        match what {
            crate::edit::Snapshot::Regions(_) | crate::edit::Snapshot::Cameras(_) | crate::edit::Snapshot::Pathing(_) => out.push(ServerMsg::Overlays(self.overlays())),
            crate::edit::Snapshot::Sounds(_) => out.push(self.sounds_msg()),
            crate::edit::Snapshot::Strings(_) => self.strings_changed(),
            crate::edit::Snapshot::Many(all) => {
                for s in all {
                    if let Some(whole) = self.undone(s, out) {
                        return Some(whole);
                    }
                }
            }
            crate::edit::Snapshot::Objects(kind, _) => {
                let k = crate::objdata::KINDS.iter().position(|x| x == kind).unwrap_or(0) as u8;
                out.push(self.object_list(k));
                out.extend(self.object_types_changed(*kind));
            }
            crate::edit::Snapshot::Whole(_) => {
                let changed = self.tileset_letter() != self.terrain.tileset;
                self.retileset(changed);
                return Some(self.redraw(changed, String::new()));
            }
            crate::edit::Snapshot::Terrain(..) => {
                out.extend(self.terrain_msgs(false));
                out.extend(self.place_objects());
                out.push(ServerMsg::Overlays(self.overlays()));
            }
            _ => out.extend(self.place_objects()),
        }
        None
    }

    /// Writes the terrain where it came from: the root folder's file, or
    /// into the root map; a terrain only the game has, into the top root.
    /// Where it went, or why it didn't.
    /// The pathing the map's files make as they are now.
    fn made_pathing(&mut self) -> wpm_rs::Pathing {
        let types = self.types.get_or_insert_with(|| crate::types::Types::load(&self.game));
        let draw = self.drawn.as_ref().expect("drawn");
        crate::pathing::compute(&self.game, &self.terrain, draw, &self.tileset, types, &self.placed)
    }

    pub fn save(&mut self) -> Vec<ServerMsg> {
        // The pathing made again, as World Editor makes it when it saves,
        // with what a brush painted over it.
        let d = self.edits.dirty;
        let mut pathing_redone = false;
        if d.terrain || d.doodads || d.pathing {
            let made = self.made_pathing();
            let mut now = made.clone();
            if let (Some(before), Some(painted)) = (&self.pathing_made, &self.placed.pathing) {
                crate::pathing::Manual::of(before, painted).apply(&mut now);
            }
            pathing_redone = self.placed.pathing.as_ref() != Some(&now);
            self.placed.pathing = Some(now);
            self.pathing_made = Some(made);
            self.edits.dirty.pathing = true;
        }
        let dirty = self.edits.dirty;
        // The pieces with a text form open: VS Code saves them.
        let docs: Vec<String> = self.docs.values().map(|(p, _)| p.clone()).collect();
        let mut saved = Vec::new();
        let mut failed = Vec::new();
        let files: [FileToSave; 10] = [
            (dirty.terrain, "war3map.w3e", self.terrain.to_bytes().map_err(|e| e.to_string())),
            (dirty.doodads, "war3map.doo", self.placed.doodads.as_ref().map_or(Ok(Vec::new()), |d| d.to_bytes().map_err(|e| e.to_string()))),
            (dirty.units, "war3mapUnits.doo", self.placed.units.as_ref().map_or(Ok(Vec::new()), |u| u.to_bytes().map_err(|e| e.to_string()))),
            (dirty.regions, "war3map.w3r", self.placed.regions.as_ref().map_or(Ok(Vec::new()), |r| r.to_bytes().map_err(|e| e.to_string()))),
            (dirty.pathing, "war3map.wpm", self.placed.pathing.as_ref().map_or(Ok(Vec::new()), |p| p.to_bytes().map_err(|e| e.to_string()))),
            (dirty.cameras && self.placed.cameras.is_some(), "war3map.w3c", self.placed.cameras.as_ref().map_or(Ok(Vec::new()), |c| c.to_bytes().map_err(|e| e.to_string()))),
            (dirty.sounds && self.placed.sounds.is_some(), "war3map.w3s", self.placed.sounds.as_ref().map_or(Ok(Vec::new()), |n| n.to_bytes().map_err(|e| e.to_string()))),
            (dirty.strings && self.placed.strings.is_some(), "war3map.wts", Ok(self.piece_bytes("war3map.wts").unwrap_or_default())),
            (dirty.info && self.info.is_some(), "war3map.w3i", self.info.as_ref().map_or(Ok(Vec::new()), |i| i.to_bytes().map_err(|e| e.to_string()))),
            (dirty.shadows && self.shadows.is_some(), "war3map.shd", Ok(self.shadows.as_ref().map_or(Vec::new(), |s| s.to_bytes()))),
        ];
        for (i, (changed, name, bytes)) in files.into_iter().enumerate() {
            if !changed {
                continue;
            }
            match bytes.and_then(|b| {
                let h = hash(&b);
                let to = self.write_file(name, b)?;
                // Our own: not a change on disk.
                if let Some(piece) = Self::PIECES.iter().find(|p| **p == name) {
                    self.disk.insert(piece, Some(h));
                }
                Ok(to)
            }) {
                Ok(to) => {
                    saved.push(to.display().to_string());
                    let d = &mut self.edits.dirty;
                    *[&mut d.terrain, &mut d.doodads, &mut d.units, &mut d.regions, &mut d.pathing, &mut d.cameras, &mut d.sounds, &mut d.strings, &mut d.info, &mut d.shadows][i] = false;
                }
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        for kind in crate::objdata::KINDS {
            let bit = crate::objdata::bit(kind);
            if self.edits.dirty.objects & bit == 0 {
                continue;
            }
            let name = crate::objdata::file_name(kind);
            let bytes = self.placed.objects.get(&kind).map_or(Ok(None), |d| d.to_bytes().map(Some).map_err(|e| e.to_string()));
            // A file the edits emptied out of a map that had none: none.
            let Ok(Some(bytes)) = bytes else {
                if let Err(e) = bytes {
                    failed.push(format!("{name}: {e}"));
                } else {
                    self.edits.dirty.objects &= !bit;
                }
                continue;
            };
            let h = hash(&bytes);
            match self.write_file(&name, bytes) {
                Ok(to) => {
                    if let Some(piece) = Self::PIECES.iter().find(|p| **p == name) {
                        self.disk.insert(piece, Some(h));
                    }
                    saved.push(to.display().to_string());
                    self.edits.dirty.objects &= !bit;
                }
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        let message = match (saved.is_empty(), failed.is_empty()) {
            (_, false) => format!("Not saved: {}", failed.join("; ")),
            (true, true) => "Nothing to save.".to_string(),
            (false, true) => format!("Saved {}", saved.join(", ")),
        };
        let mut out = Vec::new();
        if pathing_redone {
            out.push(ServerMsg::Overlays(self.overlays()));
        }
        if !docs.is_empty() {
            out.push(ServerMsg::SaveDocs(docs));
        }
        out.push(ServerMsg::State(self.state(message)));
        out
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
                // Shown as text: the game's colours left out.
                let name = wc3_server::names::plain(&named.shown());
                out.push(ObjectEntry { kind: code as u8, id: id.clone(), name, group, custom: named.custom });
            }
        }
        out
    }

    /// The icons of types (`kind` as the object palette's, id), each its
    /// `Art` (a unit's or an item's button, the map's object data's
    /// over the game's) at 32 pixels or less, read once; none for a type
    /// without one (doodads, destructables).
    pub fn icons(&mut self, wanted: &[(u8, String)]) -> Vec<IconEntry> {
        let types = self.types.get_or_insert_with(|| crate::types::Types::load(&self.game));
        wanted
            .iter()
            .map(|(kind, id)| {
                let k = crate::placed::PLACED_CODES.get(*kind as usize).copied().unwrap_or(w3abdhqtu_rs::Kind::Doodad);
                // A list (`Art=a.blp,b.blp`): its first; kind 255: a path.
                let art = if *kind == 255 { Some(id.clone()) } else { types.get(k, id, "Art").map(|a| a.split(',').next().unwrap_or(a).trim().to_string()) };
                let icon = art.and_then(|path| {
                    let path = crate::tileset::with_extension(&path);
                    self.icon_cache
                        .entry(path.to_ascii_lowercase())
                        .or_insert_with(|| {
                            crate::textures::capped(&self.game, &path, 32)
                                .ok()
                                .map(|(w, h, rgba)| crate::textures::Icon { width: w, height: h, rgba })
                        })
                        .clone()
                });
                IconEntry { kind: *kind, id: id.clone(), icon: icon.unwrap_or_default() }
            })
            .collect()
    }

    /// The map's pieces the editor changes, by name.
    pub const PIECES: [&'static str; 15] = [
        "war3map.w3e",
        "war3map.doo",
        "war3mapUnits.doo",
        "war3map.w3r",
        "war3map.wpm",
        "war3map.w3c",
        "war3map.w3s",
        "war3map.w3u",
        "war3map.w3t",
        "war3map.w3b",
        "war3map.w3d",
        "war3map.w3a",
        "war3map.w3h",
        "war3map.w3q",
        "war3map.wts",
    ];

    /// Each piece's bytes as the layers have it now, hashed (`None`: no
    /// layer has it).
    fn disk_hashes(game: &Game) -> std::collections::HashMap<&'static str, Option<u64>> {
        Self::PIECES.iter().map(|n| (*n, game.read(n).map(|(b, _)| hash(&b)))).collect()
    }

    /// Files changed on disk (`paths`): if any is in a root, the pieces
    /// read again through the layers as they are now; each that changed
    /// (not by this editor's save) taken in when it has no changes of
    /// the editor's, else said: the editor's kept, Reload takes the
    /// disk's. The page gets what changed of it.
    pub fn disk_changed(&mut self, paths: &[PathBuf]) -> Vec<ServerMsg> {
        let roots: Vec<PathBuf> = self.project.roots.iter().map(|r| project::normalize(r)).collect();
        let ours = |p: &PathBuf| {
            let p = project::normalize(p);
            roots.iter().any(|r| p == *r || p.starts_with(r))
        };
        if !paths.iter().any(ours) {
            return Vec::new();
        }
        let (game, _) = Lookup::of(&self.project).open();
        let now = Self::disk_hashes(&game);
        let changed: Vec<&str> = Self::PIECES.iter().copied().filter(|n| now.get(n) != self.disk.get(n)).collect();
        if changed.is_empty() {
            return Vec::new();
        }
        self.game = game;
        let d = self.edits.dirty;
        let dirty = |n: &str| match n {
            "war3map.w3e" => d.terrain,
            "war3map.doo" => d.doodads,
            "war3mapUnits.doo" => d.units,
            "war3map.w3r" => d.regions,
            "war3map.w3c" => d.cameras,
            "war3map.w3s" => d.sounds,
            "war3map.wts" => d.strings,
            n if crate::objdata::kind_of(n).is_some() => d.objects & crate::objdata::bit(crate::objdata::kind_of(n).unwrap()) != 0,
            _ => d.pathing,
        };
        let (kept, taken): (Vec<&str>, Vec<&str>) = changed.iter().partition(|n| dirty(n));
        let (files, notes) = crate::placed::Files::read(&self.game);
        self.notes.extend(notes);
        let mut terrain = false;
        for name in &taken {
            self.disk.insert(name, now[name]);
            match *name {
                "war3map.w3e" => {
                    match self.game.read(name).map(|(b, _)| w3e_rs::parse(&b)) {
                        Some(Ok(t)) => {
                            self.terrain = t;
                            terrain = true;
                        }
                        _ => continue,
                    }
                }
                "war3map.doo" => self.placed.doodads = files.doodads.clone(),
                "war3mapUnits.doo" => self.placed.units = files.units.clone(),
                "war3map.w3r" => self.placed.regions = files.regions.clone(),
                "war3map.w3c" => self.placed.cameras = files.cameras.clone(),
                "war3map.w3s" => self.placed.sounds = files.sounds.clone(),
                "war3map.wts" => {
                    self.placed.strings = files.strings.clone();
                    self.strings_changed();
                }
                n if crate::objdata::kind_of(n).is_some() => {
                    let kind = crate::objdata::kind_of(n).unwrap();
                    match files.objects.get(&kind) {
                        Some(d) => self.placed.objects.insert(kind, d.clone()),
                        None => self.placed.objects.remove(&kind),
                    };
                }
                _ => {
                    self.placed.pathing = files.pathing.clone();
                    self.pathing_made = None;
                }
            }
        }
        let mut out = Vec::new();
        if terrain {
            out.extend(self.terrain_msgs(false));
        }
        if !taken.is_empty() {
            out.extend(self.place_objects());
            if self.pathing_made.is_none() {
                self.pathing_made = Some(self.made_pathing());
            }
            out.push(ServerMsg::Overlays(self.overlays()));
            out.push(self.sounds_msg());
            for name in &taken {
                if let Some(kind) = crate::objdata::kind_of(name) {
                    let k = crate::objdata::KINDS.iter().position(|x| *x == kind).unwrap_or(0) as u8;
                    out.push(self.object_list(k));
                    out.extend(self.object_types_changed(kind));
                }
            }
        }
        let message = match (taken.is_empty(), kept.is_empty()) {
            (_, false) => format!("Changed on disk: {}; your changes to it kept (Reload takes the disk's).", kept.join(", ")),
            (false, true) => format!("Read again, changed on disk: {}.", taken.join(", ")),
            _ => String::new(),
        };
        out.push(ServerMsg::State(self.state(message)));
        out
    }

    /// The pieces that have a text form (`war3map.w3e`, `war3map.doo`,
    /// `war3mapUnits.doo`, `war3map.w3r`, `war3map.w3c`, `war3map.w3s`), each with the path its file
    /// is at (or would be written to): what the page's text forms are
    /// matched by.
    pub fn follow(&self) -> Vec<(String, String)> {
        Self::PIECES.iter().filter(|n| **n != "war3map.wpm").filter_map(|n| Some((n.to_string(), self.path_of(n)?.display().to_string()))).collect()
    }

    /// Where the map's file `name` is (on disk, or inside a root map), or
    /// would be written: the top root's.
    fn path_of(&self, name: &str) -> Option<PathBuf> {
        let roots: Vec<PathBuf> = self.project.roots.iter().map(|r| project::normalize(r)).collect();
        Some(match self.game.read(name).map(|(_, s)| s) {
            Some(wc3_server::game::Source::File(f)) if roots.iter().any(|r| project::normalize(&f).starts_with(r)) => f,
            Some(wc3_server::game::Source::Archive(map)) if roots.contains(&project::normalize(&map)) => map.join(name),
            _ => roots.last()?.join(name),
        })
    }

    /// A piece's bytes as the editor has it.
    fn piece_bytes(&self, name: &str) -> Option<Vec<u8>> {
        match name {
            "war3map.doo" => self.placed.doodads.as_ref()?.to_bytes().ok(),
            "war3mapUnits.doo" => self.placed.units.as_ref()?.to_bytes().ok(),
            "war3map.w3r" => self.placed.regions.as_ref()?.to_bytes().ok(),
            "war3map.w3c" => self.placed.cameras.as_ref()?.to_bytes().ok(),
            "war3map.w3s" => self.placed.sounds.as_ref()?.to_bytes().ok(),
            "war3map.wts" => self.placed.strings.as_ref().map(|(text, bom)| format!("{}{text}", if *bom { "\u{FEFF}" } else { "" }).into_bytes()),
            n if crate::objdata::kind_of(n).is_some() => self.placed.objects.get(&crate::objdata::kind_of(n)?)?.to_bytes().ok(),
            "war3map.w3e" => self.terrain.to_bytes().ok(),
            _ => None,
        }
    }

    /// A piece's text form, as the editor has it (`name` its file's, for
    /// the layout `war3map.w3i` beside it says).
    fn piece_text(&mut self, name: &'static str, path: &str) -> Option<String> {
        let bytes = self.piece_bytes(name)?;
        let text = if name == "war3map.wts" {
            // The file is its own text form (without its byte order mark).
            return self.placed.strings.as_ref().map(|(t, _)| t.clone());
        } else if let Some(kind) = crate::objdata::kind_of(name) {
            self.object_tables.load(&self.game, kind);
            wc3_server::objects::convert(wc3_server::objects::TO_TEXT, path, &bytes, &self.object_tables.meta[&kind])
        } else if name == "war3map.w3r" {
            wc3_server::regions::convert(wc3_server::objects::TO_TEXT, path, &bytes)
        } else if name == "war3map.w3c" {
            wc3_server::cameras::convert(wc3_server::objects::TO_TEXT, path, &bytes)
        } else if name == "war3map.w3s" {
            wc3_server::sounds::convert(wc3_server::objects::TO_TEXT, path, &bytes)
        } else if name == "war3map.w3e" {
            wc3_server::terrain::convert(wc3_server::objects::TO_TEXT, path, &bytes)
        } else {
            wc3_server::placement::convert(wc3_server::objects::TO_TEXT, path, &bytes)
        };
        text.ok().and_then(|t| String::from_utf8(t).ok())
    }

    /// The text of a piece's text form, opened or changed in VS Code: taken
    /// in as that piece (an undo), unless it's what the editor gave it or
    /// it has an error (said).
    pub fn text_changed(&mut self, path: &str, text: &str) -> Vec<ServerMsg> {
        let Some((name, _)) = self.follow().into_iter().find(|(_, p)| p.eq_ignore_ascii_case(path)) else { return Vec::new() };
        let name: &'static str = Self::PIECES.iter().find(|p| **p == name).copied().unwrap();
        let echo = self.docs.get(name).is_some_and(|(_, last)| last == text);
        self.docs.insert(name, (path.to_string(), text.to_string()));
        if echo || self.piece_text(name, path).as_deref() == Some(text) {
            return Vec::new();
        }
        if name == "war3map.w3e" {
            return self.terrain_text_changed(text);
        }
        if let Some(kind) = crate::objdata::kind_of(name) {
            return self.objects_text_changed(kind, path, text);
        }
        if name == "war3map.wts" {
            let (src, _) = wts_rs::split_bom(text);
            let bom = self.placed.strings.as_ref().is_some_and(|(_, b)| *b);
            let before = self.placed.strings.replace((src.to_string(), bom));
            self.edits.record(crate::edit::Snapshot::Strings(before));
            self.strings_changed();
            let mut out = self.sync_docs();
            out.push(ServerMsg::State(self.state(String::new())));
            return out;
        }
        let bytes = if name == "war3map.w3r" {
            wc3_server::regions::convert(wc3_server::objects::TO_BYTES, path, text.as_bytes())
        } else if name == "war3map.w3c" {
            wc3_server::cameras::convert(wc3_server::objects::TO_BYTES, path, text.as_bytes())
        } else if name == "war3map.w3s" {
            wc3_server::sounds::convert(wc3_server::objects::TO_BYTES, path, text.as_bytes())
        } else {
            wc3_server::placement::convert(wc3_server::objects::TO_BYTES, path, text.as_bytes())
        };
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => return vec![ServerMsg::State(self.state(e))],
        };
        let layout = self.placed.layout;
        let taken = match name {
            "war3map.doo" => doo_rs::parse_doodads(&bytes, layout).map(|d| {
                let before = self.placed.doodads.replace(d);
                crate::edit::Snapshot::Doodads(before.map(|d| d.doodads).unwrap_or_default())
            }),
            "war3mapUnits.doo" => doo_rs::parse_units(&bytes, layout).map(|u| {
                let before = self.placed.units.replace(u);
                crate::edit::Snapshot::Units(before.map(|u| u.units).unwrap_or_default())
            }),
            "war3map.w3c" => w3c_rs::parse(&bytes, wc3_server::cameras::layout(std::path::Path::new(path))).map(|c| {
                let before = self.placed.cameras.replace(c);
                crate::edit::Snapshot::Cameras(before.map(|c| c.cameras).unwrap_or_default())
            }),
            "war3map.w3s" => w3s_rs::parse(&bytes).map(|n| {
                let before = self.placed.sounds.replace(n);
                crate::edit::Snapshot::Sounds(before.map(|n| n.sounds).unwrap_or_default())
            }),
            _ => w3r_rs::parse(&bytes).map(|r| {
                let before = self.placed.regions.replace(r);
                crate::edit::Snapshot::Regions(before.map(|r| r.regions).unwrap_or_default())
            }),
        };
        let Ok(before) = taken else { return vec![ServerMsg::State(self.state(format!("Can't read {name}'s text")))] };
        self.edits.record(before);
        let mut out = match name {
            "war3map.w3r" | "war3map.w3c" => vec![ServerMsg::Overlays(self.overlays())],
            "war3map.w3s" => vec![self.sounds_msg()],
            _ => self.place_objects(),
        };
        out.extend(self.sync_docs());
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// An object data file's text form changed: taken in as that kind's
    /// object data (an undo), unless it has an error (said).
    fn objects_text_changed(&mut self, kind: w3abdhqtu_rs::Kind, path: &str, text: &str) -> Vec<ServerMsg> {
        self.object_tables.load(&self.game, kind);
        let data = wc3_server::objects::convert(wc3_server::objects::TO_BYTES, path, text.as_bytes(), &self.object_tables.meta[&kind]).and_then(|b| w3abdhqtu_rs::parse(&b, kind).map_err(|e| e.to_string()));
        let data = match data {
            Ok(d) => d,
            Err(e) => return vec![ServerMsg::State(self.state(e))],
        };
        let before = self.placed.objects.insert(kind, data);
        self.edits.record(crate::edit::Snapshot::Objects(kind, before));
        let k = crate::objdata::KINDS.iter().position(|x| *x == kind).unwrap_or(0) as u8;
        let mut out = vec![self.object_list(k)];
        out.extend(self.object_types_changed(kind));
        out.extend(self.sync_docs());
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// The terrain's text form (its header: tilesets, lists, position,
    /// version; the points aren't in it) changed: put onto the terrain as
    /// the editor has it, undone as one; refused, said, when the text has
    /// an error or the points don't allow it (another size, a tile a
    /// point uses taken out of the list).
    fn terrain_text_changed(&mut self, text: &str) -> Vec<ServerMsg> {
        let parsed = w3e_rs::text::from_text(text);
        let Some(header) = parsed.header else {
            let e = &parsed.errors[0];
            return vec![ServerMsg::State(self.state(format!("war3map.w3e's text, line {}: {}", e.line + 1, e.message)))];
        };
        let mut terrain = self.terrain.clone();
        if let Err(e) = header.apply(&mut terrain) {
            return vec![ServerMsg::State(self.state(format!("war3map.w3e's text: {e}")))];
        }
        let before = std::mem::replace(&mut self.terrain, terrain);
        let changed = before.tileset != self.terrain.tileset;
        self.edits.record(crate::edit::Snapshot::Whole(Box::new(before)));
        self.retileset(changed);
        self.redraw(changed, String::new())
    }

    /// A piece's text form closed: the editor's own again.
    pub fn text_closed(&mut self, path: &str) {
        self.docs.retain(|_, (p, _)| !p.eq_ignore_ascii_case(path));
    }

    /// What the editor changed of the pieces with a text form open, given
    /// to them (the documents then have the changes not saved, not the
    /// editor).
    pub fn sync_docs(&mut self) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        let names: Vec<&'static str> = self.docs.keys().copied().collect();
        for name in names {
            let path = self.docs[name].0.clone();
            let Some(text) = self.piece_text(name, &path) else { continue };
            if self.docs[name].1 != text {
                self.docs.insert(name, (path.clone(), text.clone()));
                out.push(ServerMsg::TextOut { path, text });
            }
            // The document saves them; the terrain's has its header
            // alone: its points the editor saves.
            let d = &mut self.edits.dirty;
            match name {
                "war3map.doo" => d.doodads = false,
                "war3mapUnits.doo" => d.units = false,
                "war3map.w3r" => d.regions = false,
                "war3map.w3c" => d.cameras = false,
                "war3map.w3s" => d.sounds = false,
                "war3map.wts" => d.strings = false,
                n if crate::objdata::kind_of(n).is_some() => d.objects &= !crate::objdata::bit(crate::objdata::kind_of(n).unwrap()),
                _ => {}
            }
        }
        out
    }

    /// What the destructables lay when they stand (not in
    /// `war3map.wpm`), at the pathing's size.
    fn destructables_pathing(&self) -> crate::overlays::Grid {
        let Some(types) = self.types.as_ref() else { return Default::default() };
        let (w, h) = ((self.terrain.width.saturating_sub(1)) * 4, (self.terrain.height.saturating_sub(1)) * 4);
        crate::overlays::Grid { width: w, height: h, data: crate::pathing::destructables(&self.game, &self.terrain, types, &self.placed) }
    }

    /// The overlays worked out again (the regions, the pathing, the
    /// minimap as they are now), kept for picks.
    fn overlays(&mut self) -> crate::overlays::Overlays {
        let draw = self.drawn.clone().expect("drawn");
        let (_, grounds, textures, layers) = self.ground_cache.as_ref().expect("drawn");
        let minimap = crate::picture::paint(&self.terrain, grounds, textures, layers, &draw, self.tileset.water.as_ref(), 1);
        let empty = crate::placed::Objects::default();
        let objects = self.objects.as_ref().unwrap_or(&empty);
        let mut o = crate::overlays::Overlays::load(&self.terrain, &draw, objects, &self.placed, self.info.as_ref(), minimap);
        o.destructables = self.destructables_pathing();
        self.overlays = Some(o.clone());
        o
    }

    /// A region edit: its state recorded for undo (`start`), `change`
    /// made, the overlays again.
    fn regions_edit(&mut self, start: bool, change: impl FnOnce(&mut w3r_rs::Regions) -> Result<(), String>) -> Vec<ServerMsg> {
        let regions = self.placed.regions.get_or_insert_with(w3r_rs::Regions::new);
        let before = regions.regions.clone();
        if let Err(message) = change(regions) {
            regions.regions = before;
            return vec![ServerMsg::State(self.state(message))];
        }
        if start {
            self.edits.record(crate::edit::Snapshot::Regions(before));
        } else {
            self.edits.dirty.regions = true;
        }
        vec![ServerMsg::Overlays(self.overlays()), ServerMsg::State(self.state(String::new()))]
    }

    /// A drag of a region from where the ray `from` meets the ground to
    /// where `to` does, its edges on World Editor's grid of 32 units:
    /// mode 0 a new one (`Region 000` ..., the next number), 1 region
    /// `index` moved, 2 its corner nearest the start moved. `start`: the
    /// drag's first step (which undo goes back to).
    pub fn region_drag(&mut self, start: bool, mode: u8, index: i32, from: ([f32; 3], [f32; 3]), to: ([f32; 3], [f32; 3])) -> Vec<ServerMsg> {
        let (Some(a), Some(b)) = (self.ground_point(from.0, from.1), self.ground_point(to.0, to.1)) else {
            return vec![ServerMsg::State(self.state(String::new()))];
        };
        let snap = |v: f32| (v / 32.0).round() * 32.0;
        if start {
            let index = match (mode, index) {
                (0, _) => None,
                (_, i) if i >= 0 => Some(i as usize),
                _ => return vec![ServerMsg::State(self.state("No region picked.".into()))],
            };
            let bounds = index.and_then(|i| self.placed.regions.as_ref()?.regions.get(i)).map(|r| [r.left, r.bottom, r.right, r.top]);
            self.region_drag = Some((index, bounds.unwrap_or_default()));
        }
        let Some((index, was)) = self.region_drag else { return vec![] };
        let mut made = None;
        let out = self.regions_edit(start, |regions| {
            let r = match index {
                Some(i) => regions.regions.get_mut(i).ok_or(format!("no region {i}"))?,
                None => {
                    let n = regions.regions.iter().filter_map(|r| r.name.strip_prefix("Region ")?.parse::<u32>().ok()).max().map_or(0, |n| n + 1);
                    let next = regions.next_index();
                    let mut r = w3r_rs::Region::new(format!("Region {n:03}"), 0.0, 0.0, 0.0, 0.0, next);
                    // A colour of its own, as World Editor gives one.
                    (r.red, r.green, r.blue) = [(255, 64, 64), (64, 160, 255), (64, 255, 64), (255, 200, 0), (200, 64, 255), (0, 220, 200)][n as usize % 6];
                    regions.regions.push(r);
                    made = Some(regions.regions.len() - 1);
                    regions.regions.last_mut().unwrap()
                }
            };
            let (ax, ay, bx, by) = (snap(a[0]), snap(a[1]), snap(b[0]), snap(b[1]));
            let [l, bo, rt, tp] = was;
            [r.left, r.bottom, r.right, r.top] = match mode {
                0 => [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)],
                1 => {
                    let (dx, dy) = (bx - ax, by - ay);
                    [l + dx, bo + dy, rt + dx, tp + dy]
                }
                _ => {
                    // The corner nearest where the drag started, to where it is.
                    let x_left = (ax - l).abs() < (ax - rt).abs();
                    let y_bottom = (ay - bo).abs() < (ay - tp).abs();
                    let (nl, nr) = if x_left { (bx, rt) } else { (l, bx) };
                    let (nb, nt) = if y_bottom { (by, tp) } else { (bo, by) };
                    [nl.min(nr), nb.min(nt), nl.max(nr), nb.max(nt)]
                }
            };
            if r.right - r.left < 32.0 || r.top - r.bottom < 32.0 {
                r.right = r.right.max(r.left + 32.0);
                r.top = r.top.max(r.bottom + 32.0);
            }
            Ok(())
        });
        // A new one: the drag's next steps resize it.
        if let Some(i) = made {
            self.region_drag = Some((Some(i), [0.0; 4]));
        }
        out
    }

    pub fn region_rename(&mut self, index: u32, name: &str) -> Vec<ServerMsg> {
        let name = name.to_string();
        self.regions_edit(true, |regions| {
            let r = regions.regions.get_mut(index as usize).ok_or(format!("no region {index}"))?;
            if name.trim().is_empty() {
                return Err("A region needs a name.".into());
            }
            r.name = name;
            Ok(())
        })
    }

    pub fn region_delete(&mut self, index: u32) -> Vec<ServerMsg> {
        self.regions_edit(true, |regions| {
            if (index as usize) >= regions.regions.len() {
                return Err(format!("no region {index}"));
            }
            regions.regions.remove(index as usize);
            Ok(())
        })
    }

    /// A camera edit: its state recorded for undo (`start`), `change`
    /// made, the overlays again. A map without `war3map.w3c` gets one in
    /// its `war3map.w3i`'s layout (a 2.0 map's version 3).
    fn cameras_edit(&mut self, start: bool, change: impl FnOnce(&mut w3c_rs::Cameras) -> Result<(), String>) -> Vec<ServerMsg> {
        let w3i = self.info.as_ref().map(|i| i.version);
        let cameras = self.placed.cameras.get_or_insert_with(|| {
            let mut c = w3c_rs::Cameras::new(w3i.map(w3c_rs::Layout::from_w3i_version).unwrap_or_default());
            if w3i.is_some_and(|v| v >= 39) {
                c.version = 3;
            }
            c
        });
        let before = cameras.cameras.clone();
        if let Err(message) = change(cameras) {
            cameras.cameras = before;
            return vec![ServerMsg::State(self.state(message))];
        }
        if start {
            self.edits.record(crate::edit::Snapshot::Cameras(before));
        } else {
            self.edits.dirty.cameras = true;
        }
        vec![ServerMsg::Overlays(self.overlays()), ServerMsg::State(self.state(String::new()))]
    }

    /// Camera `index`'s target dragged to where the ray meets the ground
    /// (its height over the ground kept). `start`: the drag's first step.
    pub fn camera_drag(&mut self, start: bool, index: u32, origin: [f32; 3], dir: [f32; 3]) -> Vec<ServerMsg> {
        let Some([x, y, _]) = self.ground_point(origin, dir) else {
            return vec![ServerMsg::State(self.state(String::new()))];
        };
        self.cameras_edit(start, |c| {
            let c = c.cameras.get_mut(index as usize).ok_or(format!("no camera {index}"))?;
            (c.target_x, c.target_y) = (x, y);
            Ok(())
        })
    }

    /// A camera made from the page's view (`index` -1: `Camera 001` ...,
    /// the next number) or camera `index` set to it: `view` the point
    /// looked at (cells, layers), the distance (cells), the horizontal
    /// and vertical angles (radians: the page looks along cos v sin h,
    /// cos v cos h, sin v), the field of view (degrees), the roll
    /// (radians; the camera's in degrees, 0 to 360). The game's
    /// rotation is then 90° − h, its angle of attack v, each 0 to 360.
    ///
    /// A camera set keeps its height over the ground (its z offset) and
    /// still looks exactly as the view does: from the same eye along the
    /// same direction, its target where that line is its height over the
    /// ground (the place nearest the view's), its distance to it. A line
    /// that never is (looking up, say) takes the view's target, said.
    pub fn camera_view(&mut self, index: i32, view: [f32; 8]) -> Vec<ServerMsg> {
        let Some(draw) = self.drawn.clone() else { return vec![] };
        let [mut x, mut y, mut z, mut distance, h, v, fov, roll] = view;
        let kept = (index >= 0).then(|| self.placed.cameras.as_ref()?.cameras.get(index as usize).map(|c| c.z_offset / 128.0)).flatten();
        let mut said = String::new();
        if let Some(height) = kept {
            let dir = [v.cos() * h.sin(), v.cos() * h.cos(), v.sin()];
            let eye = [x - dir[0] * distance, y - dir[1] * distance, z - dir[2] * distance];
            match crate::overlays::at_height(&draw, eye, dir, height, distance) {
                Some(t) => {
                    (x, y, z) = (eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t);
                    distance = t;
                }
                None => said = "The view never looks at the camera's height over the ground: its height is the view's.".into(),
            }
        }
        let degrees = |a: f32| {
            let d = a.to_degrees().rem_euclid(360.0);
            // Whole hundredths, as World Editor shows them.
            let d = (d * 100.0).round() / 100.0;
            if d >= 360.0 { 0.0 } else { d }
        };
        // A camera's z offset is over the terrain, the water left out.
        let ground = crate::placed::terrain_at(&draw, x, y);
        let out = self.cameras_edit(true, |cameras| {
            let c = if index < 0 {
                let n = cameras.cameras.iter().filter_map(|c| c.name.strip_prefix("Camera ")?.parse::<u32>().ok()).max().map_or(1, |n| n + 1);
                cameras.cameras.push(w3c_rs::Camera::new(format!("Camera {n:03}"), 0.0, 0.0));
                cameras.cameras.last_mut().unwrap()
            } else {
                cameras.cameras.get_mut(index as usize).ok_or(format!("no camera {index}"))?
            };
            c.target_x = draw.offset[0] + x * 128.0;
            c.target_y = draw.offset[1] + y * 128.0;
            c.z_offset = ((z - ground) * 128.0 * 100.0).round() / 100.0;
            c.distance = (distance * 128.0 * 100.0).round() / 100.0;
            c.rotation = degrees(std::f32::consts::FRAC_PI_2 - h);
            c.angle_of_attack = degrees(v);
            c.field_of_view = fov;
            c.roll = degrees(roll);
            Ok(())
        });
        match (said.is_empty(), out.last()) {
            (false, Some(ServerMsg::State(_))) => {
                let mut out = out;
                out.pop();
                out.push(ServerMsg::State(self.state(said)));
                out
            }
            _ => out,
        }
    }

    pub fn camera_rename(&mut self, index: u32, name: &str) -> Vec<ServerMsg> {
        let name = name.to_string();
        self.cameras_edit(true, |cameras| {
            let c = cameras.cameras.get_mut(index as usize).ok_or(format!("no camera {index}"))?;
            if name.trim().is_empty() {
                return Err("A camera needs a name.".into());
            }
            c.name = name;
            Ok(())
        })
    }

    /// Camera `index`'s field `field` (one of `w3c_rs::Camera::fields`)
    /// set to `value`; a field of view between 1 and 179 degrees, a
    /// distance over 0.
    pub fn camera_set(&mut self, index: u32, field: &str, value: f32) -> Vec<ServerMsg> {
        let field = field.to_string();
        self.cameras_edit(true, |cameras| {
            let c = cameras.cameras.get_mut(index as usize).ok_or(format!("no camera {index}"))?;
            let bad = match field.as_str() {
                "field_of_view" => !(1.0..=179.0).contains(&value),
                "distance" => value <= 0.0,
                _ => false,
            };
            if !value.is_finite() || bad {
                return Err(format!("A camera's {} can't be {value}.", field.replace('_', " ")));
            }
            *c.field_mut(&field).ok_or(format!("a camera has no {field}"))? = value;
            Ok(())
        })
    }

    pub fn camera_delete(&mut self, index: u32) -> Vec<ServerMsg> {
        self.cameras_edit(true, |cameras| {
            if (index as usize) >= cameras.cameras.len() {
                return Err(format!("no camera {index}"));
            }
            cameras.cameras.remove(index as usize);
            Ok(())
        })
    }

    /// Kind `k` of `objdata::KINDS` (the page's number for it), its
    /// tables read.
    fn object_kind(&mut self, k: u8) -> Option<w3abdhqtu_rs::Kind> {
        let kind = *crate::objdata::KINDS.get(k as usize)?;
        self.object_tables.load(&self.game, kind);
        Some(kind)
    }

    /// An object's name as World Editor shows it: its name field (and
    /// suffix) as the map changes it, else the game's (`WESTRING_`s and
    /// `TRIGSTR_`s resolved, colours off).
    fn object_name(&self, kind: w3abdhqtu_rs::Kind, object: Option<&w3abdhqtu_rs::Object>, base: &str) -> String {
        let std = &self.object_tables.standard[&kind];
        let meta = &self.object_tables.meta[&kind];
        let own = |key: &str| -> Option<String> {
            let o = object?;
            let field = meta.keys(kind, o.base).field(key)?;
            o.mods().filter(|m| m.field == field && m.level <= 1).last().map(|m| crate::objdata::shown(&m.value))
        };
        let game = |key: &str| std.get(base, key).map(|v| v.split(',').next().unwrap_or("").to_string());
        let name_key = crate::objdata::name_key(kind);
        let name = own(name_key)
            .filter(|n| !n.is_empty())
            .or_else(|| game(name_key))
            .or_else(|| (kind == w3abdhqtu_rs::Kind::Buff).then(|| game("Bufftip")).flatten())
            .unwrap_or_else(|| object.map_or(base.to_string(), |o| crate::objdata::id_of(o).to_string()));
        let suffix = own("EditorSuffix").or_else(|| game("EditorSuffix")).unwrap_or_default();
        let shown = |t: &str| wc3_server::names::plain(&self.names.resolve(&self.text(t)));
        let (name, suffix) = (shown(&name), shown(&suffix));
        if suffix.trim().is_empty() { name } else { format!("{name} {}", suffix.trim()) }
    }

    /// Object `id`'s name, of the first of `kinds` that has it (their
    /// tables read): as the object editor's list names it; else the id.
    fn id_name(&self, kinds: &[w3abdhqtu_rs::Kind], id: &str) -> String {
        for kind in kinds {
            let Some(std) = self.object_tables.standard.get(kind) else { continue };
            let custom = parser_rs::bin::Rawcode::parse(id).and_then(|code| self.placed.objects.get(kind)?.customs.iter().find(|o| o.id == code));
            if let Some(o) = custom {
                return self.object_name(*kind, Some(o), &o.base.to_string());
            }
            if std.has(id) {
                let changed = self.placed.objects.get(kind).and_then(|d| d.originals.iter().find(|o| o.base.to_string() == id));
                return self.object_name(*kind, changed, id);
            }
        }
        id.to_string()
    }

    /// Kind `k`'s objects, as the page asks for them: the field types'
    /// values first, when the page hasn't them.
    pub fn object_list_first(&mut self, k: u8) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        if !self.enums_sent {
            self.enums_sent = true;
            let enums = crate::objdata::enums(&self.game)
                .into_iter()
                .map(|e| (e.ty, e.multi, e.values.into_iter().map(|(v, key)| (v, wc3_server::names::plain(&self.names.resolve(&key)))).collect()))
                .collect();
            out.push(ServerMsg::ObjectEnums(enums));
        }
        out.push(self.object_list(k));
        out
    }

    /// Where World Editor's tree puts the game's object `base`: a unit
    /// under melee or campaign, its race, units, buildings, heroes or
    /// special; an item under its class; an ability under units, heroes
    /// or items and its race; a buff or upgrade under its race; a doodad
    /// or destructable under its category.
    fn object_group(&self, kind: w3abdhqtu_rs::Kind, base: &str) -> String {
        use w3abdhqtu_rs::Kind;
        let std = &self.object_tables.standard[&kind];
        let g = |key: &str| std.get(base, key).unwrap_or("").trim().to_string();
        let named = |ty: &str, v: String| self.object_tables.enum_names.get(&format!("{ty}:{}", v.to_ascii_lowercase())).cloned().unwrap_or(v);
        let race = || named("unitRace", g("race"));
        match kind {
            Kind::Unit => {
                let mode = if g("campaign") == "1" { "Campaign" } else { "Melee" };
                let what = if base.starts_with(|c: char| c.is_ascii_uppercase()) {
                    "Heroes"
                } else if g("isbldg") == "1" {
                    "Buildings"
                } else if g("special") == "1" {
                    "Special"
                } else {
                    "Units"
                };
                format!("{mode} · {} · {what}", race())
            }
            Kind::Item => named("itemClass", g("class")),
            Kind::Ability => {
                let what = if g("hero") == "1" { "Heroes" } else if g("item") == "1" { "Items" } else { "Units" };
                format!("{what} · {}", race())
            }
            Kind::Buff | Kind::Upgrade => race(),
            _ => g("category"),
        }
    }

    /// Kind `k`'s objects: the game's (changed by the map or not), then
    /// the map's own.
    pub fn object_list(&mut self, k: u8) -> ServerMsg {
        let Some(kind) = self.object_kind(k) else { return ServerMsg::ObjectList(Default::default()) };
        if self.object_tables.enum_names.is_empty() {
            for e in crate::objdata::enums(&self.game) {
                for (v, key) in e.values {
                    let name = wc3_server::names::plain(&self.names.resolve(&key));
                    self.object_tables.enum_names.insert(format!("{}:{}", e.ty, v.to_ascii_lowercase()), name);
                }
            }
        }
        let data = self.placed.objects.get(&kind);
        let mut objects = Vec::new();
        for id in &self.object_tables.standard[&kind].ids {
            let changed = data.and_then(|d| d.originals.iter().find(|o| o.base.to_string() == *id));
            objects.push((id.clone(), self.object_name(kind, changed, id), self.object_group(kind, id), changed.is_some() as u8));
        }
        // By their place in World Editor's tree (melee before campaign),
        // then name.
        let campaign = |g: &str| g.starts_with("Campaign");
        objects.sort_by(|a, b| (campaign(&a.2), &a.2, &a.1, &a.0).cmp(&(campaign(&b.2), &b.2, &b.1, &b.0)));
        let mut customs = Vec::new();
        for o in data.map(|d| d.customs.as_slice()).unwrap_or_default() {
            let base = o.base.to_string();
            customs.push((o.id.to_string(), self.object_name(kind, Some(o), &base), format!("Custom · {}", self.object_group(kind, &base)), 2));
        }
        customs.sort_by(|a, b| (a.2.contains("Campaign"), &a.2, &a.1, &a.0).cmp(&(b.2.contains("Campaign"), &b.2, &b.1, &b.0)));
        objects.extend(customs);
        ServerMsg::ObjectList(crate::protocol::ObjectListMsg { kind: k, objects })
    }

    /// The map's strings changed (`placed::Files::strings`): read again
    /// for what resolves `TRIGSTR_`s.
    fn strings_changed(&mut self) {
        self.strings = self.placed.strings.as_ref().map(|(t, _)| wts_rs::parse(t));
    }

    /// `kind`'s object data changed: the types of what's placed (a unit's
    /// model, scale ...) as the map's object data has them now, and, for a
    /// kind that's placed, everything placed again.
    fn object_types_changed(&mut self, kind: w3abdhqtu_rs::Kind) -> Vec<ServerMsg> {
        if !crate::types::PLACED.contains(&kind) || self.drawn.is_none() {
            return Vec::new();
        }
        if let Some(t) = self.types.as_mut() {
            t.apply(&self.placed.objects);
        }
        // The palette's names: the map's own objects as they are now, a
        // changed one's name.
        self.object_tables.load(&self.game, kind);
        let mut names = std::mem::take(&mut self.names.objects);
        if let Some(of) = names.of.get_mut(&kind) {
            of.retain(|_, n| !n.custom);
        }
        if let Some(data) = self.placed.objects.get(&kind) {
            names.add_object_data(data, &self.object_tables.meta[&kind], |t| self.text(t));
        }
        self.names.objects = names;
        let mut out = self.place_objects();
        out.push(ServerMsg::ObjectPalette(self.object_palette()));
        out
    }

    /// The version a new object data file of the map takes: its other
    /// files' (3 since 1.33), else 2.
    fn object_version(&self) -> u32 {
        self.placed.objects.values().map(|d| d.version).max().unwrap_or(2)
    }

    /// Object `id` of kind `k`: every field it has (the metadata's, for
    /// what it's based on), a row a level for a field with levels (as
    /// many as its levels field says, a doodad's variations), its value
    /// (the map's change, else the game's) and the game's, by category
    /// and name.
    pub fn object_fields(&mut self, k: u8, id: &str) -> ServerMsg {
        let none = || ServerMsg::ObjectFields(crate::protocol::ObjectFieldsMsg { kind: k, id: id.to_string(), ..Default::default() });
        let Some(kind) = self.object_kind(k) else { return none() };
        let Some(code) = parser_rs::bin::Rawcode::parse(id) else { return none() };
        let std = &self.object_tables.standard[&kind];
        let meta = &self.object_tables.meta[&kind];
        let data = self.placed.objects.get(&kind);
        let object = data.and_then(|d| crate::objdata::find(d, code).map(|(custom, i)| if custom { &d.customs[i] } else { &d.originals[i] }));
        let base = object.map_or(code, |o| o.base);
        let base_text = base.to_string();
        if object.is_none_or(|o| o.id == parser_rs::bin::Rawcode::NONE) && !std.has(id) {
            return none();
        }
        let version = data.map_or(self.object_version(), |d| d.version);
        let leveled = kind.has_levels(version);
        let own = |field, level, column| object.and_then(|o| o.mods().filter(|m| m.field == field && m.level == level && m.column == column).last());
        // How many levels: its levels field, the map's or the game's.
        let levels = crate::objdata::levels_key(kind)
            .and_then(|key| {
                let (field, f) = meta.fields().find(|(_, f)| f.key.eq_ignore_ascii_case(key) && meta.applies(kind, base, f))?;
                let column = if kind == w3abdhqtu_rs::Kind::Ability { f.column } else { 0 };
                let v = own(field, 0, column).map(|m| crate::objdata::shown(&m.value)).or_else(|| std.value(&base_text, f, 0))?;
                v.trim().parse::<f64>().ok()
            })
            .map_or(1, |n| (n as u32).clamp(1, 100));
        // A unit's fields are a unit's, a hero's or a building's (a hero's
        // id a capital first, as the game tells them; a building by its
        // `isbldg`, the map's or the game's).
        let which = (kind == w3abdhqtu_rs::Kind::Unit).then(|| {
            let building = meta
                .keys(kind, base)
                .field("isbldg")
                .and_then(|f| own(f, 0, 0))
                .map(|m| crate::objdata::shown(&m.value))
                .or_else(|| std.get(&base_text, "isbldg").map(String::from))
                .is_some_and(|v| v.trim() == "1");
            if base.0[0].is_ascii_uppercase() { 1 } else if building { 2 } else { 0 }
        });
        let mut fields = Vec::new();
        for (field, f) in meta.fields().filter(|(_, f)| meta.applies(kind, base, f) && which.is_none_or(|w| w3abdhqtu_rs::Metadata::for_unit(f, w))) {
            let column = if leveled && kind == w3abdhqtu_rs::Kind::Ability { f.column } else { 0 };
            let rows: Vec<u32> = if leveled && f.levels { (1..=levels).collect() } else { vec![0] };
            for level in rows {
                let default = std.value(&base_text, f, level).unwrap_or_default();
                let changed = own(field, level, column);
                fields.push(crate::protocol::FieldRow {
                    field: field.to_string(),
                    name: wc3_server::names::plain(&f.name),
                    category: f.category.clone(),
                    ty: f.ty.clone(),
                    level,
                    column,
                    // A string of the map's (`TRIGSTR_7`) as its text.
                    value: changed.map_or_else(|| default.clone(), |m| self.text(&crate::objdata::shown(&m.value))),
                    default,
                    changed: changed.is_some(),
                    hint: String::new(),
                });
            }
        }
        fields.sort_by(|a, b| (&a.category, &a.name, a.level).cmp(&(&b.category, &b.name, b.level)));
        let name = self.object_name(kind, object, &base_text);
        // An id list's objects by name, as the lists name them.
        for kinds in fields.iter().map(|r| crate::objdata::id_kinds(&r.ty)).collect::<Vec<_>>() {
            for k in kinds {
                self.object_tables.load(&self.game, *k);
            }
        }
        for r in &mut fields {
            let kinds = crate::objdata::id_kinds(&r.ty);
            if kinds.is_empty() || r.value.trim().is_empty() {
                continue;
            }
            let names: Vec<String> = r.value.split(',').map(str::trim).filter(|id| !id.is_empty()).map(|id| self.id_name(kinds, id)).collect();
            r.hint = names.join(", ");
        }
        ServerMsg::ObjectFields(crate::protocol::ObjectFieldsMsg { kind: k, id: id.to_string(), base: base_text, name, levels, fields })
    }

    /// An object data edit of `kind`: its file's state recorded for undo,
    /// `change` made (refused, said, when it errs; a message it gives
    /// said), the list again. A map without the file gets one.
    fn objects_edit(&mut self, k: u8, kind: w3abdhqtu_rs::Kind, change: impl FnOnce(&mut w3abdhqtu_rs::ObjectData, &w3abdhqtu_rs::Metadata) -> Result<String, String>) -> Vec<ServerMsg> {
        let before = self.placed.objects.get(&kind).cloned();
        let mut data = before.clone().unwrap_or_else(|| w3abdhqtu_rs::ObjectData { version: self.object_version(), ..w3abdhqtu_rs::ObjectData::new(kind) });
        let mut out = Vec::new();
        let message = match change(&mut data, &self.object_tables.meta[&kind]) {
            Ok(said) => {
                self.edits.record(crate::edit::Snapshot::Objects(kind, before));
                self.placed.objects.insert(kind, data);
                out = self.object_types_changed(kind);
                said
            }
            Err(message) => message,
        };
        out.insert(0, self.object_list(k));
        out.push(ServerMsg::State(self.state(message)));
        out
    }

    /// Object `id`'s field `field` (an id) at `level` in data `column`
    /// set to `value` (as text, in World Editor's limits for it), or,
    /// empty, the game's again. An undo each.
    pub fn object_set(&mut self, k: u8, id: &str, field: &str, level: u32, column: u32, value: &str) -> Vec<ServerMsg> {
        let Some(kind) = self.object_kind(k) else { return vec![ServerMsg::State(self.state(format!("no kind {k}")))] };
        let (Some(code), Some(field_id)) = (parser_rs::bin::Rawcode::parse(id), parser_rs::bin::Rawcode::parse(field)) else {
            return vec![ServerMsg::State(self.state(format!("{id}'s {field}: not ids")))];
        };
        let known = self.object_tables.standard[&kind].has(id) || self.placed.objects.get(&kind).is_some_and(|d| d.customs.iter().any(|o| o.id == code));
        if !known {
            return vec![ServerMsg::State(self.state(format!("No object {id}.")))];
        }
        // A field that's a string of the map's (`TRIGSTR_7`): the string
        // set, as World Editor does, the field still naming it.
        let string = self.placed.objects.get(&kind).and_then(|d| {
            let (custom, i) = crate::objdata::find(d, code)?;
            let o = if custom { &d.customs[i] } else { &d.originals[i] };
            let m = o.mods().filter(|m| m.field == field_id && m.level == level && m.column == column).last()?;
            match &m.value {
                w3abdhqtu_rs::Value::Str(s) => s.strip_prefix("TRIGSTR_")?.parse::<u32>().ok(),
                _ => None,
            }
        });
        if let Some(n) = string.filter(|_| !value.is_empty()) {
            let mut out = self.string_set(n, value);
            out.insert(0, self.object_list(k));
            return out;
        }
        let value = value.to_string();
        self.objects_edit(k, kind, |data, meta| {
            let f = meta.get(field_id);
            let name = f.map_or(field_id.to_string(), |f| wc3_server::names::plain(&f.name));
            let v = if value.is_empty() { None } else { Some(crate::objdata::to_value(&value, f).map_err(|e| format!("{id}'s {name}: {e}."))?) };
            crate::objdata::set(data, code, field_id, level, column, v);
            Ok(String::new())
        })
    }

    /// The map's string `n` (`TRIGSTR_n`) set to `text`, as `war3map.wts`
    /// writes it; an undo. Said when the map has no such string.
    fn string_set(&mut self, n: u32, text: &str) -> Vec<ServerMsg> {
        let Some((src, bom)) = self.placed.strings.clone() else { return vec![ServerMsg::State(self.state(format!("The map has no war3map.wts for string {n}.")))] };
        let doc = wts_rs::parse(&src);
        let edits = match wts_rs::set_string(&doc, n, text) {
            Ok(e) => e,
            Err(e) => return vec![ServerMsg::State(self.state(format!("String {n}: {e}")))],
        };
        let before = self.placed.strings.replace((wts_rs::apply(&src, &edits), bom));
        self.edits.record(crate::edit::Snapshot::Strings(before));
        self.strings_changed();
        vec![ServerMsg::State(self.state(String::new()))]
    }

    /// A custom object of `kind` based on the game's `base` (with the
    /// map's changes to it, as World Editor copies them), or a copy of the
    /// map's own `base` (its base, its changes: Copy and Paste), its id
    /// the next World Editor would give (`objdata::new_id`).
    pub fn object_new(&mut self, k: u8, base: &str) -> Vec<ServerMsg> {
        let Some(kind) = self.object_kind(k) else { return vec![ServerMsg::State(self.state(format!("no kind {k}")))] };
        let std = &self.object_tables.standard[&kind];
        // One of the map's own: a copy of it (its base, its changes).
        let own = parser_rs::bin::Rawcode::parse(base).and_then(|code| self.placed.objects.get(&kind)?.customs.iter().find(|o| o.id == code)).map(|o| (o.base, o.sets.clone()));
        let Some(base_code) = own.as_ref().map(|o| o.0).or_else(|| parser_rs::bin::Rawcode::parse(base).filter(|_| std.has(base))) else {
            return vec![ServerMsg::State(self.state(format!("{base} isn't an object to base one on.")))];
        };
        let taken: std::collections::HashSet<String> = crate::objdata::KINDS
            .iter()
            .flat_map(|k| self.object_tables.standard.get(k).map(|s| s.ids.clone()).unwrap_or_default())
            .chain(self.placed.objects.values().flat_map(|d| d.customs.iter().map(|o| o.id.to_string())))
            .collect();
        let Some(id) = crate::objdata::new_id(kind, base_code, |id| taken.contains(&id.to_string())) else {
            return vec![ServerMsg::State(self.state("No id left.".to_string()))];
        };
        let before = self.placed.objects.get(&kind).cloned();
        let mut data = before.clone().unwrap_or_else(|| w3abdhqtu_rs::ObjectData { version: self.object_version(), ..w3abdhqtu_rs::ObjectData::new(kind) });
        // The base's changes (or the one copied's), as World Editor copies them.
        let changed = own.map(|o| o.1).or_else(|| data.originals.iter().find(|o| o.base == base_code).map(|o| o.sets.clone()));
        let mut sets = changed.unwrap_or_else(|| if data.version < 3 { vec![w3abdhqtu_rs::Set { flags: 0, mods: Vec::new() }] } else { Vec::new() });
        // A string of the map's it names copied too, a new one each: the
        // copy's own, as World Editor makes them.
        let strings_before = self.placed.strings.clone();
        if let Some((mut src, bom)) = self.placed.strings.clone() {
            let mut next = wts_rs::parse(&src).strings.iter().filter_map(|e| e.id).max().map_or(0, |n| n + 1);
            let mut copied = false;
            for m in sets.iter_mut().flat_map(|s| s.mods.iter_mut()) {
                let w3abdhqtu_rs::Value::Str(v) = &m.value else { continue };
                let Some(n) = v.strip_prefix("TRIGSTR_").and_then(|n| n.parse::<u32>().ok()) else { continue };
                let doc = wts_rs::parse(&src);
                let Some(text) = doc.get(n).map(|e| e.text.clone()) else { continue };
                let Ok(edits) = wts_rs::set_string(&doc, next, &text) else { continue };
                src = wts_rs::apply(&src, &edits);
                m.value = w3abdhqtu_rs::Value::Str(format!("TRIGSTR_{next:03}"));
                next += 1;
                copied = true;
            }
            if copied {
                self.placed.strings = Some((src, bom));
                self.strings_changed();
            }
        }
        data.customs.push(w3abdhqtu_rs::Object { base: base_code, id, sets });
        self.placed.objects.insert(kind, data);
        let snapshot = if self.placed.strings != strings_before {
            crate::edit::Snapshot::Many(vec![crate::edit::Snapshot::Strings(strings_before), crate::edit::Snapshot::Objects(kind, before)])
        } else {
            crate::edit::Snapshot::Objects(kind, before)
        };
        self.edits.record(snapshot);
        let mut out = vec![self.object_list(k), self.object_fields(k, &id.to_string())];
        out.extend(self.object_types_changed(kind));
        out.push(ServerMsg::State(self.state(String::new())));
        out
    }

    /// Object `id` of `kind` taken away (one of the map's), or its
    /// changes (one of the game's): the game's again.
    pub fn object_delete(&mut self, k: u8, id: &str) -> Vec<ServerMsg> {
        let Some(kind) = self.object_kind(k) else { return vec![ServerMsg::State(self.state(format!("no kind {k}")))] };
        let Some(code) = parser_rs::bin::Rawcode::parse(id) else { return vec![ServerMsg::State(self.state(format!("{id} isn't an id")))] };
        self.objects_edit(k, kind, |data, _| {
            match crate::objdata::find(data, code) {
                Some((true, i)) => {
                    data.customs.remove(i);
                }
                Some((false, i)) => {
                    data.originals.remove(i);
                }
                None => return Err(format!("{id} has no changes to take out.")),
            }
            Ok(String::new())
        })
    }

    /// The map's sounds (`war3map.w3s`), as the page lists them.
    pub fn sounds_msg(&self) -> ServerMsg {
        ServerMsg::Sounds(self.placed.sounds.as_ref().map(|s| s.sounds.clone()).unwrap_or_default())
    }

    /// A sound edit: its state recorded for undo, `change` made (refused,
    /// said, when it errs), the sounds again. A map without
    /// `war3map.w3s` gets one.
    fn sounds_edit(&mut self, change: impl FnOnce(&mut w3s_rs::Sounds) -> Result<String, String>) -> Vec<ServerMsg> {
        let sounds = self.placed.sounds.get_or_insert_with(w3s_rs::Sounds::new);
        let before = sounds.sounds.clone();
        let message = match change(sounds) {
            Ok(said) => {
                self.edits.record(crate::edit::Snapshot::Sounds(before));
                said
            }
            Err(message) => {
                sounds.sounds = before;
                message
            }
        };
        vec![self.sounds_msg(), ServerMsg::State(self.state(message))]
    }

    /// Whether the game's files or the map's have `file`; said when not.
    fn sound_file_note(&self, file: &str) -> String {
        if self.game.read(file).is_some() { String::new() } else { format!("{file} isn't in the game's files or the map's.") }
    }

    /// A sound of `file` (a path in the game's files or the map's) made
    /// as World Editor makes one ([`w3s_rs::Sound::new`]: 3D, stopping out
    /// of range, the rest its label's defaults), its variable `gg_snd_`
    /// and the file's name (`_2` ... when that's taken).
    pub fn sound_new(&mut self, file: &str) -> Vec<ServerMsg> {
        let file = file.trim().replace('/', "\\");
        if file.is_empty() {
            return vec![ServerMsg::State(self.state("A sound needs a file.".to_string()))];
        }
        let note = self.sound_file_note(&file);
        self.sounds_edit(|sounds| {
            let stem = file.rsplit('\\').next().unwrap_or(&file);
            let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
            let base: String = format!("gg_snd_{}", stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>());
            let taken = |n: &str| sounds.sounds.iter().any(|s| s.name.eq_ignore_ascii_case(n));
            let name = if taken(&base) { (2..).map(|k| format!("{base}_{k}")).find(|n| !taken(n)).unwrap() } else { base };
            sounds.sounds.push(w3s_rs::Sound::new(name, file.clone()));
            Ok(note)
        })
    }

    /// Sound `index`'s `field` set to `value`: `name` (its variable:
    /// `gg_snd_` and a name, no other sound's), `file`, `eax` (a label of
    /// `UI\\SoundInfo\\EAXDefs.slk`), `flags` (a number:
    /// `w3s_rs::FLAGS`' bits), or a number of `w3s_rs::Sound::numbers`
    /// (empty: "use default"; `volume` 0 to 127, `channel` one of
    /// `w3s_rs::CHANNELS`, a distance, fade or cone angle not below 0,
    /// `pitch` over 0). An undo each.
    pub fn sound_set(&mut self, index: u32, field: &str, value: &str) -> Vec<ServerMsg> {
        let note = if field == "file" { self.sound_file_note(value.trim()) } else { String::new() };
        let (field, value) = (field.to_string(), value.trim().to_string());
        self.sounds_edit(move |sounds| {
            let i = index as usize;
            if i >= sounds.sounds.len() {
                return Err(format!("no sound {index}"));
            }
            let bad = || format!("A sound's {} can't be {value:?}.", field.replace('_', " "));
            match field.as_str() {
                "name" => {
                    let rest = value.strip_prefix("gg_snd_").unwrap_or("");
                    let fine = !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !rest.ends_with('_');
                    if !fine {
                        return Err(format!("A sound's name is gg_snd_ and letters, digits, underscores (not last): not {value:?}."));
                    }
                    if sounds.sounds.iter().enumerate().any(|(k, s)| k != i && s.name.eq_ignore_ascii_case(&value)) {
                        return Err(format!("Another sound is {value}."));
                    }
                    sounds.sounds[i].name = value.clone();
                }
                "file" if !value.is_empty() => sounds.sounds[i].file = value.replace('/', "\\"),
                "eax" if !value.is_empty() => sounds.sounds[i].eax = value.clone(),
                "flags" => sounds.sounds[i].flags = value.parse::<u32>().map_err(|_| bad())?,
                _ => {
                    let s = &mut sounds.sounds[i];
                    let Some((_, kind)) = s.numbers().into_iter().find(|(n, _)| *n == field) else { return Err(bad()) };
                    let v = if value.is_empty() {
                        match kind {
                            w3s_rs::Value::Int(_) => w3s_rs::Value::Int(w3s_rs::DEFAULT_INT),
                            w3s_rs::Value::Real(_) => w3s_rs::Value::Real(w3s_rs::DEFAULT_REAL),
                        }
                    } else {
                        let x: f64 = value.parse().map_err(|_| bad())?;
                        let fine = x.is_finite()
                            && match field.as_str() {
                                "volume" | "cone_outside_volume" => (0.0..=127.0).contains(&x),
                                "channel" => w3s_rs::CHANNELS.iter().any(|(c, _)| *c as f64 == x),
                                "pitch" => x > 0.0,
                                "cone_x" | "cone_y" | "cone_z" => true,
                                _ => x >= 0.0,
                            };
                        match kind {
                            w3s_rs::Value::Int(_) if fine && x.fract() == 0.0 => w3s_rs::Value::Int(x as i32),
                            w3s_rs::Value::Real(_) if fine => w3s_rs::Value::Real(x as f32),
                            _ => return Err(bad()),
                        }
                    };
                    s.set_number(&field, v);
                }
            }
            Ok(note)
        })
    }

    /// Sound `index` taken away.
    pub fn sound_delete(&mut self, index: u32) -> Vec<ServerMsg> {
        self.sounds_edit(|sounds| {
            if (index as usize) >= sounds.sounds.len() {
                return Err(format!("no sound {index}"));
            }
            sounds.sounds.remove(index as usize);
            Ok(String::new())
        })
    }

    /// A sound file's bytes, for the page to play: through the layers (a
    /// `.wav` the game has as `.flac` or `.mp3` too); empty, and why, when
    /// none has it.
    pub fn sound_data(&self, file: &str) -> ServerMsg {
        let file = file.replace('/', "\\");
        let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s);
        let found = std::iter::once(file.clone()).chain(["flac", "mp3", "wav"].iter().map(|e| format!("{stem}.{e}"))).find_map(|n| self.game.read(&n).map(|(b, _)| b));
        match found {
            Some(bytes) => ServerMsg::SoundData { file, bytes, error: String::new() },
            None => ServerMsg::SoundData { error: format!("{file} isn't in the game's files or the map's."), file, bytes: Vec::new() },
        }
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
                list.push(doo_rs::Doodad { placement, flags: 2, life: 100, item_table: -1, item_sets: Vec::new(), number, reforged: Default::default() });
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

    /// Thing `index` of `file`'s fields, as the panel shows them: where
    /// (`x`, `y`, a doodad's `z`, the game's units), `facing` (degrees),
    /// `scale_x` ...; a doodad's `variation`, `state` (its flags: 0
    /// invisible and walkable, 1 visible and walkable, 2 normal), a
    /// destructable's `life` (percent); a unit's, building's or start
    /// location's `player` (0 Player 1); a unit's or building's `hp`
    /// (percent) and `mana` (-1 its type's), `target_acquisition` (-1
    /// normal, -2 camp), `color` (-1 its owner's); a hero's (an id of a
    /// capital letter first, as the game tells them) `level`,
    /// `strength`, `agility`, `intelligence` (0 its type's); a gold
    /// mine's (an ability `Agld`) `gold`. None for no such thing.
    pub fn thing(&self, file: u8, index: u32) -> ThingMsg {
        let fields = self.thing_fields(file, index).unwrap_or_default();
        ThingMsg { file, index, fields: fields.into_iter().map(|(n, v)| (n.to_string(), v)).collect() }
    }

    fn thing_fields(&self, file: u8, index: u32) -> Option<Vec<(&'static str, f32)>> {
        let i = index as usize;
        let p = if file == 0 { &self.placed.doodads.as_ref()?.doodads.get(i)?.placement } else { &self.placed.units.as_ref()?.units.get(i)?.placement };
        let mut out = vec![("x", p.x), ("y", p.y)];
        if file == 0 {
            out.push(("z", p.z));
        }
        out.push(("facing", (p.angle.to_degrees() * 100.0).round() / 100.0));
        out.extend([("scale_x", p.scale[0]), ("scale_y", p.scale[1]), ("scale_z", p.scale[2])]);
        let id = p.id.to_string();
        let types = self.types.as_ref();
        if file == 0 {
            let d = &self.placed.doodads.as_ref()?.doodads[i];
            out.extend([("variation", d.placement.variation as f32), ("state", d.flags as f32)]);
            if types.is_some_and(|t| t.has(w3abdhqtu_rs::Kind::Destructable, &id)) {
                out.push(("life", d.life as f32));
            }
            return Some(out);
        }
        let u = &self.placed.units.as_ref()?.units[i];
        let item = id == "iDNR" || types.is_some_and(|t| t.has(w3abdhqtu_rs::Kind::Item, &id));
        if item {
            return Some(out);
        }
        out.push(("player", u.player as f32));
        if id == crate::placed::START_LOCATION {
            return Some(out);
        }
        out.extend([("hp", u.hp as f32), ("mana", u.mana as f32), ("target_acquisition", u.target_acquisition), ("color", u.color as f32)]);
        if id.starts_with(|c: char| c.is_ascii_uppercase()) {
            out.extend([("level", u.level as f32), ("strength", u.stats[0] as f32), ("agility", u.stats[1] as f32), ("intelligence", u.stats[2] as f32)]);
        }
        let gold_mine = types.and_then(|t| t.get(w3abdhqtu_rs::Kind::Unit, &id, "abilList")).is_some_and(|a| a.split(',').any(|a| a.trim() == "Agld"));
        if gold_mine {
            out.push(("gold", u.gold as f32));
        }
        Some(out)
    }

    /// Thing `index` of `file`'s `field` ([`thing`](Self::thing)'s) set
    /// to `value`, if it's one it has and in its range; an undo each.
    pub fn thing_set(&mut self, file: u8, index: u32, field: &str, value: f32) -> Vec<ServerMsg> {
        let has = self.thing_fields(file, index).is_some_and(|f| f.iter().any(|(n, _)| *n == field));
        let variations = (|| {
            let id = self.placed.doodads.as_ref()?.doodads.get(index as usize)?.placement.id.to_string();
            let k = if self.types.as_ref()?.has(w3abdhqtu_rs::Kind::Destructable, &id) { w3abdhqtu_rs::Kind::Destructable } else { w3abdhqtu_rs::Kind::Doodad };
            self.types.as_ref()?.number(k, &id, "numVar")
        })()
        .unwrap_or(1.0)
        .max(1.0);
        let whole = value.fract() == 0.0;
        let fine = value.is_finite()
            && match field {
                "scale_x" | "scale_y" | "scale_z" => value > 0.0,
                "variation" => whole && value >= 0.0 && value < variations,
                "state" => whole && (0.0..=2.0).contains(&value),
                "life" => whole && (0.0..=100.0).contains(&value),
                "player" => whole && (0.0..=27.0).contains(&value),
                "hp" => whole && (value == -1.0 || (1.0..=100.0).contains(&value)),
                "mana" => whole && value >= -1.0,
                "target_acquisition" => value == -1.0 || value == -2.0 || value >= 0.0,
                "color" => whole && (-1.0..=23.0).contains(&value),
                "level" => whole && value >= 1.0,
                "strength" | "agility" | "intelligence" | "gold" => whole && value >= 0.0,
                _ => true,
            };
        if !has {
            let what = if file == 0 { "war3map.doo" } else { "war3mapUnits.doo" };
            return vec![ServerMsg::State(self.state(format!("Thing {index} of {what} has no {}.", field.replace('_', " "))))];
        }
        if !fine {
            return vec![ServerMsg::State(self.state(format!("A thing's {} can't be {value}.", field.replace('_', " "))))];
        }
        let field = field.to_string();
        self.things_edit(file, move |files| {
            let i = index as usize;
            let p = placement(files, file, index)?;
            match field.as_str() {
                "x" => p.x = value,
                "y" => p.y = value,
                "z" => p.z = value,
                "facing" => p.angle = value.to_radians().rem_euclid(std::f32::consts::TAU),
                "scale_x" => p.scale[0] = value,
                "scale_y" => p.scale[1] = value,
                "scale_z" => p.scale[2] = value,
                "variation" => p.variation = value as u32,
                _ if file == 0 => {
                    let d = &mut files.doodads.as_mut().unwrap().doodads[i];
                    match field.as_str() {
                        "state" => d.flags = value as u8,
                        _ => d.life = value as u8,
                    }
                }
                _ => {
                    let u = &mut files.units.as_mut().unwrap().units[i];
                    match field.as_str() {
                        "player" => u.player = value as u32,
                        "hp" => u.hp = value as i32,
                        "mana" => u.mana = value as i32,
                        "target_acquisition" => u.target_acquisition = value,
                        "color" => u.color = value as i32,
                        "level" => u.level = value as u32,
                        "strength" => u.stats[0] = value as u32,
                        "agility" => u.stats[1] = value as u32,
                        "intelligence" => u.stats[2] = value as u32,
                        _ => u.gold = value as u32,
                    }
                }
            }
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

    /// The region over `(x, y)` (the game's units) drawn last: the one on
    /// top.
    fn region_at(&self, x: f32, y: f32) -> Option<u32> {
        let regions = &self.placed.regions.as_ref()?.regions;
        regions.iter().rposition(|r| x >= r.left && x <= r.right && y >= r.bottom && y <= r.top).map(|i| i as u32)
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
            let region = self.region_at(wx, wy);
            return PickedMsg { text: format!("({wx:.0}, {wy:.0}, {wz:.0}){}", in_regions(wx, wy)), ground, region, ..Default::default() };
        };
        let id = t.id.to_string();
        let name = if id == crate::placed::START_LOCATION {
            // World Editor's name for it.
            wc3_server::names::plain(&self.names.resolve("WESTRING_STARTLOCATION"))
        } else {
            self.names.objects.get(t.kind, &id).map(|n| wc3_server::names::plain(&n.shown())).unwrap_or_else(|| id.clone())
        };
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
            region: ground.and_then(|_| self.region_at(t.world[0], t.world[1])),
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

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
