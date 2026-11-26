use super::*;
use parser_rs::bin::Rawcode;
use w3e_rs::Point;

/// A terrain of `w` by `h` cells, each point's height its column and
/// row (x + 100 y), the outer ring the edge's.
fn terrain(w: u32, h: u32) -> Terrain {
    let mut points = Vec::new();
    for y in 0..=h {
        for x in 0..=w {
            let mut p = Point { height: (x + 100 * y) as u16, water: 0, flags: 0, variation: 0, layer: 2 };
            p.set_edge(x == 0 || y == 0 || x == w || y == h);
            points.push(p);
        }
    }
    Terrain { version: 11, tileset: b'L', custom_tilesets: 0, ground: vec![Rawcode(*b"Ldrt")], cliffs: vec![Rawcode(*b"CLdi")], width: w + 1, height: h + 1, offset_x: -2048.0, offset_y: -2048.0, points }
}

#[test]
fn grown_on_the_left_and_cut_at_the_top() {
    let mut t = terrain(32, 40);
    let s = Sides { left: 4, right: 0, bottom: 0, top: -8 };
    terrain_resize(&mut t, s);
    assert_eq!((t.width, t.height), (37, 33));
    // The corner moved out by four cells: the old (0, 0) is at (4, 0).
    assert_eq!((t.offset_x, t.offset_y), (-2048.0 - 512.0, -2048.0));
    assert_eq!(t.point(4, 0).unwrap().height, 0);
    assert_eq!(t.point(5, 3).unwrap().height, 1 + 300);
    // New points: copies of the nearest (the old left column).
    assert_eq!(t.point(0, 3).unwrap().height, 300);
    assert!(t.point(0, 3).unwrap().edge());
    // The top cut: row 32 is the old row 32.
    assert_eq!(t.point(4, 32).unwrap().height, 3200);
}

fn terrain_resize(t: &mut Terrain, s: Sides) {
    let mut seed = 1;
    super::terrain(t, s, &mut seed).unwrap();
}

#[test]
fn too_small_or_too_big_refused() {
    let mut t = terrain(32, 32);
    let mut seed = 1;
    assert!(super::terrain(&mut t, Sides { left: -1, ..Default::default() }, &mut seed).unwrap_err().contains("32 to 480"));
    assert!(super::terrain(&mut t, Sides { top: 449, ..Default::default() }, &mut seed).is_err());
    assert_eq!(t.width, 33);
}

#[test]
fn pathing_and_shadows_four_cells_a_cell() {
    let mut p = wpm_rs::Pathing { version: 0, width: 8, height: 4, cells: (0..32).collect() };
    pathing(&mut p, Sides { left: 1, right: -1, bottom: 0, top: 1 });
    assert_eq!((p.width, p.height, p.cells.len()), (8, 8, 64));
    // Old (0, 0) at (4, 0); the new left four copies of the old first column.
    assert_eq!(&p.cells[..8], &[0, 0, 0, 0, 0, 1, 2, 3]);
    // The new top rows: the old top row.
    assert_eq!(&p.cells[7 * 8..7 * 8 + 8], &[24, 24, 24, 24, 24, 25, 26, 27]);
    let mut sh = shd_rs::Shadows { width: 4, height: 4, cells: vec![255; 16] };
    shadows(&mut sh, Sides { left: 1, ..Default::default() });
    assert_eq!(sh.width, 8);
    // No shadow where there was no ground.
    assert_eq!(&sh.cells[..8], &[0, 0, 0, 0, 255, 255, 255, 255]);
}

#[test]
fn whats_off_the_map_removed_regions_cut() {
    let mut t = terrain(40, 32);
    terrain_resize(&mut t, Sides { right: -8, ..Default::default() });
    // The terrain: x from -2048 to -2048 + 32 cells.
    let right = -2048.0 + 32.0 * 128.0;
    let mut files = crate::placed::Files::default();
    let thing = |x: f32| doo_rs::Doodad { placement: doo_rs::Placement::new(Rawcode(*b"LTlt"), x, 0.0), flags: 2, life: 100, item_table: -1, item_sets: Vec::new(), number: 0 };
    let mut d = crate::edit::new_doodads(Default::default());
    d.doodads = vec![thing(0.0), thing(right + 10.0)];
    files.doodads = Some(d);
    let mut r = w3r_rs::Regions::new();
    let region = |l: f32, rt: f32| w3r_rs::Region::new("R", l, 0.0, rt, 64.0, 0);
    r.regions = vec![region(right - 64.0, right + 64.0), region(right + 1.0, right + 100.0)];
    files.regions = Some(r);
    let removed = placed(&mut files, &t, Sides { right: -8, ..Default::default() });
    assert_eq!(removed, Removed { things: 1, regions: 1 });
    let r = &files.regions.as_ref().unwrap().regions;
    assert_eq!((r.len(), r[0].right), (1, right));
}

#[test]
fn the_playable_area_kept_or_set() {
    let mut t = terrain(32, 32);
    let mut i = w3i_rs::Info::new(25);
    set_playable(&mut t, Some(&mut i), [2, 2, 2, 2]).unwrap();
    assert_eq!((i.camera_complements, i.playable_width, i.playable_height), ([2, 2, 2, 2], 28, 28));
    // Camera bounds: four cells in from the sides, two from the bottom
    // and top.
    assert_eq!(&i.camera_bounds[..4], &[-2048.0 + 6.0 * 128.0, -2048.0 + 4.0 * 128.0, -2048.0 + 26.0 * 128.0, -2048.0 + 28.0 * 128.0]);
    assert!(t.point(1, 5).unwrap().edge() && !t.point(2, 5).unwrap().edge());
    // The right's: its first point too (a cell is its bottom left's).
    assert!(t.point(30, 5).unwrap().edge() && !t.point(29, 5).unwrap().edge());
    // Grown: the margins grow, the bounds stay.
    let bounds = i.camera_bounds;
    let s = Sides { left: 4, top: 8, ..Default::default() };
    terrain_resize(&mut t, s);
    info(&mut i, &t, s);
    assert_eq!((i.camera_complements, i.camera_bounds), ([6, 2, 2, 10], bounds));
    // Cut into it: set again.
    let s = Sides { right: -4, ..Default::default() };
    terrain_resize(&mut t, s);
    info(&mut i, &t, s);
    assert_eq!((i.camera_complements, i.playable_width), ([6, 0, 2, 10], 26));
    assert!(set_playable(&mut t, None, [20, 20, 0, 0]).is_err());
}
