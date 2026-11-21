use super::*;

fn file() -> Vec<u8> {
    let list = imp_rs::Imports { version: 1, files: vec![imp_rs::Import { flags: 8, path: "Icon.blp".into() }] };
    list.to_bytes().unwrap()
}

#[test]
fn text_and_back() {
    let name = "/maps/x/war3map.imp";
    let text = String::from_utf8(convert(TO_TEXT, name, &file()).unwrap()).unwrap();
    assert_eq!(text, "[imp]\nversion=1\nfile=8,\"Icon.blp\"\n");
    assert_eq!(convert(TO_BYTES, name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, name, b"[imp]\nfile=x,\"a\"\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.imp: line 2: the flags are a number from 0 to 255");
}

#[test]
fn diagnostics_only_for_import_text() {
    let doc = bni_rs::parse("[imp]\nfile=1\n");
    assert_eq!(diagnostics(Some(Path::new("war3map.imp")), &doc).len(), 1);
    assert!(diagnostics(Some(Path::new("x.ini")), &doc).is_empty());
}
