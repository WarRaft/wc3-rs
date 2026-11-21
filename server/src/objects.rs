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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use w3abdhqtu_rs::{Kind, Metadata, STRINGS_FILES, from_text, parse};

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
            Ok(data.to_text_named(meta).into_bytes())
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

/// The metadata of `kind` from the game folders `game` (see
/// `crate::game`): its table, its objects' codes and the editor's
/// strings. Empty without them.
pub fn metadata(game: &[PathBuf], kind: Kind) -> Metadata {
    if game.is_empty() {
        return Metadata::default();
    }
    let (game, _) = crate::game::Game::open(game);
    let read = |name: &str| game.read(name).map(|(bytes, _)| String::from_utf8_lossy(&bytes).into_owned());
    let Some(table) = read(kind.metadata_file()) else {
        return Metadata::default();
    };
    let strings: Vec<String> = STRINGS_FILES.iter().filter_map(|f| read(f)).collect();
    let strings: Vec<&str> = strings.iter().map(String::as_str).collect();
    let meta = Metadata::from_tables(&table, &strings);
    match kind.objects_file().and_then(read) {
        Some(objects) => meta.with_codes(&objects),
        None => meta,
    }
}

/// Metadata read so far, by game folders and kind: the checks run on
/// every edit, and reading the tables from the archives takes a while.
/// A change on disk that [`affects`] a folder drops what came from it.
#[derive(Default)]
pub struct Cache {
    read: HashMap<(Vec<PathBuf>, Kind), Arc<Metadata>>,
}

impl Cache {
    pub fn get(&mut self, game: &[PathBuf], kind: Kind) -> Arc<Metadata> {
        self.read
            .entry((game.to_vec(), kind))
            .or_insert_with(|| Arc::new(metadata(game, kind)))
            .clone()
    }

    /// Drops what changes with `paths`; whether anything was dropped.
    pub fn disk_changed(&mut self, paths: &[PathBuf]) -> bool {
        let before = self.read.len();
        self.read.retain(|(game, _), _| !game.iter().any(|folder| paths.iter().any(|p| affects(folder, p))));
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
pub fn hover(text: &str, pos: crate::document::Position, meta: &Metadata) -> Option<String> {
    let lines = crate::document::LineIndex::new(text);
    let doc = bni_rs::parse(text);
    let at = w3abdhqtu_rs::field_at(&doc, pos.line as usize + 1, lines.column(text, pos))?;
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

/// What's wrong with a BNI document that is an object-data text form,
/// past its BNI errors (which the caller has): the errors that stop it
/// from saving, and what the checks against `meta` find (lint, see
/// `w3abdhqtu_rs::check`). Nothing for other BNI.
pub fn diagnostics(
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
