//! Local HTTP backend for the WC3 VS Code extension.
//!
//! Follows the same transport contract as the older JASS-Tree-sitter-Rust
//! extension's Rust backend (spawned as a subprocess, prints
//! `{"port":N,"token":"..."}` to stdout on startup, serves plain HTTP on
//! `127.0.0.1:<port>` guarded by `?token=`, and exits when stdin closes —
//! i.e. when the extension process that spawned us dies). The client side
//! (`serverClient.js`/`httpTransport.js`) is reused verbatim from that
//! project, since none of it is JASS-specific.
//!
//! Unlike that project this uses `tiny_http` (blocking, ~3 small
//! dependencies) instead of `axum`/`tokio`, and all language logic comes
//! from `jass-rs` / `zinc-rs` (both built on the shared `parser-rs`) rather
//! than a ported copy — this binary is just the glue that turns their
//! `parse`/`lint` into HTTP JSON, picking the language by the document's
//! `languageId`.

use parser_rs::{ColumnEncoding, Diagnostic, ParseError};

/// The dialect `.j` documents are read in.
const VJASS: jass_rs::Dialect = jass_rs::Dialect::VJass;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tiny_http::{Header, Method, Response, Server};

fn main() {
    let server = Server::http("127.0.0.1:0").expect("failed to bind 127.0.0.1:0");
    // `to_ip()` is portable (unlike matching `ListenAddr` directly — its
    // `Unix` variant only exists on Unix targets, so an exhaustive match
    // fails to compile on Windows).
    let port = server
        .server_addr()
        .to_ip()
        .expect("bound to an IP address (127.0.0.1), never a unix socket")
        .port();
    let token = generate_token();

    // The extension reads this exact line from our stdout to learn where
    // to connect. Must be flushed immediately — it's the handshake.
    println!("{{\"port\":{port},\"token\":\"{token}\"}}");
    std::io::stdout().flush().ok();

    spawn_stdin_watcher();

    for request in server.incoming_requests() {
        handle_request(request, &token);
    }
}

/// Exits the process once stdin closes (EOF) — that happens when the
/// parent extension process dies, so we never outlive the editor window
/// that spawned us.
fn spawn_stdin_watcher() {
    std::thread::spawn(|| {
        let mut buf = [0u8; 64];
        loop {
            match std::io::stdin().read(&mut buf) {
                Ok(0) | Err(_) => std::process::exit(0),
                Ok(_) => {}
            }
        }
    });
}

/// A localhost-only session token: unpredictable enough that another local
/// process or a webpage can't guess it and hit our port, but — like the
/// older extension's equivalent — deliberately not a CSPRNG; that's more
/// than this threat model needs.
fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let pid = std::process::id() as u64;
    let stack_addr = &nanos as *const _ as u64;

    let mut state = nanos ^ pid.rotate_left(21) ^ stack_addr.rotate_left(43);
    let mut token = String::with_capacity(32);
    for _ in 0..2 {
        state = splitmix64(state);
        token.push_str(&format!("{state:016x}"));
    }
    token
}

/// SplitMix64: a fast, well-known, non-cryptographic bit mixer. Good enough
/// to turn a handful of loosely-unpredictable inputs (timestamp, pid, a
/// stack address affected by ASLR) into a token that doesn't reveal them.
fn splitmix64(x: u64) -> u64 {
    let x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn handle_request(mut request: tiny_http::Request, token: &str) {
    if !token_ok(request.url(), token) {
        respond(
            request,
            Response::from_string("forbidden").with_status_code(403),
        );
        return;
    }

    let path = request.url().split('?').next().unwrap_or("").to_string();

    if request.method() != &Method::Post {
        respond(request, Response::from_string("not found").with_status_code(404));
        return;
    }
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        respond(
            request,
            Response::from_string("invalid body").with_status_code(400),
        );
        return;
    }

    let json = match path.as_str() {
        "/document/update" => serde_json::to_string(&handle_update(&body)),
        "/document/semanticTokens" => serde_json::to_string(&handle_semantic_tokens(&body)),
        "/semantic/legend" => serde_json::to_string(&handle_legend()),
        _ => {
            respond(request, Response::from_string("not found").with_status_code(404));
            return;
        }
    };
    match json {
        Ok(json) => respond(request, json_response(json)),
        Err(_) => respond(
            request,
            Response::from_string("serialization failed").with_status_code(500),
        ),
    }
}

fn respond(request: tiny_http::Request, response: Response<std::io::Cursor<Vec<u8>>>) {
    let _ = request.respond(response);
}

fn json_response(body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header is always valid");
    Response::from_string(body).with_header(header)
}

fn token_ok(url: &str, token: &str) -> bool {
    let query = match url.split_once('?') {
        Some((_, q)) => q,
        None => return false,
    };
    query
        .split('&')
        .any(|pair| pair.split_once('=') == Some(("token", token)))
}

#[derive(Deserialize)]
struct UpdateRequest {
    text: String,
    /// The VS Code language id of the document (`"jass"`, `"zinc"`).
    /// Missing means `"jass"`, for clients predating Zinc support.
    #[serde(rename = "languageId", default = "default_language")]
    language_id: String,
}

fn default_language() -> String {
    "jass".to_string()
}

#[derive(Serialize)]
struct DiagnosticOut {
    /// 0-indexed, matching `vscode.Position` directly — the extension does
    /// no further index math.
    line0: usize,
    /// 0-indexed, UTF-16-counted (see `parser_rs::ColumnEncoding::Utf16`) —
    /// also a direct `vscode.Position` value. `null` when only a whole-line
    /// location is known (lint diagnostics don't carry a column).
    char0: Option<usize>,
    #[serde(rename = "endChar0")]
    end_char0: Option<usize>,
    severity: &'static str,
    message: String,
}

#[derive(Serialize)]
struct UpdateResponse {
    diagnostics: Vec<DiagnosticOut>,
}

/// Parses and lints `text` as `language`. Every language crate reports
/// through the same `parser-rs` types, so everything past this dispatch is
/// language-agnostic. `None` for a language this server doesn't know.
fn analyze(language: &str, text: &str) -> Option<Result<Vec<Diagnostic>, ParseError>> {
    Some(match language {
        // `.j` files are read as vJASS, so `//! zinc` blocks are handed to
        // zinc-rs (lines stay file-absolute; Zinc lint is included).
        "jass" => jass_rs::parse_with(text, VJASS).map(|p| jass_rs::lint(&p)),
        "zinc" => zinc_rs::parse(text).map(|p| zinc_rs::lint(&p)),
        _ => return None,
    })
}

fn handle_update(body: &str) -> UpdateResponse {
    let diagnostics = serde_json::from_str::<UpdateRequest>(body)
        .ok()
        .and_then(|req| {
            let result = analyze(&req.language_id, &req.text)?;
            Some(to_diagnostics_out(&req.text, result))
        })
        .unwrap_or_default();
    UpdateResponse { diagnostics }
}

fn to_diagnostics_out(
    text: &str,
    result: Result<Vec<Diagnostic>, ParseError>,
) -> Vec<DiagnosticOut> {
    match result {
        Err(err) => {
            let (char0, end_char0) = utf16_char_range(text, err.line, err.col, err.end_col);
            vec![DiagnosticOut {
                line0: err.line.saturating_sub(1),
                char0: Some(char0),
                end_char0: Some(end_char0),
                severity: "error",
                message: err.message,
            }]
        }
        Ok(diagnostics) => diagnostics
            .into_iter()
            .map(|d| DiagnosticOut {
                line0: d.line.saturating_sub(1),
                char0: None,
                end_char0: None,
                severity: d.level.as_str(),
                message: d.message,
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct LegendResponse {
    #[serde(rename = "tokenTypes")]
    token_types: Vec<&'static str>,
    #[serde(rename = "tokenModifiers")]
    token_modifiers: Vec<&'static str>,
}

/// VS Code's standard semantic token types and modifiers, in the order
/// the encoded token data indexes them. The extension registers exactly
/// this, so the legend lives in one place (`parser_rs::semantic`).
fn handle_legend() -> LegendResponse {
    let (token_types, token_modifiers) = parser_rs::semantic::legend();
    LegendResponse {
        token_types,
        token_modifiers,
    }
}

#[derive(Serialize)]
struct SemanticTokensResponse {
    /// `deltaLine, deltaStartChar, length, tokenType, tokenModifiers` per
    /// token — ready for `new vscode.SemanticTokens(Uint32Array.from(data))`.
    data: Vec<u32>,
}

fn handle_semantic_tokens(body: &str) -> SemanticTokensResponse {
    let data = serde_json::from_str::<UpdateRequest>(body)
        .ok()
        .and_then(|req| match req.language_id.as_str() {
            "jass" => Some(jass_rs::encoded_semantic_tokens(&req.text, VJASS)),
            "zinc" => Some(zinc_rs::encoded_semantic_tokens(&req.text)),
            _ => None,
        })
        .unwrap_or_default();
    SemanticTokensResponse { data }
}

/// Converts a `(line, start_col, end_col)` — 1-indexed, codepoint counted,
/// as every `parser-rs`-based parser reports — into 0-indexed UTF-16
/// columns for `vscode.Position`.
fn utf16_char_range(source: &str, line: usize, start_col: usize, end_col: usize) -> (usize, usize) {
    let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let start = ColumnEncoding::Utf16.from_codepoints(line_text, start_col);
    let end = ColumnEncoding::Utf16.from_codepoints(line_text, end_col);
    (start.saturating_sub(1), end.saturating_sub(1))
}

#[cfg(test)]
#[path = "main.test.rs"]
mod tests;
