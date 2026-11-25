//! A picture of a terrain from above, north up: a pixel (or `scale` by
//! `scale`) per corner, the colour of its ground texture as it's drawn
//! (the average of its full cell), lit by the ground's slope as the view
//! lights it, under its water by depth (the tileset's colours), darker on
//! a boundary or past the camera's bounds. The same work the view does
//! ([`crate::terrain`]), for what has no page: `wc3 mcp`'s `w3e_image`.

use wc3_server::game::Game;
use w3e_rs::Terrain;

use crate::terrain::{self, Grounds, corner};
use crate::tileset::Tileset;

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
    // Each texture's colour: its full cell's average.
    let colors: Vec<[f32; 3]> = textures.iter().map(|g| average(&layers, g.first + 15.min(g.count - 1))).collect();

    let flags = terrain::tile_flags(t);
    let real = terrain::corner_textures(t, &grounds, &flags);
    let water = tileset.water.as_ref();
    let water_height = water.map_or(0.0, |w| w.height);
    let draw = terrain::Draw::new(t, &grounds, &|i| textures.get(i).is_some_and(|g| g.extended), water_height);
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
                let depth = (draw.water[i] - draw.heights[i]).clamp(0.0, 1.0);
                let [r, g, b, a] = if depth <= 0.5 { wt.colors[1] } else { wt.colors[3] };
                let (a, over) = (a as f32 / 255.0, [r, g, b].map(|v| v as f32 / 255.0));
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
    if water.is_none() {
        notes.push("No water for this tileset in TerrainArt\\Water.slk: its water isn't drawn.".into());
    }
    Picture { width: pw, height: ph, rgba, notes }
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
