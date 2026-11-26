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
    Brush { tool, size: 4.0, strength: 1.0, texture: None, cliff: -1 }
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
    assert!(e.dirty && !e.can_undo(), "the stroke goes on");
    e.end();
    assert!(e.undo(&mut t));
    assert_eq!(h(&t, 4, 4), 0x2000);
    assert!(e.redo(&mut t));
    assert_eq!(h(&t, 4, 4), 0x2000 + 512);
    assert!(!e.redo(&mut t));
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
    assert!(e.undo(&mut t) && e.undo(&mut t) && e.undo(&mut t));
    assert!(!e.can_undo());
    assert!(!t.ground.contains(&grass));
}
