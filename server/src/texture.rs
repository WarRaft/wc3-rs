//! Pictures for the editor's viewer: a BLP decoded with `blp-rs` into what
//! the page draws, its header and every mip as RGBA; a TGA (map previews,
//! loading screens, model textures) with the `image` crate; and a pathing map
//! (`war3map.wpm`, `wpm-rs`), which has no text, drawn as the game's
//! pathing textures draw pathing, with each cell's flags for the page to
//! show one flag at a time and name under the cursor.
//!
//! The editor reads the file itself (from disk or from inside a map) and
//! sends its bytes in a `Texture` message; the answer carries everything,
//! so the page never asks again to switch mips.

use blp::Blp;

/// A decoded texture. When it can't be read at all, `error` says why and
/// the rest is empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Texture {
    pub error: String,
    /// `BLP1`, `BLP2`.
    pub version: String,
    /// `JPEG`, `Palette`, `DXT1`, …
    pub encoding: String,
    pub alpha_bits: u8,
    pub width: u32,
    pub height: u32,
    /// The base mip first. A mip the file doesn't have, or that doesn't
    /// decode, has no pixels and says why.
    pub mips: Vec<Mip>,
    /// A pathing map's cells, one byte of flags per pixel of its one mip,
    /// in the same order (rows top to bottom); empty for a texture.
    pub cells: Vec<u8>,
    /// A pathing map's flags: each bit and its name (`wpm_rs::FLAGS`).
    pub flags: Vec<(u8, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mip {
    pub width: u32,
    pub height: u32,
    pub error: String,
    /// `width * height * 4` bytes, rows top to bottom; empty on error.
    pub rgba: Vec<u8>,
}

/// Decodes a BLP file's bytes, or a pathing map's (by its `MP3W`).
pub fn decode(bytes: &[u8]) -> Texture {
    let failed = |error: String| Texture { error, ..Default::default() };
    if bytes.starts_with(&wpm_rs::MAGIC) {
        return pathing(bytes);
    }
    if !<Blp as blp::FormatDetector>::detect(bytes) && looks_like_tga(bytes) {
        return tga(bytes);
    }
    if !<Blp as blp::FormatDetector>::detect(bytes) {
        return failed("This isn't a BLP file: it doesn't start with BLP0, BLP1 or BLP2.".into());
    }
    let (blp, frames) = match Blp::parse(bytes) {
        Ok(parsed) => parsed,
        Err(err) => return failed(format!("The header is damaged: {err}")),
    };
    let encoding = match blp.encoding() {
        Ok(e) => e.to_string(),
        Err(err) => return failed(format!("This kind of BLP isn't supported: {err}")),
    };
    let mips = frames
        .iter()
        .map(|frame| {
            let mut mip = Mip { width: frame.width, height: frame.height, ..Default::default() };
            if frame.length == 0 {
                mip.error = if blp.version == blp::Version::BLP0 { "kept in a file of its own (.b00…)".into() } else { "not in the file".into() };
            } else {
                match blp.decode(frame, bytes) {
                    Ok(img) => mip.rgba = img.into_raw(),
                    Err(err) => mip.error = err.to_string(),
                }
            }
            mip
        })
        .collect();
    Texture {
        error: String::new(),
        version: format!("{:?}", blp.version),
        encoding,
        alpha_bits: blp.alpha_bits.min(u8::MAX as u32) as u8,
        width: blp.width,
        height: blp.height,
        mips,
        cells: Vec::new(),
        flags: Vec::new(),
    }
}

/// Does `bytes` start like a TGA? It has no signature: its header says
/// a known image type (color-mapped, true-color or grayscale, plain or
/// RLE), a color map flag of 0 or 1, and a pixel depth TGA has.
pub fn looks_like_tga(bytes: &[u8]) -> bool {
    bytes.len() >= 18
        && bytes[1] <= 1
        && matches!(bytes[2], 1 | 2 | 3 | 9 | 10 | 11)
        && matches!(bytes[16], 8 | 15 | 16 | 24 | 32)
}

/// A TGA: its one image, as the `image` crate reads it.
fn tga(bytes: &[u8]) -> Texture {
    let kind = match bytes[2] {
        1 => "Color-mapped",
        2 => "True-color",
        3 => "Grayscale",
        9 => "Color-mapped, RLE",
        10 => "True-color, RLE",
        _ => "Grayscale, RLE",
    };
    let alpha_bits = bytes[17] & 0x0F;
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Tga) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            Texture {
                error: String::new(),
                version: "TGA".into(),
                encoding: format!("{kind} {}-bit", bytes[16]),
                alpha_bits,
                width,
                height,
                mips: vec![Mip { width, height, error: String::new(), rgba: rgba.into_raw() }],
                cells: Vec::new(),
                flags: Vec::new(),
            }
        }
        Err(err) => Texture { error: format!("Can't read this TGA: {err}"), ..Default::default() },
    }
}

/// A pathing map: its picture (red: no walking, green: no flying, blue:
/// no building), and its cells' flags in the picture's order.
fn pathing(bytes: &[u8]) -> Texture {
    let map = match wpm_rs::parse(bytes) {
        Ok(map) => map,
        Err(err) => return Texture { error: format!("Can't read this pathing map: {err}"), ..Default::default() },
    };
    let width = map.width.max(1) as usize;
    let cells: Vec<u8> = map.cells.chunks(width).rev().flatten().copied().collect();
    Texture {
        error: String::new(),
        version: format!("MP3W {}", map.version),
        encoding: "Pathing".into(),
        alpha_bits: 0,
        width: map.width,
        height: map.height,
        mips: vec![Mip { width: map.width, height: map.height, error: String::new(), rgba: map.to_rgba() }],
        cells,
        flags: wpm_rs::FLAGS.iter().map(|(bit, name)| (*bit, name.to_string())).collect(),
    }
}

#[cfg(test)]
#[path = "texture.test.rs"]
mod tests;
