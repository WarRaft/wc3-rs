//! The map's gameplay constants (`war3mapMisc.txt`) and game interface
//! (`war3mapSkin.txt`), as World Editor's Advanced > Gameplay Constants
//! and Game Interface have them ([`Book`]): the fields of their metadata
//! (`Units\MiscMetaData.slk`, `UI\SkinMetaData.slk`: an id, its key and
//! section, its type and limits, its name a `WESTRING_`), each the game's
//! value unless the map's file changes it. The file is a BNI of the keys
//! changed alone (`[Misc]` `MaxHeroLevel=100`); a map without it uses the
//! game's (Use Custom ... off). Its text is kept as it is: a key set or
//! taken out on its line, the rest as it was ([`set`]).

use std::collections::HashMap;

use wc3_server::game::Game;

/// A constant: `id` (`mhel`), its `key` (`MaxHeroLevel`) in `section`
/// (`Misc`), its type (`bool`, `int`, `real`, `unreal`, `string`,
/// `intList`, `unrealList`, `unitList`, `defenseTable`, `attackTable`),
/// its `name` (World Editor's), its limits (empty: none).
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub id: String,
    pub key: String,
    pub section: String,
    pub ty: String,
    pub name: String,
    pub min: String,
    pub max: String,
}

/// Which of the two: the gameplay constants or the game interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Book {
    Misc,
    Skin,
}

pub const BOOKS: [Book; 2] = [Book::Misc, Book::Skin];

impl Book {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn of(n: u8) -> Option<Book> {
        BOOKS.get(n as usize).copied()
    }

    /// The map's file.
    pub fn file(self) -> &'static str {
        match self {
            Book::Misc => "war3mapMisc.txt",
            Book::Skin => "war3mapSkin.txt",
        }
    }

    /// The map's file's first section, when it's made.
    pub fn first(self) -> &'static str {
        match self {
            Book::Misc => "[Misc]\r\n",
            Book::Skin => "[FrameDef]\r\n",
        }
    }

    fn metadata(self) -> &'static str {
        match self {
            Book::Misc => "Units\\MiscMetaData.slk",
            Book::Skin => "UI\\SkinMetaData.slk",
        }
    }
}

/// The files the gameplay constants' game values come from, each over the
/// one before.
pub const GAME_FILES: [&str; 3] = ["Units\\MiscData.txt", "Units\\MiscGame.txt", "Units\\ItemFunc.txt"];

/// The game interface's: a section of a file (`CustomSkin` the race
/// skins' `[Default]`, then `[Human]` over it), or the frames' strings
/// (`FrameDef`: the `StringList`s of the frame files
/// `UI\FrameDef\FrameDef.toc` names).
const SKIN_FILES: [(&str, &str, &str); 4] =
    [("Units\\CommandStrings.txt", "Errors", "Errors"), ("Units\\UnitGlobalStrings.txt", "Categories", "Categories"), ("UI\\war3skins.txt", "Default", "CustomSkin"), ("UI\\war3skins.txt", "Human", "CustomSkin")];

/// The constants and the game's values, once read.
#[derive(Debug, Clone, Default)]
pub struct Constants {
    pub fields: Vec<Field>,
    /// By (section, key), both in lower case.
    pub game: HashMap<(String, String), String>,
}

impl Constants {
    /// `book`'s fields in the metadata's order, named by `name` (a
    /// `WESTRING_`'s text), and the game's values through `game`.
    pub fn load(game: &Game, book: Book, name: &dyn Fn(&str) -> String) -> Constants {
        let mut fields = Vec::new();
        if let Some((bytes, _)) = game.read(book.metadata()) {
            let table = slk_rs::Table::new(&slk_rs::parse(&String::from_utf8_lossy(&bytes)));
            let cols: HashMap<String, u32> = (1..=table.cols()).map(|c| (table.text(1, c).trim().to_ascii_lowercase(), c)).collect();
            let cell = |row: u32, col: &str| cols.get(col).map(|&c| table.text(row, c).trim().trim_matches('"').to_string()).unwrap_or_default();
            for row in 2..=table.rows() {
                let id = cell(row, "id");
                if id.is_empty() {
                    continue;
                }
                let shown = cell(row, "displayname");
                fields.push(Field {
                    key: cell(row, "field"),
                    section: cell(row, "section"),
                    ty: cell(row, "type"),
                    // (A text's named by the game's value, below.)
                    name: if shown.is_empty() { id.clone() } else { shown },
                    min: cell(row, "minval"),
                    max: cell(row, "maxval"),
                    id,
                });
            }
        }
        let mut values = HashMap::new();
        let text = |file: &str| game.read(file).map(|(b, _)| String::from_utf8_lossy(&b).into_owned());
        match book {
            Book::Misc => {
                for file in GAME_FILES {
                    for (section, key, value) in entries(&text(file).unwrap_or_default()) {
                        values.insert((section.to_ascii_lowercase(), key.to_ascii_lowercase()), value);
                    }
                }
            }
            Book::Skin => {
                // The frames' strings: every frame file the game's list
                // names (`GlobalStrings.fdf`, `InfoPanelStrings.fdf` ...).
                let toc = text("UI\\FrameDef\\FrameDef.toc").unwrap_or_default();
                for file in toc.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    for (key, value) in string_list(&text(file).unwrap_or_default()) {
                        values.insert(("framedef".to_string(), key.to_ascii_lowercase()), value);
                    }
                }
                for (file, from, to) in SKIN_FILES {
                    for (section, key, value) in entries(&text(file).unwrap_or_default()) {
                        if section.eq_ignore_ascii_case(from) {
                            values.insert((to.to_ascii_lowercase(), key.to_ascii_lowercase()), value);
                        }
                    }
                }
            }
        }
        // Named as World Editor names them: a `WESTRING_`'s text, or a
        // text field by its kind and the game's text (`Text - General -
        // 'Gold'`).
        for f in &mut fields {
            let pattern = match f.name.as_str() {
                "FrameDefDefault" => Some("WESTRING_SKINVAL_FRAMEDEF_DEFAULT"),
                "ErrorStringDefault" => Some("WESTRING_SKINVAL_ERROR_DEFAULT"),
                _ => None,
            };
            f.name = match pattern {
                Some(p) => {
                    let text = values.get(&(f.section.to_ascii_lowercase(), f.key.to_ascii_lowercase())).cloned().unwrap_or_else(|| f.key.clone());
                    let p = name(p);
                    if p.contains("%s") { p.replacen("%s", &text, 1) } else { format!("{p} {text}") }
                }
                None if f.name.starts_with("WESTRING_") => name(&f.name),
                None => f.name.clone(),
            };
        }
        Constants { fields, game: values }
    }

    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    /// The game's value of `f`.
    pub fn game_value(&self, f: &Field) -> String {
        self.game.get(&(f.section.to_ascii_lowercase(), f.key.to_ascii_lowercase())).cloned().unwrap_or_default()
    }
}

/// The strings of a frame definition file's `StringList`s (`KEY
/// "text",`), read by `fdf-rs`: each statement's name and first string.
pub fn string_list(text: &str) -> Vec<(String, String)> {
    let doc = fdf_rs::parse(text);
    doc.items
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case("StringList"))
        .flat_map(|s| s.children())
        .filter_map(|e| Some((e.name.clone(), e.strings().next()?.to_string())))
        .collect()
}

/// Each `key=value` of a BNI text: its section, key and value (trimmed;
/// a comment after `//` out, quotes off).
pub fn entries(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
        } else if !line.starts_with("//")
            && let Some((key, value)) = line.split_once('=')
        {
            let value = value.split_once("//").map_or(value, |(v, _)| v).trim().trim_matches('"');
            out.push((section.clone(), key.trim().to_string(), value.to_string()));
        }
    }
    out
}

/// The value `text` gives `key` in `section` (any case), if any.
pub fn get(text: &str, section: &str, key: &str) -> Option<String> {
    entries(text).into_iter().rev().find(|(s, k, _)| s.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key)).map(|(_, _, v)| v)
}

/// `text` with `key` in `section` set to `value` (on its line, or a line
/// added at the section's end, or a section added at the end) or, with
/// `None`, its lines taken out; the rest as it was, its line ends kept
/// (`\r\n` as World Editor writes them, for a text without any).
pub fn set(text: &str, section: &str, key: &str, value: Option<&str>) -> String {
    let eol = if text.contains("\r\n") || !text.contains('\n') { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = |l: &str| l.trim().strip_prefix('[').and_then(|l| l.strip_suffix(']')).map(|n| n.trim().to_string());
    let is_key = |l: &str| l.trim().split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key)) && !l.trim().starts_with("//");
    // Where the section is: its header and the line after its last.
    let mut start = None;
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate() {
        match (header(l), start) {
            (Some(n), None) if n.eq_ignore_ascii_case(section) => start = Some(i),
            (Some(_), Some(_)) => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    let line = value.map(|v| format!("{key}={v}"));
    match start {
        Some(s) => {
            let found: Vec<usize> = (s + 1..end).filter(|&i| is_key(&lines[i])).collect();
            match (found.first(), &line) {
                (Some(&first), Some(l)) => {
                    lines[first] = l.clone();
                    for &i in found[1..].iter().rev() {
                        lines.remove(i);
                    }
                }
                (Some(_), None) => {
                    for &i in found.iter().rev() {
                        lines.remove(i);
                    }
                }
                (None, Some(l)) => {
                    // After the section's last line that isn't blank.
                    let at = (s + 1..end).rev().find(|&i| !lines[i].trim().is_empty()).map_or(s + 1, |i| i + 1);
                    lines.insert(at, l.clone());
                }
                (None, None) => {}
            }
        }
        None => {
            if let Some(l) = line {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(format!("[{section}]"));
                lines.push(l);
            }
        }
    }
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

/// `value` checked as a value of `f` (its type, its limits), written as
/// the game's files write it: a number as typed, a list without spaces.
pub fn check(f: &Field, value: &str) -> Result<String, String> {
    let value = value.trim();
    let bad = |why: &str| Err(format!("{} ({}) can't be {value}: {why}", f.name, f.key));
    let limits = |n: f64| -> Result<(), String> {
        if let Ok(min) = f.min.parse::<f64>()
            && n < min
        {
            return Err(format!("{} ({}) can't be {value}: {} at least", f.name, f.key, f.min));
        }
        if let Ok(max) = f.max.parse::<f64>()
            && n > max
        {
            return Err(format!("{} ({}) can't be {value}: {} at most", f.name, f.key, f.max));
        }
        Ok(())
    };
    let list = || value.split(',').map(str::trim).collect::<Vec<_>>();
    match f.ty.as_str() {
        "bool" => match value {
            "0" | "1" => Ok(value.to_string()),
            _ => bad("0 or 1"),
        },
        "int" => match value.parse::<i64>() {
            Ok(n) => limits(n as f64).map(|_| n.to_string()),
            Err(_) => bad("a whole number"),
        },
        "real" | "unreal" => match value.parse::<f64>() {
            Ok(n) if f.ty == "unreal" && n < 0.0 => bad("not below 0"),
            Ok(n) => limits(n).map(|_| value.to_string()),
            Err(_) => bad("a number"),
        },
        "intList" => {
            let items = list();
            if items.iter().all(|i| i.parse::<i64>().is_ok()) { Ok(items.join(",")) } else { bad("whole numbers, a comma between") }
        }
        "unrealList" | "defenseTable" | "attackTable" => {
            let items = list();
            if items.iter().all(|i| i.parse::<f64>().is_ok_and(|n| n >= 0.0)) { Ok(items.join(",")) } else { bad("numbers not below 0, a comma between") }
        }
        "unitList" => {
            let items: Vec<&str> = list().into_iter().filter(|i| !i.is_empty()).collect();
            if items.iter().all(|i| i.len() == 4 && i.is_ascii()) { Ok(items.join(",")) } else { bad("ids of four letters, a comma between") }
        }
        _ => Ok(value.to_string()),
    }
}

/// Whether two values of `f` are the same: as numbers when they're
/// numbers (`1` and `1.0`), item by item in a list.
pub fn same(a: &str, b: &str) -> bool {
    let items = |s: &str| s.split(',').map(|i| i.trim().to_string()).collect::<Vec<_>>();
    let (a, b) = (items(a), items(b));
    a.len() == b.len()
        && a.iter().zip(&b).all(|(x, y)| match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(m), Ok(n)) => (m - n).abs() < 1e-6,
            _ => x.eq_ignore_ascii_case(y),
        })
}

#[cfg(test)]
#[path = "constants.test.rs"]
mod tests;
