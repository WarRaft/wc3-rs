use super::*;
use crate::testdir::{self, TestDir};
use mdx_rs::{Chunk, Extent, Fixed, Geoset, Model};
use w3e_rs::Point;

/// A model of one triangle.
fn triangle() -> Vec<u8> {
    let g = Geoset {
        vertices: vec![[0.0, 0.0, 0.0], [128.0, 0.0, 0.0], [0.0, 128.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        face_types: vec![4],
        face_groups: vec![3],
        faces: vec![0, 1, 2],
        vertex_groups: vec![0; 3],
        matrix_groups: vec![1],
        matrix_indices: vec![0],
        extent: Extent::default(),
        uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
        ..Default::default()
    };
    let _ = Fixed::new("", 0);
    Model { chunks: vec![Chunk::Version(800), Chunk::Geosets(vec![g])] }.to_bytes().unwrap()
}

/// `w` by `h` points, flat at layer 2, then `point` on each.
fn terrain(w: u32, h: u32, point: impl Fn(u32, u32, &mut Point)) -> Terrain {
    let mut t = w3e_rs::parse(&testdir::terrain()).unwrap();
    t.width = w;
    t.height = h;
    t.points = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let mut p = Point { height: 0x2000, water: 0x2000, flags: 0, variation: 0, layer: 2 };
            point(x, y, &mut p);
            p
        })
        .collect();
    t
}

fn setup(dir: &TestDir) -> (Game, Tileset) {
    dir.write("TerrainArt/CliffTypes.slk", &testdir::slk(&[&["cliffID", "cliffModelDir", "rampModelDir", "groundTile"], &["CLdi", "Cliffs", "CliffTrans", "Ldrt"]]));
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let tileset = Tileset::load(&game, 'L');
    (game, tileset)
}

#[test]
fn a_cliff_by_its_corners_layers_and_the_variation_the_game_has() {
    let dir = TestDir::new("cliffs");
    dir.write("Doodads/Terrain/Cliffs/CliffsABBA0.mdx", &triangle());
    dir.write("Doodads/Terrain/Cliffs/CliffsABBA1.mdx", &triangle());
    let (game, tileset) = setup(&dir);
    // The right column a layer up; the cliff's corner asks for variation 3.
    let t = terrain(3, 2, |x, _, p| {
        if x == 2 {
            p.set_layer_height(3);
        }
        if x == 1 {
            p.set_cliff_variation(3);
        }
    });
    let c = Cliffs::of(&game, &t, &tileset);
    assert!(c.notes.is_empty(), "{:?}", c.notes);
    assert_eq!(c.models.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["Doodads\\Terrain\\Cliffs\\CliffsABBA1.mdx"]);
    assert_eq!(c.placed, [Placed { model: 0, x: 1, y: 0, base: 0.0, texture: 0 }]);
    assert_eq!(c.tile_flags, [0, tile::CLIFF]);
    let meshes = meshes(&c);
    assert_eq!(meshes[0].instances, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!((meshes[0].positions.len(), meshes[0].indices.len(), meshes[0].error.as_str()), (9, 3, ""));
}

#[test]
fn a_cliff_the_game_hasnt_is_said_once() {
    let dir = TestDir::new("cliffs-missing");
    let (game, tileset) = setup(&dir);
    let t = terrain(3, 3, |x, _, p| {
        if x == 2 {
            p.set_layer_height(3);
        }
    });
    let c = Cliffs::of(&game, &t, &tileset);
    assert!(c.placed.is_empty());
    assert_eq!(c.notes, ["No cliff model Doodads\\Terrain\\Cliffs\\CliffsABBA0.mdx in the game's files or the roots."]);
}

#[test]
fn a_ramp_across_two_tiles_has_no_ground_under_it() {
    let dir = TestDir::new("cliffs-ramp");
    dir.write("Doodads/Terrain/CliffTrans/CliffTransHHAA0.mdx", &triangle());
    let (game, tileset) = setup(&dir);
    let v = w3e_rs::Version::of(11);
    // The top row a layer up, its corners a ramp's.
    let t = terrain(3, 2, |_, y, p| {
        if y == 1 {
            p.set_layer_height(3);
            p.set_ramp(v, true);
        }
    });
    let c = Cliffs::of(&game, &t, &tileset);
    assert_eq!(c.placed, [Placed { model: 0, x: 0, y: 0, base: 0.0, texture: 0 }]);
    assert_eq!(c.tile_flags, [tile::CLIFF | tile::ROMP, tile::CLIFF | tile::ROMP]);
    assert!(!tile::has_ground(c.tile_flags[0]));
}

#[test]
fn the_ramp_brush_on_a_cliffs_edge_makes_world_editors_ramp() {
    // A circle a layer down, the ramp brush between two rows just inside
    // its east edge (straight there): two pieces, the game's.
    let dir = TestDir::new("cliffs-ramp-brush");
    for name in ["LHBA", "ABHL"] {
        dir.write(&format!("Doodads/Terrain/CliffTrans/CliffTrans{name}0.mdx"), &triangle());
    }
    let (game, tileset) = setup(&dir);
    let mut t = terrain(100, 100, |_, _, _| {});
    let mut e = crate::edit::Edits::default();
    let b = |tool, size| crate::edit::Brush { tool, size, strength: 1.0, texture: None, cliff: -1, flags: 0 };
    e.apply(&mut t, 60.0, 60.0, &b(crate::edit::Tool::CliffDown, 21.0), true).unwrap();
    e.end();
    e.apply(&mut t, 70.0, 60.5, &b(crate::edit::Tool::Ramp, 3.0), true).unwrap();
    let v = t.flags_version();
    let ramps: Vec<(u32, u32)> = (55..66).flat_map(|y| (65..76).map(move |x| (x, y))).filter(|&(x, y)| t.point(x, y).unwrap().ramp(v)).collect();
    assert_eq!(ramps, [(69, 60), (70, 60), (71, 60), (69, 61), (70, 61), (71, 61)]);
    let c = Cliffs::of(&game, &t, &tileset);
    let pieces: Vec<_> = c.placed.iter().filter(|p| c.models[p.model].0.contains("CliffTrans")).map(|p| (p.x, p.y)).collect();
    assert_eq!(pieces, [(69, 59), (69, 61)]);
}
