use super::*;
use crate::testdir::{self, TestDir};
use mdx_rs::{Chunk, Extent, Fixed, Material, Model as Mdx, Sequence};

#[test]
fn model_paths_as_the_game_reads_them() {
    assert_eq!(model_path("units\\human\\Footman\\Footman"), "units\\human\\Footman\\Footman.mdx");
    assert_eq!(model_path("Objects\\Chest\\chest.mdl"), "Objects\\Chest\\chest.mdx");
    assert_eq!(model_path("a.b\\c.mdx"), "a.b\\c.mdx");
    assert_eq!(strip_extension("Doodads\\x.y\\Tree.mdl"), "Doodads\\x.y\\Tree");
}

/// A box model 128 across, its one texture team colour (replaceable 1),
/// or a file.
fn cube(texture: mdx_rs::Texture) -> Vec<u8> {
    let g = mdx_rs::Geoset {
        vertices: vec![[-64.0, -64.0, 0.0], [64.0, 64.0, 128.0], [64.0, -64.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        face_types: vec![4],
        face_groups: vec![3],
        faces: vec![0, 1, 2],
        vertex_groups: vec![0; 3],
        matrix_groups: vec![1],
        matrix_indices: vec![0],
        uv_sets: vec![vec![[0.0; 2]; 3]],
        ..Default::default()
    };
    let layer = mdx_rs::Layer { filter_mode: 1, shading: 0, texture_id: 0, texture_animation_id: u32::MAX, coord_id: 0, alpha: 1.0, rest: vec![] };
    let stand = Sequence { name: Fixed::new("Stand", 80), start: 0, end: 100, move_speed: 0.0, flags: 0, rarity: 0.0, sync_point: 0, extent: Extent::default() };
    Mdx {
        chunks: vec![
            Chunk::Version(800),
            Chunk::Sequences(vec![stand]),
            Chunk::Textures(vec![texture]),
            Chunk::Materials(vec![Material { priority_plane: 0, flags: 0, shader: None, layers: vec![layer] }]),
            Chunk::Geosets(vec![g]),
        ],
    }
    .to_bytes()
    .unwrap()
}

#[test]
fn doodads_and_units_with_their_models_and_what_a_ray_meets() {
    let dir = TestDir::new("placed");
    dir.write("Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "dir", "file", "numVar", "vertR02"], &["LTrc", "Doodads", "Rock", "2", "128"]]));
    dir.write("Units/UnitUI.slk", &testdir::slk(&[&["unitUIID", "file", "modelScale"], &["hfoo", "Units\\Footman", "2"]]));
    // The doodad's second variation: its name and the number.
    dir.write("Doodads/Rock/Rock1.mdx", &cube(mdx_rs::Texture { replaceable_id: 0, path: Fixed::new("Textures\\Rock.blp", 260), flags: 0 }));
    dir.write("Textures/Rock.blp", &testdir::tga(2, 2, |_, _| [9, 9, 9, 255]));
    dir.write("Units/Footman.mdx", &cube(mdx_rs::Texture { replaceable_id: 1, path: Fixed::new("", 260), flags: 0 }));
    dir.write("ReplaceableTextures/TeamColor/TeamColor01.blp", &testdir::tga(2, 2, |_, _| [0, 0, 255, 255]));
    let doodad = |id: &[u8; 4], x: f32, y: f32| doo_rs::Doodad {
        placement: doo_rs::Placement::new(Rawcode(*id), x, y),
        flags: 2,
        life: 100,
        item_table: -1,
        item_sets: vec![],
        number: 0,
        reforged: Default::default(),
    };
    let mut rock = doodad(b"LTrc", 256.0, 128.0);
    rock.placement.variation = 1;
    rock.placement.z = 64.0;
    let doo = doo_rs::Doodads {
        version: 8,
        subversion: 11,
        layout: doo_rs::Layout::Classic,
        doodads: vec![rock, doodad(b"XXXX", 0.0, 0.0)],
        special_version: 0,
        specials: vec![],
    };
    dir.write("war3map.doo", &doo.to_bytes().unwrap());
    let mut units = doo_rs::Units { version: 8, subversion: 11, layout: doo_rs::Layout::Classic, units: vec![] };
    units.units.push(doo_rs::Unit::new(Rawcode(*b"hfoo"), 128.0, 128.0, 1));
    dir.write("war3mapUnits.doo", &units.to_bytes().unwrap());

    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let types = Types::load(&game);
    let t = w3e_rs::parse(&testdir::terrain()).unwrap();
    let draw = Draw { offset: [0.0, 0.0], ..crate::terrain::Draw::new(&t, &crate::terrain::Grounds { ids: vec![], blight: 0, cliff_ground: vec![] }, crate::terrain::tile_flags(&t), &|_| false, 0.0) };
    let o = Objects::load(&game, &types, &draw);
    assert_eq!(o.notes, ["Not drawn, types the game's tables and the map's object data don't have: XXXX"]);
    assert_eq!(o.models.iter().map(|m| m.path.as_str()).collect::<Vec<_>>(), ["Doodads\\Rock\\Rock1.mdx", "Units\\Footman.mdx"]);
    // The rock: at (2, 1) cells, 0.5 up, tinted by its
    // variation's vertR (its number one more than the variation); its texture decoded.
    let rock = &o.models[0].instances;
    assert_eq!(&rock[..4], [2.0, 1.0, 0.5, 0.0]);
    assert_eq!(rock[7], 128.0 / 255.0);
    assert_eq!(o.models[0].geosets[0].layers[0].texture, 0);
    assert_eq!(o.textures[0].rgba[..4], [9, 9, 9, 255]);
    // The footman: on the ground (0), twice its size, Player 2's colour.
    let foot = &o.models[1].instances;
    assert_eq!(&foot[..3], [1.0, 1.0, 0.0]);
    assert_eq!(foot[4], 2.0 / 128.0);
    assert_eq!(&foot[10..13], [0.0, 0.0, 1.0]);
    assert_eq!(o.models[1].geosets[0].layers[0].texture, texture::TEAM_COLOR);
    // A ray down onto the footman meets it; one beside both, nothing.
    let hit = o.pick([1.0, 1.0, 10.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!((hit.kind, hit.player, hit.index), (Kind::Unit, Some(1), 0));
    assert!(o.pick([5.0, 5.0, 10.0], [0.0, 0.0, -1.0]).is_none());
    // Along the ground from the left: the footman first, then the rock.
    assert_eq!(o.pick([-5.0, 1.0, 0.6], [1.0, 0.0, 0.0]).unwrap().kind, Kind::Unit);
}

#[test]
fn a_start_locations_model_world_edit_art_says() {
    let dir = crate::testdir::TestDir::new("placed-sloc");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &crate::testdir::terrain());
    dir.write("map/UI/WorldEditData.txt", b"[WorldEditArt]\nStartLocation=Objects\\StartLocation\\StartLocation.mdl\n");
    let s = crate::session::Session::open(dir.path()).unwrap();
    // The .mdl as the game reads it.
    assert_eq!(world_edit_art(&s.game, "StartLocation").as_deref(), Some("Objects\\StartLocation\\StartLocation.mdx"));
    assert_eq!(world_edit_art(&s.game, "Nothing"), None);
}

/// A model that isn't anywhere (a dummy unit's `1.mdx`, a map's trick to
/// have nothing drawn) and one that's a text model (`.mdl`, as Reforged
/// maps import them): each said, as its own error.
#[test]
fn a_dummys_missing_model_and_a_text_model_said() {
    let dir = TestDir::new("placed-missing");
    dir.write("Units/UnitUI.slk", &testdir::slk(&[&["unitUIID", "file"], &["hdum", "1"], &["hmdl", "war3mapImported\\Cliff"]]));
    dir.write("war3mapImported/Cliff.mdl", b"Version {\n\tFormatVersion 800,\n}\n");
    let mut units = doo_rs::Units { version: 8, subversion: 11, layout: doo_rs::Layout::Classic, units: vec![] };
    units.units.push(doo_rs::Unit::new(Rawcode(*b"hdum"), 0.0, 0.0, 0));
    units.units.push(doo_rs::Unit::new(Rawcode(*b"hdum"), 64.0, 0.0, 0));
    units.units.push(doo_rs::Unit::new(Rawcode(*b"hmdl"), 0.0, 64.0, 0));
    dir.write("war3mapUnits.doo", &units.to_bytes().unwrap());
    let game = wc3_server::game::Lookup { roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let types = Types::load(&game);
    let t = w3e_rs::parse(&testdir::terrain()).unwrap();
    let draw = crate::terrain::Draw::new(&t, &crate::terrain::Grounds { ids: vec![], blight: 0, cliff_ground: vec![] }, crate::terrain::tile_flags(&t), &|_| false, 0.0);
    let o = Objects::load(&game, &types, &draw);
    assert_eq!(o.notes, ["Models not in the game's files or the roots: 1.mdx", "Text models (.mdl), not read yet: war3mapImported\\Cliff.mdl"]);
    assert!(o.things.is_empty());
}
