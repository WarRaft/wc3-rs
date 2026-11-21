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
    /// SYLK tables (`.slk`), via `slk-rs`.
    Slk,
    /// Map string tables (`war3map.wts`), via `wts-rs`.
    Wts,
}

impl Language {
    pub fn from_id(id: &str) -> Option<Language> {
        Some(match id {
            // `.vj` files: JASS read as vJASS (see `analysis::jass_dialect`).
            "jass" | "vjass" => Language::Jass,
            "zinc" => Language::Zinc,
            "lua" => Language::Lua,
            "angelscript" => Language::AngelScript,
            "bni" => Language::Bni,
            "slk" => Language::Slk,
            "wts" => Language::Wts,
            // Object data, which the editor shows in its BNI text form
            // (see `objects`).
            "w3a" | "w3b" | "w3d" | "w3h" | "w3q" | "w3t" | "w3u" => Language::Bni,
            // Cameras, in their BNI text form too (see `cameras`).
            "w3c" | "w3e" => Language::Bni,
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
            Language::Slk => "slk",
            Language::Wts => "wts",
        }
    }

    /// The language of a file on disk, by its extension, the same mapping
    /// as the extension's `package.json`. BNI is `.bni`, and of `.txt`
    /// files, too common to claim, only the game's BNI ones by name
    /// ([`BNI_FILES`]) and a map's `war3map*.txt` (`war3mapSkin.txt` ...).
    pub fn from_path(path: &Path) -> Option<Language> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if BNI_FILES.contains(&name.as_str()) || (name.starts_with("war3map") && name.ends_with(".txt")) {
            return Some(Language::Bni);
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "j" | "vj" | "pld" | "ai" => Language::Jass,
            "zn" => Language::Zinc,
            "lua" => Language::Lua,
            "as" | "angelscript" => Language::AngelScript,
            "bni" => Language::Bni,
            "slk" => Language::Slk,
            "wts" => Language::Wts,
            // Not object data (`.w3a` ...): the files are binary. Only
            // their text form, which the editor opens under its own
            // scheme with the file's language id, is BNI (`from_id`).
            _ => return None,
        })
    }
}

/// The game's `.txt` files that are BNI, lowercase: `Units\\*Func.txt`,
/// `Units\\*Strings.txt`, `UI\\WorldEditStrings.txt`, `UI\\TriggerData.txt`
/// ... (checked: each parses without errors, but `ItemStrings.txt`'s one
/// line without a key). Not the ones that aren't BNI: `HelpStrings.txt`,
/// `StartupStrings.txt`, `UI\\Captions\\*`, the chat icon lists, the
/// licenses. Not `config.txt` either, a name too common to claim. The
/// same list as the extension's `package.json` (`filenames` of `bni`).
pub const BNI_FILES: &[&str] = &[
    "aieditordata.txt",
    "campaignabilityfunc.txt",
    "campaignabilitystrings.txt",
    "campaignstrings.txt",
    "campaignstrings_exp.txt",
    "campaignunitfunc.txt",
    "campaignunitstrings.txt",
    "campaignupgradefunc.txt",
    "campaignupgradestrings.txt",
    "commandfunc.txt",
    "commandstrings.txt",
    "commonabilityfunc.txt",
    "commonabilitystrings.txt",
    "humanabilityfunc.txt",
    "humanabilitystrings.txt",
    "humanunitfunc.txt",
    "humanunitstrings.txt",
    "humanupgradefunc.txt",
    "humanupgradestrings.txt",
    "itemabilityfunc.txt",
    "itemabilitystrings.txt",
    "itemfunc.txt",
    "itemstrings.txt",
    "macstrings.txt",
    "macworldeditstrings.txt",
    "miscdata.txt",
    "miscgame.txt",
    "miscui.txt",
    "neutralabilityfunc.txt",
    "neutralabilitystrings.txt",
    "neutralunitfunc.txt",
    "neutralunitstrings.txt",
    "neutralupgradefunc.txt",
    "neutralupgradestrings.txt",
    "nightelfabilityfunc.txt",
    "nightelfabilitystrings.txt",
    "nightelfunitfunc.txt",
    "nightelfunitstrings.txt",
    "nightelfupgradefunc.txt",
    "nightelfupgradestrings.txt",
    "orcabilityfunc.txt",
    "orcabilitystrings.txt",
    "orcunitfunc.txt",
    "orcunitstrings.txt",
    "orcupgradefunc.txt",
    "orcupgradestrings.txt",
    "telemetry.txt",
    "tipstrings.txt",
    "triggerdata.txt",
    "triggerstrings.txt",
    "undeadabilityfunc.txt",
    "undeadabilitystrings.txt",
    "undeadunitfunc.txt",
    "undeadunitstrings.txt",
    "undeadupgradefunc.txt",
    "undeadupgradestrings.txt",
    "uniteditordata.txt",
    "unitglobalstrings.txt",
    "war3skins.txt",
    "worldeditdata.txt",
    "worldeditgamestrings.txt",
    "worldeditlayout.txt",
    "worldeditstrings.txt",
];

/// A file's text as the server keeps it: UTF-8, invalid bytes replaced,
/// without a leading byte-order mark. The editor holds the text without
/// it too, so positions agree; `war3map.wts` and other World Editor files
/// start with one.
pub fn decode(bytes: Vec<u8>) -> String {
    let text = String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    match text.strip_prefix('\u{FEFF}') {
        Some(rest) => rest.to_string(),
        None => text,
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

    /// The editor range of a parser span, which may cross lines (1-based
    /// lines, codepoint columns; column 1 of a line is its start).
    pub fn span(&self, text: &str, span: parser_rs::Span) -> Range {
        let pos = |line: usize, col: usize| {
            let line0 = line.saturating_sub(1);
            let line_text = self.line(text, line0);
            Position::new(line0 as u32, ColumnEncoding::Utf16.from_codepoints(line_text, col).saturating_sub(1) as u32)
        };
        Range::new(pos(span.start_line, span.start_col), pos(span.end_line, span.end_col))
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
