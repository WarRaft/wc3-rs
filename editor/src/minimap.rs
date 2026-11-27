//! The map's minimap, as World Editor writes it on save: its picture
//! (`war3mapMap.blp`) and its icons (`war3map.mmp`), both over the
//! playable area (the map less its `camera_complements`), 256 across its
//! larger side.
//!
//! The picture: the terrain from above (`picture::paint`) fitted in,
//! its shape kept and centred (the rest black: a playable area 180 by
//! 116 a band across the middle), outside the camera bounds darker, and
//! on it each doodad and destructable the game's tables show on the
//! minimap in its colour (`showInMM`, `useMMColor`, `MMRed` ...: a tree
//! a dark green dot); not the units (the game draws them). Written as
//! World Editor writes it ([`blp`]): a JPEG BLP1, one mip.
//!
//! The icons ([`icons`]): a gold mine (its type's abilities have
//! `Agld`), a neutral building (its type's `nbmmIcon`), a player's start
//! location in its colour ([`PLAYER_COLORS`]); placed as World Editor
//! places them (Blizzard's maps' and the Diablo beta's: over the
//! playable area centred as the picture, x rounded down, y up).

/// The picture's size.
pub const SIZE: u32 = 256;

/// The players' colours, as World Editor writes a start location's
/// icon (red 255,3,3, blue 0,66,255 ...); 24 since 1.29.
pub const PLAYER_COLORS: [[u8; 3]; 24] = [
    [255, 3, 3],
    [0, 66, 255],
    [28, 230, 185],
    [84, 0, 129],
    [255, 252, 1],
    [254, 138, 14],
    [32, 192, 0],
    [229, 91, 176],
    [149, 150, 151],
    [126, 191, 241],
    [16, 98, 70],
    [78, 42, 4],
    [155, 0, 0],
    [0, 0, 195],
    [0, 234, 255],
    [190, 0, 254],
    [235, 205, 135],
    [248, 164, 139],
    [191, 255, 128],
    [220, 185, 235],
    [40, 40, 40],
    [235, 240, 255],
    [0, 120, 30],
    [164, 111, 51],
];

/// An icon's kind (`mmp_rs::KINDS`).
pub const GOLD_MINE: i32 = 0;
pub const NEUTRAL_BUILDING: i32 = 1;
pub const START_LOCATION: i32 = 2;

/// The picture: `picture` the terrain's (a pixel a point, rows from the
/// top), `area` the part shown (cells: left, bottom, right, top: the
/// playable area), `dots` the things on it (cells from the bottom left,
/// a colour each), `bounds` the camera bounds (cells). RGBA, rows from
/// the top.
pub fn image(picture: (u32, u32, &[u8]), area: [f32; 4], dots: &[(f32, f32, [u8; 3])], bounds: Option<[f32; 4]>) -> Vec<u8> {
    let (pw, ph, px) = picture;
    let [al, ab, ar, at_] = area;
    let (cw, ch) = ((ar - al).max(1.0), (at_ - ab).max(1.0));
    let [[iw, x0, sx], [ih, y0, sy]] = fit(cw, ch);
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
            let (cx, cy) = (al + fx / sx, at_ - fy / sy);
            let (x, y) = (cx.round().max(0.0) as u32, cy.round().max(0.0) as u32);
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
    let half = (sx.min(sy) / 2.0).max(0.5);
    for &(x, y, color) in dots {
        let (cx, cy) = (x0 + (x - al) * sx, y0 + (at_ - y) * sy);
        for py in ((cy - half).floor() as i64).max(0)..((cy + half).ceil() as i64).min(SIZE as i64) {
            for qx in ((cx - half).floor() as i64).max(0)..((cx + half).ceil() as i64).min(SIZE as i64) {
                let at = ((py as u32 * SIZE + qx as u32) * 4) as usize;
                out[at..at + 3].copy_from_slice(&color);
            }
        }
    }
    out
}

/// How World Editor fits the playable area (`w` by `h` cells) in the
/// picture: across each side its size in whole pixels (the larger 256,
/// the other `ceil(h / w * 256)`), centred (half a pixel too), each
/// side's own scale. `[size, margin, scale]` across and down. Its icons
/// as World Editor's in 92 of Blizzard's 104 melee maps with icons, the
/// rest a pixel off here and there on their shorter side.
fn fit(w: f32, h: f32) -> [[f32; 3]; 2] {
    let k = SIZE as f32 / w.max(h);
    [w, h].map(|v| {
        let size = (v * k - 1e-4).ceil().min(SIZE as f32);
        [size, (SIZE as f32 - size) / 2.0, size / v.max(1.0)]
    })
}

/// Where an icon goes for a thing at `(x, y)` (cells from the bottom
/// left), the playable area `area` (cells): as World Editor's (`fit`; x
/// rounded down, y up, from the top).
pub fn icon_at(area: [f32; 4], x: f32, y: f32) -> (i32, i32) {
    let [l, b, r, t] = area.map(f64::from);
    let (x, y) = (x as f64, y as f64);
    let [[_, mx, kx], [_, my, ky]] = fit((r - l).max(1.0) as f32, (t - b).max(1.0) as f32).map(|v| v.map(f64::from));
    // (On a pixel's edge exactly: as there, not past it by a rounding.)
    let (kx, ky) = (kx_exact(r - l, kx), kx_exact(t - b, ky));
    ((mx + (x - l) * kx + 1e-6).floor() as i32, (my + (t - y) * ky - 1e-6).ceil() as i32)
}

/// `scale` worked out again in `f64` (its side `v` cells): the size in
/// whole pixels over the side.
fn kx_exact(v: f64, scale: f64) -> f64 {
    let size = (scale * v.max(1.0)).round();
    size / v.max(1.0)
}

/// The icons: each `(kind, x, y, colour)` (cells from the bottom left)
/// placed over `area`.
pub fn icons(area: [f32; 4], things: &[(i32, f32, f32, [u8; 3])]) -> mmp_rs::MinimapIcons {
    let icons = things
        .iter()
        .map(|&(kind, x, y, [red, green, blue])| {
            let (x, y) = icon_at(area, x, y);
            mmp_rs::Icon { kind, x, y, red, green, blue, alpha: 255 }
        })
        .collect();
    mmp_rs::MinimapIcons { version: mmp_rs::VERSION, icons }
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
