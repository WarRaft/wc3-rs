use super::*;

fn file() -> Vec<u8> {
    let mut regions = w3r_rs::Regions::new();
    regions.regions.push(w3r_rs::Region::new("Spawn", -256.0, -256.0, 256.0, 256.5, 0));
    regions.to_bytes().unwrap()
}

#[test]
fn text_and_back() {
    let name = "/maps/x/war3map.w3r";
    let text = String::from_utf8(convert(TO_TEXT, name, &file()).unwrap()).unwrap();
    assert!(text.starts_with("[w3r]\nversion=5\n\n[Spawn]\nindex=0\nleft=-256\n"), "{text}");
    assert_eq!(convert(TO_BYTES, name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, name, b"[w3r]\n[A]\ntop=x\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.w3r: line 3: `top`: a number");
    assert!(convert(TO_TEXT, name, &[1]).unwrap_err().starts_with("Can't read war3map.w3r"));
}

#[test]
fn diagnostics_only_for_region_text() {
    let doc = bni_rs::parse("[w3r]\n[A]\ncolor=1\n");
    let errors = diagnostics(Some(Path::new("war3map.w3r")), &doc);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 3);
    assert!(diagnostics(Some(Path::new("x.ini")), &doc).is_empty());
    assert!(diagnostics(Some(Path::new("war3map.w3r")), &bni_rs::parse("[a]\n")).is_empty());
}

#[test]
fn colors_for_the_picker() {
    let text = "[w3r]\n[A]\nweather=RAhr\ncolor=0,255,0,255\n";
    assert_eq!(colors(text), "[[3,6,3,17,0,255,0,255]]");
}
