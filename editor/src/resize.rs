//! The map made bigger or smaller, and its playable area set, as World
//! Editor's "Map Size and Camera Bounds" and HiveWE's `Map::resize` and
//! `set_playable_area` do it:
//!
//! - the terrain gains or loses cells on each side; a new point is a
//!   copy of the nearest old one (its height, water, textures, cliff
//!   level, its edge flag), a random ground variation; the corner
//!   (0, 0) moves out by the cells added left and below, so what's
//!   placed stays where it is in the world;
//! - the pathing (`war3map.wpm`) and the shadows (`war3map.shd`), four
//!   cells a terrain cell, the same way, new cells copies of the
//!   nearest;
//! - `war3map.w3i`'s playable area stays as it was (the cells the
//!   camera's bounds leave out grow by what's added; when the cut goes
//!   into it, the playable area is set again);
//! - what's placed off the terrain now is removed, the regions cut to it
//!   (one with nothing left removed), a camera looking at a point off it
//!   removed; the special doodads (on the terrain's grid) move with it.
//!
//! The playable area: the points outside it (the cells left out on
//! each side) the edge's, `war3map.w3i`'s bounds and camera bounds
//! worked out from them as World Editor does (four cells in from the
//! sides, two from the bottom and top).

use w3e_rs::Terrain;

/// Cells added on each side (fewer than 0: taken off).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sides {
    pub left: i32,
    pub right: i32,
    pub bottom: i32,
    pub top: i32,
}

/// The smallest and largest a map's terrain can be, in cells.
pub const MIN: i32 = 32;
pub const MAX: i32 = 480;

/// What was removed: things placed, regions, cameras.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Removed {
    pub things: usize,
    pub regions: usize,
    pub cameras: usize,
}

/// The terrain's cells across and up after `s`, or why it can't be.
pub fn new_size(t: &Terrain, s: Sides) -> Result<(u32, u32), String> {
    let w = t.width as i32 - 1 + s.left + s.right;
    let h = t.height as i32 - 1 + s.bottom + s.top;
    if !(MIN..=MAX).contains(&w) || !(MIN..=MAX).contains(&h) {
        return Err(format!("A map is {MIN} to {MAX} cells across and up; this would be {w} by {h}."));
    }
    Ok((w as u32, h as u32))
}

/// `cells` (`w` by `h`, rows from the bottom) grown or cut by `s` times
/// `per` on each side, a new cell the nearest old one's.
fn grid<T: Copy>(cells: &[T], w: usize, h: usize, s: Sides, per: i32) -> (Vec<T>, usize, usize) {
    let (l, r, b, t) = (s.left * per, s.right * per, s.bottom * per, s.top * per);
    let nw = (w as i32 + l + r).max(0) as usize;
    let nh = (h as i32 + b + t).max(0) as usize;
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        let oy = (y as i32 - b).clamp(0, h as i32 - 1) as usize;
        for x in 0..nw {
            let ox = (x as i32 - l).clamp(0, w as i32 - 1) as usize;
            out.push(cells[oy * w + ox]);
        }
    }
    (out, nw, nh)
}

/// The terrain grown or cut by `s`: its new points' ground variations
/// from `seed`.
pub fn terrain(t: &mut Terrain, s: Sides, seed: &mut u32) -> Result<(), String> {
    let (cw, ch) = new_size(t, s)?;
    let (w, h) = (t.width as usize, t.height as usize);
    let (mut points, nw, nh) = grid(&t.points, w, h, s, 1);
    for y in 0..nh {
        for x in 0..nw {
            let (ox, oy) = (x as i32 - s.left, y as i32 - s.bottom);
            if ox < 0 || oy < 0 || ox >= w as i32 || oy >= h as i32 {
                *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let roll = (*seed >> 16) % 100;
                points[y * nw + x].set_ground_variation(if roll < 85 { 0 } else { (roll % 16) as u8 });
            }
        }
    }
    debug_assert_eq!((nw as u32, nh as u32), (cw + 1, ch + 1));
    t.points = points;
    t.width = nw as u32;
    t.height = nh as u32;
    t.offset_x -= s.left as f32 * w3e_rs::CELL;
    t.offset_y -= s.bottom as f32 * w3e_rs::CELL;
    Ok(())
}

/// The pathing grown or cut by `s` (four cells a terrain cell).
pub fn pathing(p: &mut wpm_rs::Pathing, s: Sides) {
    let (cells, w, h) = grid(&p.cells, p.width as usize, p.height as usize, s, 4);
    (p.cells, p.width, p.height) = (cells, w as u32, h as u32);
}

/// The shadows grown or cut by `s` (four cells a terrain cell; new ones
/// none).
pub fn shadows(sh: &mut shd_rs::Shadows, s: Sides) {
    let (w, h) = (sh.width as usize, sh.height as usize);
    let (mut cells, nw, nh) = grid(&sh.cells, w, h, s, 4);
    for y in 0..nh {
        for x in 0..nw {
            let (ox, oy) = (x as i32 - s.left * 4, y as i32 - s.bottom * 4);
            if ox < 0 || oy < 0 || ox >= w as i32 || oy >= h as i32 {
                cells[y * nw + x] = 0;
            }
        }
    }
    (sh.cells, sh.width, sh.height) = (cells, nw as u32, nh as u32);
}

/// What's placed off the terrain `t` (resized) removed, the regions cut
/// to it, the cameras looking off it removed, the special doodads moved
/// by `s`.
pub fn placed(files: &mut crate::placed::Files, t: &Terrain, s: Sides) -> Removed {
    let (x0, y0) = (t.offset_x, t.offset_y);
    let (x1, y1) = (x0 + (t.width - 1) as f32 * w3e_rs::CELL, y0 + (t.height - 1) as f32 * w3e_rs::CELL);
    let on = |x: f32, y: f32| x >= x0 && y >= y0 && x <= x1 && y <= y1;
    let mut removed = Removed::default();
    if let Some(d) = files.doodads.as_mut() {
        let n = d.doodads.len() + d.specials.len();
        d.doodads.retain(|d| on(d.placement.x, d.placement.y));
        for sp in &mut d.specials {
            sp.x = (sp.x as i64 + s.left as i64).max(-1) as u32;
            sp.y = (sp.y as i64 + s.bottom as i64).max(-1) as u32;
        }
        // Moved off the grid: a cell past it (or below 0, u32::MAX).
        d.specials.retain(|sp| sp.x < t.width - 1 && sp.y < t.height - 1);
        removed.things += n - d.doodads.len() - d.specials.len();
    }
    if let Some(u) = files.units.as_mut() {
        let n = u.units.len();
        u.units.retain(|u| on(u.placement.x, u.placement.y));
        removed.things += n - u.units.len();
    }
    if let Some(r) = files.regions.as_mut() {
        let n = r.regions.len();
        for g in &mut r.regions {
            g.left = g.left.max(x0);
            g.bottom = g.bottom.max(y0);
            g.right = g.right.min(x1);
            g.top = g.top.min(y1);
        }
        r.regions.retain(|g| g.right > g.left && g.top > g.bottom);
        removed.regions = n - r.regions.len();
    }
    if let Some(c) = files.cameras.as_mut() {
        let n = c.cameras.len();
        c.cameras.retain(|c| on(c.target_x, c.target_y));
        removed.cameras = n - c.cameras.len();
    }
    removed
}

/// `war3map.w3i` after the terrain was resized by `s`: the cells left
/// out grow with what was added; when that goes below none (the cut
/// went into the playable area), the playable area set again.
pub fn info(i: &mut w3i_rs::Info, t: &Terrain, s: Sides) {
    let c = i.camera_complements;
    let now = [c[0] + s.left, c[1] + s.right, c[2] + s.bottom, c[3] + s.top];
    if now.iter().all(|v| *v >= 0) {
        i.camera_complements = now;
        // The bounds are the world's: the corner moved, they didn't.
    } else {
        playable(i, t, now.map(|v| v.max(0)));
    }
}

/// The playable area of `t` set: `margins` cells left out on the left,
/// right, bottom, top; the points outside it the edge's; the info's
/// bounds and camera bounds worked out from them.
pub fn set_playable(t: &mut Terrain, i: Option<&mut w3i_rs::Info>, margins: [i32; 4]) -> Result<(), String> {
    let (w, h) = (t.width as i32 - 1, t.height as i32 - 1);
    let [l, r, b, tp] = margins;
    if margins.iter().any(|m| *m < 0) || w - l - r < 1 || h - b - tp < 1 {
        return Err(format!("Margins {l}, {r}, {b}, {tp} leave nothing of {w} by {h} cells to play on."));
    }
    // A cell is the edge's when its bottom left point is: the right and
    // top margins' first points too.
    let (r1, t1) = (if r > 0 { r + 1 } else { r }, if tp > 0 { tp + 1 } else { tp });
    for y in 0..t.height as i32 {
        for x in 0..t.width as i32 {
            let edge = x < l || x >= t.width as i32 - r1 || y < b || y >= t.height as i32 - t1;
            t.points[(y * t.width as i32 + x) as usize].set_edge(edge);
        }
    }
    if let Some(i) = i {
        playable(i, t, margins);
    }
    Ok(())
}

/// The info's playable area and camera bounds from `margins` (left,
/// right, bottom, top), as World Editor works them out.
fn playable(i: &mut w3i_rs::Info, t: &Terrain, margins: [i32; 4]) {
    let (w, h) = (t.width as i32 - 1, t.height as i32 - 1);
    let [l, r, b, tp] = margins;
    i.camera_complements = margins;
    i.playable_width = w - l - r;
    i.playable_height = h - b - tp;
    i.camera_bounds = camera_bounds(t, margins);
}

/// The camera bounds World Editor works out from `margins` (left, right,
/// bottom, top): four cells in from the playable area's sides, two from
/// its bottom and top; its corners as `w3i_rs::Info::camera_bounds`.
pub fn camera_bounds(t: &Terrain, margins: [i32; 4]) -> [f32; 8] {
    let (w, h) = (t.width as i32 - 1, t.height as i32 - 1);
    let [l, r, b, tp] = margins;
    let x = |cells: i32| cells as f32 * w3e_rs::CELL + t.offset_x;
    let y = |cells: i32| cells as f32 * w3e_rs::CELL + t.offset_y;
    let (left, right, bottom, top) = (x(l + 4), x(w - r - 4), y(b + 2), y(h - tp - 2));
    // Bottom left, top right, top left, bottom right.
    [left, bottom, right, top, left, top, right, bottom]
}

#[cfg(test)]
#[path = "resize.test.rs"]
mod tests;
