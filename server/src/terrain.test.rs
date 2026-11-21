use super::*;
use w3e_rs::{Point, Rawcode, Terrain, ZERO_HEIGHT};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wc3-server-terrain-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 2x2 terrain: every point texture 1.
fn terrain() -> Terrain {
    let mut p = Point { height: ZERO_HEIGHT, layer: 0xF0, ..Point::default() };
    p.set_texture(w3e_rs::Version::of(11), 1);
    Terrain {
        version: 11,
        tileset: b'L',
        custom_tilesets: 0,
        ground: vec![Rawcode(*b"Ldrt"), Rawcode(*b"Lgrs")],
        cliffs: vec![],
        width: 2,
        height: 2,
        offset_x: -128.0,
        offset_y: -128.0,
        points: vec![p; 4],
    }
}

#[test]
fn text_and_back_keeps_the_points() {
    let dir = temp("convert");
    let path = dir.join("war3map.w3e");
    let name = path.to_string_lossy();
    let bytes = terrain().to_bytes().unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let text = String::from_utf8(convert(TO_TEXT, &name, &bytes).unwrap()).unwrap();
    assert!(text.contains("[w3e]\n"), "{text}");
    assert!(text.contains("ground=Ldrt,Lgrs\n"), "{text}");
    assert_eq!(convert(TO_BYTES, &name, text.as_bytes()).unwrap(), bytes);

    let saved = convert(TO_BYTES, &name, text.replace("tileset=L", "tileset=A").as_bytes()).unwrap();
    let back = w3e_rs::parse(&saved).unwrap();
    assert_eq!((back.tileset, back.points), (b'A', terrain().points));

    let err = convert(TO_BYTES, &name, b"[w3e]\nground=Ldrt\n").unwrap_err();
    assert!(err.starts_with("Can't save war3map.w3e: 4 points use ground texture 1"), "{err}");
    let err = convert(TO_BYTES, &name, b"[w3e]\nwidth=x\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.w3e: line 2: a whole number, 0 or more");
    let err = convert(TO_BYTES, &dir.join("gone.w3e").to_string_lossy(), b"[w3e]\n").unwrap_err();
    assert!(err.contains("can't read its points"), "{err}");
}

#[test]
fn diagnostics_only_for_terrain_text() {
    let dir = temp("diags");
    let doc = bni_rs::parse("[w3e]\ntileset=?\n");
    let errors = diagnostics(Some(&dir.join("war3map.w3e")), &doc);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 2);
    assert!(diagnostics(Some(&dir.join("x.ini")), &doc).is_empty());
    assert!(diagnostics(Some(&dir.join("war3map.w3e")), &bni_rs::parse("[a]\n")).is_empty());
}
