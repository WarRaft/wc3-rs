//! Changing the terrain with brushes, as World Editor's terrain palette
//! does: a tool, a brush (a circle `size` cells across, its strength),
//! laid on the ground where the ray from the camera meets it, again and
//! again while the mouse is held (a stroke). Every stroke can be undone
//! and redone. Heights are the points' (`war3map.w3e`: 512 a layer).
//!
//! - **Raise, Lower**: the ground up or down, most in the middle (a
//!   cosine falloff), `strength` layers a step there.
//! - **Plateau**: to the height the stroke started on.
//! - **Smooth**: each point toward the average of it and its neighbours.
//! - **Texture**: the points' ground texture, a tile id of the palette's
//!   (added to the terrain's list when it isn't there and there's room),
//!   a variation the game picks (mostly the first).
//! - **Cliff up, Cliff down**: the points' cliff layer to the one the
//!   stroke started on, one up or down, in the cliff texture `value`.
//! - **Water, Dry**: water shown or not on the points.
//! - **Pathing on, Pathing off** (`war3map.wpm`, four cells a terrain
//!   cell): the brush's `flags` (2 no walking, 4 no flying, 8 no
//!   building) set or cleared on the cells in it.

use parser_rs::bin::Rawcode;
use w3e_rs::{Point, Terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Raise,
    Lower,
    Plateau,
    Smooth,
    Texture,
    CliffUp,
    CliffDown,
    Water,
    Dry,
    PathingOn,
    PathingOff,
    /// A ramp down a cliff, toward the brush's middle (HiveWE's
    /// `CliffOperator::update_ramp`).
    Ramp,
}

/// The tools by their code on the wire.
pub const TOOLS: [Tool; 12] = [
    Tool::Raise,
    Tool::Lower,
    Tool::Plateau,
    Tool::Smooth,
    Tool::Texture,
    Tool::CliffUp,
    Tool::CliffDown,
    Tool::Water,
    Tool::Dry,
    Tool::PathingOn,
    Tool::PathingOff,
    Tool::Ramp,
];

#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub tool: Tool,
    /// Across, in cells.
    pub size: f32,
    /// Raise and Lower: layers a step in the middle; Smooth: how far
    /// toward the average (0-1).
    pub strength: f32,
    /// Texture: the tile id; Cliff: the cliff texture (an index into the
    /// terrain's, or -1 to keep each point's).
    pub texture: Option<Rawcode>,
    pub cliff: i32,
    /// Pathing: the flags set or cleared.
    pub flags: u8,
}

/// A stroke going on.
#[derive(Debug, Clone)]
struct Stroke {
    before: Snapshot,
    /// The height (Plateau) or layer (Cliff) the stroke started on.
    start: Option<i32>,
}

/// What an edit changed, as it was before: the terrain's points and
/// ground list, the doodads, the units.
#[derive(Debug, Clone)]
pub enum Snapshot {
    Terrain(Vec<Point>, Vec<Rawcode>),
    Doodads(Vec<doo_rs::Doodad>),
    Units(Vec<doo_rs::Unit>),
    Regions(Vec<w3r_rs::Region>),
    Cameras(Vec<w3c_rs::Camera>),
    Sounds(Vec<w3s_rs::Sound>),
    /// A kind's object data, whole (`None`: the map had no file).
    Objects(w3abdhqtu_rs::Kind, Option<w3abdhqtu_rs::ObjectData>),
    /// The map's strings' text, whole (`placed::Files::strings`).
    Strings(Option<(String, bool)>),
    /// The imported files' list and the files to write or take out.
    Imports(Option<imp_rs::Imports>, std::collections::BTreeMap<String, Option<Vec<u8>>>),
    /// The map's info (`war3map.w3i`), whole.
    Info(Box<Option<w3i_rs::Info>>),
    /// Several, undone as one (an object pasted and the strings it
    /// copied).
    Many(Vec<Snapshot>),
    Pathing(Vec<u8>),
    /// The whole terrain (the tile setter's: its list, cliffs, tileset).
    Whole(Box<Terrain>),
}

/// Which files have changes not saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dirty {
    pub terrain: bool,
    pub doodads: bool,
    pub units: bool,
    pub regions: bool,
    pub pathing: bool,
    /// `war3map.w3c`.
    pub cameras: bool,
    /// `war3map.w3s`.
    pub sounds: bool,
    /// The object data's files: a bit each, by `objdata::KINDS`' order.
    pub objects: u8,
    /// `war3map.wts`.
    pub strings: bool,
    /// `war3map.imp` and the imported files.
    pub imports: bool,
    /// `war3map.w3i` (its playable area, its tileset) and `war3map.shd`.
    pub info: bool,
    pub shadows: bool,
}

impl Dirty {
    pub fn any(self) -> bool {
        self.terrain || self.doodads || self.units || self.regions || self.pathing || self.cameras || self.sounds || self.objects != 0 || self.strings || self.imports || self.info || self.shadows
    }
}

/// What an undo or redo works on.
pub struct Target<'a> {
    pub terrain: &'a mut Terrain,
    pub files: &'a mut crate::placed::Files,
    /// The map's info (`war3map.w3i`).
    pub info: &'a mut Option<w3i_rs::Info>,
}

/// The changes: undo and redo, what's unsaved.
#[derive(Debug, Default)]
pub struct Edits {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    stroke: Option<Stroke>,
    /// Changes since the last save (or the files as read).
    pub dirty: Dirty,
    seed: u32,
}

impl Edits {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Lays `brush` at `(x, y)` (cells) on `t`, a step of the stroke
    /// (`start`: its first). The error says why nothing changed (the
    /// terrain's list of textures full).
    pub fn apply(&mut self, t: &mut Terrain, x: f32, y: f32, brush: &Brush, start: bool) -> Result<(), String> {
        if start || self.stroke.is_none() {
            self.end();
            self.stroke = Some(Stroke { before: Snapshot::Terrain(t.points.clone(), t.ground.clone()), start: None });
        }
        let v = t.flags_version();
        let (w, h) = (t.width as i64, t.height as i64);
        let r = (brush.size / 2.0).max(0.5);
        let at = |px: i64, py: i64| (py * w + px) as usize;
        let nearest = (x.round() as i64).clamp(0, w - 1);
        let nearest_y = (y.round() as i64).clamp(0, h - 1);
        let stroke = self.stroke.as_mut().unwrap();
        let start_value = *stroke.start.get_or_insert_with(|| {
            let p = &t.points[at(nearest, nearest_y)];
            match brush.tool {
                Tool::Plateau => p.height as i32,
                Tool::CliffUp => (p.layer_height() as i32 + 1).min(15),
                Tool::CliffDown => (p.layer_height() as i32 - 1).max(0),
                _ => 0,
            }
        });
        let texture = match (brush.tool, brush.texture) {
            (Tool::Texture, Some(id)) => Some(match t.ground.iter().position(|g| *g == id) {
                Some(i) => i,
                None if t.ground.len() < v.textures() => {
                    t.ground.push(id);
                    t.ground.len() - 1
                }
                None => {
                    let message = format!("The terrain has {} ground textures, as many as it can: {id} can't be added.", t.ground.len());
                    return Err(self.failed(start, message));
                }
            }),
            (Tool::Texture, None) => return Err(self.failed(start, "No texture picked.".into())),
            _ => None,
        };
        let before = t.points.clone();
        // The cliff brush's corners, to pull those around them after.
        let mut seeds = Vec::new();
        let (x0, x1) = (((x - r).floor() as i64).max(0), ((x + r).ceil() as i64).min(w - 1));
        let (y0, y1) = (((y - r).floor() as i64).max(0), ((y + r).ceil() as i64).min(h - 1));
        if brush.tool == Tool::Ramp {
            // Each corner under the brush: a ramp down from it, toward the
            // brush's middle.
            for py in y0..=y1 {
                for px in x0..=x1 {
                    if ((px as f32 - x).powi(2) + (py as f32 - y).powi(2)).sqrt() > r {
                        continue;
                    }
                    let sign = |d: f32| if d > 0.0 { 1 } else if d < 0.0 { -1 } else { 0 };
                    ramp_at(t, px, py, sign(x - px as f32), sign(y - py as f32));
                }
            }
            self.dirty.terrain = true;
            return Ok(());
        }
        for py in y0..=y1 {
            for px in x0..=x1 {
                let d = ((px as f32 - x).powi(2) + (py as f32 - y).powi(2)).sqrt();
                if d > r {
                    continue;
                }
                let falloff = 0.5 + 0.5 * (std::f32::consts::PI * d / r).cos();
                let i = at(px, py);
                let p = &mut t.points[i];
                let step = (brush.strength * 512.0 * falloff).round() as i32;
                match brush.tool {
                    Tool::Raise => p.height = (p.height as i32 + step).clamp(0, 0xFFFF) as u16,
                    Tool::Lower => p.height = (p.height as i32 - step).clamp(0, 0xFFFF) as u16,
                    Tool::Plateau => p.height = start_value.clamp(0, 0xFFFF) as u16,
                    Tool::Smooth => {
                        let mut sum = 0i64;
                        let mut n = 0i64;
                        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                            let (qx, qy) = (px + dx, py + dy);
                            if qx >= 0 && qy >= 0 && qx < w && qy < h {
                                sum += before[at(qx, qy)].height as i64;
                                n += 1;
                            }
                        }
                        let avg = sum as f32 / n as f32;
                        let k = (brush.strength * falloff).clamp(0.0, 1.0);
                        p.height = (p.height as f32 + (avg - p.height as f32) * k).round() as u16;
                    }
                    Tool::Texture => {
                        p.set_texture(v, texture.unwrap() as u16);
                        // Mostly the first variation, now and then another.
                        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let roll = (self.seed >> 16) % 100;
                        p.set_ground_variation(if roll < 85 { 0 } else { (roll % 16) as u8 });
                    }
                    Tool::CliffUp | Tool::CliffDown => {
                        // A corner of another layer: no ramp's any more.
                        if p.layer_height() as i32 != start_value {
                            p.set_ramp(v, false);
                        }
                        p.set_layer_height(start_value as u8);
                        if brush.cliff >= 0 && (brush.cliff as usize) < t.cliffs.len() {
                            p.set_cliff_texture(brush.cliff as u8);
                        }
                        seeds.push((px, py));
                    }
                    Tool::Water => p.set_has_water(v, true),
                    Tool::Dry => p.set_has_water(v, false),
                    // Not the terrain's: `pathing`; the ramps above.
                    Tool::PathingOn | Tool::PathingOff | Tool::Ramp => {}
                }
            }
        }
        if !seeds.is_empty() {
            let cliff = (brush.cliff >= 0 && (brush.cliff as usize) < t.cliffs.len()).then_some(brush.cliff as u8);
            pull_nearby(t, &seeds, cliff);
        }
        self.dirty.terrain = true;
        Ok(())
    }

    /// An edit of the things placed about to be made: what it changes, as
    /// it is, to undo it.
    pub fn record(&mut self, before: Snapshot) {
        self.end();
        self.mark(&before);
        self.undo.push(before);
        self.redo.clear();
    }

    /// The files `s` changes have changes not saved.
    fn mark(&mut self, s: &Snapshot) {
        match s {
            Snapshot::Terrain(..) | Snapshot::Whole(_) => self.dirty.terrain = true,
            Snapshot::Doodads(_) => self.dirty.doodads = true,
            Snapshot::Units(_) => self.dirty.units = true,
            Snapshot::Regions(_) => self.dirty.regions = true,
            Snapshot::Cameras(_) => self.dirty.cameras = true,
            Snapshot::Sounds(_) => self.dirty.sounds = true,
            Snapshot::Objects(kind, _) => self.dirty.objects |= crate::objdata::bit(*kind),
            Snapshot::Strings(_) => self.dirty.strings = true,
            Snapshot::Imports(..) => self.dirty.imports = true,
            Snapshot::Info(_) => self.dirty.info = true,
            Snapshot::Pathing(_) => self.dirty.pathing = true,
            Snapshot::Many(all) => all.iter().for_each(|s| self.mark(s)),
        }
    }

    /// Lays a pathing brush at `(x, y)` (terrain cells) on `p`: its
    /// `flags` set (`PathingOn`) or cleared on the cells within it.
    pub fn pathing(&mut self, p: &mut wpm_rs::Pathing, x: f32, y: f32, brush: &Brush, start: bool) {
        if start || self.stroke.is_none() {
            self.end();
            self.stroke = Some(Stroke { before: Snapshot::Pathing(p.cells.clone()), start: None });
        }
        // Four pathing cells a terrain cell; a cell's middle at +0.5.
        let (cx, cy, r) = (x * 4.0, y * 4.0, (brush.size * 2.0).max(0.5));
        let (w, h) = (p.width as i64, p.height as i64);
        for py in ((cy - r).floor() as i64).max(0)..=((cy + r).ceil() as i64).min(h - 1) {
            for px in ((cx - r).floor() as i64).max(0)..=((cx + r).ceil() as i64).min(w - 1) {
                if ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt() > r {
                    continue;
                }
                let c = &mut p.cells[(py * w + px) as usize];
                if brush.tool == Tool::PathingOn { *c |= brush.flags } else { *c &= !brush.flags }
            }
        }
        self.dirty.pathing = true;
    }

    /// A step that changed nothing: a stroke it started is no stroke.
    fn failed(&mut self, start: bool, message: String) -> String {
        if start {
            self.stroke = None;
        }
        message
    }

    /// Nothing to undo or redo any more (a resize's: as World Editor's,
    /// the changes before it can't be undone over it).
    pub fn forget(&mut self) {
        self.stroke = None;
        self.undo.clear();
        self.redo.clear();
    }

    /// The stroke done: it can be undone.
    pub fn end(&mut self) {
        if let Some(s) = self.stroke.take() {
            self.undo.push(s.before);
            self.redo.clear();
        }
    }

    /// Back to before the last edit: what it was of (the snapshot of
    /// the state undone, for redo), or `None` with nothing to undo.
    pub fn undo(&mut self, target: Target) -> Option<Snapshot> {
        self.end();
        let before = self.undo.pop()?;
        let now = self.swap(before, target);
        self.redo.push(now.clone());
        Some(now)
    }

    pub fn redo(&mut self, target: Target) -> Option<Snapshot> {
        self.end();
        let after = self.redo.pop()?;
        let now = self.swap(after, target);
        self.undo.push(now.clone());
        Some(now)
    }

    /// Puts `s` in place; what was there.
    fn swap(&mut self, s: Snapshot, t: Target) -> Snapshot {
        match s {
            Snapshot::Terrain(points, ground) => {
                self.dirty.terrain = true;
                Snapshot::Terrain(std::mem::replace(&mut t.terrain.points, points), std::mem::replace(&mut t.terrain.ground, ground))
            }
            Snapshot::Whole(terrain) => {
                self.dirty.terrain = true;
                Snapshot::Whole(Box::new(std::mem::replace(t.terrain, *terrain)))
            }
            Snapshot::Doodads(d) => {
                self.dirty.doodads = true;
                let file = t.files.doodads.get_or_insert_with(|| new_doodads(t.files.layout));
                Snapshot::Doodads(std::mem::replace(&mut file.doodads, d))
            }
            Snapshot::Units(u) => {
                self.dirty.units = true;
                let file = t.files.units.get_or_insert_with(|| new_units(t.files.layout));
                Snapshot::Units(std::mem::replace(&mut file.units, u))
            }
            Snapshot::Regions(r) => {
                self.dirty.regions = true;
                let file = t.files.regions.get_or_insert_with(w3r_rs::Regions::new);
                Snapshot::Regions(std::mem::replace(&mut file.regions, r))
            }
            Snapshot::Cameras(c) => {
                self.dirty.cameras = true;
                let file = t.files.cameras.get_or_insert_with(|| w3c_rs::Cameras::new(Default::default()));
                Snapshot::Cameras(std::mem::replace(&mut file.cameras, c))
            }
            Snapshot::Sounds(s) => {
                self.dirty.sounds = true;
                let file = t.files.sounds.get_or_insert_with(w3s_rs::Sounds::new);
                Snapshot::Sounds(std::mem::replace(&mut file.sounds, s))
            }
            Snapshot::Objects(kind, data) => {
                self.dirty.objects |= crate::objdata::bit(kind);
                let before = match data {
                    Some(d) => t.files.objects.insert(kind, d),
                    None => t.files.objects.remove(&kind),
                };
                Snapshot::Objects(kind, before)
            }
            Snapshot::Many(all) => {
                let mut back: Vec<Snapshot> = all.into_iter().rev().map(|s| self.swap(s, Target { terrain: &mut *t.terrain, files: &mut *t.files, info: &mut *t.info })).collect();
                back.reverse();
                Snapshot::Many(back)
            }
            Snapshot::Info(info) => {
                self.dirty.info = true;
                Snapshot::Info(Box::new(std::mem::replace(t.info, *info)))
            }
            Snapshot::Imports(list, files) => {
                self.dirty.imports = true;
                Snapshot::Imports(std::mem::replace(&mut t.files.imports, list), std::mem::replace(&mut t.files.import_files, files))
            }
            Snapshot::Strings(text) => {
                self.dirty.strings = true;
                Snapshot::Strings(std::mem::replace(&mut t.files.strings, text))
            }
            Snapshot::Pathing(cells) => {
                self.dirty.pathing = true;
                match t.files.pathing.as_mut() {
                    Some(p) => Snapshot::Pathing(std::mem::replace(&mut p.cells, cells)),
                    None => Snapshot::Pathing(cells),
                }
            }
        }
    }
}

/// The corners around those a cliff brush set (`seeds`), pulled to within
/// two layers of them, and those around them in turn, as HiveWE's
/// `CliffOperator::check_nearby`: a step of three layers has no model.
/// Those pulled are no ramp's; they and the seeds' neighbours take the
/// brush's cliff type (`cliff`), when it has one.
fn pull_nearby(t: &mut Terrain, seeds: &[(i64, i64)], cliff: Option<u8>) {
    const MAX: i64 = 2;
    let v = t.flags_version();
    let (w, h) = (t.width as i64, t.height as i64);
    let at = |x: i64, y: i64| (y * w + x) as usize;
    let mut brushed = vec![false; t.points.len()];
    for &(x, y) in seeds {
        brushed[at(x, y)] = true;
    }
    // +1 pulls the corners too low up, -1 those too high down.
    for direction in [1i64, -1] {
        let mut stack: Vec<(i64, i64)> = seeds.to_vec();
        while let Some((cx, cy)) = stack.pop() {
            let target = t.points[at(cx, cy)].layer_height() as i64 - direction * MAX;
            for ny in (cy - 1).max(0)..=(cy + 1).min(h - 1) {
                for nx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                    let i = at(nx, ny);
                    if let Some(c) = cliff {
                        t.points[i].set_cliff_texture(c);
                    }
                    if brushed[i] || direction * (t.points[i].layer_height() as i64 - target) >= 0 {
                        continue;
                    }
                    let p = &mut t.points[i];
                    p.set_layer_height(target.clamp(0, 15) as u8);
                    p.set_ramp(v, false);
                    stack.push((nx, ny));
                }
            }
        }
    }
}

/// A ramp from corner `(i, j)` down a layer, `horizontal` and `vertical`
/// its way (-1, 0, 1), as HiveWE's `CliffOperator::update_ramp` lays one:
/// across (three corners in a row), up (three in a column) or both (the
/// nine), where the two corners after it are a layer below and nothing
/// beside it is higher; the corners of an L between two filled.
fn ramp_at(t: &mut Terrain, i: i64, j: i64, horizontal: i64, vertical: i64) {
    let v = t.flags_version();
    let (w, h) = (t.width as i64, t.height as i64);
    let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h;
    let at = |x: i64, y: i64| (y * w + x) as usize;
    let level = |t: &Terrain, x: i64, y: i64| if inside(x, y) { t.points[at(x, y)].layer_height() as i64 } else { i64::MIN };
    let is_ramp = |t: &Terrain, x: i64, y: i64| inside(x, y) && t.points[at(x, y)].ramp(v);
    let origin = level(t, i, j);
    let target = origin - 1;
    let tex = t.points[at(i, j)].cliff_texture();
    let valid = |t: &Terrain, x: i64, y: i64| inside(x, y) && level(t, x, y) == target;
    let direction = |t: &Terrain, dx: i64, dy: i64| -> bool {
        if (dx == 0 && dy == 0) || !inside(i + 2 * dx, j + 2 * dy) {
            return false;
        }
        if !(1..=2).all(|s| valid(t, i + s * dx, j + s * dy)) {
            return false;
        }
        // Nothing higher beside it.
        if level(t, i + dy, j + dx) > origin || level(t, i - dy, j - dx) > origin {
            return false;
        }
        // Not beside an edge whose ramp goes the other way alone.
        for side in [-1, 1] {
            if is_ramp(t, i, j + side) && (!is_ramp(t, i + dx, j + side + dy) || !is_ramp(t, i + 2 * dx, j + side + 2 * dy)) {
                return false;
            }
            if is_ramp(t, i + side, j) && (!is_ramp(t, i + side + dx, j + dy) || !is_ramp(t, i + side + 2 * dx, j + 2 * dy)) {
                return false;
            }
        }
        true
    };
    let across = horizontal != 0 && direction(t, horizontal, 0);
    let up = vertical != 0 && direction(t, 0, vertical);
    let diagonal = horizontal != 0 && vertical != 0 && (0..=2).all(|dx| (0..=2).all(|dy| (dx == 0 && dy == 0) || valid(t, i + dx * horizontal, j + dy * vertical)));
    let set = |t: &mut Terrain, x: i64, y: i64| {
        if inside(x, y) {
            let p = &mut t.points[at(x, y)];
            p.set_ramp(v, true);
            p.set_cliff_texture(tex);
        }
    };
    if across {
        for s in 0..=2 {
            set(t, i + s * horizontal, j);
        }
    }
    if up {
        for s in 0..=2 {
            set(t, i, j + s * vertical);
        }
    }
    if diagonal {
        for dx in 0..=2 {
            for dy in 0..=2 {
                set(t, i + dx * horizontal, j + dy * vertical);
            }
        }
    }
    // The middle of an L: its corner a ramp's too.
    if across || diagonal {
        for side in [1, -1] {
            if valid(t, i + horizontal, j + side) && (0..=2).all(|s| is_ramp(t, i + s, j)) && (1..=2).all(|s| is_ramp(t, i, j + s * side)) {
                let (x, y) = (i + horizontal, j + side);
                t.points[at(x, y)].set_ramp(v, true);
            }
        }
    }
    if up || diagonal {
        for side in [1, -1] {
            if valid(t, i + side, j + vertical) && (0..=2).all(|s| is_ramp(t, i, j + s)) && (1..=2).all(|s| is_ramp(t, i + s * side, j)) {
                let (x, y) = (i + side, j + vertical);
                t.points[at(x, y)].set_ramp(v, true);
            }
        }
    }
}

/// An empty `war3map.doo`, as The Frozen Throne writes it.
pub fn new_doodads(layout: doo_rs::Layout) -> doo_rs::Doodads {
    doo_rs::Doodads { version: 8, subversion: 11, layout, doodads: Vec::new(), special_version: 0, specials: Vec::new() }
}

/// An empty `war3mapUnits.doo`.
pub fn new_units(layout: doo_rs::Layout) -> doo_rs::Units {
    doo_rs::Units { version: 8, subversion: 11, layout, units: Vec::new() }
}

#[cfg(test)]
#[path = "edit.test.rs"]
mod tests;
