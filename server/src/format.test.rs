use super::*;

fn doc(language: Language, path: &str, text: &str) -> Document {
    Document { language, version: 1, text: text.into(), path: Some(path.into()) }
}

fn apply(text: &str, edits: &[Edit]) -> String {
    parser_rs::apply_edits(text, edits)
}

#[test]
fn formats_jass_in_its_dialect() {
    // `.vj`: vJASS, so `library` is a block.
    let d = doc(Language::Jass, "/p/a.vj", "library L\nfunction F takes nothing returns nothing\ncall X()\nendfunction\nendlibrary\n");
    assert_eq!(
        apply(&d.text, &edits(&d, None, "\t", 1..=usize::MAX)),
        "library L\n\tfunction F takes nothing returns nothing\n\t\tcall X()\n\tendfunction\nendlibrary\n"
    );
    // `.j`: plain JASS, where `library` is a name.
    let d = doc(Language::Jass, "/p/a.j", &d.text);
    assert_eq!(
        apply(&d.text, &edits(&d, None, "\t", 1..=usize::MAX)),
        "library L\nfunction F takes nothing returns nothing\n\tcall X()\nendfunction\nendlibrary\n"
    );
}

#[test]
fn formats_zinc_and_angelscript() {
    let d = doc(Language::Zinc, "/p/a.zn", "library A {\nfunction f() {}\n}\n");
    assert_eq!(apply(&d.text, &edits(&d, None, "  ", 1..=usize::MAX)), "library A {\n  function f() {}\n}\n");
    let d = doc(Language::AngelScript, "/p/a.as", "void f() {\nx();\n}\n");
    assert_eq!(apply(&d.text, &edits(&d, None, "  ", 1..=usize::MAX)), "void f() {\n  x();\n}\n");
}

#[test]
fn a_unit_that_isnt_whitespace_is_four_spaces() {
    let d = doc(Language::AngelScript, "/p/a.as", "void f() {\nx();\n}\n");
    assert_eq!(apply(&d.text, &edits(&d, None, "x", 1..=usize::MAX)), "void f() {\n    x();\n}\n");
}

#[test]
fn other_languages_get_nothing() {
    let d = doc(Language::Bni, "/p/a.bni", "  [a]\n");
    assert!(edits(&d, None, "    ", 1..=usize::MAX).is_empty());
}

#[test]
fn ranges_are_the_indentation_in_utf16() {
    let e = Edit { span: parser_rs::Span::new(3, 2, 3, 5), text: "\t".into() };
    assert_eq!(range(&e), Range::new(Position::new(2, 1), Position::new(2, 4)));
}
