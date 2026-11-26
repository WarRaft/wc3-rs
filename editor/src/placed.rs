//! What a map places, as the page draws it: the doodads and destructables
//! of `war3map.doo`, the units and items of `war3mapUnits.doo`, each with
//! its type's model, as HiveWE finds it (`doodads.ixx`, `units.ixx`):
//!
//! - a doodad's `file`, with its variation added to the name (even after
//!   an extension) when its type has more than one (`numVar`), else the
//!   file without one;
//! - a destructable's `file`, its variation in place of the extension;
//!   its `texFile` in place of the model's replaceable texture `texID`
//!   (trees: 31);
//! - a unit's `file`, an item's `file` (`Units\ItemFunc.txt`).
//!
//! A path's `.mdl` is read as `.mdx`, as the game does. The models are
//! drawn as their geosets are in the file (no bones yet: step 8), those
//! hidden at the start of `Stand` left out; a geoset's material's layers
//! each with its texture (its biggest mip of 256 or less), or the
//! player's colour for replaceable 1 (the average of
//! `ReplaceableTextures\TeamColor\TeamColor<nn>.blp`). A doodad stands at
//! its height, a unit or item on the ground (or the water) under it.
//!
//! [`Objects::pick`] says what a ray from the camera meets first: each
//! thing's box (its model's vertices), turned and scaled as it stands.

use std::collections::HashMap;

use parser_rs::bin::Rawcode;
use w3abdhqtu_rs::Kind;
use wc3_server::game::Game;

use crate::terrain::Draw;
use crate::types::Types;

/// A texture a layer is drawn with.
pub mod texture {
    /// The player's colour (replaceable 1).
    pub const TEAM_COLOR: i32 = -1;
    /// Not drawn: team glow (replaceable 2), or a replaceable texture
    /// nothing gives.
    pub const NONE: i32 = -2;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// An index into [`Objects::textures`], or [`texture`]'s.
    pub texture: i32,
    pub filter_mode: u32,
    pub shading: u32,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Geoset {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u16>,
    pub layers: Vec<Layer>,
}

/// A model and where it stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub path: String,
    pub geosets: Vec<Geoset>,
    /// The box of its vertices, the model's units.
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Each one placed, [`INSTANCE`] floats: x, y, z (cells, layers),
    /// angle (radians), scale x, y, z (of the model's units to cells),
    /// tint r, g, b, the player's colour r, g, b (0-1), kind (0 doodad, 1
    /// destructable, 2 unit, 3 item), its index in its file, 0.
    pub instances: Vec<f32>,
    pub error: String,
}

/// Floats per instance.
pub const INSTANCE: usize = 16;

/// A texture, RGBA, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Texture {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub error: String,
}

/// One thing placed, for [`Objects::pick`] and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct Thing {
    pub kind: Kind,
    pub id: Rawcode,
    /// Its index in its file (`war3map.doo` or `war3mapUnits.doo`).
    pub index: usize,
    pub player: Option<u32>,
    /// Where it is, in the game's units.
    pub world: [f32; 3],
    pub model: usize,
    pub instance: usize,
}

/// What a map places, drawn.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    pub models: Vec<Model>,
    pub textures: Vec<Texture>,
    pub things: Vec<Thing>,
    pub notes: Vec<String>,
    /// The models read, by path and replaceable texture (`None`: not
    /// there); the textures read, by path; the players' colours.
    model_of: HashMap<ModelKey, Option<usize>>,
    texture_of: HashMap<String, usize>,
    colors: HashMap<u32, [f32; 3]>,
}

/// `path` as the game reads a model: `.mdl` as `.mdx`, `.mdx` added to a
/// name without an extension.
pub fn model_path(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    match file.rfind('.') {
        Some(dot) if file[dot..].eq_ignore_ascii_case(".mdl") => format!("{}.mdx", &path[..path.len() - 4]),
        Some(_) => path.to_string(),
        None => format!("{path}.mdx"),
    }
}

fn strip_extension(path: &str) -> &str {
    let file_start = path.rfind(['\\', '/']).map_or(0, |i| i + 1);
    match path[file_start..].rfind('.') {
        Some(dot) => &path[..file_start + dot],
        None => path,
    }
}

/// A type's model as the game finds it ([`model_of`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOf {
    pub kind: Kind,
    /// The paths to try, the first first (a doodad's variation, then its
    /// file alone); `None` when its type names no model.
    pub paths: Option<Vec<String>>,
    /// A replaceable texture's id and its stand-in (a destructable's
    /// `texID` and `texFile`).
    pub replaceable: Option<(u32, String)>,
}

/// The model of type `id` placed with `variation`: in `war3map.doo` a
/// doodad or a destructable, in `war3mapUnits.doo` (`units`) a unit or
/// an item. `None` for a type `types` hasn't.
pub fn model_of(types: &Types, id: Rawcode, variation: u32, units: bool) -> Option<ModelOf> {
    let id = id.to_string();
    let kinds: [Kind; 2] = if units { [Kind::Unit, Kind::Item] } else { [Kind::Doodad, Kind::Destructable] };
    let kind = kinds.into_iter().find(|k| types.has(*k, &id))?;
    let file = types.get(kind, &id, "file");
    let vars = types.number(kind, &id, "numVar").unwrap_or(1.0) as u32;
    let paths = file.map(|file| match kind {
        // The variation after the name, even after an extension.
        Kind::Doodad if vars > 1 => vec![format!("{file}{variation}.mdx"), model_path(file)],
        // The variation in place of the extension.
        Kind::Destructable if vars > 1 => vec![format!("{}{variation}.mdx", strip_extension(file))],
        _ => vec![model_path(file)],
    });
    let replaceable = (kind == Kind::Destructable)
        .then(|| {
            types
                .number(kind, &id, "texID")
                .zip(types.get(kind, &id, "texFile").filter(|t| !t.eq_ignore_ascii_case("none")))
                .map(|(n, t)| (n as u32, crate::tileset::with_extension(t)))
        })
        .flatten();
    Some(ModelOf { kind, paths, replaceable })
}

/// A model by its path (lower case) and the stand-in for its
/// replaceable texture, if its type gives one.
type ModelKey = (String, Option<(u32, String)>);

/// A thing to draw: its type's model (path, and a replaceable texture's
/// stand-in), where and how it stands.
struct Place {
    kind: Kind,
    id: Rawcode,
    index: usize,
    paths: Vec<String>,
    replaceable: Option<(u32, String)>,
    world: [f32; 3],
    on_ground: bool,
    angle: f32,
    scale: [f32; 3],
    tint: [f32; 3],
    player: Option<u32>,
}

/// What a map places, its files read: `war3map.doo` (doodads and
/// destructables) and `war3mapUnits.doo` (units and items); what the
/// editor changes.
#[derive(Debug, Clone, Default)]
pub struct Files {
    pub doodads: Option<doo_rs::Doodads>,
    pub units: Option<doo_rs::Units>,
    /// The regions (`war3map.w3r`) and the pathing (`war3map.wpm`).
    pub regions: Option<w3r_rs::Regions>,
    pub pathing: Option<wpm_rs::Pathing>,
    /// The layout the map's `war3map.w3i` says (1.32's skins), for new
    /// files.
    pub layout: doo_rs::Layout,
}

impl Files {
    /// The files through `game`, and what couldn't be read.
    pub fn read(game: &Game) -> (Files, Vec<String>) {
        let mut notes = Vec::new();
        let layout = game.read("war3map.w3i").and_then(|(b, _)| doo_rs::Layout::from_w3i(&b));
        let doodads = game.read("war3map.doo").and_then(|(bytes, _)| {
            match layout.map_or_else(|| doo_rs::parse_doodads_any(&bytes), |l| doo_rs::parse_doodads(&bytes, l)) {
                Ok(d) => Some(d),
                Err(e) => {
                    notes.push(format!("Can't read war3map.doo: {e}"));
                    None
                }
            }
        });
        let units = game.read("war3mapUnits.doo").and_then(|(bytes, _)| {
            match layout.map_or_else(|| doo_rs::parse_units_any(&bytes), |l| doo_rs::parse_units(&bytes, l)) {
                Ok(u) => Some(u),
                Err(e) => {
                    notes.push(format!("Can't read war3mapUnits.doo: {e}"));
                    None
                }
            }
        });
        let regions = game.read("war3map.w3r").and_then(|(b, _)| match w3r_rs::parse(&b) {
            Ok(r) => Some(r),
            Err(e) => {
                notes.push(format!("Can't read war3map.w3r: {e}"));
                None
            }
        });
        let pathing = game.read("war3map.wpm").and_then(|(b, _)| match wpm_rs::parse(&b) {
            Ok(p) => Some(p),
            Err(e) => {
                notes.push(format!("Can't read war3map.wpm: {e}"));
                None
            }
        });
        let files = Files { doodads, units, regions, pathing, layout: layout.unwrap_or_default() };
        (files, notes)
    }
}

impl Objects {
    /// What `game` places on the terrain `draw` (heights for what stands
    /// on the ground), with `types`.
    pub fn load(game: &Game, types: &Types, draw: &Draw) -> Objects {
        let (files, notes) = Files::read(game);
        let mut out = Objects::default();
        out.place(game, types, draw, &files);
        out.notes.splice(0..0, notes);
        out
    }

    /// Everything placed again from `files` on `draw`: each thing where
    /// it is now, a model or texture not read yet read (the ones read
    /// kept, so a change costs what it changes).
    pub fn place(&mut self, game: &Game, types: &Types, draw: &Draw, files: &Files) {
        self.things.clear();
        self.notes.clear();
        for m in &mut self.models {
            m.instances.clear();
        }
        let mut places = Vec::new();
        let mut unknown: HashMap<String, usize> = HashMap::new();
        for (index, d) in files.doodads.iter().flat_map(|d| d.doodads.iter().enumerate()) {
            let p = &d.placement;
            let id = p.id.to_string();
            let Some(found) = model_of(types, p.id, p.variation, false) else {
                *unknown.entry(id).or_default() += 1;
                continue;
            };
            let Some(paths) = found.paths else { continue };
            let kind = found.kind;
            let tint = if kind == Kind::Doodad {
                let v = p.variation + 1;
                let c = |k: &str| types.number(Kind::Doodad, &id, &format!("vert{k}{v}")).unwrap_or(255.0) / 255.0;
                [c("R"), c("G"), c("B")]
            } else {
                let c = |k: &str| types.number(Kind::Destructable, &id, &format!("color{k}")).unwrap_or(255.0) / 255.0;
                [c("R"), c("G"), c("B")]
            };
            let scale = if kind == Kind::Doodad { types.number(Kind::Doodad, &id, "defScale").unwrap_or(1.0) } else { 1.0 };
            places.push(Place {
                kind,
                id: p.id,
                index,
                paths,
                replaceable: found.replaceable,
                world: [p.x, p.y, p.z],
                on_ground: false,
                angle: p.angle,
                scale: p.scale.map(|s| s * scale),
                tint,
                player: None,
            });
        }
        for (index, u) in files.units.iter().flat_map(|u| u.units.iter().enumerate()) {
            let p = &u.placement;
            let id = p.id.to_string();
            let Some(found) = model_of(types, p.id, 0, true) else {
                *unknown.entry(id).or_default() += 1;
                continue;
            };
            let (kind, Some(paths)) = (found.kind, found.paths) else { continue };
            let (scale, tint) = if kind == Kind::Unit {
                let c = |k: &str| types.number(kind, &id, k).unwrap_or(255.0) / 255.0;
                (types.number(kind, &id, "modelScale").unwrap_or(1.0), [c("red"), c("green"), c("blue")])
            } else {
                (types.number(kind, &id, "scale").unwrap_or(1.0), [1.0; 3])
            };
            places.push(Place {
                kind,
                id: p.id,
                index,
                paths,
                replaceable: None,
                world: [p.x, p.y, p.z],
                on_ground: true,
                angle: p.angle,
                scale: [scale; 3],
                tint,
                player: (kind == Kind::Unit).then_some(u.player),
            });
        }
        let mut unknown: Vec<_> = unknown.into_iter().collect();
        unknown.sort();
        if !unknown.is_empty() {
            let list: Vec<String> = unknown.iter().map(|(id, n)| if *n > 1 { format!("{id} ({n})") } else { id.clone() }).collect();
            self.notes.push(format!("Not drawn, types the game's tables and the map's object data don't have: {}", list.join(", ")));
        }

        let mut missing: Vec<String> = Vec::new();
        for place in places {
            let key = (place.paths[0].to_ascii_lowercase(), place.replaceable.clone());
            let model = match self.model_of.get(&key) {
                Some(m) => *m,
                None => {
                    let found = place.paths.iter().find_map(|p| game.read(p).map(|(b, _)| (p.clone(), b)));
                    let m = found.map(|(path, bytes)| {
                        let m = load_model(&path, &bytes, place.replaceable.as_ref(), game, &mut self.textures, &mut self.texture_of);
                        self.models.push(m);
                        self.models.len() - 1
                    });
                    self.model_of.insert(key, m);
                    m
                }
            };
            let Some(model) = model else {
                if !missing.contains(&place.paths[0]) {
                    missing.push(place.paths[0].clone());
                }
                continue;
            };
            let [x, y] = [(place.world[0] - draw.offset[0]) / 128.0, (place.world[1] - draw.offset[1]) / 128.0];
            let z = if place.on_ground { ground_at(draw, x, y) } else { place.world[2] / 128.0 };
            let team = match place.player {
                Some(player) => *self.colors.entry(player).or_insert_with(|| {
                    crate::textures::average(game, &format!("ReplaceableTextures\\TeamColor\\TeamColor{player:02}.blp"))
                        .map_or([0.5; 3], |c| c.map(|v| v as f32 / 255.0))
                }),
                None => [1.0; 3],
            };
            let kind = PLACED_CODES.iter().position(|k| *k == place.kind).unwrap_or(0) as f32;
            let m = &mut self.models[model];
            let instance = m.instances.len() / INSTANCE;
            let s = place.scale.map(|v| v / 128.0);
            m.instances.extend([x, y, z, place.angle, s[0], s[1], s[2], place.tint[0], place.tint[1], place.tint[2], team[0], team[1], team[2], kind, place.index as f32, 0.0]);
            self.things.push(Thing { kind: place.kind, id: place.id, index: place.index, player: place.player, world: place.world, model, instance });
        }
        if !missing.is_empty() {
            self.notes.push(format!("Models not in the game's files or the roots: {}", missing.join(", ")));
        }
    }

    /// What the ray from `origin` along `dir` (cells, layers) meets
    /// first, by the box of each thing's model.
    pub fn pick(&self, origin: [f32; 3], dir: [f32; 3]) -> Option<&Thing> {
        let mut best: Option<(f32, &Thing)> = None;
        for t in &self.things {
            let m = &self.models[t.model];
            let i = &m.instances[t.instance * INSTANCE..][..INSTANCE];
            // Into the model's space: moved, turned back, scaled back.
            let (sin, cos) = (-i[3]).sin_cos();
            let local = |v: [f32; 3], point: bool| -> [f32; 3] {
                let v = if point { [v[0] - i[0], v[1] - i[1], v[2] - i[2]] } else { v };
                let (x, y) = (v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos);
                [x / i[4], y / i[5], v[2] / i[6]]
            };
            let (o, d) = (local(origin, true), local(dir, false));
            let mut near = f32::NEG_INFINITY;
            let mut far = f32::INFINITY;
            for k in 0..3 {
                if d[k].abs() < 1e-9 {
                    if o[k] < m.min[k] || o[k] > m.max[k] {
                        far = f32::NEG_INFINITY;
                    }
                    continue;
                }
                let (a, b) = ((m.min[k] - o[k]) / d[k], (m.max[k] - o[k]) / d[k]);
                near = near.max(a.min(b));
                far = far.min(a.max(b));
            }
            if far >= near.max(0.0) && best.is_none_or(|(t0, _)| near.max(0.0) < t0) {
                best = Some((near.max(0.0), t));
            }
        }
        best.map(|(_, t)| t)
    }
}

/// The kinds by their code in an instance.
pub const PLACED_CODES: [Kind; 4] = [Kind::Doodad, Kind::Destructable, Kind::Unit, Kind::Item];

/// The ground's height (or the water's, over it) at `(x, y)` cells,
/// between its corners.
pub fn ground_at(draw: &Draw, x: f32, y: f32) -> f32 {
    let (w, h) = (draw.width as usize, draw.height as usize);
    if w < 2 || h < 2 {
        return 0.0;
    }
    let cx = x.clamp(0.0, w as f32 - 1.001);
    let cy = y.clamp(0.0, h as f32 - 1.001);
    let (ix, iy) = (cx as usize, cy as usize);
    let (fx, fy) = (cx - ix as f32, cy - iy as f32);
    let at = |i: usize, j: usize| {
        let k = j * w + i;
        let wet = draw.corners[k] & crate::terrain::corner::WATER != 0;
        if wet { draw.heights[k].max(draw.water[k]) } else { draw.heights[k] }
    };
    let bottom = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
    let top = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
    bottom * (1.0 - fy) + top * fy
}

fn load_model(
    path: &str,
    bytes: &[u8],
    replaceable: Option<&(u32, String)>,
    game: &Game,
    textures: &mut Vec<Texture>,
    texture_of: &mut HashMap<String, usize>,
) -> Model {
    let mut model = Model { path: path.to_string(), ..Default::default() };
    let m = match mdx_rs::parse(bytes) {
        Ok(m) => m,
        Err(e) => {
            model.error = format!("{path}: {e}");
            return model;
        }
    };
    let mut texture = |file: &str| -> i32 {
        let key = file.to_ascii_lowercase();
        if let Some(&i) = texture_of.get(&key) {
            return i as i32;
        }
        let t = match crate::textures::capped(game, file, 256) {
            Ok((width, height, rgba)) => Texture { path: file.to_string(), width, height, rgba, error: String::new() },
            Err(error) => Texture { path: file.to_string(), error, ..Default::default() },
        };
        textures.push(t);
        texture_of.insert(key, textures.len() - 1);
        textures.len() as i32 - 1
    };
    let texs: Vec<i32> = m
        .textures()
        .iter()
        .map(|t| match t.replaceable_id {
            0 => texture(&t.path.text()),
            1 => texture::TEAM_COLOR,
            id => match replaceable {
                Some((r, file)) if *r == id => texture(file),
                _ => texture::NONE,
            },
        })
        .collect();
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for (gi, g) in m.geosets().iter().enumerate() {
        if !m.shown(gi, "Stand") || g.uv_sets.is_empty() || g.uv_sets[0].len() != g.vertices.len() || g.normals.len() != g.vertices.len() {
            continue;
        }
        for v in &g.vertices {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let layers = m
            .materials()
            .get(g.material_id as usize)
            .map(|mat| {
                mat.layers
                    .iter()
                    .map(|l| Layer {
                        texture: texs.get(l.texture_id as usize).copied().unwrap_or(texture::NONE),
                        filter_mode: l.filter_mode,
                        shading: l.shading,
                        alpha: l.alpha,
                    })
                    .collect()
            })
            .unwrap_or_default();
        model.geosets.push(Geoset {
            positions: g.vertices.iter().flatten().copied().collect(),
            normals: g.normals.iter().flatten().copied().collect(),
            uvs: g.uv_sets[0].iter().flatten().copied().collect(),
            indices: g.faces.clone(),
            layers,
        });
    }
    // Nothing shown (a model of particles alone): nothing to draw yet,
    // and nothing to pick.
    if !model.geosets.is_empty() {
        (model.min, model.max) = (min, max);
    }
    model
}

#[cfg(test)]
#[path = "placed.test.rs"]
mod tests;
