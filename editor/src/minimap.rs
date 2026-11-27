//! The map's minimap picture (`war3mapMap.blp`), as World Editor writes
//! it on save: 256 by 256, the terrain from above (`picture::paint`)
//! fitted in it, its shape kept (the rest black: a map 192 by 128 a band
//! across the middle), outside the camera bounds darker, and on it each
//! doodad and destructable the game's tables show on the minimap in its
//! colour (`showInMM`, `useMMColor`, `MMRed` ...: a tree a dark green
//! dot); not the units (the game draws them). Written as a paletted
//! BLP1 ([`blp`]): World Editor's is a JPEG one, which needs a JPEG
//! encoder this build hasn't; the game reads both.

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

/// `rgba` (`width` by `height`, rows from the top) as a paletted BLP1:
/// no alpha, no mipmaps, its 256 colours by median cut.
pub fn blp(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let pixels: Vec<[u8; 3]> = rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
    let (palette, index) = quantize(&pixels);
    let mut out = Vec::with_capacity(1180 + pixels.len());
    let u32le = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"BLP1");
    // Paletted, no alpha, its size, a picture type of 5 (World Editor's), no mipmaps.
    for v in [1, 0, width, height, 5, 0] {
        u32le(&mut out, v);
    }
    let data_at = 28 + 64 + 64 + 1024;
    u32le(&mut out, data_at);
    for _ in 1..16 {
        u32le(&mut out, 0);
    }
    u32le(&mut out, pixels.len() as u32);
    for _ in 1..16 {
        u32le(&mut out, 0);
    }
    // The palette: blue, green, red, 0.
    for i in 0..256 {
        let [r, g, b] = palette.get(i).copied().unwrap_or([0, 0, 0]);
        out.extend_from_slice(&[b, g, r, 0]);
    }
    out.extend(index);
    out
}

/// 256 colours at most for `pixels` (median cut: the box of the most
/// colours, split at its widest channel's median, until 256), and each
/// pixel's colour among them.
fn quantize(pixels: &[[u8; 3]]) -> (Vec<[u8; 3]>, Vec<u8>) {
    let mut colors: Vec<[u8; 3]> = pixels.to_vec();
    colors.sort_unstable();
    colors.dedup();
    let mut boxes: Vec<Vec<[u8; 3]>> = vec![colors];
    while boxes.len() < 256 {
        let Some((i, _)) = boxes.iter().enumerate().filter(|(_, b)| b.len() > 1).max_by_key(|(_, b)| b.len()) else { break };
        let mut b = boxes.swap_remove(i);
        let range = |c: usize| b.iter().map(|p| p[c]).max().unwrap() - b.iter().map(|p| p[c]).min().unwrap();
        let channel = (0..3).max_by_key(|&c| range(c)).unwrap();
        b.sort_unstable_by_key(|p| p[channel]);
        let upper = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(upper);
    }
    let palette: Vec<[u8; 3]> = boxes
        .iter()
        .map(|b| {
            let n = b.len().max(1) as u32;
            let sum = b.iter().fold([0u32; 3], |s, p| [s[0] + p[0] as u32, s[1] + p[1] as u32, s[2] + p[2] as u32]);
            sum.map(|v| (v / n) as u8)
        })
        .collect();
    let mut nearest = std::collections::HashMap::new();
    let index = pixels
        .iter()
        .map(|p| {
            *nearest.entry(*p).or_insert_with(|| {
                let d = |c: &[u8; 3]| (0..3).map(|k| (c[k] as i32 - p[k] as i32).pow(2)).sum::<i32>();
                (0..palette.len()).min_by_key(|&k| d(&palette[k])).unwrap_or(0) as u8
            })
        })
        .collect();
    (palette, index)
}

#[cfg(test)]
#[path = "minimap.test.rs"]
mod tests;
