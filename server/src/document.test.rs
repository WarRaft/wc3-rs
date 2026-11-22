use super::*;

fn doc(text: &str) -> Document {
    Document {
        language: Language::Jass,
        version: 0,
        text: text.into(),
        path: None,
    }
}

fn range(l1: u32, c1: u32, l2: u32, c2: u32) -> Range {
    Range::new(Position::new(l1, c1), Position::new(l2, c2))
}

#[test]
fn languages_come_from_editor_ids() {
    assert_eq!(Language::from_id("jass"), Some(Language::Jass));
    assert_eq!(Language::from_id("bni"), Some(Language::Bni));
    assert_eq!(Language::from_id("w3a"), Some(Language::Bni));
    // The object-data files themselves are binary: only their text form
    // (by the editor's id) is BNI.
    assert_eq!(Language::from_path(Path::new("map.w3x/war3map.w3a")), None);
    assert_eq!(Language::from_id("cobol"), None);
}

#[test]
fn edits_insert_replace_and_delete() {
    let mut d = doc("hello\nworld\n");
    d.apply(range(1, 0, 1, 0), "big ");
    assert_eq!(d.text, "hello\nbig world\n");
    d.apply(range(0, 1, 1, 4), "i, ");
    assert_eq!(d.text, "hi, world\n");
    d.apply(range(0, 2, 0, 9), "");
    assert_eq!(d.text, "hi\n");
}

#[test]
fn positions_count_utf16_units() {
    // 😀 is two UTF-16 units: character 3 is right after it.
    let mut d = doc("a😀b");
    d.apply(range(0, 3, 0, 4), "c");
    assert_eq!(d.text, "a😀c");
}

#[test]
fn positions_past_the_end_are_clamped() {
    let mut d = doc("ab\r\ncd");
    // Past the end of line 0 means before its `\r\n`.
    d.apply(range(0, 99, 0, 99), "!");
    assert_eq!(d.text, "ab!\r\ncd");
    d.apply(range(9, 0, 9, 0), "?");
    assert_eq!(d.text, "ab!\r\ncd?");
}

#[test]
fn ranges_convert_codepoint_columns_to_utf16() {
    let text = "x\naю😀b";
    let lines = LineIndex::new(text);
    // Line 2, codepoint columns 4..5 (`b`) sit after a two-unit emoji.
    assert_eq!(lines.range(text, 2, 4, 5), Range::new(Position::new(1, 4), Position::new(1, 5)));
    assert_eq!(lines.whole_line(text, 2), Range::new(Position::new(1, 0), Position::new(1, 5)));
}

#[test]
fn the_games_bni_txt_files() {
    let bni = |p: &str| Language::from_path(Path::new(p));
    assert_eq!(bni("Game/Units/HumanUnitFunc.txt"), Some(Language::Bni));
    assert_eq!(bni("UI/worldeditstrings.TXT"), Some(Language::Bni));
    assert_eq!(bni("maps/a.w3x/war3mapSkin.txt"), Some(Language::Bni));
    assert_eq!(bni("war3mapMisc.txt"), Some(Language::Bni));
    // Their patches.
    assert_eq!(bni("jngp/UI/TriggerData.patch.txt"), Some(Language::Bni));
    assert_eq!(bni("UI/worldeditstrings.PATCH.txt"), Some(Language::Bni));
    assert_eq!(bni("UI/HelpStrings.patch.txt"), None);
    assert_eq!(bni("notes.patch.txt"), None);
    // Not BNI, or too common a name.
    assert_eq!(bni("UI/HelpStrings.txt"), None);
    assert_eq!(bni("UI/Captions/HumanOp.txt"), None);
    assert_eq!(bni("config.txt"), None);
    assert_eq!(bni("notes.txt"), None);
    assert_eq!(bni("war3map.j"), Some(Language::Jass));
}

/// `BNI_FILES` (and a patch to each) and the extension's `package.json`
/// name the same files.
#[test]
fn bni_files_match_the_extension() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../extension/package.json")).unwrap();
    let start = manifest.find("\"id\": \"bni\"").unwrap();
    let rest = &manifest[start..];
    let list = &rest[rest.find("\"filenames\": [").unwrap()..];
    let list = &list[..list.find(']').unwrap()];
    let mut names: Vec<String> = list
        .split('"')
        .skip(3)
        .step_by(2)
        .map(|n| n.to_ascii_lowercase())
        .filter(|n| n != "wc3.bni")
        .collect();
    names.sort();
    let mut ours: Vec<String> = BNI_FILES
        .iter()
        .flat_map(|n| [n.to_string(), crate::game::patch_name(n)])
        .collect();
    ours.sort();
    assert_eq!(names, ours);
    let block = &rest[..rest.find('}').unwrap()];
    let patterns = &block[block.find("\"filenamePatterns\": [").unwrap()..];
    let patterns: Vec<&str> = patterns[..patterns.find(']').unwrap()].split('"').skip(3).step_by(2).collect();
    assert_eq!(patterns, vec!["war3map*.txt"]);
}

#[test]
fn decoded_without_a_bom() {
    assert_eq!(decode(b"\xEF\xBB\xBFSTRING 1".to_vec()), "STRING 1");
    assert_eq!(decode(b"a\xFFb".to_vec()), "a\u{FFFD}b");
}
