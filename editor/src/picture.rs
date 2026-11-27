//! A picture of a terrain from above, north up: a pixel (or `scale` by
//! `scale`) per corner, the colour of its ground texture as it's drawn
//! (the average of its full cell), lit by the ground's slope as the view
//! lights it, under its water by depth (the tileset's colours), darker on
//! a boundary or past the camera's bounds. The same work the view does
//! ([`crate::terrain`]), for what has no page: `wc3 mcp`'s `w3e_image`.

use wc3_server::game::Game;
use w3e_rs::Terrain;

use crate::terrain::{self, Grounds, corner};
use crate::textures::{GroundTexture, Layers};
use crate::tileset::{Tileset, Water};

/// RGBA, rows top to bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// What was missing (a texture, the water), drawn grey or left out.
    pub notes: Vec<String>,
}

/// The terrain `t` from above, its textures through `game`.
pub fn picture(game: &Game, t: &Terrain, scale: u32) -> Picture {
    let tileset = Tileset::load(game, t.tileset as char);
    let mut notes = tileset.notes.clone();
    let grounds = Grounds::of(t, &tileset);
    let paths: Vec<(String, Option<String>)> = grounds
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), if i == grounds.blight { tileset.blight.clone() } else { tileset.ground_path(id) }))
        .collect();
    let (textures, layers) = crate::textures::ground(game, &paths);
    notes.extend(textures.iter().filter(|g| !g.error.is_empty() && grounds.ids.get(grounds.blight) != Some(&g.id)).map(|g| g.error.clone()));
    let flags = crate::cliffs::Cliffs::of(game, t, &tileset).tile_flags;
    let water = tileset.water.as_ref();
    let water_height = water.map_or(0.0, |w| w.height);
    let draw = terrain::Draw::new(t, &grounds, flags, &|i| textures.get(i).is_some_and(|g| g.extended), water_height);
    let (pw, ph, rgba) = paint(t, &grounds, &textures, &layers, &draw, water, scale);
    if water.is_none() {
        notes.push("No water for this tileset in TerrainArt\\Water.slk: its water isn't drawn.".into());
    }
    Picture { width: pw, height: ph, rgba, notes }
}

/// The picture of terrain `t` as `draw` has it, `scale` pixels a corner,
/// its ground textures `textures` (in `layers`), its tileset's `water`:
/// width, height, RGBA (north up).
pub fn paint(
    t: &Terrain,
    grounds: &Grounds,
    textures: &[GroundTexture],
    layers: &Layers,
    draw: &terrain::Draw,
    water: Option<&Water>,
    scale: u32,
) -> (u32, u32, Vec<u8>) {
    // Each texture's colour: its full cell's average.
    let colors: Vec<[f32; 3]> = textures.iter().map(|g| average(layers, g.first + 15.min(g.count - 1))).collect();

    let real = terrain::corner_textures(t, grounds, &draw.tile_flags);
    let (w, h) = (t.width as usize, t.height as usize);
    let ground = |x: usize, y: usize| draw.ground[y.min(h - 1) * w + x.min(w - 1)];
    let light = [-0.3f32, -0.3, -0.25];
    let l = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();

    let scale = scale.max(1);
    let (pw, ph) = (t.width * scale, t.height * scale);
    let mut rgba = vec![0u8; (pw * ph * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut c = colors.get(real[i]).copied().unwrap_or([0.5; 3]);
            // The view's light on the ground's normal.
            let n = [ground(x.saturating_sub(1), y) - ground(x + 1, y), ground(x, y.saturating_sub(1)) - ground(x, y + 1), 2.0];
            let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let d = -(light[0] * n[0] + light[1] * n[1] + light[2] * n[2]) / (l * nl);
            let lit = ((d + 1.0) * 0.5).clamp(0.0, 1.0);
            c = c.map(|v| v * lit);
            if let Some(wt) = water
                && draw.corners[i] & corner::WATER != 0
                && draw.water[i] > draw.heights[i]
            {
                // Its colour by its depth, as the game's (HiveWE's water
                // shader): shallow from its least to its most at 10 to 64
                // of 128, deep to 72 of 128.
                const MIN_DEPTH: f32 = 10.0 / 128.0;
                const DEEP: f32 = 64.0 / 128.0;
                const MAX_DEPTH: f32 = 72.0 / 128.0;
                let depth = (draw.water[i] - draw.heights[i]).clamp(0.0, 1.0);
                let mix = |a: [u8; 4], b: [u8; 4], t: f32| [0, 1, 2, 3].map(|k| a[k] as f32 * (1.0 - t) + b[k] as f32 * t);
                let [r, g, b, a] = if depth <= DEEP {
                    mix(wt.colors[0], wt.colors[1], (depth - MIN_DEPTH).max(0.0) / (DEEP - MIN_DEPTH))
                } else {
                    mix(wt.colors[2], wt.colors[3], (depth - DEEP).clamp(0.0, MAX_DEPTH - DEEP) / (MAX_DEPTH - DEEP))
                };
                // As the game: the water's texture, tinted by its colour.
                let texture = wt.average.unwrap_or([255; 3]).map(|v| v as f32 / 255.0);
                let (a, over) = (a / 255.0, [r, g, b].map(|v| v / 255.0));
                let over = [0, 1, 2].map(|k| over[k] * texture[k]);
                c = [0, 1, 2].map(|k| over[k] * a + c[k] * (1.0 - a));
            }
            if draw.corners[i] & (corner::BOUNDARY | corner::EDGE) != 0 {
                c = c.map(|v| v * 0.45);
            }
            let px = [c[0], c[1], c[2]].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
            // North up: the top row first.
            let row = (h - 1 - y) as u32;
            for sy in 0..scale {
                for sx in 0..scale {
                    let at = (((row * scale + sy) * pw + x as u32 * scale + sx) * 4) as usize;
                    rgba[at..at + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
        }
    }
    (pw, ph, rgba)
}

/// What [`mark`] draws over a picture.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Marks {
    /// The regions' outlines, in their colours.
    pub regions: bool,
    /// Units and items: a dot each, a unit in its owner's colour.
    pub units: bool,
    /// The camera's bounds (blue) and the playable area (yellow).
    pub bounds: bool,
}

/// Draws `marks` over `p`, a picture of terrain `t` at `scale` pixels a
/// corner, from the map's files through `game`.
pub fn mark(game: &Game, t: &Terrain, p: &mut Picture, scale: u32, marks: Marks) {
    let scale = scale.max(1) as f32;
    let (w, h) = (p.width as i64, p.height as i64);
    // A point in the game's units to a pixel (north up).
    let px = |wx: f32, wy: f32| -> (i64, i64) {
        let (cx, cy) = ((wx - t.offset_x) / 128.0, (wy - t.offset_y) / 128.0);
        ((cx * scale).round() as i64, h - 1 - (cy * scale).round() as i64)
    };
    let mut put = |x: i64, y: i64, c: [u8; 3]| {
        if x >= 0 && y >= 0 && x < w && y < h {
            let at = ((y * w + x) * 4) as usize;
            p.rgba[at..at + 3].copy_from_slice(&c);
        }
    };
    let rect = |put: &mut dyn FnMut(i64, i64, [u8; 3]), (l, b): (i64, i64), (r, tp): (i64, i64), c: [u8; 3]| {
        for x in l.min(r)..=l.max(r) {
            put(x, b, c);
            put(x, tp, c);
        }
        for y in tp.min(b)..=tp.max(b) {
            put(l, y, c);
            put(r, y, c);
        }
    };
    let read = |name: &str| game.read(name).map(|(b, _)| b);
    if marks.bounds
        && let Some(Ok(info)) = read("war3map.w3i").map(|b| w3i_rs::parse(&b))
    {
        let c = info.camera_bounds;
        rect(&mut put, px(c[0], c[1]), px(c[2], c[3]), [90, 150, 255]);
        let k = info.camera_complements.map(|v| v as f32 * 128.0);
        let (right, top) = (t.offset_x + (t.width as f32 - 1.0) * 128.0, t.offset_y + (t.height as f32 - 1.0) * 128.0);
        rect(&mut put, px(t.offset_x + k[0], t.offset_y + k[2]), px(right - k[1], top - k[3]), [255, 215, 50]);
    }
    if marks.regions
        && let Some(Ok(rs)) = read("war3map.w3r").map(|b| w3r_rs::parse(&b))
    {
        for r in &rs.regions {
            rect(&mut put, px(r.left, r.bottom), px(r.right, r.top), [r.red, r.green, r.blue]);
        }
    }
    if marks.units
        && let Some(Ok(units)) = read("war3mapUnits.doo").map(|b| doo_rs::parse_units_any(&b))
    {
        let mut colors = std::collections::HashMap::new();
        let types = crate::types::Types::load(game);
        for u in &units.units {
            let id = u.placement.id.to_string();
            let c = if types.has(w3abdhqtu_rs::Kind::Item, &id) {
                [255, 255, 255]
            } else {
                *colors.entry(u.player).or_insert_with(|| {
                    crate::textures::average(game, &format!("ReplaceableTextures\\TeamColor\\TeamColor{:02}.blp", u.player)).unwrap_or([255, 0, 255])
                })
            };
            let (x, y) = px(u.placement.x, u.placement.y);
            let r = (scale as i64 / 2).max(1);
            for dy in -r..=r {
                for dx in -r..=r {
                    put(x + dx, y + dy, c);
                }
            }
        }
    }
}

/// Layer `n`'s average colour, 0-1.
fn average(layers: &crate::textures::Layers, n: u32) -> [f32; 3] {
    let size = (layers.size * layers.size * 4) as usize;
    let Some(layer) = layers.rgba.get(n as usize * size..(n as usize + 1) * size) else { return [0.5; 3] };
    let mut sum = [0f64; 3];
    for px in layer.chunks_exact(4) {
        for (s, v) in sum.iter_mut().zip(px) {
            *s += *v as f64;
        }
    }
    let count = (layer.len() / 4).max(1) as f64;
    sum.map(|s| (s / count / 255.0) as f32)
}

#[cfg(test)]
#[path = "picture.test.rs"]
mod tests;
