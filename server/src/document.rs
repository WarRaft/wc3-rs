//! An open document: the editor's current text of a file, kept in sync
//! through incremental edits, and the position math between the editor's
//! UTF-16 `(line, character)` and the parsers' 1-based codepoint columns.

use std::path::{Path, PathBuf};

use parser_rs::ColumnEncoding;

/// An editor position: 0-based line and 0-based UTF-16 character, as
/// VS Code counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Position {
        Position { line, character }
    }
}

/// An editor range, end-exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Range {
        Range { start, end }
    }
}

/// A language this server analyzes, from the editor's `languageId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Jass,
    Zinc,
    Lua,
    AngelScript,
    Bni,
}

impl Language {
    pub fn from_id(id: &str) -> Option<Language> {
        Some(match id {
            "jass" => Language::Jass,
            "zinc" => Language::Zinc,
            "lua" => Language::Lua,
            "angelscript" => Language::AngelScript,
            "bni" => Language::Bni,
            _ => return None,
        })
    }

    /// The editor's id for the language (also a Markdown code-fence tag).
    pub fn id(self) -> &'static str {
        match self {
            Language::Jass => "jass",
            Language::Zinc => "zinc",
            Language::Lua => "lua",
            Language::AngelScript => "angelscript",
            Language::Bni => "bni",
        }
    }

    /// The language of a file on disk, by its extension, the same mapping
    /// as the extension's `package.json` (BNI only as `.bni`: `.txt` is
    /// too common to claim).
    pub fn from_path(path: &Path) -> Option<Language> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "j" | "pld" | "ai" => Language::Jass,
            "zn" => Language::Zinc,
            "lua" => Language::Lua,
            "as" | "angelscript" => Language::AngelScript,
            "bni" => Language::Bni,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub language: Language,
    pub version: i32,
    pub text: String,
    /// The file on disk, used to find the document's project. `None` for
    /// an unsaved or non-file document.
    pub path: Option<PathBuf>,
}

impl Document {
    /// Replaces `range` with `new_text`.
    pub fn apply(&mut self, range: Range, new_text: &str) {
        let lines = LineIndex::new(&self.text);
        let start = lines.offset(&self.text, range.start);
        let end = lines.offset(&self.text, range.end).max(start);
        self.text.replace_range(start..end, new_text);
    }
}

/// The byte offset where each line starts, so a position converts without
/// rescanning the file.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { starts }
    }

    /// The text of 0-based `line`, without its line break. Empty past the
    /// end.
    pub fn line<'a>(&self, text: &'a str, line: usize) -> &'a str {
        let Some(&start) = self.starts.get(line) else {
            return "";
        };
        let end = self.starts.get(line + 1).map_or(text.len(), |&next| next - 1);
        text[start..end].strip_suffix('\r').unwrap_or(&text[start..end])
    }

    /// The byte offset of an editor position. A character past the line's
    /// end is clamped to it, and a line past the end to the end of text.
    pub fn offset(&self, text: &str, pos: Position) -> usize {
        let Some(&start) = self.starts.get(pos.line as usize) else {
            return text.len();
        };
        let line = self.line(text, pos.line as usize);
        let mut units = 0;
        for (i, c) in line.char_indices() {
            if units >= pos.character as usize {
                return start + i;
            }
            units += c.len_utf16();
        }
        start + line.len()
    }

    /// The 1-based codepoint column of an editor position (the parsers'
    /// convention), clamped to the line's end.
    pub fn column(&self, text: &str, pos: Position) -> usize {
        let line = self.line(text, pos.line as usize);
        let mut units = 0;
        let mut col = 1;
        for c in line.chars() {
            if units >= pos.character as usize {
                break;
            }
            units += c.len_utf16();
            col += 1;
        }
        col
    }

    /// The editor range of 1-based `line`, codepoint columns
    /// `col..end_col` (end-exclusive), as every parser reports them.
    pub fn range(&self, text: &str, line: usize, col: usize, end_col: usize) -> Range {
        let line0 = line.saturating_sub(1);
        let line_text = self.line(text, line0);
        let utf16 = |c: usize| ColumnEncoding::Utf16.from_codepoints(line_text, c).saturating_sub(1) as u32;
        Range::new(
            Position::new(line0 as u32, utf16(col)),
            Position::new(line0 as u32, utf16(end_col)),
        )
    }

    /// The editor range covering 1-based `line` as a whole.
    pub fn whole_line(&self, text: &str, line: usize) -> Range {
        let line0 = line.saturating_sub(1);
        let len = self.line(text, line0).encode_utf16().count() as u32;
        Range::new(Position::new(line0 as u32, 0), Position::new(line0 as u32, len))
    }
}

#[cfg(test)]
#[path = "document.test.rs"]
mod tests;
