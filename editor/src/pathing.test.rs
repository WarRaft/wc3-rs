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
    // Green (no flying) isn't taken.
    let green = vec![0, 255, 0, 255, 0, 255, 0, 255];
    let mut none = vec![0u8; 16];
    blit(&mut none, 4, 4, (2.0, 2.0), 0, (2, 1, &green));
    assert!(none.iter().all(|c| *c == 0));
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
