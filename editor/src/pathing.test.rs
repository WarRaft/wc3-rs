use super::*;

/// The real map's `war3map.wpm` against what its files make, flag by
/// flag (`WC3_PROJECT`; skipped without it).
#[test]
fn as_the_map_has_it() {
    let Some(root) = std::env::var_os("WC3_PROJECT") else { return };
    let mut s = crate::session::Session::open(std::path::Path::new(&root)).unwrap();
    s.draw();
    let types = Types::load(&s.game);
    let made = compute(&s.game, &s.terrain, s.drawn.as_ref().unwrap(), &s.tileset, &types, &s.placed);
    let file = s.placed.pathing.as_ref().unwrap();
    assert_eq!((made.width, made.height), (file.width, file.height));
    for (bit, name) in wpm_rs::FLAGS {
        let both = made.cells.iter().zip(&file.cells).filter(|(m, f)| (*m & bit) == (*f & bit)).count();
        let only_made = made.cells.iter().zip(&file.cells).filter(|(m, f)| *m & bit != 0 && *f & bit == 0).count();
        let only_file = made.cells.iter().zip(&file.cells).filter(|(m, f)| *m & bit == 0 && *f & bit != 0).count();
        eprintln!("{name:>12}: {:.2}% the same; only made {only_made}, only the file's {only_file}", both as f64 * 100.0 / file.cells.len() as f64);
        // Where they differ, for a look (`WPM_DIFF`: a folder).
        if let Some(out) = std::env::var_os("WPM_DIFF") {
            let diff: Vec<u8> = made.cells.iter().zip(&file.cells).map(|(m, f)| ((m & bit != 0) as u8) << 1 | (f & bit != 0) as u8).collect();
            std::fs::write(std::path::Path::new(&out).join(format!("{bit:02x}.raw")), diff).unwrap();
        }
    }
}

#[test]
fn manual_changes_kept_over_what_the_files_make() {
    let made = Pathing { version: 0, width: 2, height: 1, cells: vec![NO_WATER, NO_WATER | NO_BUILD] };
    let painted = Pathing { version: 0, width: 2, height: 1, cells: vec![NO_WATER | NO_WALK, NO_WATER] };
    let manual = Manual::of(&made, &painted);
    // Made again, the second cell now walkable-only: the paint over it.
    let mut again = Pathing { version: 0, width: 2, height: 1, cells: vec![NO_WATER, NO_WATER | NO_BUILD | NO_FLY] };
    manual.apply(&mut again);
    assert_eq!(again.cells, [NO_WATER | NO_WALK, NO_WATER | NO_FLY]);
}

#[test]
fn a_pathing_texture_turned_onto_the_cells() {
    // 2 by 1: its left pixel red (no walking), right blue (no building).
    let rgba = vec![255, 0, 0, 255, 0, 0, 255, 255];
    let mut cells = vec![0u8; 4 * 4];
    blit(&mut cells, 4, 4, (2.0, 2.0), 0, (2, 1, &rgba));
    assert_eq!((cells[2 * 4 + 1], cells[2 * 4 + 2]), (NO_WALK, NO_BUILD));
    // Green: no flying.
    let green = vec![0, 255, 0, 255, 0, 255, 0, 255];
    let mut flyers = vec![0u8; 16];
    blit(&mut flyers, 4, 4, (2.0, 2.0), 0, (2, 1, &green));
    assert_eq!((flyers[2 * 4 + 1], flyers[2 * 4 + 2]), (NO_FLY, NO_FLY));
    let mut turned = vec![0u8; 4 * 4];
    blit(&mut turned, 4, 4, (2.0, 2.0), 90, (2, 1, &rgba));
    assert_eq!((turned[4 + 2], turned[2 * 4 + 2]), (NO_WALK, NO_BUILD));
}

#[test]
#[ignore]
fn flag_0x80_against_the_others() {
    let Some(root) = std::env::var_os("WC3_PROJECT") else { return };
    let s = crate::session::Session::open(std::path::Path::new(&root)).unwrap();
    let (files, _) = Files::read(&s.game);
    let f = files.pathing.unwrap();
    let mut table: std::collections::BTreeMap<u8, (u32, u32)> = Default::default();
    for c in &f.cells {
        let e = table.entry(c & !0x80).or_default();
        if c & 0x80 != 0 { e.0 += 1 } else { e.1 += 1 }
    }
    for (rest, (with, without)) in table {
        eprintln!("{rest:#04x}: with 0x80 {with}, without {without}");
    }
}

#[test]
fn the_destructables_pathing_a_blockers_and_a_bridges_deck() {
    let dir = crate::testdir::TestDir::new("pathing-destructables");
    dir.write("Units/DestructableData.slk", &crate::testdir::slk(&[
        &["DestructableID", "pathTex", "pathTexDeath", "walkable"],
        &["YTpb", "PathTextures\\Block.tga", "_", "0"],
        &["YT00", "PathTextures\\Bridge.tga", "PathTextures\\Gone.tga", "1"],
    ]));
    // A blocker: 2 by 2, no walking and no flying; a bridge: 2 by 3,
    // its rails (top and bottom rows) no walking, its deck no building.
    dir.write("PathTextures/Block.tga", &crate::testdir::tga(2, 2, |_, _| [255, 255, 0, 255]));
    dir.write("PathTextures/Bridge.tga", &crate::testdir::tga(2, 3, |_, y| if y == 1 { [0, 0, 255, 255] } else { [255, 0, 255, 255] }));
    dir.write("PathTextures/Gone.tga", &crate::testdir::tga(2, 3, |_, _| [255, 0, 255, 255]));
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let types = Types::load(&game);
    // A terrain of 4 by 2 cells: 16 by 8 pathing cells, from (0, 0).
    let mut t = w3e_rs::parse(&crate::testdir::terrain()).unwrap();
    t.width = 5;
    t.height = 3;
    t.points = vec![t.points[0]; 15];
    (t.offset_x, t.offset_y) = (0.0, 0.0);
    let thing = |id: &[u8; 4], x: f32, flags: u8, life: u8| doo_rs::Doodad {
        placement: doo_rs::Placement::new(parser_rs::bin::Rawcode(*id), x, 128.0),
        flags,
        life,
        item_table: -1,
        item_sets: Vec::new(),
        number: 0,
    };
    let mut doodads = crate::edit::new_doodads(Default::default());
    // At cells (4, 4) and (12, 4); a non-solid blocker at (8, 4): nothing.
    doodads.doodads = vec![thing(b"YTpb", 128.0, 2, 100), thing(b"YT00", 384.0, 2, 100), thing(b"YTpb", 256.0, 0, 100)];
    let files = Files { doodads: Some(doodads), ..Default::default() };
    let cells = destructables(&game, &t, &types, &files);
    let at = |x: usize, y: usize| cells[y * 16 + x];
    assert_eq!(at(4, 4), NO_WALK | NO_FLY);
    assert_eq!(at(8, 4), 0);
    // The bridge turned a quarter (angle 0: as World Editor places it,
    // + 90): its rails and its deck.
    let bridge: Vec<u8> = (2..6).flat_map(|y| (10..14).map(move |x| (x, y))).map(|(x, y)| at(x, y)).filter(|c| *c != 0).collect();
    assert!(bridge.contains(&(NO_WALK | NO_BUILD)) && bridge.contains(&(NO_BUILD | DECK)), "{bridge:?}");
    // Dead: its death's texture, no deck.
    let mut dead = files.clone();
    dead.doodads.as_mut().unwrap().doodads[1].life = 0;
    let cells = destructables(&game, &t, &types, &dead);
    assert!(cells.iter().all(|c| c & DECK == 0));
}
