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
    let text = convert(TO_TEXT, "/maps/x.w3x/war3map.w3u", UNITS, &Metadata::default()).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(
        text.contains("[w3u]\nversion=2\n\n[hfoo]\nuhpm=500\n"),
        "{text}"
    );
    assert_eq!(
        convert(TO_BYTES, "war3map.w3u", text.as_bytes(), &Metadata::default()).unwrap(),
        UNITS
    );
}

#[test]
fn errors_are_messages() {
    assert_eq!(
        convert(TO_TEXT, "a.txt", UNITS, &Metadata::default()).unwrap_err(),
        "a.txt isn't object data (.w3a .w3b .w3d .w3h .w3q .w3t .w3u)"
    );
    assert_eq!(
        convert(TO_TEXT, "/m/war3map.w3u", &UNITS[..10], &Metadata::default()).unwrap_err(),
        "Can't read war3map.w3u: count 1 doesn't fit in the 2 bytes left (at byte 0x4)"
    );
    assert_eq!(
        convert(
            TO_BYTES,
            "war3map.w3u",
            b"[w3u]\nversion=2\n[hfoo]\nuhpm=x\nunam=1.5.1\n",
            &Metadata::default()
        )
        .unwrap_err(),
        "Can't save war3map.w3u: line 4: expected a value: a number, `unreal` and a number, or a \"string\" (and 1 more)"
    );
    assert_eq!(
        convert(TO_BYTES, "war3map.w3a", b"[w3u]\nversion=2\n", &Metadata::default()).unwrap_err(),
        "Can't save war3map.w3a: the text holds .w3u data, not .w3a"
    );
}

#[test]
fn diagnostics_only_for_object_text() {
    let doc = bni_rs::parse("[w3u]\nversion=2\n[hfoo]\nuhpm=x\nbroken line\numvs=1.5\n");
    assert_eq!(doc.errors.len(), 1);
    let (errors, lint) = diagnostics(&doc, None);
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].line, errors[0].col), (4, 6));
    assert!(lint.is_empty());

    // With the game's metadata, the fields are checked too.
    let table = "C;X1;Y1;K\"ID\"\nC;X2;K\"type\"\nC;X3;K\"field\"\nC;X1;Y2;K\"umvs\"\nC;X2;K\"int\"\nC;X3;K\"spd\"\n";
    let meta = Metadata::from_tables(table, &[]);
    let (_, lint) = diagnostics(&doc, Some(&meta));
    let rules: Vec<_> = lint.iter().map(|d| (d.line, d.rule)).collect();
    assert_eq!(rules, vec![(6, parser_rs::rule::OBJECT_VALUE_TYPE)]);

    assert_eq!(diagnostics(&bni_rs::parse("[Section]\nuhpm=x\n"), Some(&meta)), (Vec::new(), Vec::new()));
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

    let units = metadata(std::slice::from_ref(&wc3), Kind::Unit);
    assert_eq!(units.name(w3abdhqtu_rs::Rawcode(*b"uhpm")), Some("Hit Points Maximum"));
    // Abilities' table isn't there.
    assert!(metadata(std::slice::from_ref(&wc3), Kind::Ability).is_empty());

    let text = convert(TO_TEXT, "war3map.w3u", UNITS, &units).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("\nuhpm=500 // Hit Points Maximum\n"), "{text}");
    assert_eq!(convert(TO_BYTES, "war3map.w3u", text.as_bytes(), &Metadata::default()).unwrap(), UNITS);
}

#[test]
fn which_files_change_the_metadata() {
    let folder = Path::new("/g/wc3");
    assert!(affects(folder, Path::new("/g/wc3/War3Patch.mpq")));
    assert!(affects(folder, Path::new("/g/wc3/units/abilitydata.slk")));
    assert!(affects(folder, Path::new("/g/wc3/UI/WorldEditStrings.txt")));
    assert!(!affects(folder, Path::new("/g/wc3/Units/Other.slk")));
    assert!(!affects(folder, Path::new("/g/other/War3Patch.mpq")));
    assert!(is_game_file_name(Path::new("/x/UNITMETADATA.SLK")));
    assert!(!is_game_file_name(Path::new("/x/a.slk")));
}

#[test]
fn the_cache_drops_what_changed() {
    let tmp = crate::project::tests::TempDir::new("objects-cache");
    let wc3 = tmp.0.join("wc3");
    std::fs::create_dir_all(wc3.join("Units")).unwrap();
    let table = |name: &str| format!("C;X1;Y1;K\"ID\"\nC;X2;K\"field\"\nC;X1;Y2;K\"uhpm\"\nC;X2;K\"{name}\"\n");
    std::fs::write(wc3.join("Units/UnitMetaData.slk"), table("HP")).unwrap();
    let game = vec![wc3.clone()];
    let mut cache = Cache::default();
    assert_eq!(cache.get(&game, Kind::Unit).name(w3abdhqtu_rs::Rawcode(*b"uhpm")), Some("HP"));

    std::fs::write(wc3.join("Units/UnitMetaData.slk"), table("Life")).unwrap();
    // Until the change is reported, the cache answers.
    assert_eq!(cache.get(&game, Kind::Unit).name(w3abdhqtu_rs::Rawcode(*b"uhpm")), Some("HP"));
    assert!(!cache.disk_changed(&[tmp.0.join("elsewhere.slk")]));
    assert!(cache.disk_changed(&[wc3.join("Units/UnitMetaData.slk")]));
    assert_eq!(cache.get(&game, Kind::Unit).name(w3abdhqtu_rs::Rawcode(*b"uhpm")), Some("Life"));
}

#[test]
fn hovers() {
    use crate::document::Position;
    let table = "C;X1;Y1;K\"ID\"\nC;X2;K\"field\"\nC;X3;K\"type\"\nC;X4;K\"minVal\"\nC;X5;K\"maxVal\"\nC;X6;K\"repeat\"\nC;X7;K\"data\"\nC;X8;K\"useSpecific\"\n\
C;X1;Y2;K\"Hbz1\"\nC;X2;K\"Number of Waves\"\nC;X3;K\"int\"\nC;X4;K\"1\"\nC;X5;K\"99999\"\nC;X6;K4\nC;X7;K1\nC;X8;K\"AHbz,ACbz\"\n\
C;X1;Y3;K\"aher\"\nC;X2;K\"Hero Ability\"\nC;X3;K\"bool\"\nC;X6;K0\nC;X7;K0\n";
    let objects = "C;X1;Y1;K\"alias\"\nC;X2;K\"code\"\nC;X1;Y2;K\"AIbz\"\nC;X2;K\"AHbz\"\n";
    let meta = Metadata::from_tables(table, &[]).with_codes(objects);
    let text = "[w3a]\nversion=2\n[A000]\n_parent=AIbz\nHbz1[2].A=x\naher=1\nzzzz=1\n";
    let at = |line, character| hover(text, Position::new(line, character), &meta);
    assert_eq!(
        at(4, 2).unwrap(),
        "**Number of Waves** `Hbz1`\n\n`int`: a whole number · 1 to 99999 in World Editor  \nper level · data column A (`.A`)\n\nData of `AHbz`, `ACbz`. `AIbz` has the data of `AHbz`."
    );
    assert_eq!(at(5, 0).unwrap(), "**Hero Ability** `aher`\n\n`bool`: a whole number · 0 or 1");
    let strings = hover("[w3a]\nversion=2\n[A000]\nanam=\"x\"\n", Position::new(3, 0), &Metadata::from_tables("C;X1;Y1;K\"ID\"\nC;X2;K\"type\"\nC;X3;K\"field\"\nC;X1;Y2;K\"anam\"\nC;X2;K\"string\"\nC;X3;K\"Name\"\n", &[]));
    assert_eq!(strings.unwrap(), "**Name** `anam`\n\n`string`");
    assert_eq!(at(6, 1).unwrap(), "`zzzz`\n\nNot a field of abilities in this game's metadata.");
    // Off a key: on a value, on `[A000]`.
    assert_eq!(at(4, 11), None);
    assert_eq!(at(2, 1), None);
    // No game.
    assert!(hover(text, Position::new(4, 0), &Metadata::default()).unwrap().contains("isn't in a project with `game=`"));
}
