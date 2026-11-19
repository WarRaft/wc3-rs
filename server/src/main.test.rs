use super::*;

#[test]
fn token_ok_accepts_matching_token() {
    assert!(token_ok("/document/update?token=abc123", "abc123"));
}

#[test]
fn token_ok_rejects_missing_or_wrong_token() {
    assert!(!token_ok("/document/update", "abc123"));
    assert!(!token_ok("/document/update?token=wrong", "abc123"));
    assert!(!token_ok("/document/update?other=1", "abc123"));
}

#[test]
fn token_ok_finds_token_among_other_params() {
    assert!(token_ok("/x?a=1&token=abc123&b=2", "abc123"));
}

#[test]
fn handle_update_reports_no_diagnostics_for_clean_source() {
    let body = serde_json::json!({
        "text": "function Foo takes nothing returns nothing\nendfunction"
    })
    .to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert!(resp.diagnostics.is_empty());
}

#[test]
fn handle_update_reports_lint_warning_without_a_column() {
    let src = "function Foo takes nothing returns nothing\n    local integer x = 1\nendfunction";
    let body = serde_json::json!({ "text": src }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    assert_eq!(d.line0, 1);
    assert_eq!(d.char0, None);
    assert_eq!(d.severity, "warning");
    assert!(d.message.contains("unused local variable 'x'"));
}

#[test]
fn handle_update_reports_parse_error_with_utf16_columns() {
    let src = "function Foo takes nothing returns nothing\n    local integer x = 1 +\nendfunction";
    let body = serde_json::json!({ "text": src }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    // "endfunction" starts the error, on line index 2 (0-indexed), 11 chars wide.
    assert_eq!(d.line0, 2);
    assert_eq!(d.char0, Some(0));
    assert_eq!(d.end_char0, Some(11));
    assert_eq!(d.severity, "error");
}

#[test]
fn handle_update_returns_empty_diagnostics_for_malformed_request_body() {
    let resp = handle_update("not json", &mut Projects::default());
    assert!(resp.diagnostics.is_empty());
}

#[test]
fn utf16_char_range_widens_after_an_astral_codepoint() {
    // "aю😀b" — 😀 is outside the Basic Multilingual Plane, so it costs 2
    // UTF-16 units; anything after it shifts right by one relative to its
    // codepoint column (see parser-rs's own tests for the general case).
    let (start, end) = utf16_char_range("aю😀b", 1, 4, 5);
    assert_eq!((start, end), (4, 5));
}

#[test]
fn handle_update_dispatches_zinc_by_language_id() {
    let src = "library L {\n    function F() {\n        integer unused;\n    }\n}";
    let body = serde_json::json!({ "text": src, "languageId": "zinc" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    assert_eq!(d.line0, 2);
    assert_eq!(d.severity, "warning");
    assert!(d.message.contains("unused local variable 'unused'"));
}

#[test]
fn handle_update_reports_zinc_parse_errors_with_utf16_columns() {
    // `😀` costs two UTF-16 units, so the `;` after it shifts right by one.
    let src = "library L {\n    function F() { s = \"😀\" ; x = ; }\n}";
    let body = serde_json::json!({ "text": src, "languageId": "zinc" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    assert_eq!(d.line0, 1);
    assert_eq!((d.char0, d.end_char0), (Some(34), Some(35)));
    assert_eq!(d.severity, "error");
}

#[test]
fn handle_update_ignores_unknown_languages() {
    let body = serde_json::json!({ "text": "garbage", "languageId": "cobol" }).to_string();
    assert!(handle_update(&body, &mut Projects::default()).diagnostics.is_empty());
}

#[test]
fn legend_lists_every_standard_type_and_modifier() {
    let legend = handle_legend();
    assert_eq!(legend.token_types.len(), 23);
    assert_eq!(legend.token_types[0], "namespace");
    assert_eq!(legend.token_types[22], "operator");
    assert_eq!(legend.token_modifiers.len(), 10);
    assert_eq!(legend.token_modifiers[9], "defaultLibrary");
}

#[test]
fn semantic_tokens_are_encoded_per_language() {
    let jass = serde_json::json!({ "text": "set x = 1", "languageId": "jass" }).to_string();
    assert_eq!(handle_semantic_tokens(&jass, &mut Projects::default()).data.len(), 4 * 5);

    let zinc = serde_json::json!({ "text": "x = 1;", "languageId": "zinc" }).to_string();
    // x (variable), = (operator), 1 (number); `;` isn't highlighted
    assert_eq!(handle_semantic_tokens(&zinc, &mut Projects::default()).data.len(), 3 * 5);

    let other = serde_json::json!({ "text": "x", "languageId": "cobol" }).to_string();
    assert!(handle_semantic_tokens(&other, &mut Projects::default()).data.is_empty());
}

#[test]
fn vjass_documents_hand_zinc_blocks_to_zinc() {
    let src = "// @dialect vjass\n//! zinc\nlibrary L {\n    function F() {\n        integer unused;\n    }\n}\n//! endzinc\nfunction G takes nothing returns nothing\nendfunction";
    let body = serde_json::json!({ "text": src, "languageId": "jass" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    assert_eq!(resp.diagnostics[0].line0, 4); // file line 5
    assert!(resp.diagnostics[0].message.contains("unused local variable 'unused'"));

    // and highlighting covers the block: `library` is a Zinc keyword token
    let tokens = handle_semantic_tokens(&body, &mut Projects::default()).data;
    assert!(tokens.chunks(5).any(|t| t[3] == parser_rs::SemanticType::Namespace.index()));
}

#[test]
fn jass_documents_are_plain_jass_unless_they_opt_in() {
    // No header: `library` is just a name, and `//! zinc` a comment.
    let src = "globals\n    integer library = 1\nendglobals";
    let body = serde_json::json!({ "text": src, "languageId": "jass" }).to_string();
    assert!(handle_update(&body, &mut Projects::default()).diagnostics.is_empty());

    // The same code in a vJASS file: `library` is a keyword now.
    let vjass = format!("// @dialect vjass\n{src}");
    let body = serde_json::json!({ "text": vjass, "languageId": "jass" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    assert_eq!(resp.diagnostics[0].severity, "error");
}

#[test]
fn lua_and_angelscript_documents_are_analyzed() {
    let lua = serde_json::json!({ "text": "local unused = 1\nbreak", "languageId": "lua" }).to_string();
    let resp = handle_update(&lua, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 2);
    assert_eq!(resp.diagnostics[1].severity, "error"); // break outside a loop
    assert!(!handle_semantic_tokens(&lua, &mut Projects::default()).data.is_empty());

    let angel = serde_json::json!({ "text": "int f() {\n}", "languageId": "angelscript" }).to_string();
    let resp = handle_update(&angel, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    assert!(resp.diagnostics[0].message.contains("not all paths"));

    let broken = serde_json::json!({ "text": "class {", "languageId": "angelscript" }).to_string();
    assert_eq!(handle_update(&broken, &mut Projects::default()).diagnostics[0].severity, "error");
}

#[test]
fn jass_documents_take_the_project_dialect_unless_their_header_says_otherwise() {
    let tmp = project::tests::TempDir::new("main-dialect");
    std::fs::write(tmp.0.join(project::MANIFEST), "[jass]\ndialect=vjass\n").unwrap();
    let path = tmp.0.join("src").join("x.j");
    let path = path.to_str().unwrap();
    let mut projects = Projects::default();

    // `library` is a keyword in the project's vJASS, so this fails to parse.
    let src = "globals\n    integer library = 1\nendglobals";
    let body = serde_json::json!({ "text": src, "languageId": "jass", "path": path }).to_string();
    let resp = handle_update(&body, &mut projects);
    assert_eq!(resp.diagnostics.len(), 1);
    assert_eq!(resp.diagnostics[0].severity, "error");

    // A header still wins over the project.
    let plain = format!("// @dialect jass\n{src}");
    let body = serde_json::json!({ "text": plain, "languageId": "jass", "path": path }).to_string();
    assert!(handle_update(&body, &mut projects).diagnostics.is_empty());

    // Without a path there's no project: plain JASS.
    let body = serde_json::json!({ "text": src, "languageId": "jass" }).to_string();
    assert!(handle_update(&body, &mut projects).diagnostics.is_empty());
}

#[test]
fn bni_documents_report_syntax_errors_and_lint() {
    let src = "[A]\nno equals sign\n[A]\n";
    let body = serde_json::json!({ "text": src, "languageId": "bni", "path": "/x/Data.txt" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    let got: Vec<_> = resp.diagnostics.iter().map(|d| (d.line0, d.severity)).collect();
    assert_eq!(got, vec![(1, "error"), (2, "warning")]);
    assert_eq!(resp.diagnostics[0].char0, Some(0));
    assert!(!handle_semantic_tokens(&body, &mut Projects::default()).data.is_empty());
}

#[test]
fn manifest_settings_are_checked_from_the_edited_text() {
    let src = "[jass]\ndialect=cjass\n";
    let body = serde_json::json!({ "text": src, "languageId": "bni", "path": "/nowhere/wc3.bni" }).to_string();
    let resp = handle_update(&body, &mut Projects::default());
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    assert_eq!((d.line0, d.char0, d.end_char0), (1, Some(8), Some(13)));
    assert_eq!(d.severity, "error");

    // Any other BNI file is just data: no settings to check.
    let body = serde_json::json!({ "text": src, "languageId": "bni", "path": "/x/Data.txt" }).to_string();
    assert!(handle_update(&body, &mut Projects::default()).diagnostics.is_empty());
}
