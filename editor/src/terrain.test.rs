use super::*;
use parser_rs::bin::Rawcode;

/// A terrain of `w` by `h` points, each made by `point(x, y)` from flat
/// ground of texture 0 at layer 2.
fn terrain(w: u32, h: u32, point: impl Fn(u32, u32, &mut Point)) -> Terrain {
    let mut points = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let mut p = Point { height: 0x2000, water: 0x2000, flags: 0, variation: 0, layer: 2 };
            point(x, y, &mut p);
            points.push(p);
        }
    }
    Terrain {
        version: 11,
        tileset: b'L',
        custom_tilesets: 0,
        ground: vec![Rawcode(*b"Ldrt"), Rawcode(*b"Lgrs")],
        cliffs: vec![Rawcode(*b"CLdi")],
        width: w,
        height: h,
        offset_x: 0.0,
        offset_y: 0.0,
        points,
    }
}

fn grounds() -> Grounds {
    // CLdi's ground: Ldrt, the terrain's first.
    Grounds { ids: vec!["Ldrt".into(), "Lgrs".into(), "blight".into()], blight: 2, cliff_ground: vec![0] }
}

#[test]
fn variations_as_hivewe_picks_them() {
    assert_eq!(variation(false, 0), 0);
    assert_eq!(variation(false, 7), 15);
    assert_eq!(variation(true, 3), 19);
    assert_eq!(variation(true, 16), 15);
    assert_eq!(variation(true, 17), 0);
}

#[test]
fn a_tile_of_two_textures_blends_the_upper_through_its_corners() {
    let v = w3e_rs::Version::of(11);
    // Grass (1) at the top right corner only.
    let t = terrain(2, 2, |x, y, p| p.set_texture(v, if (x, y) == (1, 1) { 1 } else { 0 }));
    let d = Draw::new(&t, &grounds(), &|_| false, 0.0);
    assert_eq!(d.tiles, [[0, 1 | 4 << 16, 0xFFFF, 0xFFFF]]);
    assert_eq!(d.tile_flags, [0]);
    assert!(d.heights.iter().all(|&h| h == 0.0));
}

#[test]
fn heights_are_in_layers_with_the_cliff_layer() {
    let t = terrain(2, 2, |x, _, p| {
        p.height = 0x2000 + 512 * x as u16;
        p.layer = (p.layer & 0xF0) | 3;
    });
    let d = Draw::new(&t, &grounds(), &|_| false, -0.7);
    assert_eq!(d.ground, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(d.heights, [1.0, 2.0, 1.0, 2.0]);
    assert_eq!(d.water, [-0.7; 4]);
}

#[test]
fn a_cliff_tile_and_the_ground_beside_it_take_the_cliffs_ground() {
    let v = w3e_rs::Version::of(11);
    // Grass everywhere; the right column a layer up: the tiles between
    // are cliffs, their corners the cliff's ground (dirt, 0).
    let t = terrain(3, 2, |x, _, p| {
        p.set_texture(v, 1);
        if x == 2 {
            p.layer = (p.layer & 0xF0) | 3;
        }
    });
    let d = Draw::new(&t, &grounds(), &|_| false, 0.0);
    assert_eq!(d.tile_flags, [0, tile::CLIFF]);
    // The left tile's right corners are beside the cliff.
    assert_eq!(d.tiles[0][0] & 0xFFFF, 0);
    assert_eq!(d.tiles[0][1], 1 | (0b0010 | 0b1000) << 16);
}

#[test]
fn blight_and_flags() {
    let v = w3e_rs::Version::of(11);
    let t = terrain(2, 2, |x, y, p| {
        p.set_blight(v, x == 0 && y == 0);
        p.set_has_water(v, x == 1);
        p.set_boundary(v, y == 1);
    });
    let d = Draw::new(&t, &grounds(), &|_| false, 0.0);
    assert_eq!(d.tiles[0][0], 0);
    assert_eq!(d.tiles[0][1], 2 | 0b0010 << 16);
    assert_eq!(d.corners, [corner::BLIGHT, corner::WATER, corner::BOUNDARY, corner::WATER | corner::BOUNDARY]);
}

#[test]
fn a_ramps_foot_is_half_a_layer_up() {
    let v = w3e_rs::Version::of(11);
    // Every corner a ramp, the top ones a layer up.
    let t = terrain(2, 2, |_, y, p| {
        p.set_ramp(v, true);
        if y == 1 {
            p.layer = (p.layer & 0xF0) | 3;
        }
    });
    let d = Draw::new(&t, &grounds(), &|_| false, 0.0);
    assert_eq!(d.tile_flags, [tile::CLIFF | tile::RAMP_ENTRANCE]);
    assert_eq!(d.heights, [0.5, 0.5, 1.0, 1.0]);
}
