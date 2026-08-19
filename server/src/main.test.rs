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
