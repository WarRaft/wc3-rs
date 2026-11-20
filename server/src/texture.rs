//! Textures for the editor's viewer: a BLP decoded with `blp-rs` into what
//! the page draws, its header and every mip as RGBA.
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
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mip {
    pub width: u32,
    pub height: u32,
    pub error: String,
    /// `width * height * 4` bytes, rows top to bottom; empty on error.
    pub rgba: Vec<u8>,
}

/// Decodes a BLP file's bytes.
pub fn decode(bytes: &[u8]) -> Texture {
    let failed = |error: String| Texture { error, ..Default::default() };
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
    }
}

#[cfg(test)]
#[path = "texture.test.rs"]
mod tests;
