use super::*;
use crate::testdir::{self, TestDir};

#[test]
fn a_pixel_per_corner_north_up_in_its_textures_colour() {
    let dir = TestDir::new("picture");
    dir.write("TerrainArt/Terrain.slk", &testdir::slk(&[&["tileID", "dir", "file"], &["Ldrt", "T", "Dirt"]]));
    dir.write("TerrainArt/CliffTypes.slk", &testdir::slk(&[&["cliffID", "groundTile"], &["CLdi", "Ldrt"]]));
    // Its full cell (15: column 3, row 3) red, the rest blue.
    dir.write("T/Dirt.blp", &testdir::tga(16, 16, |x, y| if x >= 12 && y >= 12 { [200, 0, 0, 255] } else { [0, 0, 200, 255] }));
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()] }.open().0;
    let mut t = w3e_rs::parse(&testdir::terrain()).unwrap();
    // The top left corner a boundary: darker.
    let v = t.flags_version();
    t.point_mut(0, 1).unwrap().set_boundary(v, true);
    let p = picture(&game, &t, 2);
    assert_eq!((p.width, p.height), (6, 4));
    let px = |x: u32, y: u32| &p.rgba[((y * p.width + x) * 4) as usize..][..4];
    // Flat ground: lit at (0.25/0.49 + 1) / 2 of its colour.
    let lit = ((0.25f32 / (0.3f32 * 0.3 + 0.3 * 0.3 + 0.25 * 0.25).sqrt() + 1.0) * 0.5 * 200.0).round() as u8;
    assert_eq!(px(5, 3), [lit, 0, 0, 255]);
    // The top row is the terrain's top: its left corner the boundary.
    assert!(px(0, 0)[0] < lit / 2 + 1 && px(1, 1) == px(0, 0));
    assert!(p.notes.iter().any(|n| n.contains("Water.slk")), "{:?}", p.notes);
}
