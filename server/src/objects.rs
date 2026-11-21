//! Object data (`.w3a`, `.w3b`, `.w3d`, `.w3h`, `.w3q`, `.w3t`, `.w3u`)
//! for the editor, which shows it as text: its BNI text form
//! (`w3abdhqtu-rs`), and back to the file's bytes when that's saved. While
//! the text is edited, it's a BNI document like any other, and
//! `analysis` reports its errors.
//!
//! The game's metadata tables, from the `game=` folders of the file's
//! project (see [`metadata`]), name each field in the text the way World
//! Editor does, with its type, in a comment after it, and check the
//! fields while the text is edited ([`diagnostics`]). Without them,
//! there are no names and no checks.
//!
//! A collection (`.w3o`, World Editor's export of every kind) shows as
//! `[w3o]` and each kind's text form after it (`w3o-rs`); each block is
//! read, hinted and checked with its kind's metadata ([`Meta`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::game::Lookup;
use std::sync::Arc;

use w3abdhqtu_rs::{Kind, Metadata, STRINGS_FILES, from_text, parse};

/// The metadata a text form is read with: its kind's, or for a
/// collection (`.w3o`) every kind's.
#[derive(Default, Clone)]
pub struct Meta(pub Vec<(Kind, Arc<Metadata>)>);

static EMPTY: std::sync::LazyLock<Metadata> = std::sync::LazyLock::new(Metadata::default);

impl Meta {
    /// The metadata of `kind`; empty without it.
    pub fn of(&self, kind: Kind) -> &Metadata {
        self.0.iter().find(|(k, _)| *k == kind).map_or(&EMPTY, |(_, m)| m)
    }

    /// The one kind's metadata; empty for a collection's.
    pub fn single(&self) -> &Metadata {
        match self.0.as_slice() {
            [(_, m)] => m,
            _ => &EMPTY,
        }
    }

    /// Is it a collection's, every kind's?
    pub fn is_collection(&self) -> bool {
        self.0.len() > 1
    }
}

/// Is `path` a collection of object data (`.w3o`), by its extension?
pub fn is_collection_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("w3o"))
}

/// [`convert`] for a collection (`.w3o`), each kind with its metadata.
pub fn convert_collection(op: u8, name: &str, data: &[u8], meta: &Meta) -> Result<Vec<u8>, String> {
    let file = Path::new(name).file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            let c = w3o_rs::parse(data).map_err(|err| format!("Can't read {file}: {err}"))?;
            Ok(w3o_rs::text::to_text(&c, |k| meta.of(k)).into_bytes())
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            let c = w3o_rs::text::from_text(text, |k| meta.of(k)).map_err(|errors| {
                let e = &errors[0];
                let more = if errors.len() > 1 { format!(" (and {} more)", errors.len() - 1) } else { String::new() };
                format!("Can't save {file}: line {}: {}{more}", e.line, e.message)
            })?;
            c.to_bytes().map_err(|err| format!("Can't save {file}: {err}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// `ClientMsg::Objects`' ops.
pub const TO_TEXT: u8 = 0;
pub const TO_BYTES: u8 = 1;

/// Converts for the editor: the bytes of the file `name` to its text form
/// (`TO_TEXT`), or a text form to the bytes of `name` (`TO_BYTES`). The
/// text gets field names and types from `meta`, the metadata of the
/// file's kind. The error is a message to show.
pub fn convert(op: u8, name: &str, data: &[u8], meta: &Metadata) -> Result<Vec<u8>, String> {
    let file = Path::new(name)
        .file_name()
        .map_or(name.into(), |n| n.to_string_lossy());
    let kind = Kind::from_path(name).ok_or(format!(
        "{file} isn't object data (.w3a .w3b .w3d .w3h .w3q .w3t .w3u)"
    ))?;
    match op {
        TO_TEXT => {
            let data = parse(data, kind).map_err(|err| format!("Can't read {file}: {err}"))?;
            // Names and types show as inlay hints (`hints`), not in the text.
            Ok(data.to_text_with(meta, false).into_bytes())
        }
        TO_BYTES => {
            let text = std::str::from_utf8(data).map_err(|_| "the text isn't UTF-8".to_string())?;
            to_bytes(kind, text, meta).map_err(|err| format!("Can't save {file}: {err}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// The game files the metadata is read from, for every kind: the tables
/// ([`Kind::metadata_file`], [`Kind::objects_file`]) and the editor's
/// strings.
pub fn game_files() -> Vec<&'static str> {
    let mut files: Vec<&str> = Kind::ALL
        .iter()
        .flat_map(|k| [Some(k.metadata_file()), k.objects_file()])
        .flatten()
        .chain(STRINGS_FILES)
        .chain(crate::names::FILES)
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Is `path` named like one of [`game_files`]? For the disk watcher.
pub fn is_game_file_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    game_files().iter().any(|f| f.rsplit('\\').next().is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// Does a change to `path` change the metadata the game folder `folder`
/// gives: is it one of its archives or one of [`game_files`] in it?
pub fn affects(folder: &Path, path: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let (folder, path) = (lower(folder), lower(path));
    let Some(rel) = path.strip_prefix(&folder).and_then(|r| r.strip_prefix('/')) else {
        return false;
    };
    crate::game::ARCHIVES.iter().any(|a| rel == a.to_lowercase())
        || game_files().iter().any(|f| rel == f.replace('\\', "/").to_lowercase())
}

/// The metadata of `kind` looked up through `lookup` (the game folders,
/// then the map's roots: see `crate::game`): its table, its objects'
/// codes and the editor's strings. Empty without them.
pub fn metadata(lookup: &Lookup, kind: Kind) -> Metadata {
    if lookup.is_empty() {
        return Metadata::default();
    }
    let (game, _) = lookup.open();
    Metadata::load(kind, |name| game.read(name).map(|(bytes, _)| String::from_utf8_lossy(&bytes).into_owned()))
}

/// Metadata read so far, by game folders and kind: the checks run on
/// every edit, and reading the tables from the archives takes a while.
/// A change on disk that [`affects`] a folder drops what came from it.
#[derive(Default)]
pub struct Cache {
    read: HashMap<(Lookup, Kind), Arc<Metadata>>,
}

impl Cache {
    pub fn get(&mut self, lookup: &Lookup, kind: Kind) -> Arc<Metadata> {
        self.read
            .entry((lookup.clone(), kind))
            .or_insert_with(|| Arc::new(metadata(lookup, kind)))
            .clone()
    }

    /// Drops what changes with `paths`; whether anything was dropped.
    pub fn disk_changed(&mut self, paths: &[PathBuf]) -> bool {
        let before = self.read.len();
        self.read.retain(|(lookup, _), _| !paths.iter().any(|p| lookup.affected_by(p, affects)));
        self.read.len() != before
    }
}

/// The bytes of a text form; a plain number takes its field's type from
/// `meta`, the same metadata the text was written with.
fn to_bytes(kind: Kind, text: &str, meta: &Metadata) -> Result<Vec<u8>, String> {
    let parsed = from_text(text, meta);
    let Some(data) = parsed.data else {
        let n = parsed.errors.len();
        let first = &parsed.errors[0];
        let more = if n > 1 {
            format!(" (and {} more)", n - 1)
        } else {
            String::new()
        };
        return Err(format!("line {}: {}{more}", first.line, first.message));
    };
    if data.kind != kind {
        return Err(format!(
            "the text holds .{} data, not .{}",
            data.kind.extension(),
            kind.extension()
        ));
    }
    data.to_bytes().map_err(|err| err.message)
}

/// The hover for the field key at `pos` in the text form `text`: what
/// `meta`, the game's metadata, says about the field. `None` off a key.
pub fn hover(text: &str, pos: crate::document::Position, meta: &Meta) -> Option<String> {
    if !meta.is_collection() {
        return hover_with(text, pos, meta.single());
    }
    let line = pos.line as usize;
    let block = w3o_rs::text::blocks(text).into_iter().find(|b| b.first <= line && line <= b.last)?;
    hover_with(&block.text, pos, meta.of(block.kind?))
}

fn hover_with(text: &str, pos: crate::document::Position, meta: &Metadata) -> Option<String> {
    let lines = crate::document::LineIndex::new(text);
    let doc = bni_rs::parse(text);
    let at = w3abdhqtu_rs::field_at(&doc, pos.line as usize + 1, lines.column(text, pos), meta)?;
    let id = w3abdhqtu_rs::id_text(at.field);
    if meta.is_empty() {
        return Some(format!(
            "`{id}`\n\nNo field names or checks: the file isn't in a project with `game=` folders."
        ));
    }
    let Some(field) = meta.get(at.field) else {
        return Some(format!(
            "`{id}`\n\nNot a field of {} in this game's metadata.",
            at.kind.title()
        ));
    };
    let mut out = vec![format!("**{}** `{id}`", field.name)];

    // How it's stored, unless the type's name says so already.
    let plain = ["string", "real", "unreal"].contains(&field.ty.as_str());
    let mut ty = match field.storage {
        Some(storage) if !field.ty.is_empty() && !plain => format!("`{}`: {}", field.ty, storage.describe()),
        _ if !field.ty.is_empty() => format!("`{}`", field.ty),
        _ => String::new(),
    };
    let limit = |v: f64| if v.fract() == 0.0 { format!("{v:.0}") } else { v.to_string() };
    let limits = match (field.min, field.max) {
        _ if field.ty == "bool" => "0 or 1".to_string(),
        (Some(min), Some(max)) => format!("{} to {} in World Editor", limit(min), limit(max)),
        (Some(min), None) => format!("at least {} in World Editor", limit(min)),
        (None, Some(max)) => format!("at most {} in World Editor", limit(max)),
        (None, None) => String::new(),
    };
    if !limits.is_empty() {
        ty = if ty.is_empty() { limits } else { format!("{ty} · {limits}") };
    }
    let mut shape = Vec::new();
    if field.levels && at.kind.has_levels(w3abdhqtu_rs::LATEST_VERSION) {
        shape.push(if at.kind == Kind::Doodad { "per variation" } else { "per level" }.to_string());
    }
    if at.kind == Kind::Ability && field.column > 0 {
        let c = field.column;
        let letter = if c <= 26 { ((b'A' + (c - 1) as u8) as char).to_string() } else { c.to_string() };
        shape.push(format!("data column {letter} (`.{letter}`)"));
    }
    let shape = shape.join(" · ");
    out.push([ty, shape].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  \n"));

    let list = |ids: &[w3abdhqtu_rs::Rawcode]| {
        ids.iter().map(|&i| format!("`{}`", w3abdhqtu_rs::id_text(i))).collect::<Vec<_>>().join(", ")
    };
    let mut whose = Vec::new();
    if !field.specific.is_empty() {
        whose.push(format!("Data of {}.", list(&field.specific)));
    }
    if !field.not_specific.is_empty() {
        whose.push(format!("Not of {}.", list(&field.not_specific)));
    }
    let code = meta.code(at.base);
    if !at.base.is_none() && code != at.base && !field.specific.is_empty() {
        whose.push(format!("`{}` has the data of `{}`.", w3abdhqtu_rs::id_text(at.base), w3abdhqtu_rs::id_text(code)));
    }
    match (field.units, field.items) {
        (Some(true), Some(false)) => whose.push("Units only, not items.".into()),
        (Some(false), Some(true)) => whose.push("Items only.".into()),
        _ => {}
    }
    if !whose.is_empty() {
        out.push(whose.join(" "));
    }
    Some(out.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n"))
}

/// The inlay hints of lines `first..=last` (0-based) of the text form
/// `text`: each field's name and type from `meta`, after its value, as
/// JSON `[[line, character, "label"], ...]` in editor coordinates (0-based
/// lines, UTF-16 characters). Only those lines are worked out: the editor
/// asks for what's in view.
pub fn hints(text: &str, first: u32, last: u32, meta: &Meta) -> String {
    if !meta.is_collection() {
        return hints_with(text, first, last, meta.single());
    }
    let mut items = Vec::new();
    for b in w3o_rs::text::blocks(text) {
        let (Some(kind), from, to) = (b.kind, (first as usize).max(b.first), (last as usize).min(b.last)) else { continue };
        if from <= to {
            let json = hints_with(&b.text, from as u32, to as u32, meta.of(kind));
            let inner = &json[1..json.len() - 1];
            if !inner.is_empty() {
                items.push(inner.to_string());
            }
        }
    }
    format!("[{}]", items.join(","))
}

fn hints_with(text: &str, first: u32, last: u32, meta: &Metadata) -> String {
    let doc = bni_rs::parse(text);
    let lines = crate::document::LineIndex::new(text);
    let found = w3abdhqtu_rs::field_hints(&doc, meta, first as usize + 1, last as usize + 1);
    let items: Vec<String> = found
        .iter()
        .map(|h| {
            let at = lines.range(text, h.line, h.col, h.col).start;
            let label = json_string(&h.label);
            format!("[{},{},{label}]", at.line, at.character)
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// A JSON string literal.
pub(crate) fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            c if (c as u32) < 0x20 => out += &format!("\\u{:04x}", c as u32),
            c => out.push(c),
        }
    }
    out + "\""
}

/// What's wrong with a BNI document that is an object-data text form,
/// past its BNI errors (which the caller has): the errors that stop it
/// from saving, and what the checks against `meta` find (lint, see
/// `w3abdhqtu_rs::check`). Nothing for other BNI.
pub fn diagnostics(
    doc: &bni_rs::Document,
    text: &str,
    meta: Option<&Meta>,
) -> (Vec<parser_rs::ParseError>, Vec<parser_rs::Diagnostic>) {
    if w3o_rs::text::is_collection_text(doc) {
        let (mut errors, mut lint) = (Vec::new(), Vec::new());
        for b in w3o_rs::text::blocks(text) {
            match b.kind {
                None => {
                    let bni = bni_rs::parse(&b.text).errors;
                    errors.extend(w3o_rs::text::header(&b.text).into_iter().filter(|e| !bni.contains(e)));
                }
                Some(kind) => {
                    let block = bni_rs::parse(&b.text);
                    let (e, l) = diagnostics_with(&block, meta.map(|m| m.of(kind)));
                    errors.extend(e);
                    lint.extend(l);
                }
            }
        }
        return (errors, lint);
    }
    diagnostics_with(doc, meta.map(Meta::single))
}

fn diagnostics_with(
    doc: &bni_rs::Document,
    meta: Option<&Metadata>,
) -> (Vec<parser_rs::ParseError>, Vec<parser_rs::Diagnostic>) {
    if !w3abdhqtu_rs::is_object_text(doc) {
        return (Vec::new(), Vec::new());
    }
    let empty = Metadata::default();
    let mut parsed = w3abdhqtu_rs::from_bni(doc, meta.unwrap_or(&empty));
    let lint = meta.map_or(Vec::new(), |meta| w3abdhqtu_rs::check(&parsed, meta));
    parsed.errors.drain(..doc.errors.len());
    (parsed.errors, lint)
}

#[cfg(test)]
#[path = "objects.test.rs"]
mod tests;
