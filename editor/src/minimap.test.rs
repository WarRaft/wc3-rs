use super::*;

#[test]
fn a_wide_map_fitted_its_shape_kept_outside_the_bounds_darker_a_tree_a_dot() {
    // 5 by 3 points (4 by 2 cells): grey; a band across the middle.
    let picture = vec![200u8; 5 * 3 * 4];
    let img = image((5, 3, &picture), [0.0, 0.0, 4.0, 2.0], &[(1.0, 1.0, [0, 54, 0])], Some([1.0, 0.0, 4.0, 2.0]));
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
fn a_jpeg_blp1_as_world_editors_our_decoder_reads() {
    let mut rgba = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            rgba.extend_from_slice(&[x as u8, y as u8, 128, 255]);
        }
    }
    let bytes = blp(&rgba, SIZE, SIZE).unwrap();
    let t = wc3_server::texture::decode(&bytes);
    assert_eq!((t.version.as_str(), t.encoding.as_str(), t.width, t.height), ("BLP1", "JPEG", SIZE, SIZE), "{}", t.mips[0].error);
    assert!(t.mips.iter().skip(1).all(|m| m.rgba.is_empty()), "one mip");
    // World Editor's header: JPEG, no alpha, its picture type 5, no mipmaps.
    let field = |i: usize| u32::from_le_bytes(bytes[4 + 4 * i..8 + 4 * i].try_into().unwrap());
    assert_eq!([field(0), field(1), field(4), field(5)], [0, 0, 5, 0]);
    let back = &t.mips[0].rgba;
    let worst = rgba.chunks_exact(4).zip(back.chunks_exact(4)).map(|(a, b)| (0..3).map(|k| (a[k] as i32 - b[k] as i32).abs()).max().unwrap()).max().unwrap();
    assert!(worst <= 16, "{worst}");
}

#[test]
fn icons_where_world_editor_puts_them() {
    // (2)Circumvention: 96 cells, its playable area 7 to 89 across, 5 to 87 up.
    let area = [7.0, 5.0, 89.0, 87.0];
    let cell = |x: f32, y: f32| ((x + 6144.0) / 128.0, (y + 6144.0) / 128.0);
    for ((x, y), want) in [((-768.0, 4544.0), (109, 11)), ((1024.0, -4992.0), (152, 244)), ((-2880.0, -1344.0), (57, 155)), ((3136.0, 448.0), (204, 111)), ((4416.0, -4544.0), (235, 233)), ((-1024.0, 3776.0), (103, 30))] {
        let (cx, cy) = cell(x, y);
        assert_eq!(icon_at(area, cx, cy), want, "{x}, {y}");
    }
    // The Diablo beta's: 180 by 116 playable, 256 across it, centred up and down.
    let area = [6.0, 4.0, 186.0, 120.0];
    let cell = |x: f32, y: f32| ((x + 12288.0) / 128.0, (y + 12288.0) / 128.0);
    for ((x, y), want) in [((5248.0, 1472.0), (186, 64)), ((-576.0, -6080.0), (121, 148))] {
        let (cx, cy) = cell(x, y);
        assert_eq!(icon_at(area, cx, cy), want, "{x}, {y}");
    }
    let file = icons(area, &[(START_LOCATION, 137.0, 107.5, PLAYER_COLORS[1])]);
    assert_eq!((file.icons[0].x, file.icons[0].y, file.icons[0].blue), (186, 64, 255));
}
