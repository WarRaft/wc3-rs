use super::*;

fn file() -> Vec<u8> {
    let f = mmp_rs::MinimapIcons { version: 0, icons: vec![mmp_rs::Icon { kind: 2, x: 1, y: 2, red: 255, green: 3, blue: 3, alpha: 255 }] };
    f.to_bytes().unwrap()
}

#[test]
fn text_and_back() {
    let name = "/maps/x/war3map.mmp";
    let text = String::from_utf8(convert(TO_TEXT, name, &file()).unwrap()).unwrap();
    assert_eq!(text, "[mmp]\nversion=0\n\n[icon]\nkind=2\nx=1\ny=2\ncolor=255,3,3,255\n");
    assert_eq!(convert(TO_BYTES, name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, name, b"[mmp]\n[icon]\nx=a\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.mmp: line 3: `x` is a whole number");
}
