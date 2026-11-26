//! The pathing map (`war3map.wpm`) as World Editor makes it when it saves
//! the map: four cells a terrain cell, each one's flags from the terrain
//! under it and what stands on it, as HiveWE's `get_terrain_pathing` and
//! `blit_pathing_texture` work it out:
//!
//! - the ground texture of the nearest point: `TerrainArt\Terrain.slk`'s
//!   `walkable`, `flyable`, `buildable` (0: forbidden);
//! - a cliff (not a whole ramp): no walking, no building;
//! - blight: blight;
//! - water: deeper than 0.4 layers at the nearest point (no walking, no
//!   building), or over every corner of the cell (no building): not "no
//!   water"; shallower water at the nearest point alone is a shore, dry
//!   land with no building;
//! - a boundary or the map's edge: nothing allowed, no water;
//! - 0x80 where nothing walks on land (no walking, no water);
//! - each solid doodad's pathing texture (`pathTex`, a TGA:
//!   red no walking, green no flying, blue no building), turned to the
//!   nearest quarter of its angle, centred on it.
//!
//! On the Diablo III beta map this makes 99.45 to 100% of each flag's
//! cells as its own `war3map.wpm` has them (walking 99.86, building
//! 99.46, water 99.45); what's left looks painted by hand (straight
//! bars of water in dry halls, walkable deep water), which is kept as a
//! brush's paint is. Bridges and other walkable destructables aren't in
//! the file at all: the game lays a destructable's pathing when it
//! stands.
//!
//! What a pathing brush changed is kept over it: [`Manual`], the cells'
//! flags set and cleared against what the map's files make.

use w3e_rs::Terrain;
use wc3_server::game::Game;
use wpm_rs::{BLIGHT, NO_BUILD, NO_FLY, NO_WALK, NO_WATER, Pathing};

use crate::placed::Files;
use crate::terrain::{Draw, tile};
use crate::tileset::Tileset;
use crate::types::Types;

/// The pathing the map's files make: the terrain `t` (drawn as `draw`),
/// what `files` place on it.
pub fn compute(game: &Game, t: &Terrain, draw: &Draw, tileset: &Tileset, types: &Types, files: &Files) -> Pathing {
    compute_with(game, t, draw, tileset, types, files, None)
}

/// [`compute`] with ground tile `index`'s pathing `forbids` (the tile
/// pather's) in place of its table's.
pub fn compute_with(game: &Game, t: &Terrain, draw: &Draw, tileset: &Tileset, types: &Types, files: &Files, tile: Option<(usize, u8)>) -> Pathing {
    let (w, h) = (t.width as usize, t.height as usize);
    let (pw, ph) = ((w.saturating_sub(1)) * 4, (h.saturating_sub(1)) * 4);
    let v = t.flags_version();
    let mut cells = vec![0u8; pw * ph];
    // Each ground texture's flags, by its index in the terrain's list.
    let ground: Vec<u8> = t
        .ground
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if let Some((_, forbids)) = tile.filter(|(n, _)| *n == i) {
                return forbids;
            }
            let id = g.to_string();
            let allowed = |k: &str| tileset.terrain.as_ref().and_then(|s| s.number(&id, k)).is_none_or(|n| n != 0.0);
            (if allowed("walkable") { 0 } else { NO_WALK }) | (if allowed("flyable") { 0 } else { NO_FLY }) | (if allowed("buildable") { 0 } else { NO_BUILD })
        })
        .collect();
    let at = |x: usize, y: usize| &t.points[y.min(h - 1) * w + x.min(w - 1)];
    for j in 0..ph {
        for i in 0..pw {
            let (cx, cy) = (i / 4, j / 4);
            let (nx, ny) = (((i as f32) / 4.0).round() as usize, ((j as f32) / 4.0).round() as usize);
            let near = at(nx, ny);
            let mut f = ground.get(near.texture(v) as usize).copied().unwrap_or(0);
            let flags = draw.tile_flags.get(cy * (w - 1) + cx).copied().unwrap_or(0);
            let ramp = (0..4).all(|k| at(cx + k % 2, cy + k / 2).ramp(v));
            if flags & (tile::CLIFF | tile::ROMP) != 0 && !ramp {
                f |= NO_BUILD | NO_WALK;
            }
            if near.blight(v) {
                f |= BLIGHT;
            }
            // Water: deep (over 0.4 layers) at the nearest point, or
            // over every corner of the cell; shallow water at the nearest
            // point alone isn't (World Editor's shores are a cell
            // narrower than the nearest point's).
            let depth = |x: usize, y: usize| {
                let k = y.min(h - 1) * w + x.min(w - 1);
                if t.points[k].has_water(v) { draw.water[k] - draw.heights[k] } else { f32::NEG_INFINITY }
            };
            let deep = depth(nx, ny) > 0.4;
            let whole = (0..4).all(|k| depth(cx + k % 2, cy + k / 2) > 0.0);
            let bl = at(cx, cy);
            let edge = bl.edge() || bl.boundary(v);
            if (deep || whole) && !edge {
                f |= if deep { NO_BUILD | NO_WALK } else { NO_BUILD };
            } else {
                f |= NO_WATER;
                // A shore: dry land, but nothing's built at the water.
                if depth(nx, ny) > 0.0 {
                    f |= NO_BUILD;
                }
            }
            // The edge and boundaries: nothing allowed, no water either.
            if edge {
                f |= NO_BUILD | NO_FLY | NO_WALK;
            }
            cells[j * pw + i] = f;
        }
    }
    // What stands on it: each pathing texture, once read.
    let mut textures: std::collections::HashMap<String, Option<(u32, u32, Vec<u8>)>> = Default::default();
    for d in files.doodads.iter().flat_map(|d| &d.doodads) {
        let p = &d.placement;
        let id = p.id.to_string();
        // Doodads alone, solid ones (flags 2): a destructable's pathing
        // the game lays when it stands (blockers, barrels, trees' walls
        // aren't in any war3map.wpm World Editor saved), a non-solid
        // doodad has none.
        if !types.has(w3abdhqtu_rs::Kind::Doodad, &id) || d.flags & 2 == 0 {
            continue;
        }
        let kind = w3abdhqtu_rs::Kind::Doodad;
        let Some(path) = types.get(kind, &id, "pathTex").filter(|p| !p.eq_ignore_ascii_case("none")) else { continue };
        let tex = textures.entry(path.to_ascii_lowercase()).or_insert_with(|| crate::textures::capped(game, path, 1 << 16).ok());
        let Some((tw, th, rgba)) = tex else { continue };
        // A quarter turn at a time, as World Editor places them.
        let rotation = (((p.angle.to_degrees() + 90.0) / 90.0).round() as i64).rem_euclid(4) * 90;
        let (px, py) = ((p.x - t.offset_x) / 32.0, (p.y - t.offset_y) / 32.0);
        blit(&mut cells, pw, ph, (px, py), rotation, (*tw, *th, rgba));
    }
    // 0x80: land nothing walks on (as every map World Editor saved has
    // it: set exactly where no walking and no water are).
    for c in &mut cells {
        if *c & NO_WALK != 0 && *c & NO_WATER != 0 {
            *c |= 0x80;
        }
    }
    Pathing { version: wpm_rs::VERSION, width: pw as u32, height: ph as u32, cells }
}

/// A pathing texture (rows top to bottom) onto `cells` (rows from the
/// bottom), centred on `at` (cells), turned `rotation` degrees.
fn blit(cells: &mut [u8], pw: usize, ph: usize, at: (f32, f32), rotation: i64, (tw, th, rgba): (u32, u32, &Vec<u8>)) {
    let (tw, th) = (tw as i64, th as i64);
    let (dw, dh) = if rotation % 180 != 0 { (th, tw) } else { (tw, th) };
    for j in 0..th {
        for i in 0..tw {
            let (x, y) = match rotation {
                90 => (th - 1 - j, i),
                180 => (tw - 1 - i, th - 1 - j),
                270 => (j, tw - 1 - i),
                _ => (i, j),
            };
            let xx = at.0 as i64 + x - dw / 2;
            let yy = at.1 as i64 + y - dh / 2;
            if xx < 0 || yy < 0 || xx >= pw as i64 || yy >= ph as i64 {
                continue;
            }
            let px = (((th - 1 - j) * tw + i) * 4) as usize;
            let on = |c: usize, bit: u8| if rgba[px + c] > 250 { bit } else { 0 };
            cells[yy as usize * pw + xx as usize] |= on(0, NO_WALK) | on(1, NO_FLY) | on(2, NO_BUILD);
        }
    }
}

/// What a pathing brush changed, against what the files make: the flags
/// set and cleared on each cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manual {
    pub set: Vec<u8>,
    pub clear: Vec<u8>,
}

impl Manual {
    /// The difference of `painted` (the pathing as it is) from `made`
    /// (what the files make); none when their sizes differ.
    pub fn of(made: &Pathing, painted: &Pathing) -> Manual {
        if made.cells.len() != painted.cells.len() {
            return Manual::default();
        }
        let set = made.cells.iter().zip(&painted.cells).map(|(m, p)| p & !m).collect();
        let clear = made.cells.iter().zip(&painted.cells).map(|(m, p)| m & !p).collect();
        Manual { set, clear }
    }

    /// `made` with these changes over it.
    pub fn apply(&self, made: &mut Pathing) {
        if self.set.len() != made.cells.len() {
            return;
        }
        for ((c, s), k) in made.cells.iter_mut().zip(&self.set).zip(&self.clear) {
            *c = (*c | s) & !k;
        }
    }
}

#[cfg(test)]
#[path = "pathing.test.rs"]
mod tests;
