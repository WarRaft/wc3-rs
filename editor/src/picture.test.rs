use super::*;
use crate::testdir::{self, TestDir};

#[test]
fn a_pixel_per_corner_north_up_in_its_textures_colour() {
    let dir = TestDir::new("picture");
    dir.write("TerrainArt/Terrain.slk", &testdir::slk(&[&["tileID", "dir", "file"], &["Ldrt", "T", "Dirt"]]));
    dir.write("TerrainArt/CliffTypes.slk", &testdir::slk(&[&["cliffID", "groundTile"], &["CLdi", "Ldrt"]]));
    // Its full cell (15: column 3, row 3) red, the rest blue.
    dir.write("T/Dirt.blp", &testdir::tga(16, 16, |x, y| if x >= 12 && y >= 12 { [200, 0, 0, 255] } else { [0, 0, 200, 255] }));
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
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

#[test]
fn regions_and_units_marked_over_it() {
    let dir = TestDir::new("picture-marks");
    // A region over the whole terrain (-128..128 across, -64..64 up), green.
    let mut regions = w3r_rs::Regions::new();
    let mut r = w3r_rs::Region::new("All", -128.0, -64.0, 128.0, 64.0, 0);
    (r.red, r.green, r.blue) = (0, 255, 0);
    regions.regions.push(r);
    dir.write("war3map.w3r", &regions.to_bytes().unwrap());
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let t = w3e_rs::parse(&testdir::terrain()).unwrap();
    let mut p = Picture { width: 6, height: 4, rgba: vec![0; 6 * 4 * 4], notes: vec![] };
    mark(&game, &t, &mut p, 2, Marks { regions: true, ..Default::default() });
    let px = |x: u32, y: u32| &p.rgba[((y * 6 + x) * 4) as usize..][..3];
    // Its outline: the corners' pixels; inside it, nothing.
    assert_eq!(px(0, 3), [0, 255, 0]);
    assert_eq!(px(4, 1), [0, 255, 0]);
    assert_eq!(px(2, 2), [0, 0, 0]);
}
