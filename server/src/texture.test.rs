use super::*;

/// A BLP1 palette texture: 2×1, no alpha, red then blue, one mip.
fn palette_blp() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"BLP1");
    for v in [1u32, 0, 2, 1, 4, 0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    let mut table = [0u32; 32];
    table[0] = 156 + 1024;
    table[16] = 2;
    table.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
    let mut palette = vec![0u8; 1024];
    palette[..4].copy_from_slice(&[0, 0, 255, 0]);
    palette[4..8].copy_from_slice(&[255, 0, 0, 0]);
    b.extend_from_slice(&palette);
    b.extend_from_slice(&[0, 1]);
    b
}

#[test]
fn a_blp_decodes_with_its_header_and_mips() {
    let t = decode(&palette_blp());
    assert_eq!(t.error, "");
    assert_eq!((t.version.as_str(), t.encoding.as_str(), t.alpha_bits, t.width, t.height), ("BLP1", "Palette", 0, 2, 1));
    assert_eq!(t.mips.len(), 1);
    assert_eq!(t.mips[0].rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
}

#[test]
fn a_missing_mip_says_so_and_the_rest_still_show() {
    let mut b = palette_blp();
    // Claim a second mip that runs past the end of the file.
    b[28 + 4..28 + 8].copy_from_slice(&1180u32.to_le_bytes());
    b[28 + 64 + 4..28 + 64 + 8].copy_from_slice(&100u32.to_le_bytes());
    let t = decode(&b);
    assert_eq!(t.mips.len(), 2);
    assert!(!t.mips[0].rgba.is_empty());
    assert!(t.mips[1].rgba.is_empty() && !t.mips[1].error.is_empty());
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    let t = decode(b"not a texture");
    assert!(!t.error.is_empty());
    assert!(t.mips.is_empty());
    for len in 0..palette_blp().len() {
        decode(&palette_blp()[..len]);
    }
}
