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
}

/// The tools by their code on the wire.
pub const TOOLS: [Tool; 9] =
    [Tool::Raise, Tool::Lower, Tool::Plateau, Tool::Smooth, Tool::Texture, Tool::CliffUp, Tool::CliffDown, Tool::Water, Tool::Dry];

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
}

/// A stroke going on.
#[derive(Debug, Clone)]
struct Stroke {
    before: Vec<Point>,
    ground: Vec<Rawcode>,
    /// The height (Plateau) or layer (Cliff) the stroke started on.
    start: Option<i32>,
}

/// The terrain's changes: undo and redo, what's unsaved.
#[derive(Debug, Default)]
pub struct Edits {
    undo: Vec<(Vec<Point>, Vec<Rawcode>)>,
    redo: Vec<(Vec<Point>, Vec<Rawcode>)>,
    stroke: Option<Stroke>,
    /// Changes since the last save (or the file as read).
    pub dirty: bool,
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
            self.stroke = Some(Stroke { before: t.points.clone(), ground: t.ground.clone(), start: None });
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
                }
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// A step that changed nothing: a stroke it started is no stroke.
    fn failed(&mut self, start: bool, message: String) -> String {
        if start {
            self.stroke = None;
        }
        message
    }

    /// The stroke done: it can be undone.
    pub fn end(&mut self) {
        if let Some(s) = self.stroke.take() {
            self.undo.push((s.before, s.ground));
            self.redo.clear();
        }
    }

    /// Back to before the last stroke; whether there was one.
    pub fn undo(&mut self, t: &mut Terrain) -> bool {
        self.end();
        let Some((points, ground)) = self.undo.pop() else { return false };
        self.redo.push((std::mem::replace(&mut t.points, points), std::mem::replace(&mut t.ground, ground)));
        self.dirty = true;
        true
    }

    pub fn redo(&mut self, t: &mut Terrain) -> bool {
        self.end();
        let Some((points, ground)) = self.redo.pop() else { return false };
        self.undo.push((std::mem::replace(&mut t.points, points), std::mem::replace(&mut t.ground, ground)));
        self.dirty = true;
        true
    }
}

#[cfg(test)]
#[path = "edit.test.rs"]
mod tests;
