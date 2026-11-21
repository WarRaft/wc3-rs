//! What the server knows about a document: its diagnostics (parse errors,
//! lint, project-manifest checks), its semantic tokens and its symbols. Every language
//! crate reports through the same `parser-rs` types, so everything past
//! the language dispatch is language-agnostic.

use std::path::Path;

use parser_rs::{Level, Occurrence, ParseError};

use crate::document::{Document, Language, LineIndex, Range};
use crate::project::{self, Project};
use w3abdhqtu_rs::Metadata;

/// A diagnostic as the editor shows it: an exact range, a severity, a
/// message, for lint the id of its rule (empty for parse errors and
/// manifest checks), which is what `[lint]` in `wc3.bni` refers to, and
/// the quick fix the rule offers, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub range: Range,
    pub level: Level,
    pub message: String,
    pub rule: &'static str,
    pub fix: Option<Fix>,
}

/// A quick fix in editor coordinates: a title and edits (replace the range
/// with the text).
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub title: String,
    pub edits: Vec<(Range, String)>,
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
#[cfg(test)]
pub fn diagnostics(doc: &Document, project: Option<&Project>) -> Vec<Diagnostic> {
    diagnostics_with(doc, project, None)
}

/// `diagnostics`, with `objects`, the game's metadata when `doc` is the
/// text form of object data, to check its fields against.
pub fn diagnostics_with(doc: &Document, project: Option<&Project>, objects: Option<&Metadata>) -> Vec<Diagnostic> {
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
        Language::Bni => return bni(doc, project, objects, &out),
        Language::Slk => return slk(project, &out),
        Language::Wts => return wts(project, &out),
    };
    let mut diags: Vec<Diagnostic> = match result {
        Err(err) => vec![out.error(&err)],
        Ok(lint) => project::apply_lint(lint, project).into_iter().map(|d| out.lint(d)).collect(),
    };
    // A map's custom script (`.wct`) in its text form: markers that
    // wouldn't save.
    let markers = crate::customtext::diagnostics(doc.path.as_deref(), text);
    diags.extend(markers.iter().map(|e| out.error(e)));
    diags
}

/// BNI never fails as a whole: every malformed line is its own error, and
/// lint still runs. The text form of object data, cameras, regions, sounds, terrain or map info also gets the
/// errors that would stop it from saving. A project manifest (`wc3.bni`) also gets its settings
/// checked, against the text being edited rather than the saved file.
fn bni(doc: &Document, project: Option<&Project>, objects: Option<&Metadata>, out: &Out) -> Vec<Diagnostic> {
    let parsed = bni_rs::parse(out.text);
    let mut diags: Vec<Diagnostic> = parsed.errors.iter().map(|e| out.error(e)).collect();
    // The text form of object data (`.w3a` ...): what it can't save, and
    // what the game's metadata says is wrong with its fields.
    let (errors, mut lint) = crate::objects::diagnostics(&parsed, objects);
    diags.extend(errors.iter().map(|e| out.error(e)));
    // The text form of cameras (`.w3c`): what it can't save.
    let cameras = crate::cameras::diagnostics(doc.path.as_deref(), &parsed);
    diags.extend(cameras.iter().map(|e| out.error(e)));
    // The terrain's header (`.w3e`): what it can't save.
    let terrain = crate::terrain::diagnostics(doc.path.as_deref(), &parsed);
    diags.extend(terrain.iter().map(|e| out.error(e)));
    // Map info (`.w3i`): what it can't save.
    let info = crate::mapinfo::diagnostics(doc.path.as_deref(), &parsed);
    diags.extend(info.iter().map(|e| out.error(e)));
    // Regions (`.w3r`): what they can't save.
    let regions = crate::regions::diagnostics(doc.path.as_deref(), &parsed);
    diags.extend(regions.iter().map(|e| out.error(e)));
    // Sounds (`.w3s`): what they can't save.
    let sounds = crate::sounds::diagnostics(doc.path.as_deref(), &parsed);
    diags.extend(sounds.iter().map(|e| out.error(e)));
    lint.extend(bni_rs::lint(&parsed));
    let lint = project::apply_lint(lint, project);
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

/// SYLK never fails as a whole either: its parse errors, then its lint.
fn slk(project: Option<&Project>, out: &Out) -> Vec<Diagnostic> {
    let parsed = slk_rs::parse(out.text);
    let mut diags: Vec<Diagnostic> = parsed.errors.iter().map(|e| out.error(e)).collect();
    let lint = project::apply_lint(slk_rs::lint(&parsed), project);
    diags.extend(lint.into_iter().map(|d| out.lint(d)));
    diags.sort_by_key(|d| d.range.start.line);
    diags
}

/// A string table never fails as a whole either: its parse errors, then
/// its lint.
fn wts(project: Option<&Project>, out: &Out) -> Vec<Diagnostic> {
    let parsed = wts_rs::parse(out.text);
    let mut diags: Vec<Diagnostic> = parsed.errors.iter().map(|e| out.error(e)).collect();
    let lint = project::apply_lint(wts_rs::lint(&parsed), project);
    diags.extend(lint.into_iter().map(|d| out.lint(d)));
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

    /// A lint diagnostic covers its span, or its whole line when the rule
    /// only knows the line.
    fn lint(&self, d: parser_rs::Diagnostic) -> Diagnostic {
        let range = match d.span {
            Some(span) => self.lines.span(self.text, span),
            None => self.lines.whole_line(self.text, d.line),
        };
        let fix = d.fix.map(|f| Fix {
            title: f.title,
            edits: f.edits.into_iter().map(|e| (self.lines.span(self.text, e.span), e.text)).collect(),
        });
        Diagnostic { fix, ..self.make(range, d.level, d.message, d.rule) }
    }

    fn make(&self, range: Range, level: Level, message: String, rule: &'static str) -> Diagnostic {
        Diagnostic { range, level, message, rule, fix: None }
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
pub fn analyze(doc: &Document, project: Option<&Project>, highlight: bool, objects: Option<&Metadata>) -> Analysis {
    let diagnostics = diagnostics_with(doc, project, objects);
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
        Language::Slk => slk_rs::encoded_semantic_tokens(text),
        Language::Wts => wts_rs::encoded_semantic_tokens(text),
    }
}

#[cfg(test)]
#[path = "analysis.test.rs"]
mod tests;
