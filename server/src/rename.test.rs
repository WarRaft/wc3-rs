use super::*;
use crate::project::tests::TempDir;

fn workspace(name: &str, files: &[(&str, &str)]) -> (TempDir, Workspace) {
    let tmp = TempDir::new(name);
    std::fs::write(tmp.0.join("wc3.bni"), "").unwrap();
    for (file, text) in files {
        std::fs::write(tmp.0.join(file), text).unwrap();
    }
    let mut ws = Workspace::default();
    ws.set_roots(vec![tmp.0.clone()]);
    ws.take_reports();
    (tmp, ws)
}

fn id(tmp: &TempDir, file: &str) -> String {
    tmp.0.join(file).to_string_lossy().into_owned()
}

/// (file, line, character) of each edit.
fn edits(answer: &Answer) -> Vec<(String, u32, u32)> {
    assert!(answer.text.is_empty(), "refused: {}", answer.text);
    answer
        .locations
        .iter()
        .map(|l| (file_name(&l.id).to_string(), l.range.start.line, l.range.start.character))
        .collect()
}

fn ren(ws: &Workspace, tmp: &TempDir, file: &str, line: u32, ch: u32, new: &str) -> Answer {
    rename(ws, &id(tmp, file), Position::new(line, ch), new)
}

const LIB: &str = "\
function Helper takes integer n returns integer
    local integer twice = n * 2
    return twice
endfunction
";

const USER: &str = "\
function Main takes nothing returns nothing
    call Helper(1)
endfunction
";

#[test]
fn renames_a_function_in_every_file() {
    let (tmp, ws) = workspace("ren-fn", &[("lib.j", LIB), ("main.j", USER)]);
    let a = ren(&ws, &tmp, "main.j", 1, 10, "Assist");
    assert_eq!(edits(&a), vec![("lib.j".into(), 0, 9), ("main.j".into(), 1, 9)]);
}

#[test]
fn renames_a_local_in_its_function_only() {
    let (tmp, ws) = workspace("ren-local", &[("lib.j", LIB)]);
    let a = ren(&ws, &tmp, "lib.j", 2, 12, "doubled");
    assert_eq!(edits(&a), vec![("lib.j".into(), 1, 18), ("lib.j".into(), 2, 11)]);
}

#[test]
fn prepare_returns_the_name_under_the_cursor() {
    let (tmp, ws) = workspace("ren-prepare", &[("lib.j", LIB), ("main.j", USER)]);
    let a = prepare(&ws, &id(&tmp, "main.j"), Position::new(1, 12));
    assert_eq!(edits(&a), vec![("main.j".into(), 1, 9)]);
    assert_eq!(a.locations[0].name, "Helper");
}

#[test]
fn refuses_what_isnt_declared_in_the_project_or_is_a_native() {
    let (tmp, ws) = workspace("ren-foreign", &[("main.j", USER)]);
    let a = ren(&ws, &tmp, "main.j", 1, 10, "X");
    assert!(a.text.contains("isn't declared in this project"), "{}", a.text);
    assert!(a.locations.is_empty());

    let common = "native GetUnitX takes unit u returns real\n";
    let call = "function F takes nothing returns real\n    return GetUnitX(null)\nendfunction\n";
    let (tmp, ws) = workspace("ren-native", &[("common.j", common), ("f.j", call)]);
    let a = ren(&ws, &tmp, "f.j", 1, 12, "X");
    assert!(a.text.contains("native"), "{}", a.text);
    let a = prepare(&ws, &id(&tmp, "f.j"), Position::new(1, 12));
    assert!(a.text.contains("native"), "{}", a.text);
}

#[test]
fn refuses_nothing_under_the_cursor() {
    let (tmp, ws) = workspace("ren-nothing", &[("lib.j", LIB)]);
    assert!(ren(&ws, &tmp, "lib.j", 3, 2, "x").text.contains("no symbol"));
}

#[test]
fn refuses_invalid_names_and_keywords_of_every_language_involved() {
    let lua = "function main()\n  Helper(1)\nend\n";
    let (tmp, ws) = workspace("ren-names", &[("lib.j", LIB), ("main.lua", lua)]);
    let try_name = |new: &str| ren(&ws, &tmp, "lib.j", 0, 10, new).text;
    assert!(try_name("2fast").contains("isn't a valid name"));
    assert!(try_name("has space").contains("isn't a valid name"));
    assert!(try_name("_hidden").contains("must start with a letter"));
    assert!(try_name("Trailing_").contains("can't end with `_`"));
    assert!(try_name("endfunction").contains("keyword in JASS"));
    assert!(try_name("library").contains("keyword in JASS")); // vJASS counts
    assert!(try_name("repeat").contains("keyword in Lua")); // used from Lua too
    assert!(try_name("Fine_Name2").is_empty());
}

#[test]
fn refuses_a_name_already_declared_in_the_same_scope() {
    let (tmp, ws) = workspace("ren-conflict", &[("lib.j", LIB), ("main.j", USER)]);
    let a = ren(&ws, &tmp, "main.j", 1, 10, "Main");
    assert_eq!(a.text, "`Main` is already declared in main.j:1.");
    // A local may take a name used elsewhere in the project.
    assert!(ren(&ws, &tmp, "lib.j", 2, 12, "Main").text.is_empty());
    // ...but not one of its own function's.
    assert!(ren(&ws, &tmp, "lib.j", 2, 12, "n").text.contains("already declared"));
}

#[test]
fn members_are_renamed_only_when_declared_once() {
    let zinc = "\
library L {
    public struct A { integer x; method get() -> integer { return this.x; } }
}
";
    let (tmp, ws) = workspace("ren-member", &[("a.zn", zinc)]);
    let a = ren(&ws, &tmp, "a.zn", 1, 71, "y");
    assert_eq!(edits(&a), vec![("a.zn".into(), 1, 30), ("a.zn".into(), 1, 71)]);

    let two = format!("{zinc}library M {{ public struct B {{ integer x; }} }}\n");
    let (tmp, ws) = workspace("ren-member-two", &[("a.zn", &two)]);
    let a = ren(&ws, &tmp, "a.zn", 1, 71, "y");
    assert!(a.text.contains("declared 2 times"), "{}", a.text);
}

#[test]
fn an_angelscript_class_takes_its_constructors_along() {
    let src = "class Unit {\n    Unit() { }\n    ~Unit() { }\n}\nvoid f() { Unit u; Unit@ p = Unit(); }\n";
    let (tmp, ws) = workspace("ren-class", &[("u.as", src)]);
    let expected: Vec<(String, u32, u32)> = vec![
        ("u.as".into(), 0, 6),
        ("u.as".into(), 1, 4),
        ("u.as".into(), 2, 5),
        ("u.as".into(), 4, 11),
        ("u.as".into(), 4, 19),
        ("u.as".into(), 4, 29),
    ];
    // From the class name...
    let mut got = edits(&ren(&ws, &tmp, "u.as", 0, 7, "Hero"));
    got.sort();
    assert_eq!(got, expected);
    // ...or from the constructor.
    let mut got = edits(&ren(&ws, &tmp, "u.as", 1, 5, "Hero"));
    got.sort();
    assert_eq!(got, expected);
}

#[test]
fn the_same_name_is_no_edit() {
    let (tmp, ws) = workspace("ren-same", &[("lib.j", LIB)]);
    assert_eq!(ren(&ws, &tmp, "lib.j", 0, 10, "Helper"), Answer::default());
}

#[test]
fn angelscript_rename_conflicts_only_within_the_namespace() {
    let src = "namespace A { class X { } }\nnamespace B { class Y { } }\nvoid f() { A::X@ x; }\n";
    let (tmp, ws) = workspace("ren-as-ns", &[("n.as", src)]);
    // B already has a `Y`, A doesn't: renaming A::X to Y is fine.
    let a = ren(&ws, &tmp, "n.as", 0, 21, "Y");
    assert_eq!(edits(&a), vec![("n.as".into(), 0, 20), ("n.as".into(), 2, 14)]);
    // ...but not B::Y to... a name B already has.
    let src2 = format!("{src}namespace B {{ class Z {{ }} }}\n");
    let (tmp, ws) = workspace("ren-as-ns2", &[("n.as", &src2)]);
    let a = ren(&ws, &tmp, "n.as", 1, 21, "Z");
    assert_eq!(a.text, "`Z` is already declared in n.as:4.");
}

#[test]
fn refuses_when_a_file_in_a_map_uses_the_name() {
    let (tmp, mut ws) = workspace("ren-map", &[("lib.j", LIB)]);
    std::fs::write(tmp.0.join("wc3.bni"), "[project]\nsources=.\n").unwrap();
    ws.disk_changed(&[tmp.0.join("wc3.bni")]);
    crate::map::tests::make_map(&tmp.0.join("Test.w3x"), &[("war3map.j", USER)]);
    ws.disk_changed(&[tmp.0.join("Test.w3x")]);
    ws.take_reports();
    let a = ren(&ws, &tmp, "lib.j", 0, 10, "Assist");
    assert!(a.locations.is_empty());
    assert!(
        a.text.contains("used in war3map.j inside the map Test.w3x"),
        "{}",
        a.text
    );
    // A name only the project's own files use still renames.
    assert_eq!(edits(&ren(&ws, &tmp, "lib.j", 1, 20, "double")).len(), 2);
}

#[test]
fn content_answers_with_the_text_of_a_file_in_a_map() {
    let (tmp, mut ws) = workspace("content-map", &[]);
    let map = tmp.0.join("Test.w3x");
    crate::map::tests::make_map(&map, &[("war3map.j", USER)]);
    ws.disk_changed(std::slice::from_ref(&map));
    ws.take_reports();
    let answer = |id: &std::path::Path| crate::index::answer(&ws, crate::index::QueryKind::Content, &id.to_string_lossy(), Position::new(0, 0), "");
    assert_eq!(answer(&map.join("war3map.j")).text, USER);
    assert_eq!(answer(&map.join("missing.j")).text, "");
}
