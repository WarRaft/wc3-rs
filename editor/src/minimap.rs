//! The map's minimap picture (`war3mapMap.blp`), as World Editor writes
//! it on save: 256 by 256, the terrain from above (`picture::paint`)
//! fitted in it, its shape kept (the rest black: a map 192 by 128 a band
//! across the middle), outside the camera bounds darker, and on it each
//! doodad and destructable the game's tables show on the minimap in its
//! colour (`showInMM`, `useMMColor`, `MMRed` ...: a tree a dark green
//! dot); not the units (the game draws them). Written as World Editor
//! writes it ([`blp`]): a JPEG BLP1, one mip.

/// The picture's size.
pub const SIZE: u32 = 256;

/// The picture: `picture` the terrain's (a pixel a point, `width` by
/// `height` points, rows from the top), `dots` the things on it (cells
/// from the bottom left, a colour each), `bounds` the camera bounds
/// (cells: left, bottom, right, top). RGBA, rows from the top.
pub fn image(picture: (u32, u32, &[u8]), dots: &[(f32, f32, [u8; 3])], bounds: Option<[f32; 4]>) -> Vec<u8> {
    let (pw, ph, px) = picture;
    let (cw, ch) = ((pw.max(2) - 1) as f32, (ph.max(2) - 1) as f32);
    let s = SIZE as f32 / cw.max(ch);
    let (iw, ih) = (cw * s, ch * s);
    let (x0, y0) = ((SIZE as f32 - iw) / 2.0, (SIZE as f32 - ih) / 2.0);
    let mut out = vec![0u8; (SIZE * SIZE * 4) as usize];
    for py in 0..SIZE {
        for qx in 0..SIZE {
            let at = ((py * SIZE + qx) * 4) as usize;
            out[at + 3] = 255;
            let (fx, fy) = (qx as f32 + 0.5 - x0, py as f32 + 0.5 - y0);
            if fx < 0.0 || fy < 0.0 || fx >= iw || fy >= ih {
                continue;
            }
            // The cell's point: x from the left, y from the bottom.
            let (cx, cy) = (fx / s, ch - fy / s);
            let (x, y) = (cx.round() as u32, cy.round() as u32);
            let src = (((ph - 1 - y.min(ph - 1)) * pw + x.min(pw - 1)) * 4) as usize;
            let mut c = [px[src], px[src + 1], px[src + 2]];
            if let Some([l, b, r, t]) = bounds
                && (cx < l || cx > r || cy < b || cy > t)
            {
                c = c.map(|v| (v as f32 * 0.35) as u8);
            }
            out[at..at + 3].copy_from_slice(&c);
        }
    }
    // A dot a thing, a cell across (a pixel at least).
    let half = (s / 2.0).max(0.5);
    for &(x, y, color) in dots {
        let (cx, cy) = (x0 + x * s, y0 + (ch - y) * s);
        for py in ((cy - half).floor() as i64).max(0)..((cy + half).ceil() as i64).min(SIZE as i64) {
            for qx in ((cx - half).floor() as i64).max(0)..((cx + half).ceil() as i64).min(SIZE as i64) {
                let at = ((py as u32 * SIZE + qx as u32) * 4) as usize;
                out[at..at + 3].copy_from_slice(&color);
            }
        }
    }
    out
}

/// `rgba` (`width` by `height`, rows from the top) as World Editor
/// writes the picture: a JPEG BLP1, its one mip (blp-rs, libjpeg-turbo).
pub fn blp(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let image = blp::AnyImage::from_rgba(rgba.to_vec(), width, height).map_err(|e| format!("{e:?}"))?;
    let one = blp::EncodeMipOptions { mip_count: Some(1), ..Default::default() };
    let mut bytes = image.encode(&blp::EncodeOptions::Blp { quality: QUALITY, mip_options: Some(one), raw: None }).map_err(|e| format!("{e:?}"))?;
    // World Editor's header: no alpha, its picture type 5.
    bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&5u32.to_le_bytes());
    Ok(bytes)
}

/// The JPEG's quality.
pub const QUALITY: u8 = 85;

#[cfg(test)]
#[path = "minimap.test.rs"]
mod tests;
