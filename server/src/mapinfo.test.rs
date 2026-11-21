use super::*;

fn file() -> Vec<u8> {
    let mut info = w3i_rs::Info::new(25);
    info.name = "My map".into();
    info.players.push(w3i_rs::Player { number: 0, kind: 1, race: 1, ..Default::default() });
    info.to_bytes().unwrap()
}

#[test]
fn text_and_back() {
    let name = "/maps/x/war3map.w3i";
    let text = String::from_utf8(convert(TO_TEXT, name, &file()).unwrap()).unwrap();
    assert!(text.starts_with("[w3i]\nversion=25\n"), "{text}");
    assert!(text.contains("name=\"My map\"\n"), "{text}");
    assert_eq!(convert(TO_BYTES, name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, name, b"[w3i]\nversion=25\nscript=1\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.w3i: line 3: `script` is in version 28 (1.31) and later; this file is 25");
    let err = convert(TO_TEXT, name, &[1, 2]).unwrap_err();
    assert!(err.starts_with("Can't read war3map.w3i: "), "{err}");
}

#[test]
fn diagnostics_only_for_info_text() {
    let doc = bni_rs::parse("[w3i]\nversion=18\nweather=RAhr\n[player]\nrace=x\n");
    let errors = diagnostics(Some(Path::new("war3map.w3i")), &doc);
    let lines: Vec<usize> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [3, 5], "{errors:?}");
    assert!(diagnostics(Some(Path::new("x.ini")), &doc).is_empty());
    assert!(diagnostics(Some(Path::new("war3map.w3i")), &bni_rs::parse("[a]\n")).is_empty());
}
