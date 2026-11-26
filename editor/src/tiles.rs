//! The map's ground tiles: the tile setter (its list of ground textures
//! and its tileset, as World Editor's "Modify Tileset" and HiveWE's
//! `TileSetter` change them) and the tile pather (a tile's walking,
//! flying, building, over the pathing where it lies).
//!
//! The list: at most as many as the terrain's version has room for (16,
//! 64 since 1.31), each a row of `TerrainArt\Terrain.slk`; a tile taken
//! out is replaced on the points that have it by one that stays. The
//! cliffs follow: a cliff type for each tile that's one's
//! (`TerrainArt\CliffTypes.slk` `groundTile`), at most 15; a point's
//! cliff the same type, else the cliff of the tile its old one's tile
//! went to, else the first.

use parser_rs::bin::Rawcode;
use w3e_rs::Terrain;

use crate::tileset::Tileset;

/// The cliff types a terrain has room for.
pub const MAX_CLIFFS: usize = 15;

/// A tile of the game's: its id, its tileset's letter, whether it's
/// under a cliff type, its pathing (`wpm_rs` flags: what it forbids).
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub id: String,
    pub cliff: Option<String>,
    pub pathing: u8,
}

/// Every ground tile of the game's tables (`Terrain.slk`), by id.
pub fn all(tileset: &Tileset) -> Vec<Tile> {
    let Some(t) = tileset.terrain.as_ref() else { return Vec::new() };
    let mut out: Vec<Tile> = t.rows().map(|(id, _)| Tile { id: id.clone(), cliff: cliff_of(tileset, id), pathing: pathing(tileset, id) }).filter(|t| t.id.len() == 4).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The cliff type whose ground tile `id` is.
pub fn cliff_of(tileset: &Tileset, id: &str) -> Option<String> {
    let c = tileset.cliffs.as_ref()?;
    let mut ids: Vec<&String> = c.rows().filter(|(_, r)| r.get("groundtile").is_some_and(|g| g == id)).map(|(k, _)| k).collect();
    ids.sort();
    ids.first().map(|s| s.to_string())
}

/// What tile `id` forbids (`Terrain.slk` `walkable`, `flyable`,
/// `buildable`: 0).
pub fn pathing(tileset: &Tileset, id: &str) -> u8 {
    let no = |k: &str| tileset.terrain.as_ref().and_then(|s| s.number(id, k)).is_some_and(|n| n == 0.0);
    (if no("walkable") { wpm_rs::NO_WALK } else { 0 }) | (if no("flyable") { wpm_rs::NO_FLY } else { 0 }) | (if no("buildable") { wpm_rs::NO_BUILD } else { 0 })
}

/// The terrain's tiles set to `ids` and its tileset to `letter`: each
/// old tile's points to the new one `to[old]`; the cliffs after them.
pub fn set(t: &mut Terrain, tileset: &Tileset, ids: &[String], to: &[u8], letter: u8) -> Result<(), String> {
    let v = t.flags_version();
    if ids.is_empty() || ids.len() > v.textures() {
        return Err(format!("A terrain of this version has 1 to {} ground tiles; {} given.", v.textures(), ids.len()));
    }
    let known = tileset.terrain.as_ref();
    let mut codes = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            return Err(format!("{id} is in the list twice."));
        }
        if known.is_some_and(|k| k.get(id, "dir").is_none()) {
            return Err(format!("{id} isn't a tile of TerrainArt\\Terrain.slk."));
        }
        codes.push(Rawcode::parse(id).ok_or_else(|| format!("{id} isn't a tile's id."))?);
    }
    if to.len() != t.ground.len() || to.iter().any(|n| *n as usize >= ids.len()) {
        return Err(format!("Each of the {} tiles the terrain has goes to one of the {} new ones.", t.ground.len(), ids.len()));
    }
    // The cliffs: one for each tile that's a cliff's.
    let mut cliffs: Vec<String> = Vec::new();
    for id in ids {
        if let Some(c) = cliff_of(tileset, id)
            && !cliffs.contains(&c)
            && cliffs.len() < MAX_CLIFFS
        {
            cliffs.push(c);
        }
    }
    if cliffs.is_empty() {
        return Err("No tile of the list is under a cliff type (TerrainArt\\CliffTypes.slk groundTile): a terrain has at least one.".into());
    }
    let old_ground: Vec<String> = t.ground.iter().map(|g| g.to_string()).collect();
    let cliff_to: Vec<u8> = t
        .cliffs
        .iter()
        .map(|c| {
            let c = c.to_string();
            if let Some(i) = cliffs.iter().position(|n| *n == c) {
                return i as u8;
            }
            // Its tile's new tile's cliff.
            tileset
                .cliff_ground(&c)
                .and_then(|g| old_ground.iter().position(|o| o == g))
                .and_then(|o| cliff_of(tileset, &ids[to[o] as usize]))
                .and_then(|n| cliffs.iter().position(|x| *x == n))
                .unwrap_or(0) as u8
        })
        .collect();
    for p in &mut t.points {
        let g = p.texture(v) as usize;
        p.set_texture(v, to.get(g).copied().unwrap_or(0) as u16);
        let c = p.cliff_texture() as usize;
        // 15: no cliff's (kept).
        if c != 15 {
            p.set_cliff_texture(cliff_to.get(c).copied().unwrap_or(0));
        }
    }
    t.ground = codes;
    t.cliffs = cliffs.iter().filter_map(|c| Rawcode::parse(c)).collect();
    t.tileset = letter;
    Ok(())
}

/// The pathing cells whose nearest point has ground tile `index` and no
/// cliff over it (as `pathing::compute` takes a cell's tile).
pub fn cells_of(t: &Terrain, draw: &crate::terrain::Draw, index: usize) -> Vec<usize> {
    let (w, h) = (t.width as usize, t.height as usize);
    let (pw, ph) = ((w - 1) * 4, (h - 1) * 4);
    let v = t.flags_version();
    let mut out = Vec::new();
    for j in 0..ph {
        for i in 0..pw {
            let (nx, ny) = (((i as f32) / 4.0).round() as usize, ((j as f32) / 4.0).round() as usize);
            let near = &t.points[ny.min(h - 1) * w + nx.min(w - 1)];
            let flags = draw.tile_flags.get((j / 4) * (w - 1) + i / 4).copied().unwrap_or(0);
            if near.texture(v) as usize == index && flags & (crate::terrain::tile::CLIFF | crate::terrain::tile::ROMP) == 0 {
                out.push(j * pw + i);
            }
        }
    }
    out
}

/// The tile pather: on `cells`, the walking, flying and building flags
/// as `made` has them (the pathing the files make with the tile's
/// pathing changed); the rest of each cell's flags kept.
pub fn repath(p: &mut wpm_rs::Pathing, made: &wpm_rs::Pathing, cells: &[usize]) {
    let mask = wpm_rs::NO_WALK | wpm_rs::NO_FLY | wpm_rs::NO_BUILD;
    for &c in cells {
        let (Some(x), Some(m)) = (p.cells.get_mut(c), made.cells.get(c)) else { continue };
        *x = (*x & !mask) | (m & mask);
        // 0x80: land nothing walks on.
        if *x & wpm_rs::NO_WALK != 0 && *x & wpm_rs::NO_WATER != 0 {
            *x |= 0x80
        } else {
            *x &= !0x80
        }
    }
}

#[cfg(test)]
#[path = "tiles.test.rs"]
mod tests;
