use super::*;

const UNITS: &[u8] = &[
    2, 0, 0, 0, // version
    1, 0, 0, 0, b'h', b'f', b'o', b'o', 0, 0, 0, 0, // one original: hfoo
    1, 0, 0, 0, b'u', b'h', b'p', b'm', 0, 0, 0, 0, 0xF4, 1, 0, 0, b'h', b'f', b'o',
    b'o', // uhpm = 500
    0, 0, 0, 0, // no customs
];

#[test]
fn to_text_and_back() {
    let text = convert(TO_TEXT, "/maps/x.w3x/war3map.w3u", UNITS, &[]).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(
        text.contains("[w3u]\nversion=2\n\n[hfoo]\nuhpm=500\n"),
        "{text}"
    );
    assert_eq!(
        convert(TO_BYTES, "war3map.w3u", text.as_bytes(), &[]).unwrap(),
        UNITS
    );
}

#[test]
fn errors_are_messages() {
    assert_eq!(
        convert(TO_TEXT, "a.txt", UNITS, &[]).unwrap_err(),
        "a.txt isn't object data (.w3a .w3b .w3d .w3h .w3q .w3t .w3u)"
    );
    assert_eq!(
        convert(TO_TEXT, "/m/war3map.w3u", &UNITS[..10], &[]).unwrap_err(),
        "Can't read war3map.w3u: count 1 doesn't fit in the 2 bytes left (at byte 0x4)"
    );
    assert_eq!(
        convert(
            TO_BYTES,
            "war3map.w3u",
            b"[w3u]\nversion=2\n[hfoo]\nuhpm=x\nunam=1.5.1\n",
            &[]
        )
        .unwrap_err(),
        "Can't save war3map.w3u: line 4: expected a value: a number, `unreal` and a number, or a \"string\" (and 1 more)"
    );
    assert_eq!(
        convert(TO_BYTES, "war3map.w3a", b"[w3u]\nversion=2\n", &[]).unwrap_err(),
        "Can't save war3map.w3a: the text holds .w3u data, not .w3a"
    );
}

#[test]
fn text_errors_only_for_object_text() {
    let doc = bni_rs::parse("[w3u]\nversion=2\n[hfoo]\nuhpm=x\nbroken line\n");
    assert_eq!(doc.errors.len(), 1);
    let errors = text_errors(&doc);
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].line, errors[0].col), (4, 6));
    assert!(text_errors(&bni_rs::parse("[Section]\nuhpm=x\n")).is_empty());
}

#[test]
fn names_from_the_game() {
    use crate::project::tests::TempDir;
    let tmp = TempDir::new("objects-names");
    let wc3 = tmp.0.join("wc3");
    std::fs::create_dir_all(&wc3).unwrap();
    let meta = "ID;PWXL;N;E\nC;X1;Y1;K\"ID\"\nC;X2;K\"displayName\"\nC;X1;Y2;K\"uhpm\"\nC;X2;K\"WESTRING_UEVAL_UHPM\"\nE\n";
    storm_rs::archive::MpqArchiveBuilder::new(wc3.join("War3.mpq"))
        .add_file("Units\\UnitMetaData.slk", meta.as_bytes().to_vec())
        .add_file("UI\\WorldEditStrings.txt", b"WESTRING_UEVAL_UHPM=Hit Points\n".to_vec())
        .build()
        .unwrap();
    // A loose file in the folder sits over the archive's.
    std::fs::create_dir_all(wc3.join("UI")).unwrap();
    std::fs::write(wc3.join("UI/WorldEditGameStrings.txt"), "WESTRING_UEVAL_UHPM=Hit Points Maximum\n").unwrap();

    let units = names(std::slice::from_ref(&wc3), Kind::Unit);
    assert_eq!(units.get(w3abdhqtu_rs::Rawcode(*b"uhpm")), Some("Hit Points Maximum"));
    // Abilities' table isn't there.
    assert!(names(std::slice::from_ref(&wc3), Kind::Ability).is_empty());

    let text = convert(TO_TEXT, "war3map.w3u", UNITS, &[wc3]).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("\nuhpm=500 // Hit Points Maximum\n"), "{text}");
    assert_eq!(convert(TO_BYTES, "war3map.w3u", text.as_bytes(), &[]).unwrap(), UNITS);
}
