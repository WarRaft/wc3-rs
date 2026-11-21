use super::*;

fn units(layout: Layout) -> Vec<u8> {
    let mut u = doo_rs::Unit::new(doo_rs::Rawcode(*b"hfoo"), 64.0, -32.0, 0);
    if layout == Layout::Skins {
        u.placement.skin = doo_rs::Rawcode(*b"hfoo");
    }
    doo_rs::Units { version: 8, subversion: 11, layout, units: vec![u] }.to_bytes().unwrap()
}

#[test]
fn text_and_back_in_the_maps_layout() {
    let tmp = crate::project::tests::TempDir::new("placement");
    let path = tmp.0.join("war3mapUnits.doo");
    let name = path.to_string_lossy();
    // No war3map.w3i: the layout the file reads in, and the text's.
    for layout in [Layout::Classic, Layout::Skins] {
        let text = String::from_utf8(convert(TO_TEXT, &name, &units(layout)).unwrap()).unwrap();
        assert!(text.starts_with("[units]\nversion=8\nsubversion=11\n\n[unit]\nid=hfoo\n"), "{text}");
        assert_eq!(text.contains("skin=hfoo"), layout == Layout::Skins);
        assert_eq!(convert(TO_BYTES, &name, text.as_bytes()).unwrap(), units(layout));
    }
    // A 1.32 map: skins.
    std::fs::write(tmp.0.join("war3map.w3i"), [31, 0, 0, 0]).unwrap();
    assert_eq!(layout(&path), Some(Layout::Skins));
    assert!(convert(TO_TEXT, &name, &units(Layout::Classic)).unwrap_err().contains("1.32 and later"));
    let err = convert(TO_BYTES, &name, b"[units]\n[unit]\nhp=x\n").unwrap_err();
    assert_eq!(err, "Can't save war3mapUnits.doo: line 3: `hp`: a whole number in range");
}

#[test]
fn diagnostics_only_for_placement_text() {
    let text = "[doodads]\n[doodad]\nlife=300\n";
    let doc = bni_rs::parse(text);
    let errors = diagnostics(Some(Path::new("war3map.doo")), &doc, text);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 3);
    assert!(diagnostics(Some(Path::new("x.ini")), &doc, text).is_empty());
}
