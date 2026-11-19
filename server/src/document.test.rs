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
