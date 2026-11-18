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
    let resp = handle_update(&body);
    assert!(resp.diagnostics.is_empty());
}

#[test]
fn handle_update_reports_lint_warning_without_a_column() {
    let src = "function Foo takes nothing returns nothing\n    local integer x = 1\nendfunction";
    let body = serde_json::json!({ "text": src }).to_string();
    let resp = handle_update(&body);
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
    let resp = handle_update(&body);
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
    let resp = handle_update("not json");
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
    let resp = handle_update(&body);
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
    let resp = handle_update(&body);
    assert_eq!(resp.diagnostics.len(), 1);
    let d = &resp.diagnostics[0];
    assert_eq!(d.line0, 1);
    assert_eq!((d.char0, d.end_char0), (Some(34), Some(35)));
    assert_eq!(d.severity, "error");
}

#[test]
fn handle_update_ignores_unknown_languages() {
    let body = serde_json::json!({ "text": "garbage", "languageId": "lua" }).to_string();
    assert!(handle_update(&body).diagnostics.is_empty());
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
    assert_eq!(handle_semantic_tokens(&jass).data.len(), 4 * 5);

    let zinc = serde_json::json!({ "text": "x = 1;", "languageId": "zinc" }).to_string();
    // x (variable), = (operator), 1 (number); `;` isn't highlighted
    assert_eq!(handle_semantic_tokens(&zinc).data.len(), 3 * 5);

    let other = serde_json::json!({ "text": "x", "languageId": "lua" }).to_string();
    assert!(handle_semantic_tokens(&other).data.is_empty());
}

#[test]
fn jass_documents_hand_zinc_blocks_to_zinc() {
    let src = "//! zinc\nlibrary L {\n    function F() {\n        integer unused;\n    }\n}\n//! endzinc\nfunction G takes nothing returns nothing\nendfunction";
    let body = serde_json::json!({ "text": src, "languageId": "jass" }).to_string();
    let resp = handle_update(&body);
    assert_eq!(resp.diagnostics.len(), 1);
    assert_eq!(resp.diagnostics[0].line0, 3); // file line 4
    assert!(resp.diagnostics[0].message.contains("unused local variable 'unused'"));

    // and highlighting covers the block: `library` is a Zinc keyword token
    let tokens = handle_semantic_tokens(&body).data;
    assert!(tokens.chunks(5).any(|t| t[3] == parser_rs::SemanticType::Namespace.index()));
}
