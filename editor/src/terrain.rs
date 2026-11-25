//! What the page draws of a terrain (`war3map.w3e`), worked out here as
//! HiveWE's `terrain.ixx` does (the reference; see the README's "World
//! Editor"). Distances are in cells (128 units), heights in layers (128
//! units too): a point's ground height is `(height - 0x2000) / 512`, over
//! its cliff layer `layer - 2`.
//!
//! - **Heights**, a corner each: the ground's final height (its layer
//!   and, at the foot of a ramp, half a layer up), and the ground alone,
//!   for its normals (a cliff doesn't darken the ground beside it).
//! - **Ground layers**, a tile each: up to four textures, lowest first, the
//!   first whole (with its variation: a cell of its texture), each after it
//!   through the blend mask of the corners that have it (cells 0-15 of a
//!   ground texture are its masks, by which corners it covers). A corner's
//!   texture is its own, or blight's, or, next to a cliff, the cliff's
//!   ground tile (`real_tile_texture`).
//! - **Water**, a corner each: the level (with the tileset's water height
//!   over it) and whether it has water.

use crate::tileset::Tileset;
use w3e_rs::{Point, Terrain};

/// Corner flags.
pub mod corner {
    pub const WATER: u8 = 1;
    pub const BOUNDARY: u8 = 2;
    pub const EDGE: u8 = 4;
    pub const BLIGHT: u8 = 8;
    pub const RAMP: u8 = 16;
}

/// Tile flags.
pub mod tile {
    /// Its corners' cliff layers differ: a cliff stands on it.
    pub const CLIFF: u8 = 1;
    /// A ramp's foot: its ground is drawn though it's a cliff tile.
    pub const RAMP_ENTRANCE: u8 = 2;
}

/// The ground textures a terrain is drawn with, in the order its tiles
/// name them: the terrain's own (`war3map.w3e`'s list), then blight, then
/// the cliffs' ground tiles it hasn't.
#[derive(Debug, Clone, PartialEq)]
pub struct Grounds {
    /// Each one's tile id (`Ldrt`; `blight` for blight).
    pub ids: Vec<String>,
    pub blight: usize,
    /// For each of the terrain's cliff textures, its ground texture.
    pub cliff_ground: Vec<usize>,
}

impl Grounds {
    pub fn of(terrain: &Terrain, tileset: &Tileset) -> Grounds {
        let mut ids: Vec<String> = terrain.ground.iter().map(|r| r.to_string()).collect();
        let blight = ids.len();
        ids.push("blight".into());
        let cliff_ground = terrain
            .cliffs
            .iter()
            .map(|c| {
                let Some(ground) = tileset.cliff_ground(&c.to_string()) else { return 0 };
                match ids.iter().position(|i| i == ground) {
                    Some(at) => at,
                    None => {
                        ids.push(ground.to_string());
                        ids.len() - 1
                    }
                }
            })
            .collect();
        Grounds { ids, blight, cliff_ground }
    }
}

/// A terrain as the page draws it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Draw {
    /// Corners across and up.
    pub width: u32,
    pub height: u32,
    /// Corner (0, 0)'s place, in units.
    pub offset: [f32; 2],
    /// A corner each, row by row from the bottom left.
    pub heights: Vec<f32>,
    pub ground: Vec<f32>,
    pub water: Vec<f32>,
    pub corners: Vec<u8>,
    /// A tile each (`width - 1` by `height - 1`), four entries: texture
    /// index in the low 16 bits, its cell (variation or blend mask) in the
    /// high; `0xFFFF` none.
    pub tiles: Vec<[u32; 4]>,
    pub tile_flags: Vec<u8>,
}

/// A point's ground height in layers.
pub fn ground_height(p: &Point) -> f32 {
    (p.height as f32 - 8192.0) / 512.0
}

/// A point's water level in layers.
pub fn water_level(p: &Point) -> f32 {
    (p.water_height() as f32 - 8192.0) / 512.0
}

impl Draw {
    /// `extended(t)`: is ground texture `t` (in `grounds`) extended (twice
    /// as wide as high, with 16 more variations)? `water_height`: the
    /// tileset's, over the points' level.
    pub fn new(t: &Terrain, grounds: &Grounds, extended: &dyn Fn(usize) -> bool, water_height: f32) -> Draw {
        let (w, h) = (t.width as usize, t.height as usize);
        let v = t.flags_version();
        let at = |x: usize, y: usize| &t.points[y * w + x];
        let layer = |x: usize, y: usize| at(x, y).layer_height();

        let tile_flags = tile_flags(t);

        let mut heights = Vec::with_capacity(w * h);
        let mut ground = Vec::with_capacity(w * h);
        let mut water = Vec::with_capacity(w * h);
        let mut corners = Vec::with_capacity(w * h);
        for p in &t.points {
            let g = ground_height(p);
            ground.push(g);
            heights.push(g + p.layer_height() as f32 - 2.0);
            water.push(water_level(p) + water_height);
            let mut f = 0;
            for (on, bit) in [
                (p.has_water(v), corner::WATER),
                (p.boundary(v), corner::BOUNDARY),
                (p.edge(), corner::EDGE),
                (p.blight(v), corner::BLIGHT),
                (p.ramp(v), corner::RAMP),
            ] {
                if on {
                    f |= bit;
                }
            }
            corners.push(f);
        }
        // A ramp's foot: its lower corners half a layer up.
        for y in 0..h.saturating_sub(1) {
            for x in 0..w - 1 {
                if tile_flags[y * (w - 1) + x] & tile::RAMP_ENTRANCE == 0 {
                    continue;
                }
                let quad = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
                let base = quad.iter().map(|&(i, j)| layer(i, j)).min().unwrap();
                for (i, j) in quad {
                    if layer(i, j) == base {
                        heights[j * w + i] = ground[j * w + i] + base as f32 - 2.0 + 0.5;
                    }
                }
            }
        }

        let textures = corner_textures(t, grounds, &tile_flags);
        let mut tiles = Vec::with_capacity(tile_flags.len());
        for y in 0..h.saturating_sub(1) {
            for x in 0..w - 1 {
                let (bl, br, tl, tr) = (textures[y * w + x], textures[y * w + x + 1], textures[(y + 1) * w + x], textures[(y + 1) * w + x + 1]);
                let mut u = vec![bl, br, tr, tl];
                u.sort_unstable();
                u.dedup();
                let mut out = [0xFFFF_u32; 4];
                out[0] = u[0] as u32 | (variation(extended(u[0]), at(x, y).ground_variation()) << 16);
                for (k, &id) in u.iter().enumerate().skip(1) {
                    let mask = (br == id) as u32 | ((bl == id) as u32) << 1 | ((tr == id) as u32) << 2 | ((tl == id) as u32) << 3;
                    out[k] = id as u32 | mask << 16;
                }
                tiles.push(out);
            }
        }
        Draw { width: t.width, height: t.height, offset: [t.offset_x, t.offset_y], heights, ground, water, corners, tiles, tile_flags }
    }
}

/// Each tile's flags ([`tile`]), `width - 1` by `height - 1`.
pub fn tile_flags(t: &Terrain) -> Vec<u8> {
    let (w, h) = (t.width as usize, t.height as usize);
    let v = t.flags_version();
    let at = |x: usize, y: usize| &t.points[y * w + x];
    let layer = |x: usize, y: usize| at(x, y).layer_height();
    let ramp = |x: usize, y: usize| at(x, y).ramp(v);
    let mut flags = vec![0u8; w.saturating_sub(1) * h.saturating_sub(1)];
    for y in 0..h.saturating_sub(1) {
        for x in 0..w - 1 {
            let (bl, br, tl, tr) = (layer(x, y), layer(x + 1, y), layer(x, y + 1), layer(x + 1, y + 1));
            let mut f = 0;
            if bl != br || bl != tl || bl != tr {
                f |= tile::CLIFF;
            }
            if ramp(x, y) && ramp(x + 1, y) && ramp(x, y + 1) && ramp(x + 1, y + 1) && !(bl == tr && tl == br) {
                f |= tile::RAMP_ENTRANCE;
            }
            flags[y * (w - 1) + x] = f;
        }
    }
    flags
}

/// Each corner's ground texture as it's drawn (an index into `grounds`):
/// next to a cliff (and not on a ramp), the cliff's ground; else
/// blight's, or its own (HiveWE's `real_tile_texture`).
pub fn corner_textures(t: &Terrain, grounds: &Grounds, tile_flags: &[u8]) -> Vec<usize> {
    let (w, h) = (t.width as usize, t.height as usize);
    let v = t.flags_version();
    let cliff_tile = |x: usize, y: usize| x < w - 1 && y < h - 1 && tile_flags[y * (w - 1) + x] & tile::CLIFF != 0;
    let real = |x: usize, y: usize| -> usize {
        let p = &t.points[y * w + x];
        let near_cliff = cliff_tile(x, y)
            || (x > 0 && cliff_tile(x - 1, y))
            || (y > 0 && cliff_tile(x, y - 1))
            || (x > 0 && y > 0 && cliff_tile(x - 1, y - 1));
        if near_cliff && !p.ramp(v) {
            let mut c = p.cliff_texture() as usize;
            // 15: what the game writes for some; HiveWE's reading.
            if c == 15 {
                c = 1;
            }
            return grounds.cliff_ground.get(c).copied().unwrap_or(0);
        }
        if p.blight(v) { grounds.blight } else { p.texture(v) as usize }
    };
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| real(x, y)).collect()
}

/// The cell of a ground texture a whole tile shows, by the variation its
/// point has (`get_tile_variation`): an extended texture's 16 more, its
/// 16th's full cell, else the first; a plain one's first (0) or full
/// cell (15).
pub fn variation(extended: bool, variation: u8) -> u32 {
    match (extended, variation) {
        (true, 0..=15) => 16 + variation as u32,
        (true, 16) => 15,
        (true, _) => 0,
        (false, 0) => 0,
        (false, _) => 15,
    }
}

#[cfg(test)]
#[path = "terrain.test.rs"]
mod tests;
