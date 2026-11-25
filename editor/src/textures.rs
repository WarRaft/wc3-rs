//! The pictures a terrain is drawn with, decoded here (BLP, TGA:
//! `wc3_server::texture`) into what the page uploads as is.
//!
//! - **Ground**: each ground texture cut into its cells (4 by 4, or 8 by 4
//!   when it's extended), one layer each, all of one size (the biggest
//!   cell; a smaller texture is scaled up), left to right and top to
//!   bottom, an extended texture's right half after its left. A texture
//!   that can't be read is a grey checker of 16 cells, said in a note.
//! - **Water**: its frames, one layer each, of the first frame's size.

use wc3_server::game::Game;

/// Layers of one size, RGBA, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layers {
    pub size: u32,
    pub count: u32,
    pub rgba: Vec<u8>,
}

impl Layers {
    fn push(&mut self, rgba: &[u8], size: u32) {
        debug_assert_eq!(rgba.len(), (size * size * 4) as usize);
        let layer = if size == self.size { rgba.to_vec() } else { scale(rgba, size, self.size) };
        self.rgba.extend_from_slice(&layer);
        self.count += 1;
    }
}

/// A ground texture's place among the layers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroundTexture {
    pub id: String,
    pub path: String,
    pub first: u32,
    pub count: u32,
    pub extended: bool,
    /// Why it isn't the file's picture; empty when it is.
    pub error: String,
}

/// A picture's base mip: width, height, RGBA.
pub type Picture = (u32, u32, Vec<u8>);

fn picture(game: &Game, path: &str) -> Result<Picture, String> {
    let (bytes, _) = game.read(path).ok_or_else(|| format!("{path}: not in the game's files or the roots"))?;
    let t = wc3_server::texture::decode_file(path, &bytes);
    if !t.error.is_empty() {
        return Err(format!("{path}: {}", t.error));
    }
    let mip = t.mips.into_iter().next().ok_or_else(|| format!("{path}: no picture in it"))?;
    if !mip.error.is_empty() || mip.rgba.is_empty() {
        return Err(format!("{path}: {}", mip.error));
    }
    Ok((mip.width, mip.height, mip.rgba))
}

/// A picture's biggest mip no wider or higher than `max`, else its
/// smallest: what a model's texture is drawn with.
pub fn capped(game: &Game, path: &str, max: u32) -> Result<Picture, String> {
    let (bytes, _) = game.read(path).ok_or_else(|| format!("{path}: not in the game's files or the roots"))?;
    let t = wc3_server::texture::decode_file(path, &bytes);
    if !t.error.is_empty() {
        return Err(format!("{path}: {}", t.error));
    }
    let good: Vec<_> = t.mips.into_iter().filter(|m| m.error.is_empty() && !m.rgba.is_empty()).collect();
    let mip = good.iter().find(|m| m.width.max(m.height) <= max).or(good.last()).ok_or_else(|| format!("{path}: no picture in it"))?;
    Ok((mip.width, mip.height, mip.rgba.clone()))
}

/// The average colour of a picture, 0-255: a team's colour from its
/// team colour texture.
pub fn average(game: &Game, path: &str) -> Option<[u8; 3]> {
    let (_, _, rgba) = capped(game, path, 64).ok()?;
    let mut sum = [0u64; 3];
    for px in rgba.chunks_exact(4) {
        for (s, v) in sum.iter_mut().zip(px) {
            *s += *v as u64;
        }
    }
    let n = (rgba.len() / 4).max(1) as u64;
    Some(sum.map(|s| (s / n) as u8))
}

/// Cell `(col, row)` of `size` pixels of a picture `width` wide.
fn cell(rgba: &[u8], width: u32, col: u32, row: u32, size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        let start = (((row * size + y) * width + col * size) * 4) as usize;
        out.extend_from_slice(&rgba[start..start + (size * 4) as usize]);
    }
    out
}

/// A square picture of `from` pixels to `to`, nearest.
fn scale(rgba: &[u8], from: u32, to: u32) -> Vec<u8> {
    stretch(rgba, from, from, to)
}

/// A picture `w` by `h` to a square of `to`, nearest.
fn stretch(rgba: &[u8], w: u32, h: u32, to: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((to * to * 4) as usize);
    for y in 0..to {
        for x in 0..to {
            let (sx, sy) = (x * w / to, y * h / to);
            let at = ((sy * w + sx) * 4) as usize;
            out.extend_from_slice(&rgba[at..at + 4]);
        }
    }
    out
}

/// Whole pictures as layers, each stretched to the biggest one's size
/// (a square); one that can't be read is a grey checker, its error said.
pub fn whole(game: &Game, paths: &[String]) -> (Layers, Vec<String>) {
    let pictures: Vec<Result<Picture, String>> = paths.iter().map(|p| picture(game, p)).collect();
    let size = pictures.iter().filter_map(|p| p.as_ref().ok()).map(|(w, h, _)| *w.max(h)).max().unwrap_or(64);
    let mut layers = Layers { size, ..Default::default() };
    let mut errors = Vec::new();
    for pic in pictures {
        match pic {
            Ok((w, h, rgba)) => {
                layers.rgba.extend(stretch(&rgba, w, h, size));
                layers.count += 1;
                errors.push(String::new());
            }
            Err(e) => {
                layers.push(&checker(size), size);
                errors.push(e);
            }
        }
    }
    (layers, errors)
}

/// The ground textures `paths` (id and path; `None` when the tables don't
/// name one) as layers.
pub fn ground(game: &Game, paths: &[(String, Option<String>)]) -> (Vec<GroundTexture>, Layers) {
    let pictures: Vec<Result<Picture, String>> = paths
        .iter()
        .map(|(id, path)| match path {
            None => Err(format!("{id}: not in TerrainArt\\Terrain.slk")),
            Some(p) => picture(game, p).and_then(|pic| {
                let (w, h, _) = &pic;
                if *h < 4 || (*w != *h && *w != 2 * *h) {
                    Err(format!("{p}: {w} by {h}, neither 4 by 4 cells nor 8 by 4"))
                } else {
                    Ok(pic)
                }
            }),
        })
        .collect();
    let size = pictures.iter().filter_map(|p| p.as_ref().ok()).map(|(_, h, _)| h / 4).max().unwrap_or(64);
    let mut layers = Layers { size, ..Default::default() };
    let mut out = Vec::new();
    for ((id, path), pic) in paths.iter().zip(pictures) {
        let first = layers.count;
        let (extended, error) = match pic {
            Ok((w, h, rgba)) => {
                let cell_size = h / 4;
                let extended = w == 2 * h;
                for half in 0..if extended { 2 } else { 1 } {
                    for row in 0..4 {
                        for col in 0..4 {
                            layers.push(&cell(&rgba, w, half * 4 + col, row, cell_size), cell_size);
                        }
                    }
                }
                (extended, String::new())
            }
            Err(e) => {
                let checker = checker(size);
                for _ in 0..16 {
                    layers.push(&checker, size);
                }
                (false, e)
            }
        };
        out.push(GroundTexture {
            id: id.clone(),
            path: path.clone().unwrap_or_default(),
            first,
            count: layers.count - first,
            extended,
            error,
        });
    }
    (out, layers)
}

fn checker(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let v = if (x * 8 / size + y * 8 / size).is_multiple_of(2) { 90 } else { 140 };
            out.extend_from_slice(&[v, v, v, 255]);
        }
    }
    out
}

/// The water's frames, `<texture>00.blp` ...; the error says what's
/// missing (the frames up to it are kept).
pub fn water(game: &Game, texture: &str, count: u32) -> (Layers, String) {
    let mut layers = Layers::default();
    for i in 0..count {
        let path = format!("{texture}{i:02}.blp");
        match picture(game, &path) {
            Ok((w, h, rgba)) if w == h => {
                if layers.count == 0 {
                    layers.size = w;
                }
                layers.push(&rgba, w);
            }
            Ok((w, h, _)) => return (layers, format!("{path}: {w} by {h}, not square")),
            Err(e) => return (layers, e),
        }
    }
    (layers, String::new())
}

#[cfg(test)]
#[path = "textures.test.rs"]
mod tests;
