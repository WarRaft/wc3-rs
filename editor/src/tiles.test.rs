use super::*;
use crate::testdir::slk;
use crate::tileset::Slk;
use parser_rs::bin::Rawcode;
use w3e_rs::Point;

fn tileset() -> Tileset {
    let parse = |b: Vec<u8>| Slk::parse(std::str::from_utf8(&b).unwrap());
    Tileset {
        terrain: Some(parse(slk(&[
            &["tileID", "dir", "file", "walkable", "flyable", "buildable"],
            &["Ldrt", "T", "Dirt", "1", "1", "1"],
            &["Lgrs", "T", "Grass", "1", "1", "1"],
            &["Lrok", "T", "Rock", "1", "1", "0"],
            &["Xblk", "T", "Black", "0", "0", "0"],
        ]))),
        cliffs: Some(parse(slk(&[&["cliffID", "groundTile"], &["CLdi", "Ldrt"], &["CLgr", "Lgrs"]]))),
        letter: b'L',
        ..Default::default()
    }
}

fn terrain() -> Terrain {
    let v = w3e_rs::Version::of(11);
    let mut points = vec![Point { height: 0x2000, water: 0, flags: 0, variation: 0, layer: 2 }; 4];
    points[1].set_texture(v, 1);
    points[2].set_cliff_texture(1);
    points[3].set_cliff_texture(15);
    Terrain {
        version: 11,
        tileset: b'L',
        custom_tilesets: 0,
        ground: vec![Rawcode(*b"Ldrt"), Rawcode(*b"Lgrs")],
        cliffs: vec![Rawcode(*b"CLdi"), Rawcode(*b"CLgr")],
        width: 2,
        height: 2,
        offset_x: 0.0,
        offset_y: 0.0,
        points,
    }
}

#[test]
fn the_games_tiles_with_their_cliffs_and_pathing() {
    let all = all(&tileset());
    assert_eq!(all.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["Ldrt", "Lgrs", "Lrok", "Xblk"]);
    assert_eq!(all[0].cliff.as_deref(), Some("CLdi"));
    assert_eq!((all[2].cliff.clone(), all[2].pathing), (None, wpm_rs::NO_BUILD));
    assert_eq!(all[3].pathing, wpm_rs::NO_WALK | wpm_rs::NO_FLY | wpm_rs::NO_BUILD);
}

#[test]
fn grass_taken_out_its_points_to_rock_its_cliff_to_dirts() {
    let mut t = terrain();
    let v = t.flags_version();
    let ids = ["Lrok".to_string(), "Ldrt".to_string()];
    // Dirt (0) to 1, grass (1) to rock (0).
    set(&mut t, &tileset(), &ids, &[1, 0], b'X').unwrap();
    assert_eq!(t.ground, vec![Rawcode(*b"Lrok"), Rawcode(*b"Ldrt")]);
    assert_eq!((t.points[0].texture(v), t.points[1].texture(v)), (1, 0));
    // Only dirt's cliff is left; grass's went with it to rock, which
    // has none: the first.
    assert_eq!(t.cliffs, vec![Rawcode(*b"CLdi")]);
    assert_eq!((t.points[2].cliff_texture(), t.points[3].cliff_texture()), (0, 15));
    assert_eq!(t.tileset, b'X');
}

#[test]
fn a_list_that_cant_be_refused() {
    let mut t = terrain();
    let ts = tileset();
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(set(&mut t, &ts, &ids(&["Ldrt", "Ldrt"]), &[0, 0], b'L').unwrap_err().contains("twice"));
    assert!(set(&mut t, &ts, &ids(&["Nope"]), &[0, 0], b'L').unwrap_err().contains("Terrain.slk"));
    assert!(set(&mut t, &ts, &ids(&["Lrok"]), &[0, 0], b'L').unwrap_err().contains("cliff"));
    assert!(set(&mut t, &ts, &ids(&["Ldrt"]), &[0], b'L').is_err());
    let many: Vec<String> = (0..17).map(|i| format!("L{i:03}")).collect();
    assert!(set(&mut t, &ts, &many, &[0, 0], b'L').unwrap_err().contains("1 to 16"));
    assert_eq!(t, terrain());
}

#[test]
fn the_tile_pather_sets_three_flags_keeps_the_rest() {
    let mut p = wpm_rs::Pathing { version: 0, width: 2, height: 1, cells: vec![wpm_rs::BLIGHT | wpm_rs::NO_WATER, wpm_rs::NO_WALK] };
    let made = wpm_rs::Pathing { version: 0, width: 2, height: 1, cells: vec![wpm_rs::NO_WALK | wpm_rs::NO_BUILD, 0] };
    repath(&mut p, &made, &[0]);
    assert_eq!(p.cells[0], wpm_rs::BLIGHT | wpm_rs::NO_WATER | wpm_rs::NO_WALK | wpm_rs::NO_BUILD | 0x80);
    assert_eq!(p.cells[1], wpm_rs::NO_WALK);
}
