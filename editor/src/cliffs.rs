//! Where a terrain's cliffs and ramps stand, and their meshes, as HiveWE's
//! `terrain.ixx` places them (`update_cliff_meshes`).
//!
//! - **A ramp** (`Doodads\Terrain\<rampModelDir>\<rampModelDir><ABCD>0.mdx`,
//!   `CliffTrans`): over two tiles, up or across, whose corners' ramp
//!   flags and layers make one; each corner a letter (top left, top
//!   right, bottom right, bottom left), `A` plus its layer over the lowest
//!   on a plain corner, `L` less four a layer on a ramp's. Its tiles have
//!   no ground of their own ([`tile::ROMP`]).
//! - **A cliff** (`Doodads\Terrain\<cliffModelDir>\<cliffModelDir><ABCD><n>.mdx`,
//!   `Cliffs`): a cliff tile (its corners' layers differ) that isn't a
//!   ramp's foot or under a ramp; each corner `A` plus its layer over the
//!   lowest; `n` the bottom left corner's cliff variation, or the highest
//!   lower one whose model the game has (which ones it has is read from
//!   its files, not a table).
//!
//! The model's folder is its cliff type's (`TerrainArt\CliffTypes.slk`,
//! by the bottom left corner's cliff texture), and so is its texture
//! (`texDir\texFile`). A cliff stands at its tile, its layer the lowest
//! corner's; the page lifts each vertex by the ground under it.

use std::collections::HashMap;

use w3e_rs::Terrain;
use wc3_server::game::Game;

use crate::terrain::{self, tile};
use crate::tileset::Tileset;

/// A cliff or ramp placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Its model, by its index in [`Cliffs::models`].
    pub model: usize,
    pub x: u32,
    pub y: u32,
    /// Its layer, as the terrain's heights count them (`layer - 2`).
    pub base: f32,
    /// Its texture: an index into the terrain's cliff textures.
    pub texture: u32,
}

/// A terrain's cliffs and ramps.
#[derive(Debug, Clone, Default)]
pub struct Cliffs {
    /// The models used, by path, and their files' bytes.
    pub models: Vec<(String, Vec<u8>)>,
    pub placed: Vec<Placed>,
    /// [`terrain::tile_flags`] with [`tile::ROMP`] under the ramps.
    pub tile_flags: Vec<u8>,
    /// Models the game hasn't, each once.
    pub notes: Vec<String>,
}

impl Cliffs {
    pub fn of(game: &Game, t: &Terrain, tileset: &Tileset) -> Cliffs {
        let (w, h) = (t.width as usize, t.height as usize);
        let v = t.flags_version();
        let mut tile_flags = terrain::tile_flags(t);
        let mut out = Cliffs::default();
        if w < 2 || h < 2 {
            out.tile_flags = tile_flags;
            return out;
        }
        let at = |x: usize, y: usize| &t.points[y * w + x];
        let layer = |x: usize, y: usize| at(x, y).layer_height() as i32;
        let ramp = |x: usize, y: usize| at(x, y).ramp(v);
        // A corner's cliff type: its texture's, the last when past them.
        let kind = |x: usize, y: usize| -> u32 {
            let c = at(x, y).cliff_texture() as usize;
            c.min(t.cliffs.len().saturating_sub(1)) as u32
        };
        let dir = |k: u32, column: &str, default: &str| -> String {
            let id = t.cliffs.get(k as usize).map(|c| c.to_string()).unwrap_or_default();
            tileset.cliffs.as_ref().and_then(|s| s.get(&id, column)).unwrap_or(default).to_string()
        };

        let mut files: HashMap<String, Option<usize>> = HashMap::new();
        let mut model = |path: String, out: &mut Cliffs| -> Option<usize> {
            if let Some(found) = files.get(&path.to_ascii_lowercase()) {
                return *found;
            }
            let found = game.read(&path).map(|(bytes, _)| {
                out.models.push((path.clone(), bytes));
                out.models.len() - 1
            });
            files.insert(path.to_ascii_lowercase(), found);
            found
        };
        let mut romp = vec![false; w * h];
        let letter = |x: usize, y: usize, base: i32| -> char {
            let (start, step) = if ramp(x, y) { (b'L' as i32, -4) } else { (b'A' as i32, 1) };
            (start + (layer(x, y) - base) * step) as u8 as char
        };
        let lowest = |quad: [(usize, usize); 4]| quad.iter().map(|&(x, y)| layer(x, y)).min().unwrap();

        // Column by column, as HiveWE goes: a ramp marks the tiles it
        // covers before the next ones are looked at.
        for x in 0..w - 1 {
            for y in 0..h - 1 {
                let (bl, br, tl, tr) = ((x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1));
                let quad = [bl, br, tl, tr];
                let k = kind(x, y);
                let place = |m: usize, out: &mut Cliffs| {
                    out.placed.push(Placed { model: m, x: x as u32, y: y as u32, base: lowest(quad) as f32 - 2.0, texture: k })
                };

                // A ramp up: this tile and the one above.
                if y + 2 < h {
                    let (ttl, ttr) = ((x, y + 2), (x + 1, y + 2));
                    let ae = layer(bl.0, bl.1).min(layer(ttl.0, ttl.1));
                    let cf = layer(br.0, br.1).min(layer(ttr.0, ttr.1));
                    if layer(tl.0, tl.1) == ae
                        && layer(tr.0, tr.1) == cf
                        && ramp(bl.0, bl.1) == ramp(tl.0, tl.1)
                        && ramp(bl.0, bl.1) == ramp(ttl.0, ttl.1)
                        && ramp(br.0, br.1) == ramp(tr.0, tr.1)
                        && ramp(br.0, br.1) == ramp(ttr.0, ttr.1)
                        && ramp(bl.0, bl.1) != ramp(br.0, br.1)
                    {
                        let base = ae.min(cf);
                        let name: String = [ttl, ttr, br, bl].iter().map(|&(cx, cy)| letter(cx, cy, base)).collect();
                        let d = dir(k, "rampModelDir", "CliffTrans");
                        if let Some(m) = model(format!("Doodads\\Terrain\\{d}\\{d}{name}0.mdx"), &mut out) {
                            place(m, &mut out);
                            romp[y * w + x] = true;
                            romp[(y + 1) * w + x] = true;
                            continue;
                        }
                    }
                }
                // A ramp across: this tile and the one to the right.
                if x + 2 < w {
                    let (brr, trr) = ((x + 2, y), (x + 2, y + 1));
                    let ae = layer(bl.0, bl.1).min(layer(brr.0, brr.1));
                    let bf = layer(tl.0, tl.1).min(layer(trr.0, trr.1));
                    if layer(br.0, br.1) == ae
                        && layer(tr.0, tr.1) == bf
                        && ramp(bl.0, bl.1) == ramp(br.0, br.1)
                        && ramp(bl.0, bl.1) == ramp(brr.0, brr.1)
                        && ramp(tl.0, tl.1) == ramp(tr.0, tr.1)
                        && ramp(tl.0, tl.1) == ramp(trr.0, trr.1)
                        && ramp(bl.0, bl.1) != ramp(tl.0, tl.1)
                    {
                        let base = ae.min(bf);
                        let name: String = [tl, trr, brr, bl].iter().map(|&(cx, cy)| letter(cx, cy, base)).collect();
                        let d = dir(k, "rampModelDir", "CliffTrans");
                        if let Some(m) = model(format!("Doodads\\Terrain\\{d}\\{d}{name}0.mdx"), &mut out) {
                            place(m, &mut out);
                            romp[y * w + x] = true;
                            romp[y * w + x + 1] = true;
                            continue;
                        }
                    }
                }

                let flags = tile_flags[y * (w - 1) + x];
                if flags & tile::CLIFF == 0 || romp[y * w + x] || flags & tile::RAMP_ENTRANCE != 0 {
                    continue;
                }
                let base = lowest(quad);
                let name: String = [tl, tr, br, bl].iter().map(|&(cx, cy)| (b'A' + (layer(cx, cy) - base) as u8) as char).collect();
                if name == "AAAA" {
                    continue;
                }
                let d = dir(k, "cliffModelDir", "Cliffs");
                let found = (0..=at(x, y).cliff_variation()).rev().find_map(|n| model(format!("Doodads\\Terrain\\{d}\\{d}{name}{n}.mdx"), &mut out));
                match found {
                    Some(m) => place(m, &mut out),
                    None => {
                        let note = format!("No cliff model Doodads\\Terrain\\{d}\\{d}{name}0.mdx in the game's files or the roots.");
                        if !out.notes.contains(&note) {
                            out.notes.push(note);
                        }
                    }
                }
            }
        }
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                if romp[y * w + x] {
                    tile_flags[y * (w - 1) + x] |= tile::ROMP;
                }
            }
        }
        out.tile_flags = tile_flags;
        out
    }
}

/// A model's first geoset as the page draws it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub path: String,
    pub positions: Vec<f32>,
    pub uvs: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u16>,
    /// Each one placed: x, y, layer, texture.
    pub instances: Vec<f32>,
    /// Why it has nothing to draw; empty when it has.
    pub error: String,
}

/// The meshes of `cliffs`' models, each with where it stands.
pub fn meshes(cliffs: &Cliffs) -> Vec<Mesh> {
    let mut out: Vec<Mesh> = cliffs
        .models
        .iter()
        .map(|(path, bytes)| {
            let mut mesh = Mesh { path: path.clone(), ..Default::default() };
            match mdx_rs::parse(bytes) {
                Err(e) => mesh.error = format!("{path}: {e}"),
                Ok(m) => match m.geosets().first() {
                    None => mesh.error = format!("{path}: no geoset"),
                    Some(g) => {
                        mesh.positions = g.vertices.iter().flatten().copied().collect();
                        mesh.normals = g.normals.iter().flatten().copied().collect();
                        mesh.uvs = g.uv_sets.first().map(|s| s.iter().flatten().copied().collect()).unwrap_or_default();
                        mesh.indices = g.faces.clone();
                        if mesh.uvs.len() / 2 != g.vertices.len() || mesh.normals.len() != mesh.positions.len() {
                            mesh.error = format!("{path}: its normals or texture coordinates don't match its vertices");
                        }
                    }
                },
            }
            mesh
        })
        .collect();
    for p in &cliffs.placed {
        out[p.model].instances.extend([p.x as f32, p.y as f32, p.base, p.texture as f32]);
    }
    out
}

#[cfg(test)]
#[path = "cliffs.test.rs"]
mod tests;
