//! Maps: `.w3x` and `.w3m` files, MPQ archives read with `storm-rs`. A
//! map in a project is read like a folder of the project: its scripts
//! are files of the model, each at a path inside the map's own,
//! `<map>/war3map.j`, `<map>/Scripts/war3map.lua`. That path is the
//! file's id and its `Document::path`, so its project, settings and
//! scope work as for any file next to the map.
//!
//! - **Which files.** Every file the map's `(listfile)` names with a
//!   known extension (`Language::from_path`), plus the map script under
//!   the names the game looks for (`war3map.j`, `Scripts\war3map.j`,
//!   `war3map.lua`), which protected maps often leave out of the list.
//!   Names differing only in case are the same file.
//! - **Read-only.** A file in a map isn't written back: the editor shows
//!   it read-only, and a rename that would touch it is refused.
//! - **Text** is read as UTF-8, invalid bytes replaced, like files on
//!   disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use storm_rs::MpqArchive;

use crate::document::{Document, Language};

/// Map file extensions (lowercase).
pub const EXTENSIONS: [&str; 2] = ["w3x", "w3m"];

/// Archives a path can lead into (lowercase): maps, and the game's own
/// `.mpq`s, whose files (`War3Patch.mpq/Scripts/Blizzard.j`) the model
/// holds as game files (see `workspace`). Only maps are found by walking
/// a project.
const ARCHIVES: [&str; 3] = ["w3x", "w3m", "mpq"];

/// Where the game looks for a map's script.
const SCRIPTS: [&str; 3] = ["war3map.j", "Scripts\\war3map.j", "war3map.lua"];

/// Is `path` named like a map?
pub fn is_map_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

/// Is `path` a map file on disk?
pub fn is_map(path: &Path) -> bool {
    is_map_name(path) && path.is_file()
}

/// Is `path` an archive on disk: a map or a game `.mpq`?
fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ARCHIVES.iter().any(|m| e.eq_ignore_ascii_case(m)))
        && path.is_file()
}

/// The path of the file `name` (`\`-separated, as in the archive) inside
/// `map`.
pub fn inner_path(map: &Path, name: &str) -> PathBuf {
    let mut path = map.to_path_buf();
    path.extend(name.split(['\\', '/']).filter(|p| !p.is_empty()));
    path
}

/// Splits a path inside an archive (a map or a game `.mpq`) into the
/// archive and the file's name in it: the nearest ancestor that's an
/// archive on disk.
pub fn split(path: &Path) -> Option<(&Path, String)> {
    let map = path.ancestors().skip(1).find(|a| is_archive(a))?;
    let rest = path.strip_prefix(map).ok()?;
    let parts: Vec<&str> = rest.iter().filter_map(|p| p.to_str()).collect();
    (!parts.is_empty()).then(|| (map, parts.join("\\")))
}

/// Every source file in `map`, at its path inside the map (see the
/// module docs). Empty if the map can't be read.
pub fn files(map: &Path) -> Vec<(PathBuf, Document)> {
    let Ok(archive) = MpqArchive::open(map) else {
        return Vec::new();
    };
    let mut names: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| Language::from_path(Path::new(&n.replace('\\', "/"))).is_some())
        .collect();
    names.sort();
    names.extend(SCRIPTS.iter().map(|s| s.to_string()));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in names {
        if !seen.insert(name.to_ascii_lowercase()) || !archive.has_file(&name) {
            continue;
        }
        let path = inner_path(map, &name);
        if let Some(doc) = read(&archive, &name, &path) {
            out.push((path, doc));
        }
    }
    out
}

/// The document at `path`, a path inside a map or a game `.mpq`.
pub fn load(path: &Path) -> Option<Document> {
    let (map, name) = split(path)?;
    let archive = MpqArchive::open(map).ok()?;
    read(&archive, &name, path)
}

fn read(archive: &MpqArchive, name: &str, path: &Path) -> Option<Document> {
    let language = Language::from_path(path)?;
    let bytes = archive.read_file(name).ok()?;
    let text = String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Some(Document { language, version: 0, text, path: Some(path.to_path_buf()) })
}

#[cfg(test)]
#[path = "map.test.rs"]
pub(crate) mod tests;
