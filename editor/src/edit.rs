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
}

/// The tools by their code on the wire.
pub const TOOLS: [Tool; 11] = [
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
    /// `war3map.w3i` (its playable area, its tileset) and `war3map.shd`.
    pub info: bool,
    pub shadows: bool,
}

impl Dirty {
    pub fn any(self) -> bool {
        self.terrain || self.doodads || self.units || self.regions || self.pathing || self.cameras || self.sounds || self.objects != 0 || self.strings || self.info || self.shadows
    }
}

/// What an undo or redo works on.
pub struct Target<'a> {
    pub terrain: &'a mut Terrain,
    pub files: &'a mut crate::placed::Files,
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
        let (x0, x1) = (((x - r).floor() as i64).max(0), ((x + r).ceil() as i64).min(w - 1));
        let (y0, y1) = (((y - r).floor() as i64).max(0), ((y + r).ceil() as i64).min(h - 1));
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
                        p.set_layer_height(start_value as u8);
                        if brush.cliff >= 0 && (brush.cliff as usize) < t.cliffs.len() {
                            p.set_cliff_texture(brush.cliff as u8);
                        }
                    }
                    Tool::Water => p.set_has_water(v, true),
                    Tool::Dry => p.set_has_water(v, false),
                    // Not the terrain's: `pathing`.
                    Tool::PathingOn | Tool::PathingOff => {}
                }
            }
        }
        self.dirty.terrain = true;
        Ok(())
    }

    /// An edit of the things placed about to be made: what it changes, as
    /// it is, to undo it.
    pub fn record(&mut self, before: Snapshot) {
        self.end();
        match &before {
            Snapshot::Terrain(..) | Snapshot::Whole(_) => self.dirty.terrain = true,
            Snapshot::Doodads(_) => self.dirty.doodads = true,
            Snapshot::Units(_) => self.dirty.units = true,
            Snapshot::Regions(_) => self.dirty.regions = true,
            Snapshot::Cameras(_) => self.dirty.cameras = true,
            Snapshot::Sounds(_) => self.dirty.sounds = true,
            Snapshot::Objects(kind, _) => self.dirty.objects |= crate::objdata::bit(*kind),
            Snapshot::Strings(_) => self.dirty.strings = true,
            Snapshot::Pathing(_) => self.dirty.pathing = true,
        }
        self.undo.push(before);
        self.redo.clear();
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
