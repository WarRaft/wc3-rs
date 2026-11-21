use super::*;

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wc3-server-cameras-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A camera file with one camera, in `layout`.
fn file(layout: Layout) -> Vec<u8> {
    let mut cameras = w3c_rs::Cameras::new(layout);
    cameras.cameras.push(w3c_rs::Camera::new("Intro", 512.0, -256.5));
    cameras.to_bytes().unwrap()
}

#[test]
fn text_and_back_in_the_w3i_layout() {
    let dir = temp("layout");
    let path = dir.join("war3map.w3c");
    let name = path.to_string_lossy();
    // No war3map.w3i: 1.27a's layout.
    assert_eq!(layout(&path), Layout::Classic);
    let text = String::from_utf8(convert(TO_TEXT, &name, &file(Layout::Classic)).unwrap()).unwrap();
    assert!(text.starts_with("[w3c]\nversion=0\n\n[Intro]\ntarget_x=512\ntarget_y=-256.5\n"), "{text}");
    assert!(!text.contains("local_yaw"));
    assert_eq!(convert(TO_BYTES, &name, text.as_bytes()).unwrap(), file(Layout::Classic));

    // A 1.32 map: the local rotation, and the older file no longer reads.
    std::fs::write(dir.join("war3map.w3i"), [31, 0, 0, 0, 9, 9]).unwrap();
    assert_eq!(layout(&path), Layout::Local);
    let err = convert(TO_TEXT, &name, &file(Layout::Classic)).unwrap_err();
    assert!(err.contains("1.31 and later"), "{err}");
    let text = String::from_utf8(convert(TO_TEXT, &name, &file(Layout::Local)).unwrap()).unwrap();
    assert!(text.contains("local_yaw=0\n"));
    assert_eq!(convert(TO_BYTES, &name, text.as_bytes()).unwrap(), file(Layout::Local));

    let err = convert(TO_BYTES, &name, b"[w3c]\n[A]\nroll=x\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.w3c: line 3: `roll` is a number");
}

#[test]
fn diagnostics_only_for_camera_text() {
    let dir = temp("diags");
    let doc = bni_rs::parse("[w3c]\n[A]\nlocal_yaw=1\nroll=1\n");
    let errors = diagnostics(Some(&dir.join("war3map.w3c")), &doc);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("1.31 and later"));
    assert_eq!(errors[0].line, 3);
    // Another file, or BNI that isn't a camera file's text: nothing.
    assert!(diagnostics(Some(&dir.join("x.bni")), &doc).is_empty());
    assert!(diagnostics(Some(&dir.join("war3map.w3c")), &bni_rs::parse("[w3u]\nx=1\n")).is_empty());
}
