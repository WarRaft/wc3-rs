use super::*;
use crate::testdir::{self, TestDir};

/// The test terrain (3 by 2 points, flat at 0, offset -128, -64) drawn.
fn draw() -> (Terrain, Draw) {
    let t = w3e_rs::parse(&testdir::terrain()).unwrap();
    let grounds = crate::terrain::Grounds { ids: vec![], blight: 0, cliff_ground: vec![] };
    let d = Draw::new(&t, &grounds, crate::terrain::tile_flags(&t), &|_| false, 0.0);
    (t, d)
}

#[test]
fn a_ray_meets_the_ground_where_it_is() {
    let (_, d) = draw();
    let p = ground_hit(&d, [1.0, 0.5, 5.0], [0.0, 0.0, -1.0]).unwrap();
    assert!((p[0] - 1.0).abs() < 1e-4 && (p[1] - 0.5).abs() < 1e-4 && p[2] == 0.0);
    // Slanting down from the left.
    let p = ground_hit(&d, [-1.0, 0.5, 1.0], [1.0, 0.0, -0.5]).unwrap();
    assert!((p[0] - 1.0).abs() < 0.01, "{p:?}");
    // Up, or off the map: nothing.
    assert!(ground_hit(&d, [1.0, 0.5, 5.0], [0.0, 0.0, 1.0]).is_none());
    assert!(ground_hit(&d, [9.0, 9.0, 5.0], [0.0, 0.0, -1.0]).is_none());
}

#[test]
fn regions_bounds_cameras_and_the_minimap() {
    let dir = TestDir::new("overlays");
    // A region over the left cell: from -128 to 0 across, -64 to 64 up.
    let mut regions = w3r_rs::Regions::new();
    let mut r = w3r_rs::Region::new("Spawn", -128.0, -64.0, 0.0, 64.0, 0);
    (r.red, r.green, r.blue) = (255, 0, 0);
    regions.regions.push(r);
    dir.write("war3map.w3r", &regions.to_bytes().unwrap());
    let mut cams = w3c_rs::Cameras::new(w3c_rs::Layout::Classic);
    let mut c = w3c_rs::Camera::new("Camera 001", 0.0, 0.0);
    (c.rotation, c.angle_of_attack, c.distance, c.z_offset) = (90.0, 270.0, 256.0, 0.0);
    cams.cameras.push(c);
    dir.write("war3map.w3c", &cams.to_bytes().unwrap());
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()] }.open().0;
    let (t, d) = draw();
    let minimap = (3, 2, vec![0u8; 3 * 2 * 4]);
    let o = Overlays::load(&game, &t, &d, &Objects::default(), minimap);
    assert!(o.notes.is_empty(), "{:?}", o.notes);
    // The region's picture: the left cell, four by four pixels, its
    // edge stronger; the right cell clear.
    assert_eq!((o.region_picture.width, o.region_picture.height), (8, 4));
    let px = |x: u32, y: u32| &o.region_picture.data[((y * 8 + x) * 4) as usize..][..4];
    assert_eq!(px(0, 0), [230, 0, 0, 230]);
    assert_eq!(px(1, 1), [77, 0, 0, 77]);
    assert_eq!(px(5, 1), [0, 0, 0, 0]);
    assert_eq!(o.regions_at(-64.0, 0.0), ["Spawn"]);
    assert!(o.regions_at(64.0, 0.0).is_empty());
    // The camera looks straight down (270°) at (1, 0.5), from two cells up.
    assert_eq!(o.cameras[0].target, [1.0, 0.5, 0.0]);
    let eye = o.cameras[0].eye;
    assert!((eye[0] - 1.0).abs() < 1e-4 && (eye[1] - 0.5).abs() < 1e-4 && (eye[2] - 2.0).abs() < 1e-4, "{eye:?}");
    assert_eq!(o.camera_lines.len() % (2 * POINT), 0);
    // No w3i: no bounds; no wpm: no pathing.
    assert!(o.bounds.is_empty() && o.pathing.data.is_empty());
}

#[test]
fn bounds_follow_the_ground() {
    let (_, d) = draw();
    let mut out = Vec::new();
    rectangle(&d, [0.0, 0.0, 2.0, 1.0], PLAYABLE, &mut out);
    // A point every quarter of a cell, two to a segment, all on the
    // ground (0) lifted.
    assert_eq!(out.len() / POINT, 2 * 4 * (2 + 1 + 2 + 1));
    assert!(out.chunks(POINT).all(|p| (p[2] - 0.08).abs() < 1e-6));
}
