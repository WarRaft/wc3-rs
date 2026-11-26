//! What the view shows over the map, each its own layer to turn on and
//! off: the pathing (`war3map.wpm`), the regions (`war3map.w3r`), the
//! cameras (`war3map.w3c`), the bounds (`war3map.w3i`: the camera's, and
//! the playable area), and a minimap.
//!
//! The page draws, this works out: the pathing and the regions as
//! pictures of four pixels a cell, laid on the ground as it lies; the
//! bounds and the cameras as lines, each point on the ground under it;
//! the minimap the terrain from above ([`crate::picture::paint`]), what's
//! placed on it in its owner's colour. [`ground_hit`] is where a ray from
//! the camera meets the ground.

use w3e_rs::Terrain;

use crate::placed::{INSTANCE, Objects, ground_at};
use crate::terrain::Draw;

/// A picture, rows from the bottom (as the terrain's), RGBA or a byte a
/// pixel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// A region: its name, bounds (the game's units: left, bottom, right,
/// top) and colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub name: String,
    pub bounds: [f32; 4],
    pub color: [u8; 3],
}

/// A camera: its name, where it looks and where it's from (cells, layers).
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub name: String,
    pub target: [f32; 3],
    pub eye: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overlays {
    /// A cell's pathing flags each (`wpm_rs`: 2 no walking, 4 no flying,
    /// 8 no building).
    pub pathing: Grid,
    /// What the destructables lay when they stand (`pathing::
    /// destructables`: the same flags, and 1 a walkable one's deck), the
    /// pathing's size.
    pub destructables: Grid,
    pub regions: Vec<Region>,
    /// The regions' colours, RGBA.
    pub region_picture: Grid,
    /// Lines, two points each, a point x, y, z, r, g, b, a.
    pub bounds: Vec<f32>,
    pub camera_lines: Vec<f32>,
    pub cameras: Vec<Camera>,
    /// RGBA, rows from the top (north up): a pixel a corner.
    pub minimap: Grid,
    pub notes: Vec<String>,
}

/// Floats a line's point has.
pub const POINT: usize = 7;

/// A line from `a` to `b` (cells) on the ground, `lift` over it.
fn on_ground(draw: &Draw, a: [f32; 2], b: [f32; 2], color: [f32; 4], lift: f32, out: &mut Vec<f32>) {
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let steps = (len * 4.0).ceil().max(1.0) as usize;
    let at = |k: usize| {
        let t = k as f32 / steps as f32;
        let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        [x, y, ground_at(draw, x, y) + lift]
    };
    for k in 0..steps {
        for p in [at(k), at(k + 1)] {
            out.extend(p);
            out.extend(color);
        }
    }
}

fn line(a: [f32; 3], b: [f32; 3], color: [f32; 4], out: &mut Vec<f32>) {
    for p in [a, b] {
        out.extend(p);
        out.extend(color);
    }
}

/// A rectangle (cells) on the ground.
fn rectangle(draw: &Draw, r: [f32; 4], color: [f32; 4], out: &mut Vec<f32>) {
    let [l, b, rt, t] = r;
    for (p, q) in [([l, b], [rt, b]), ([rt, b], [rt, t]), ([rt, t], [l, t]), ([l, t], [l, b])] {
        on_ground(draw, p, q, color, 0.08, out);
    }
}

/// The bounds' colours: the camera's, the playable area's.
const CAMERA_BOUNDS: [f32; 4] = [0.35, 0.6, 1.0, 1.0];
const PLAYABLE: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
const CAMERA: [f32; 4] = [1.0, 0.4, 0.9, 1.0];

impl Overlays {
    /// The overlays of the map: its regions, pathing and cameras
    /// `files`' and its bounds `info`'s (as the editor has them).
    pub fn load(t: &Terrain, draw: &Draw, objects: &Objects, files: &crate::placed::Files, info: Option<&w3i_rs::Info>, minimap: (u32, u32, Vec<u8>)) -> Overlays {
        let mut out = Overlays::default();
        let cells = |wx: f32, wy: f32| [(wx - draw.offset[0]) / 128.0, (wy - draw.offset[1]) / 128.0];

        if let Some(p) = &files.pathing {
            out.pathing = Grid { width: p.width, height: p.height, data: p.cells.clone() };
        }

        // The regions, four pixels a cell.
        let (gw, gh) = ((t.width.saturating_sub(1)) * 4, (t.height.saturating_sub(1)) * 4);
        let mut picture = vec![0f32; (gw * gh * 4) as usize];
        {
            if let Some(rs) = &files.regions {
                for r in &rs.regions {
                    let color = [r.red, r.green, r.blue];
                    out.regions.push(Region { name: r.name.clone(), bounds: [r.left, r.bottom, r.right, r.top], color });
                    let [l, b] = cells(r.left, r.bottom).map(|v| (v * 4.0).round() as i64);
                    let [rt, tp] = cells(r.right, r.top).map(|v| (v * 4.0).round() as i64);
                    let c = color.map(|v| v as f32 / 255.0);
                    for y in b.max(0)..tp.min(gh as i64) {
                        for x in l.max(0)..rt.min(gw as i64) {
                            let edge = x == l || y == b || x == rt - 1 || y == tp - 1;
                            let a = if edge { 0.9 } else { 0.3 };
                            let at = ((y as u32 * gw + x as u32) * 4) as usize;
                            let px = &mut picture[at..at + 4];
                            for k in 0..3 {
                                px[k] = c[k] * a + px[k] * (1.0 - a);
                            }
                            px[3] = a + px[3] * (1.0 - a);
                        }
                    }
                }
            }
        }
        out.region_picture = Grid { width: gw, height: gh, data: picture.iter().map(|v| (v * 255.0).round() as u8).collect() };

        // The bounds.
        if let Some(info) = info {
            let b = info.camera_bounds;
            let [l, bo] = cells(b[0], b[1]);
            let [r, tp] = cells(b[2], b[3]);
            rectangle(draw, [l, bo, r, tp], CAMERA_BOUNDS, &mut out.bounds);
            let c = info.camera_complements.map(|v| v as f32);
            let (w, h) = (draw.width as f32 - 1.0, draw.height as f32 - 1.0);
            rectangle(draw, [c[0], c[2], w - c[1], h - c[3]], PLAYABLE, &mut out.bounds);
        }

        // The cameras: a line from where each is to where it looks, and a
        // cross where it looks.
        // (`placed::Files` read them, and said an error.)
        if let Some(cams) = files.cameras.as_ref() {
            for c in &cams.cameras {
                let [x, y] = cells(c.target_x, c.target_y);
                let target = [x, y, ground_at(draw, x, y) + c.z_offset / 128.0];
                let (rot, aoa) = (c.rotation.to_radians(), c.angle_of_attack.to_radians());
                let forward = [rot.cos() * aoa.cos(), rot.sin() * aoa.cos(), aoa.sin()];
                let d = c.distance / 128.0;
                let eye = [target[0] - forward[0] * d, target[1] - forward[1] * d, target[2] - forward[2] * d];
                line(eye, target, CAMERA, &mut out.camera_lines);
                for (dx, dy) in [(0.5, 0.0), (0.0, 0.5)] {
                    line([x - dx, y - dy, target[2]], [x + dx, y + dy, target[2]], CAMERA, &mut out.camera_lines);
                }
                out.cameras.push(Camera { name: c.name.clone(), target, eye });
            }
        }

        // The minimap: the terrain, what's placed on it in its owner's
        // colour (a unit, an item), a doodad darker.
        let (mw, mh, mut rgba) = minimap;
        for th in &objects.things {
            let i = &objects.models[th.model].instances[th.instance * INSTANCE..][..INSTANCE];
            let (x, y) = (i[0].round() as i64, i[1].round() as i64);
            if x < 0 || y < 0 || x >= mw as i64 || y >= mh as i64 {
                continue;
            }
            let color = match th.kind {
                w3abdhqtu_rs::Kind::Unit => [i[10], i[11], i[12]].map(|v| (v * 255.0) as u8),
                w3abdhqtu_rs::Kind::Item => [255, 255, 255],
                _ => continue,
            };
            let row = mh as i64 - 1 - y;
            let at = ((row * mw as i64 + x) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&color);
        }
        out.minimap = Grid { width: mw, height: mh, data: rgba };
        out
    }

    /// The names of the regions over `(x, y)` (the game's units).
    pub fn regions_at(&self, x: f32, y: f32) -> Vec<&str> {
        self.regions
            .iter()
            .filter(|r| x >= r.bounds[0] && x <= r.bounds[2] && y >= r.bounds[1] && y <= r.bounds[3])
            .map(|r| r.name.as_str())
            .collect()
    }
}

/// Where the ray from `origin` along `dir` (cells, layers) first meets
/// the ground (or the water over it), within the map.
pub fn ground_hit(draw: &Draw, origin: [f32; 3], dir: [f32; 3]) -> Option<[f32; 3]> {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len == 0.0 {
        return None;
    }
    let d = dir.map(|v| v / len);
    let at = |t: f32| [origin[0] + d[0] * t, origin[1] + d[1] * t, origin[2] + d[2] * t];
    let inside = |p: [f32; 3]| p[0] >= 0.0 && p[1] >= 0.0 && p[0] <= draw.width as f32 - 1.0 && p[1] <= draw.height as f32 - 1.0;
    let below = |p: [f32; 3]| p[2] <= ground_at(draw, p[0], p[1]);
    let (step, far) = (0.05, 4000.0);
    let mut t = 0.0;
    while t < far {
        let p = at(t + step);
        if inside(p) && below(p) {
            // Halved down to a hundredth of a step.
            let (mut lo, mut hi) = (t, t + step);
            for _ in 0..8 {
                let mid = (lo + hi) / 2.0;
                if below(at(mid)) { hi = mid } else { lo = mid }
            }
            let p = at(hi);
            return Some([p[0], p[1], ground_at(draw, p[0], p[1])]);
        }
        t += step;
    }
    None
}

#[cfg(test)]
#[path = "overlays.test.rs"]
mod tests;
