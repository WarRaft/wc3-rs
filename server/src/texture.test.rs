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

#[test]
fn a_pathing_map_draws_with_its_cells_flags() {
    let map = wpm_rs::Pathing { version: 0, width: 2, height: 2, cells: vec![0x40, 0x48, 0xCA, 0x46] };
    let t = decode(&map.to_bytes().unwrap());
    assert_eq!(t.error, "");
    assert_eq!((t.version.as_str(), t.encoding.as_str(), t.width, t.height), ("MP3W 0", "Pathing", 2, 2));
    // North up: the file's second row first.
    assert_eq!(t.cells, [0xCA, 0x46, 0x40, 0x48]);
    assert_eq!(&t.mips[0].rgba[..8], &[255, 0, 255, 255, 255, 255, 0, 255]);
    assert_eq!(t.flags.len(), 8);
    assert_eq!(t.flags[1], (0x02, "no walking".to_string()));
    assert!(decode(b"MP3W\x01\0\0\0").error.contains("unknown wpm version"));
}

#[test]
fn a_tga_decodes_bottom_up_or_top_down() {
    // 2x1, 24-bit true-color, bottom-up (the default): blue, green, red
    // bytes per pixel.
    let mut tga = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0];
    tga.extend([0, 0, 255, 0, 255, 0]);
    assert!(looks_like_tga(&tga));
    let t = decode(&tga);
    assert_eq!(t.error, "");
    assert_eq!((t.version.as_str(), t.encoding.as_str(), t.width, t.height, t.alpha_bits), ("TGA", "True-color 24-bit", 2, 1, 0));
    assert_eq!(t.mips[0].rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
    assert!(!looks_like_tga(b"BLP1 not a tga at all"));
    assert!(decode(&tga[..20]).error.starts_with("Can't read this TGA"));
}

/// Every `.tga` under the folder `WC3_TGA` names decodes.
#[test]
#[ignore]
fn real_tgas() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tga")) {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(std::path::Path::new(&std::env::var("WC3_TGA").unwrap()), &mut files);
    let bad: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let bytes = std::fs::read(f).unwrap();
            let t = decode(&bytes);
            (!t.error.is_empty() || t.version != "TGA").then(|| format!("{}: {}", f.display(), t.error))
        })
        .collect();
    println!("{} files", files.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
