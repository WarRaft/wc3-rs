//! Pictures for the editor's viewer: a BLP decoded with `blp-rs` into what
//! the page draws, its header and every mip as RGBA (a DDS, Reforged's,
//! with `blp-rs`'s `dds` too); a TGA (map previews,
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

/// A small picture (an editor's icon) as a `data:image/png;base64,...`
/// URL, for a page to show as it is: the base mip of a BLP (or a TGA);
/// `None` when it doesn't decode.
pub fn png_data_url(bytes: &[u8]) -> Option<String> {
    use image::ImageEncoder;
    let t = decode(bytes);
    let mip = t.mips.first().filter(|m| !m.rgba.is_empty())?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&mip.rgba, mip.width, mip.height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

/// Standard Base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes the picture file `name` with bytes `bytes`: a shadow map by
/// its name (`.shd`, which has no header: its size is its terrain's, the
/// `war3map.w3e` beside it, on disk or in the same map), else as
/// [`decode`] does.
pub fn decode_file(name: &str, bytes: &[u8]) -> Texture {
    let path = std::path::Path::new(name);
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("shd")) {
        return shadows(path, bytes);
    }
    decode(bytes)
}

/// A shadow map: dark where there's shadow, its cells for the page (one
/// flag, `shadow`).
fn shadows(path: &std::path::Path, bytes: &[u8]) -> Texture {
    let failed = |error: String| Texture { error, ..Default::default() };
    let Some(w3e) = path.parent().map(|p| p.join("war3map.w3e")) else {
        return failed("A shadow map's size is its terrain's, and there's no war3map.w3e beside it.".into());
    };
    let terrain = if w3e.is_file() { std::fs::read(&w3e).ok() } else { crate::map::bytes(&w3e) };
    let Some(terrain) = terrain.and_then(|b| w3e_rs::parse(&b).ok()) else {
        return failed("A shadow map's size is its terrain's: war3map.w3e beside it can't be read.".into());
    };
    let (width, height) = shd_rs::size_for_terrain(terrain.width, terrain.height);
    let map = match shd_rs::parse(bytes, width, height) {
        Ok(map) => map,
        Err(err) => return failed(format!("Can't read this shadow map: {err}")),
    };
    let row = width.max(1) as usize;
    Texture {
        error: String::new(),
        version: "Shadows".into(),
        encoding: "Shadows".into(),
        alpha_bits: 0,
        width,
        height,
        mips: vec![Mip { width, height, error: String::new(), rgba: map.to_rgba() }],
        cells: map.cells.chunks(row).rev().flatten().map(|&c| u8::from(c != 0)).collect(),
        flags: vec![(0x01, "shadow".into())],
    }
}

/// Decodes a BLP file's bytes, or a pathing map's (by its `MP3W`).
pub fn decode(bytes: &[u8]) -> Texture {
    let failed = |error: String| Texture { error, ..Default::default() };
    if bytes.starts_with(&wpm_rs::MAGIC) {
        return pathing(bytes);
    }
    if blp::dds::is_dds(bytes) {
        return dds(bytes);
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

/// A DDS (Reforged's textures: `blp::dds`), every mip.
fn dds(bytes: &[u8]) -> Texture {
    let d = match blp::dds::Dds::parse(bytes) {
        Ok(d) => d,
        Err(e) => return Texture { error: format!("Can't read this DDS: {e}"), ..Default::default() },
    };
    let mips = d
        .mips
        .iter()
        .map(|m| match d.decode(m, bytes) {
            Ok(rgba) => Mip { width: m.width, height: m.height, error: String::new(), rgba },
            Err(e) => Mip { width: m.width, height: m.height, error: e.to_string(), rgba: Vec::new() },
        })
        .collect();
    Texture { version: "DDS".into(), encoding: d.format.to_string(), alpha_bits: 8, width: d.width, height: d.height, mips, ..Default::default() }
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
