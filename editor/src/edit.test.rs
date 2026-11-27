use super::*;

/// A flat terrain of `n` by `n` points at layer 2, height 0x2000.
fn flat(n: u32) -> Terrain {
    let mut t = w3e_rs::parse(&crate::testdir::terrain()).unwrap();
    t.width = n;
    t.height = n;
    t.points = vec![Point { height: 0x2000, water: 0x2000, flags: 0, variation: 0, layer: 2 }; (n * n) as usize];
    t
}

fn brush(tool: Tool) -> Brush {
    Brush { tool, size: 4.0, strength: 1.0, texture: None, cliff: -1, flags: 0, variation: -1, square: false }
}

#[test]
fn raise_most_in_the_middle_then_undo_and_redo() {
    let mut t = flat(9);
    let mut e = Edits::default();
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::Raise), true).unwrap();
    let h = |t: &Terrain, x: u32, y: u32| t.point(x, y).unwrap().height;
    assert_eq!(h(&t, 4, 4), 0x2000 + 512);
    assert!(h(&t, 5, 4) > 0x2000 && h(&t, 5, 4) < 0x2000 + 512);
    assert_eq!(h(&t, 7, 4), 0x2000, "out of the brush");
    assert!(e.dirty.terrain && !e.dirty.doodads && !e.can_undo(), "the stroke goes on");
    e.end();
    let mut files = crate::placed::Files::default();
    assert!(e.undo(Target { terrain: &mut t, files: &mut files, info: &mut None }).is_some());
    assert_eq!(h(&t, 4, 4), 0x2000);
    assert!(e.redo(Target { terrain: &mut t, files: &mut files, info: &mut None }).is_some());
    assert_eq!(h(&t, 4, 4), 0x2000 + 512);
    assert!(e.redo(Target { terrain: &mut t, files: &mut files, info: &mut None }).is_none());
}

#[test]
fn plateau_to_where_the_stroke_started_smooth_toward_the_average() {
    let mut t = flat(9);
    t.point_mut(4, 4).unwrap().height = 0x3000;
    let mut e = Edits::default();
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::Plateau), true).unwrap();
    // A second step elsewhere keeps the stroke's height.
    e.apply(&mut t, 2.0, 2.0, &brush(Tool::Plateau), false).unwrap();
    assert_eq!(t.point(2, 2).unwrap().height, 0x3000);
    let mut t = flat(9);
    t.point_mut(4, 4).unwrap().height = 0x2000 + 900;
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::Smooth), true).unwrap();
    assert_eq!(t.point(4, 4).unwrap().height, 0x2000 + 100);
}

#[test]
fn texture_added_to_the_list_cliffs_and_water() {
    let mut t = flat(9);
    let v = t.flags_version();
    let mut e = Edits::default();
    let grass = Rawcode(*b"Lgrs");
    e.apply(&mut t, 4.0, 4.0, &Brush { texture: Some(grass), ..brush(Tool::Texture) }, true).unwrap();
    assert_eq!(t.ground.last(), Some(&grass));
    assert_eq!(t.point(4, 4).unwrap().texture(v) as usize, t.ground.len() - 1);
    // Full: said, nothing changed.
    t.ground = (0..16).map(|i| Rawcode([b'X', b'X', b'X', b'A' + i])).collect();
    assert!(e.apply(&mut t, 4.0, 4.0, &Brush { texture: Some(Rawcode(*b"Zzzz")), ..brush(Tool::Texture) }, true).unwrap_err().contains("16"));
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::CliffUp), true).unwrap();
    assert_eq!(t.point(4, 4).unwrap().layer_height(), 3);
    assert_eq!(t.point(8, 8).unwrap().layer_height(), 2);
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::Water), true).unwrap();
    assert!(t.point(4, 4).unwrap().has_water(v));
    // Undone: the list as it was too (the step that failed is no stroke).
    e.end();
    let mut files = crate::placed::Files::default();
    for _ in 0..3 {
        assert!(e.undo(Target { terrain: &mut t, files: &mut files, info: &mut None }).is_some());
    }
    assert!(!e.can_undo());
    assert!(!t.ground.contains(&grass));
}

#[test]
fn the_things_placed_undone_and_redone_with_the_terrain() {
    let mut t = flat(9);
    let mut files = crate::placed::Files::default();
    let mut e = Edits::default();
    // A doodad added to a map that had none: the file made.
    e.record(Snapshot::Doodads(Vec::new()));
    files.doodads = Some(new_doodads(doo_rs::Layout::Classic));
    files.doodads.as_mut().unwrap().doodads.push(doo_rs::Doodad {
        placement: doo_rs::Placement::new(Rawcode(*b"LTlt"), 0.0, 0.0),
        flags: 2,
        life: 100,
        item_table: -1,
        item_sets: vec![],
        number: 0,
        reforged: Default::default(),
    });
    e.apply(&mut t, 4.0, 4.0, &brush(Tool::Raise), true).unwrap();
    e.end();
    assert_eq!(e.dirty, Dirty { terrain: true, doodads: true, ..Default::default() });
    // The terrain's stroke undone first, then the doodad.
    assert!(matches!(e.undo(Target { terrain: &mut t, files: &mut files, info: &mut None }), Some(Snapshot::Terrain(..))));
    assert!(matches!(e.undo(Target { terrain: &mut t, files: &mut files, info: &mut None }), Some(Snapshot::Doodads(_))));
    assert!(files.doodads.as_ref().unwrap().doodads.is_empty());
    assert!(e.redo(Target { terrain: &mut t, files: &mut files, info: &mut None }).is_some());
    assert_eq!(files.doodads.as_ref().unwrap().doodads.len(), 1);
}

#[test]
fn a_ramp_down_a_cliff_toward_the_brush_and_taken_off_by_a_cliff_brush() {
    // Layer 2, a layer lower from column 5 on.
    let mut t = flat(9);
    for y in 0..9 {
        for x in 5..9 {
            t.point_mut(x, y).unwrap().set_layer_height(1);
        }
    }
    let v = t.flags_version();
    let ramp = |t: &Terrain, x: u32, y: u32| t.point(x, y).unwrap().ramp(v);
    let mut e = Edits::default();
    // The corner at (4, 4), the brush's middle to its right: down across.
    e.apply(&mut t, 4.4, 4.0, &Brush { size: 1.0, ..brush(Tool::Ramp) }, true).unwrap();
    assert!(ramp(&t, 4, 4) && ramp(&t, 5, 4) && ramp(&t, 6, 4), "three corners in a row");
    assert!(!ramp(&t, 4, 3) && !ramp(&t, 7, 4) && !ramp(&t, 4, 5));
    // Toward the higher side: nothing (no layer below there).
    e.apply(&mut t, 1.6, 2.0, &Brush { size: 1.0, ..brush(Tool::Ramp) }, true).unwrap();
    assert!(!ramp(&t, 2, 2) && !ramp(&t, 1, 2));
    // A cliff brush over it: its corners another layer, no ramp's.
    e.apply(&mut t, 5.0, 4.0, &Brush { size: 1.0, ..brush(Tool::CliffUp) }, true).unwrap();
    assert!(!ramp(&t, 5, 4) && t.point(5, 4).unwrap().layer_height() == 2);
}

#[test]
fn a_cliff_brush_pulls_the_corners_around_to_within_two_layers() {
    let mut t = flat(15);
    let mut e = Edits::default();
    let layer = |t: &Terrain, x: u32, y: u32| t.point(x, y).unwrap().layer_height();
    // Up from 2 to 6 in four strokes at the middle, one corner across.
    for _ in 0..4 {
        e.apply(&mut t, 7.0, 7.0, &Brush { size: 1.0, ..brush(Tool::CliffUp) }, true).unwrap();
        e.end();
    }
    assert_eq!(layer(&t, 7, 7), 6);
    // Its neighbours 4, theirs 2 (as they were): steps of two at most.
    assert_eq!((layer(&t, 8, 7), layer(&t, 8, 8), layer(&t, 9, 7), layer(&t, 10, 7)), (4, 4, 2, 2));
    // Down to 0: the ring (4) pulled down to 2 with it, the rest left.
    for _ in 0..6 {
        e.apply(&mut t, 7.0, 7.0, &Brush { size: 1.0, ..brush(Tool::CliffDown) }, true).unwrap();
        e.end();
    }
    assert_eq!((layer(&t, 7, 7), layer(&t, 8, 7), layer(&t, 9, 7)), (0, 2, 2));
}

#[test]
fn shallow_and_deep_water_and_one_into_the_other_its_surface_kept() {
    let mut t = flat(9);
    let v = t.flags_version();
    let mut e = Edits::default();
    let at = |t: &Terrain| *t.point(4, 4).unwrap();
    let small = |tool| Brush { size: 1.0, ..brush(tool) };
    // Shallow: the cliff a layer down (2 → 1), the surface at layer 0 (0x2000).
    e.apply(&mut t, 4.0, 4.0, &small(Tool::ShallowWater), true).unwrap();
    e.end();
    assert_eq!((at(&t).layer_height(), at(&t).has_water(v), at(&t).water_height()), (1, true, 0x2000));
    // Deep from it: a layer further down, the same surface.
    e.apply(&mut t, 4.0, 4.0, &small(Tool::DeepWater), true).unwrap();
    e.end();
    assert_eq!((at(&t).layer_height(), at(&t).water_height()), (0, 0x2000));
    // Again: stays.
    e.apply(&mut t, 4.0, 4.0, &small(Tool::DeepWater), true).unwrap();
    e.end();
    assert_eq!(at(&t).layer_height(), 0);
    // Shallow from deep: back up a layer.
    e.apply(&mut t, 4.0, 4.0, &small(Tool::ShallowWater), true).unwrap();
    e.end();
    assert_eq!((at(&t).layer_height(), at(&t).water_height()), (1, 0x2000));
    // Raised above its water by cliffs: dry.
    e.apply(&mut t, 4.0, 4.0, &small(Tool::CliffUp), true).unwrap();
    e.end();
    assert!(at(&t).has_water(v), "level with it: still");
    e.apply(&mut t, 4.0, 4.0, &small(Tool::CliffUp), true).unwrap();
    assert!(!at(&t).has_water(v));
}

#[test]
fn blight_not_by_a_cliff_taken_off_by_a_texture_and_a_cliff() {
    let mut t = flat(15);
    let v = t.flags_version();
    // A cliff at the left: column 3 on a layer up.
    for y in 0..15 {
        for x in 0..4 {
            t.point_mut(x, y).unwrap().set_layer_height(3);
        }
    }
    let mut e = Edits::default();
    let blight = |t: &Terrain, x: u32, y: u32| t.point(x, y).unwrap().blight(v);
    e.apply(&mut t, 5.0, 7.0, &Brush { size: 5.0, ..brush(Tool::Blight) }, true).unwrap();
    e.end();
    assert!(blight(&t, 5, 7) && blight(&t, 7, 7));
    // Corner 4's 3 by 3 has corner 3, a cliff's: none there.
    assert!(!blight(&t, 3, 7) && !blight(&t, 4, 7));
    // A texture over it: off.
    e.apply(&mut t, 7.0, 7.0, &Brush { size: 1.0, texture: Some(Rawcode(*b"Ldrt")), ..brush(Tool::Texture) }, true).unwrap();
    e.end();
    assert!(!blight(&t, 7, 7) && blight(&t, 6, 7));
    // A cliff raised by it: its corners clean.
    e.apply(&mut t, 5.0, 7.0, &Brush { size: 1.0, ..brush(Tool::CliffUp) }, true).unwrap();
    assert!(!blight(&t, 5, 7) && !blight(&t, 6, 7) && !blight(&t, 5, 8));
}

#[test]
fn boundary_on_the_cells_under_the_brush_and_off_again() {
    let mut t = flat(9);
    let v = t.flags_version();
    let mut e = Edits::default();
    let on = |t: &Terrain, x: u32, y: u32| t.point(x, y).unwrap().boundary(v);
    // A brush 2 across at (4, 4): the four cells around it (their middles 0.7 off).
    e.apply(&mut t, 4.0, 4.0, &Brush { size: 2.0, ..brush(Tool::BoundaryOn) }, true).unwrap();
    e.end();
    let cells: Vec<(u32, u32)> = (0..9).flat_map(|y| (0..9).map(move |x| (x, y))).filter(|&(x, y)| on(&t, x, y)).collect();
    assert_eq!(cells, [(3, 3), (4, 3), (3, 4), (4, 4)]);
    // The top row and right column: no cells of their own.
    e.apply(&mut t, 8.0, 8.0, &Brush { size: 2.0, ..brush(Tool::BoundaryOn) }, true).unwrap();
    e.end();
    assert!(on(&t, 7, 7) && !on(&t, 8, 8) && !on(&t, 8, 7));
    e.apply(&mut t, 4.0, 4.0, &Brush { size: 1.6, ..brush(Tool::BoundaryOff) }, true).unwrap();
    assert!(!on(&t, 3, 3) && !on(&t, 4, 4));
}

#[test]
fn variations_at_random_as_world_editors_or_the_brushs_own() {
    // The table: its 570 rolls, the whole ones and the extended alike.
    let mut count = [0u32; 18];
    for roll in 0..570 {
        count[ground_variation(roll) as usize] += 1;
    }
    assert_eq!(count, [85, 10, 4, 1, 85, 10, 4, 1, 85, 10, 4, 1, 85, 10, 4, 1, 85, 85]);

    let mut t = flat(21);
    let mut e = Edits::default();
    let grass = Rawcode(*b"Lgrs");
    let all = |t: &Terrain, r: std::ops::Range<u32>, f: fn(Point) -> u8| -> Vec<u8> { r.clone().flat_map(|y| r.clone().map(move |x| (x, y))).map(|(x, y)| f(*t.point(x, y).unwrap())).collect() };
    // At random: not all the first; a second step over it changes none.
    let paint = Brush { size: 12.0, texture: Some(grass), ..brush(Tool::Texture) };
    e.apply(&mut t, 10.0, 10.0, &paint, true).unwrap();
    let first = all(&t, 7..14, Point::ground_variation);
    assert!(first.iter().any(|&n| n != 0) && first.iter().all(|&n| n <= 17));
    e.apply(&mut t, 10.0, 10.0, &paint, false).unwrap();
    assert_eq!(all(&t, 7..14, Point::ground_variation), first);
    e.end();
    // The brush's own: 17 on all of them.
    e.apply(&mut t, 10.0, 10.0, &Brush { variation: 17, ..paint }, true).unwrap();
    assert!(all(&t, 7..14, Point::ground_variation).iter().all(|&n| n == 17));
    e.end();

    // A cliff at random: 0-2, each of them somewhere.
    e.apply(&mut t, 10.0, 10.0, &Brush { size: 12.0, ..brush(Tool::CliffUp) }, true).unwrap();
    e.end();
    let cliffs = all(&t, 7..14, Point::cliff_variation);
    assert!(cliffs.iter().all(|&n| n <= 2) && (0..=2).all(|n| cliffs.contains(&n)), "{cliffs:?}");
    // Its own: 5, though the layer is as it was, and on the corners around
    // (a cell's bottom left one outside it); none further.
    e.apply(&mut t, 10.0, 10.0, &Brush { size: 12.0, variation: 5, ..brush(Tool::CliffUp) }, true).unwrap();
    assert!(all(&t, 7..14, Point::cliff_variation).iter().all(|&n| n == 5));
    assert_eq!((t.point(17, 10).unwrap().cliff_variation(), t.point(19, 10).unwrap().cliff_variation()), (5, 0));
}

#[test]
fn noise_up_and_down_at_random_and_a_square_brush_to_its_corners() {
    let mut t = flat(21);
    let mut e = Edits::default();
    e.apply(&mut t, 10.0, 10.0, &Brush { size: 10.0, ..brush(Tool::Noise) }, true).unwrap();
    let h: Vec<i32> = (5..16).flat_map(|y| (5..16).map(move |x| (x, y))).map(|(x, y)| t.point(x, y).unwrap().height as i32 - 0x2000).collect();
    assert!(h.iter().any(|&d| d > 0) && h.iter().any(|&d| d < 0) && h.iter().all(|&d| d.abs() <= 512), "{h:?}");
    assert_eq!(t.point(0, 0).unwrap().height, 0x2000, "out of the brush");
    e.end();
    // A square of 6: its corner (3, 3) from the middle in, a circle's out.
    let mut t = flat(21);
    e.apply(&mut t, 10.0, 10.0, &Brush { size: 6.0, square: true, ..brush(Tool::Raise) }, true).unwrap();
    assert!(t.point(12, 12).unwrap().height > 0x2000);
    let mut t = flat(21);
    e.apply(&mut t, 10.0, 10.0, &Brush { size: 6.0, ..brush(Tool::Raise) }, true).unwrap();
    assert_eq!(t.point(13, 13).unwrap().height, 0x2000);
    assert_eq!(Brush { square: true, ..brush(Tool::Raise) }.distance(3.0, -2.0), 3.0);
}
