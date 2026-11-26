use super::*;
use crate::testdir::{TestDir, tga};

fn game(dir: &TestDir) -> Game {
    let lookup = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() };
    lookup.open().0
}

/// A picture whose every cell is one colour: red its column, green its row.
fn cells(cols: u32, cell: u32) -> Vec<u8> {
    tga((cols * cell) as u16, (4 * cell) as u16, |x, y| [(x / cell) as u8, (y / cell) as u8, 0, 255])
}

#[test]
fn ground_textures_are_cut_into_their_cells_of_one_size() {
    let dir = TestDir::new("ground");
    dir.write("a.blp", &cells(4, 2));
    dir.write("b.blp", &cells(8, 4));
    let paths = [("A".to_string(), Some("a.blp".to_string())), ("B".into(), Some("b.blp".into())), ("C".into(), None)];
    let (textures, layers) = ground(&game(&dir), &paths);
    // The biggest cell: b's, 4; a's scaled up.
    assert_eq!(layers.size, 4);
    assert_eq!(layers.count, 16 + 32 + 16);
    assert_eq!((textures[0].first, textures[0].count, textures[0].extended), (0, 16, false));
    assert_eq!((textures[1].first, textures[1].count, textures[1].extended), (16, 32, true));
    assert!(textures[2].error.contains("not in TerrainArt"));
    let layer = |n: u32| &layers.rgba[(n * 16 * 4) as usize..][..4];
    // Left to right, top to bottom: a's 6th cell is column 1, row 1.
    assert_eq!(layer(5), [1, 1, 0, 255]);
    // An extended texture's right half after its left: b's 17th cell is
    // column 4, row 0.
    assert_eq!(layer(16 + 16), [4, 0, 0, 255]);
    assert_eq!(layer(16 + 15), [3, 3, 0, 255]);
}

#[test]
fn water_frames_until_one_is_missing() {
    let dir = TestDir::new("water");
    dir.write("W00.blp", &tga(2, 2, |_, _| [1, 2, 3, 4]));
    dir.write("W01.blp", &tga(2, 2, |_, _| [5, 6, 7, 8]));
    let (frames, error) = water(&game(&dir), "W", 3);
    assert_eq!((frames.size, frames.count), (2, 2));
    assert_eq!(&frames.rgba[16..20], [5, 6, 7, 8]);
    assert!(error.contains("W02.blp"), "{error}");
}

/// In HD a ground texture is its `_Diffuse.dds` (2.0's tilesets, as
/// `Lordc_GrassTrim`, have no other); without one, the file the table
/// names; in SD, always that one.
#[test]
fn hd_ground_textures_their_diffuse() {
    let dir = TestDir::new("ground-hd");
    dir.write("TerrainArt/Lordc_GrassTrim_Diffuse.dds", &dds_rgba(8, 8, [10, 20, 30, 255]));
    dir.write("TerrainArt/Lordc_GrassTrim.blp", &cells(4, 2));
    dir.write("TerrainArt/Old.blp", &cells(4, 2));
    let paths = [("eCt9".to_string(), Some("TerrainArt\\Lordc_GrassTrim.blp".to_string())), ("Ldrt".into(), Some("TerrainArt\\Old.blp".into()))];
    let open = |hd| wc3_server::game::Lookup { roots: vec![dir.path().to_path_buf()], hd, ..Default::default() }.open().0;
    let (textures, layers) = ground(&open(true), &paths);
    assert!(textures.iter().all(|t| t.error.is_empty()), "{textures:?}");
    assert_eq!(layers.size, 2);
    assert_eq!(&layers.rgba[..4], [10, 20, 30, 255]);
    assert_eq!(&layers.rgba[(16 * 2 * 2 * 4) as usize..][..4], [0, 0, 0, 255]);
    let (_, layers) = ground(&open(false), &paths);
    assert_eq!(&layers.rgba[..4], [0, 0, 0, 255]);
    // A cliff's texture the same way.
    let (layers, errors) = whole(&open(true), &["TerrainArt\\Lordc_GrassTrim.blp".to_string()]);
    assert_eq!((&layers.rgba[..4], errors), ([10, 20, 30, 255].as_slice(), vec![String::new()]));
}

/// An uncompressed RGBA DDS of one colour.
fn dds_rgba(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    let mut b = b"DDS ".to_vec();
    let mut head = [0u32; 31];
    head[0] = 124;
    head[1] = 0x1 | 0x2 | 0x4 | 0x1000;
    head[2] = h;
    head[3] = w;
    head[4] = w * 4;
    head[18] = 32;
    head[19] = 0x41;
    head[21] = 32;
    (head[22], head[23], head[24], head[25]) = (0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000);
    head[26] = 0x1000;
    head.iter().for_each(|v| b.extend(v.to_le_bytes()));
    for _ in 0..w * h {
        b.extend(px);
    }
    b
}
