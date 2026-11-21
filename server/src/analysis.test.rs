use super::*;
use crate::project::tests::TempDir;
use crate::document::Position;

fn doc(language: Language, text: &str, path: Option<&Path>) -> Document {
    Document {
        language,
        version: 0,
        text: text.into(),
        path: path.map(Path::to_path_buf),
    }
}

fn diags(language: Language, text: &str) -> Vec<Diagnostic> {
    diagnostics(&doc(language, text, None), None)
}

fn tokens(language: Language, text: &str) -> Vec<u32> {
    semantic_tokens(&doc(language, text, None), None)
}

#[test]
fn clean_jass_has_no_diagnostics() {
    assert!(diags(Language::Jass, "function Foo takes nothing returns nothing\nendfunction").is_empty());
}

#[test]
fn lint_warnings_cover_their_whole_line() {
    let src = "function Foo takes nothing returns nothing\n    local integer x = 1\nendfunction";
    let d = diags(Language::Jass, src);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range, Range::new(Position::new(1, 0), Position::new(1, 23)));
    assert_eq!(d[0].level, Level::Warning);
    assert!(d[0].message.contains("unused local variable 'x'"));
}

#[test]
fn parse_errors_are_exact() {
    let src = "function Foo takes nothing returns nothing\n    local integer x = 1 +\nendfunction";
    let d = diags(Language::Jass, src);
    assert_eq!(d.len(), 1);
    // `endfunction` is the offending token.
    assert_eq!(d[0].range, Range::new(Position::new(2, 0), Position::new(2, 11)));
    assert_eq!(d[0].level, Level::Error);
}

#[test]
fn zinc_errors_use_utf16_columns() {
    // `😀` costs two UTF-16 units, so the `;` after it shifts right by one.
    let src = "library L {\n    function F() { s = \"😀\" ; x = ; }\n}";
    let d = diags(Language::Zinc, src);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range, Range::new(Position::new(1, 34), Position::new(1, 35)));
}

#[test]
fn vjass_documents_hand_zinc_blocks_to_zinc() {
    let src = "// @dialect vjass\n//! zinc\nlibrary L {\n    function F() {\n        integer unused;\n    }\n}\n//! endzinc\nfunction G takes nothing returns nothing\nendfunction";
    let d = diags(Language::Jass, src);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range.start.line, 4);
    assert!(d[0].message.contains("unused local variable 'unused'"));
    // and highlighting covers the block: `library` is a Zinc keyword token
    let t = tokens(Language::Jass, src);
    assert!(t.chunks(5).any(|t| t[3] == parser_rs::SemanticType::Namespace.index()));
}

#[test]
fn jass_is_plain_unless_the_file_opts_in() {
    let src = "globals\n    integer library = 1\nendglobals";
    assert!(diags(Language::Jass, src).is_empty());
    let d = diags(Language::Jass, &format!("// @dialect vjass\n{src}"));
    assert_eq!(d[0].level, Level::Error);
}

#[test]
fn jass_takes_the_project_dialect_unless_its_header_says_otherwise() {
    let tmp = TempDir::new("analysis-dialect");
    std::fs::write(tmp.0.join(project::MANIFEST), "[jass]\ndialect=vjass\n").unwrap();
    let path = tmp.0.join("src").join("x.j");
    let project = crate::project::Projects::default().for_file(&path);
    let project = project.as_deref();

    // `library` is a keyword in the project's vJASS, so this fails to parse.
    let src = "globals\n    integer library = 1\nendglobals";
    let d = diagnostics(&doc(Language::Jass, src, Some(&path)), project);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].level, Level::Error);

    // A header still wins over the project.
    let plain = format!("// @dialect jass\n{src}");
    assert!(diagnostics(&doc(Language::Jass, &plain, Some(&path)), project).is_empty());
}

#[test]
fn lua_and_angelscript_are_analyzed() {
    let d = diags(Language::Lua, "local unused = 1\nbreak");
    assert_eq!(d.len(), 2);
    assert_eq!(d[1].level, Level::Error); // break outside a loop
    assert!(!tokens(Language::Lua, "local x = 1").is_empty());

    let d = diags(Language::AngelScript, "int f() {\n}");
    assert!(d[0].message.contains("not all paths"));
}

#[test]
fn bni_reports_syntax_errors_and_lint() {
    let d = diags(Language::Bni, "[A]\nno equals sign\n[A]\n");
    let got: Vec<_> = d.iter().map(|d| (d.range.start.line, d.level)).collect();
    assert_eq!(got, vec![(1, Level::Error), (2, Level::Warning)]);
    assert_eq!(d[0].range.start.character, 0);
    assert!(!tokens(Language::Bni, "[A]\nk=1").is_empty());
}

#[test]
fn manifest_settings_are_checked_from_the_edited_text() {
    let src = "[jass]\ndialect=cjass\n";
    let manifest = doc(Language::Bni, src, Some(Path::new("/nowhere/wc3.bni")));
    let d = diagnostics(&manifest, None);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range, Range::new(Position::new(1, 8), Position::new(1, 13)));
    assert_eq!(d[0].level, Level::Error);

    // Any other BNI file is just data: no settings to check.
    let data = doc(Language::Bni, src, Some(Path::new("/x/Data.txt")));
    assert!(diagnostics(&data, None).is_empty());
}

#[test]
fn semantic_tokens_are_per_language() {
    assert_eq!(tokens(Language::Jass, "set x = 1").len(), 4 * 5);
    // x (variable), = (operator), 1 (number); `;` isn't highlighted
    assert_eq!(tokens(Language::Zinc, "x = 1;").len(), 3 * 5);
}

#[test]
fn lint_diagnostics_carry_their_rule() {
    let src = "function F takes nothing returns nothing\n    local integer x = 1\nendfunction";
    let d = diags(Language::Jass, src);
    assert_eq!(d[0].rule, "unused-local");
    // A parse error has none.
    assert_eq!(diags(Language::Jass, "function")[0].rule, "");
}

#[test]
fn infinite_loops_are_off_unless_the_project_turns_them_on() {
    let src = "function F takes nothing returns nothing\n    loop\n        call Sleep(1)\n    endloop\nendfunction";
    assert!(diags(Language::Jass, src).is_empty());

    let tmp = TempDir::new("analysis-lint");
    std::fs::write(tmp.0.join(project::MANIFEST), "[lint]\ninfinite-loop=error\n").unwrap();
    let path = tmp.0.join("x.j");
    let project = crate::project::Projects::default().for_file(&path);
    let d = diagnostics(&doc(Language::Jass, src, Some(&path)), project.as_deref());
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].rule, d[0].level), ("infinite-loop", Level::Error));
}

#[test]
fn pure_jass_reports_left_out_takes_and_returns() {
    let src = "function main
endfunction";
    assert!(diags(Language::Jass, src).is_empty());

    let tmp = TempDir::new("analysis-lint-pure");
    std::fs::write(tmp.0.join(project::MANIFEST), "[lint]
pure=error
").unwrap();
    let path = tmp.0.join("x.j");
    let project = crate::project::Projects::default().for_file(&path);
    let d = diagnostics(&doc(Language::Jass, src, Some(&path)), project.as_deref());
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].rule, d[0].level), ("implicit-signature", Level::Error));
    assert!(d[0].message.contains("'takes nothing returns nothing'"), "{}", d[0].message);

    // The rest of the group: a global outside `globals`, after a function.
    let src = "function main takes nothing returns nothing\nendfunction\ninteger late\n";
    assert!(diags(Language::Jass, src).is_empty());
    let rules: Vec<_> = diagnostics(&doc(Language::Jass, src, Some(&path)), project.as_deref())
        .iter()
        .map(|d| d.rule)
        .collect();
    assert_eq!(rules, vec!["global-outside-globals", "declaration-after-function"]);
}

#[test]
fn bni_lint_follows_the_project_too() {
    let tmp = TempDir::new("analysis-lint-bni");
    std::fs::write(tmp.0.join(project::MANIFEST), "[lint]\nduplicate-section=off\n").unwrap();
    let path = tmp.0.join("Data.bni");
    let project = crate::project::Projects::default().for_file(&path);
    let src = "[A]\n[A]\n";
    assert_eq!(diags(Language::Bni, src)[0].rule, "duplicate-section");
    assert!(diagnostics(&doc(Language::Bni, src, Some(&path)), project.as_deref()).is_empty());
}

#[test]
fn a_vj_file_is_vjass_unless_its_header_says_otherwise() {
    let file = |name: &str, text: &str| doc(Language::Jass, text, Some(Path::new(name)));
    assert_eq!(jass_dialect(&file("/p/a.vj", ""), None), jass_rs::Dialect::VJass);
    assert_eq!(jass_dialect(&file("/p/A.VJ", ""), None), jass_rs::Dialect::VJass);
    assert_eq!(jass_dialect(&file("/p/a.vj", "// @dialect jass\n"), None), jass_rs::Dialect::Jass);
    assert_eq!(jass_dialect(&file("/p/a.j", ""), None), jass_rs::Dialect::Jass);
    assert_eq!(Language::from_path(Path::new("/p/a.vj")), Some(Language::Jass));
    assert_eq!(Language::from_id("vjass"), Some(Language::Jass));
    // `library` is a vJASS keyword: an error only in plain JASS.
    let src = "globals\n    integer library = 1\nendglobals\n";
    assert!(!diagnostics(&file("/p/a.vj", src), None).is_empty());
    assert!(diagnostics(&file("/p/a.j", src), None).is_empty());
}

#[test]
fn an_empty_else_is_marked_on_else_with_a_fix_that_deletes_it() {
    let text = "function F takes nothing returns nothing\n    if true then\n        call F()\n    else\n    endif\nendfunction\n";
    let d = diags(Language::Jass, text).into_iter().find(|d| d.message == "empty 'else' branch").expect("reported");
    assert_eq!(d.range, Range::new(Position::new(3, 4), Position::new(3, 8)));
    let fix = d.fix.expect("a fix");
    assert_eq!(fix.edits, vec![(Range::new(Position::new(3, 0), Position::new(4, 0)), String::new())]);
}

#[test]
fn object_text_errors() {
    let doc = doc(Language::Bni, "[w3u]\nversion=2\n[hfoo]\nuhpm=x\n", None);
    let d = diagnostics(&doc, None);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range, Range::new(Position::new(3, 5), Position::new(3, 6)));
    assert!(d[0].message.starts_with("expected a value"));
}

#[test]
fn string_table_errors_lint_and_tokens() {
    let text = "STRING 1\n{\nKnight\n}\nSTRING 1\n{\nx\n}\nhello\n";
    let d = diagnostics(&doc(Language::Wts, text, None), None);
    let found: Vec<(u32, &str, &str)> = d.iter().map(|d| (d.range.start.line, d.rule, d.message.as_str())).collect();
    assert_eq!(
        found,
        vec![
            (4, "duplicate-string", "STRING 1 is already defined at line 1"),
            (8, "", "expected `STRING <number>`, a `// comment`, '{' or '}', found 'hello'"),
        ]
    );
    assert!(!semantic_tokens(&doc(Language::Wts, text, None), None).is_empty());
}
