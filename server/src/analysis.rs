//! What the server knows about a document: its diagnostics (parse errors,
//! lint, project-manifest checks), its semantic tokens and its symbols. Every language
//! crate reports through the same `parser-rs` types, so everything past
//! the language dispatch is language-agnostic.

use std::path::Path;

use parser_rs::{Level, Occurrence, ParseError};

use crate::document::{Document, Language, LineIndex, Range};
use crate::project::{self, Project};

/// A diagnostic as the editor shows it: an exact range, a severity, a
/// message, and for lint the id of its rule (empty for parse errors and
/// manifest checks), which is what `[lint]` in `wc3.bni` refers to.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub range: Range,
    pub level: Level,
    pub message: String,
    pub rule: &'static str,
}

/// The dialect a JASS document is read in: whatever its own header
/// declares (`// @dialect vjass`), else vJASS for a `.vj` file, else its
/// project's default (`[jass] dialect=` in `wc3.bni`), else plain JASS.
/// The header wins, so a project can still mix dialects.
pub fn jass_dialect(doc: &Document, project: Option<&Project>) -> jass_rs::Dialect {
    let vj = doc
        .path
        .as_deref()
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("vj"));
    jass_rs::Dialect::declared(&doc.text)
        .or(vj.then_some(jass_rs::Dialect::VJass))
        .or_else(|| project.and_then(|p| p.jass_dialect))
        .unwrap_or_default()
}

/// Parse errors and lint warnings of `doc`, read with the settings of its
/// `project` (its dialect, its lint rules; the default rules without one),
/// sorted by line. Pure, so documents can be analyzed in parallel.
pub fn diagnostics(doc: &Document, project: Option<&Project>) -> Vec<Diagnostic> {
    let text = doc.text.as_str();
    let lines = LineIndex::new(text);
    let out = Out { text, lines: &lines };
    let result = match doc.language {
        // In a vJASS file, `//! zinc` blocks are handed to zinc-rs (lines
        // stay file-absolute; Zinc lint is included).
        Language::Jass => {
            jass_rs::parse_with(text, jass_dialect(doc, project)).map(|p| jass_rs::lint(&p))
        }
        Language::Zinc => zinc_rs::parse(text).map(|p| zinc_rs::lint(&p)),
        Language::Lua => lua_rs::parse(text).map(|c| lua_rs::lint(&c)),
        Language::AngelScript => as_rs::parse(text).map(|s| as_rs::lint(&s)),
        Language::Bni => return bni(doc, project, &out),
    };
    match result {
        Err(err) => vec![out.error(&err)],
        Ok(lint) => project::apply_lint(lint, project).into_iter().map(|d| out.lint(d)).collect(),
    }
}

/// BNI never fails as a whole: every malformed line is its own error, and
/// lint still runs. A project manifest (`wc3.bni`) also gets its settings
/// checked, against the text being edited rather than the saved file.
fn bni(doc: &Document, project: Option<&Project>, out: &Out) -> Vec<Diagnostic> {
    let parsed = bni_rs::parse(out.text);
    let mut diags: Vec<Diagnostic> = parsed.errors.iter().map(|e| out.error(e)).collect();
    let lint = project::apply_lint(bni_rs::lint(&parsed), project);
    diags.extend(lint.into_iter().map(|d| out.lint(d)));
    if let Some(path) = doc.path.as_deref().filter(|p| project::is_manifest(p)) {
        let root = path.parent().unwrap_or(Path::new(""));
        let (_, issues) = project::load(root, out.text);
        diags.extend(issues.into_iter().map(|i| {
            out.make(out.lines.range(out.text, i.line, i.col, i.end_col), i.level, i.message, "")
        }));
    }
    diags.sort_by_key(|d| d.range.start.line);
    diags
}

/// Builds editor diagnostics for one text.
struct Out<'a> {
    text: &'a str,
    lines: &'a LineIndex,
}

impl Out<'_> {
    fn error(&self, err: &ParseError) -> Diagnostic {
        let range = self.lines.range(self.text, err.line, err.col, err.end_col);
        self.make(range, Level::Error, err.message.clone(), "")
    }

    /// Lint diagnostics carry a line, not columns: they cover it whole.
    fn lint(&self, d: parser_rs::Diagnostic) -> Diagnostic {
        self.make(self.lines.whole_line(self.text, d.line), d.level, d.message, d.rule)
    }

    fn make(&self, range: Range, level: Level, message: String, rule: &'static str) -> Diagnostic {
        Diagnostic { range, level, message, rule }
    }
}

/// Everything known about one document.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    /// Empty unless asked for: highlighting is for open documents only.
    pub tokens: Vec<u32>,
    /// Every name declared or referenced, for the symbol index. Every
    /// language but BNI (a data format) produces them.
    pub symbols: Vec<Occurrence>,
}

/// Analyzes `doc` with the settings of its `project`. Pure, so documents
/// can be analyzed in parallel. Tokens and symbols come from one
/// classification pass where the language supports it.
pub fn analyze(doc: &Document, project: Option<&Project>, highlight: bool) -> Analysis {
    let diagnostics = diagnostics(doc, project);
    let text = doc.text.as_str();
    let (tokens, symbols) = match doc.language {
        Language::Jass => {
            let dialect = jass_dialect(doc, project);
            if highlight {
                let (tokens, symbols) = jass_rs::semantic_tokens_and_symbols(text, dialect);
                (parser_rs::semantic::encode(tokens, text), symbols)
            } else {
                (Vec::new(), jass_rs::symbols(text, dialect))
            }
        }
        Language::Zinc => {
            let (tokens, symbols, _) = zinc_rs::semantic_tokens_and_symbols_at(text, 1, 1);
            let tokens = if highlight { parser_rs::semantic::encode(tokens, text) } else { Vec::new() };
            (tokens, symbols)
        }
        Language::Lua => {
            let (tokens, symbols) = lua_rs::semantic_tokens_and_symbols(text);
            let tokens = if highlight { parser_rs::semantic::encode(tokens, text) } else { Vec::new() };
            (tokens, symbols)
        }
        Language::AngelScript => {
            let (tokens, symbols) = as_rs::semantic_tokens_and_symbols(text);
            let tokens = if highlight { parser_rs::semantic::encode(tokens, text) } else { Vec::new() };
            (tokens, symbols)
        }
        _ if highlight => (semantic_tokens(doc, project), Vec::new()),
        _ => (Vec::new(), Vec::new()),
    };
    Analysis { diagnostics, tokens, symbols }
}

/// `deltaLine, deltaStartChar, length, tokenType, tokenModifiers` per
/// token, indexed by `parser_rs::semantic::legend`.
pub fn semantic_tokens(doc: &Document, project: Option<&Project>) -> Vec<u32> {
    let text = doc.text.as_str();
    match doc.language {
        Language::Jass => jass_rs::encoded_semantic_tokens(text, jass_dialect(doc, project)),
        Language::Zinc => zinc_rs::encoded_semantic_tokens(text),
        Language::Lua => lua_rs::encoded_semantic_tokens(text),
        Language::AngelScript => as_rs::encoded_semantic_tokens(text),
        Language::Bni => bni_rs::encoded_semantic_tokens(text),
    }
}

#[cfg(test)]
#[path = "analysis.test.rs"]
mod tests;
