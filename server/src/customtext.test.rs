use super::*;

fn file() -> Vec<u8> {
    let f = wct_rs::CustomText {
        version: 1,
        header: wct_rs::Code::new(b"function Helper takes nothing returns nothing\r\nendfunction\r\n"),
        triggers: vec![wct_rs::Code(None), wct_rs::Code::new(b"function T takes nothing returns nothing\r\n    call Helper()\r\nendfunction\r\n")],
        ..wct_rs::CustomText::default()
    };
    f.to_bytes().unwrap()
}

#[test]
fn text_and_back_and_lua_maps() {
    let tmp = crate::project::tests::TempDir::new("customtext");
    let path = tmp.0.join("war3map.wct");
    let name = path.to_string_lossy();
    let text = String::from_utf8(convert(TO_TEXT, &name, &file()).unwrap()).unwrap();
    assert!(text.starts_with("// @wct version=1\n// @wct comment \"\"\n// @wct header\nfunction Helper"), "{text}");
    assert_eq!(convert(TO_BYTES, &name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, &name, b"call X()\n").unwrap_err();
    assert!(err.starts_with("Can't save war3map.wct: line 1: code goes after"), "{err}");
    assert_eq!(document(&path, &file()).unwrap().language, Language::Jass);

    // A Lua map's: `--` markers, Lua.
    let mut info = w3i_rs::Info::new(28);
    info.script = 1;
    std::fs::write(tmp.0.join("war3map.w3i"), info.to_bytes().unwrap()).unwrap();
    let doc = document(&path, &file()).unwrap();
    assert_eq!(doc.language, Language::Lua);
    assert!(doc.text.starts_with("-- @wct version=1\n"));
}

#[test]
fn marker_errors_and_trigger_hints() {
    let path = Path::new("war3map.wct");
    let text = "// @wct version=1\n// @wct header\n// @wct trigger none\n// @wct trigger\ncall X()\n// @wct oops\n";
    let errors = diagnostics(Some(path), text);
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].line, errors[0].col, errors[0].end_col), (6, 1, 13));
    assert!(diagnostics(Some(Path::new("a.j")), text).is_empty());
    assert_eq!(hints(text, 0, 10), r#"[[2,20,"trigger 1"],[3,15,"trigger 2"]]"#);
    assert_eq!(hints(text, 3, 10), r#"[[3,15,"trigger 2"]]"#);
}
