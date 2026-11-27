use super::*;

#[test]
fn a_wide_map_fitted_its_shape_kept_outside_the_bounds_darker_a_tree_a_dot() {
    // 5 by 3 points (4 by 2 cells): grey; a band across the middle.
    let picture = vec![200u8; 5 * 3 * 4];
    let img = image((5, 3, &picture), &[(1.0, 1.0, [0, 54, 0])], Some([1.0, 0.0, 4.0, 2.0]));
    let at = |x: u32, y: u32| {
        let i = ((y * SIZE + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    };
    // 64 a cell: the map from row 64 to 192.
    assert_eq!(at(128, 10), [0, 0, 0, 255], "black above it");
    assert_eq!(at(200, 128), [200, 200, 200, 255]);
    assert_eq!(at(20, 100), [70, 70, 70, 255], "left of the camera bounds: darker");
    assert_eq!(at(64, 128), [0, 54, 0, 255], "the tree at cell (1, 1)");
}

#[test]
fn a_paletted_blp1_the_game_and_our_decoder_read() {
    // A gradient of more than 256 colours.
    let mut rgba = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            rgba.extend_from_slice(&[x as u8, y as u8, ((x + y) / 2) as u8, 255]);
        }
    }
    let bytes = blp(&rgba, SIZE, SIZE);
    assert_eq!(bytes.len(), 1180 + 65536);
    let t = wc3_server::texture::decode(&bytes);
    assert_eq!((t.version.as_str(), t.width, t.height), ("BLP1", SIZE, SIZE), "{}", t.mips[0].error);
    let back = &t.mips[0].rgba;
    // Each pixel near its colour.
    let worst = rgba.chunks_exact(4).zip(back.chunks_exact(4)).map(|(a, b)| (0..3).map(|k| (a[k] as i32 - b[k] as i32).abs()).max().unwrap()).max().unwrap();
    assert!(worst <= 24, "{worst}");
}
