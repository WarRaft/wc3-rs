//! Maps: `.w3x` and `.w3m` files, MPQ archives read with `storm-rs`. A
//! map in a project is read like a folder of the project: its scripts
//! are files of the model, each at a path inside the map's own,
//! `<map>/war3map.j`, `<map>/Scripts/war3map.lua`. That path is the
//! file's id and its `Document::path`, so its project, settings and
//! scope work as for any file next to the map.
//!
//! - **Which files.** Every file the map's `(listfile)` names with a
//!   known extension (`Language::from_path`), plus the map script under
//!   the names the game looks for (`Scripts\war3map.j`, `war3map.j`,
//!   `war3map.lua`), which protected maps often leave out of the list,
//!   the files the editor keeps at the top of a map (`war3map.w3i`, ...),
//!   and any of the game's own files (`war3.listfile.txt`, the list of
//!   everything in its archives) the map carries, so a protected map's
//!   `Scripts\Blizzard.j` is found too. Names differing only in case are
//!   the same file.
//! - **One JASS script.** The game runs `Scripts\war3map.j` when there is
//!   one, and `war3map.j` only otherwise; an unused `war3map.j` isn't a
//!   file of the model (the tree of maps marks it).
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

/// Where the game looks for a map's script, in its order: a JASS script
/// in `Scripts` wins over one at the top of the map.
const SCRIPTS: [&str; 3] = ["Scripts\\war3map.j", "war3map.j", "war3map.lua"];

/// The JASS script at the top of a map, unused when `Scripts` has one.
const ROOT_SCRIPT: &str = "war3map.j";

/// The files the editor and the game keep at the top of a map.
const MAP_FILES: [&str; 31] = [
    "war3map.imp", "war3map.j", "war3map.lua", "war3map.w3e", "war3map.shd",
    "war3map.doo", "war3mapUnits.doo", "war3map.w3u", "war3map.w3t", "war3map.w3b",
    "war3map.w3d", "war3map.w3a", "war3map.w3h", "war3map.w3q", "war3map.w3s",
    "war3map.w3r", "war3map.w3c", "war3map.w3i", "war3map.wtg", "war3map.wct",
    "war3map.wts", "war3mapMap.blp", "war3mapMap.tga", "war3mapMap.b00", "war3mapPreview.tga",
    "war3mapMisc.txt", "war3mapSkin.txt", "war3mapExtra.txt", "war3map.mmp", "war3map.wpm",
    "war3mapPath.tga",
];

/// Every file of the game's own archives (`War3.mpq` and the rest), one
/// name per line. A map that leaves its `(listfile)` out (protected maps
/// do) still has its replacements of game files found by these names.
const GAME_LISTFILE: &str = include_str!("war3.listfile.txt");

/// The archive's own bookkeeping files, never shown.
const SPECIAL: [&str; 4] = ["(listfile)", "(attributes)", "(signature)", "(user data)"];

/// The names to look for in `archive`: what its `(listfile)` names, the
/// map script names and the game's own file names; unsorted, repeats
/// left in.
fn candidates(archive: &MpqArchive) -> impl Iterator<Item = String> {
    archive
        .list_files()
        .into_iter()
        .chain(SCRIPTS.iter().chain(&MAP_FILES).map(|s| s.to_string()))
        .chain(GAME_LISTFILE.lines().map(|l| l.trim().to_string()))
}

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
pub fn is_archive(path: &Path) -> bool {
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
    // The first spelling wins: the map's own `(listfile)` comes first.
    let mut seen = HashSet::new();
    let mut names: Vec<String> = candidates(&archive)
        .filter(|n| Language::from_path(Path::new(&n.replace('\\', "/"))).is_some())
        .filter(|n| seen.insert(n.to_ascii_lowercase()) && archive.has_file(n))
        .collect();
    names.sort();
    if names.iter().any(|n| n.eq_ignore_ascii_case(SCRIPTS[0])) {
        names.retain(|n| !n.eq_ignore_ascii_case(ROOT_SCRIPT));
    }
    let mut out = Vec::new();
    for name in names {
        let path = inner_path(map, &name);
        if let Some(doc) = read(&archive, &name, &path) {
            out.push((path, doc));
        }
    }
    out
}

/// Every file in `map`, by its name in the archive (`Scripts\\war3map.j`):
/// what the `(listfile)` names, plus the map script under the names the
/// game looks for and the game's own files the map replaces. Sorted, names differing only in case once, the
/// archive's own `(listfile)`-style entries left out. Empty if the map
/// can't be read. For the editor's tree of maps.
#[cfg(test)]
pub fn names(map: &Path) -> Vec<String> {
    MpqArchive::open(map).map(|a| names_in(&a)).unwrap_or_default()
}

/// `names` of an archive already open.
pub fn names_in(archive: &MpqArchive) -> Vec<String> {
    // The first spelling wins: the map's own `(listfile)` comes first.
    let mut seen = HashSet::new();
    let mut out: Vec<String> = candidates(archive)
        .filter(|n| !n.is_empty() && !SPECIAL.iter().any(|s| n.eq_ignore_ascii_case(s)))
        .filter(|n| seen.insert(n.to_ascii_lowercase()) && archive.has_file(n))
        .collect();
    out.sort_by_key(|n| n.to_ascii_lowercase());
    out
}

/// Is `name` the script at the top of a map whose `Scripts` folder has
/// one too, so the game never runs it? `names` is everything in the map.
pub fn is_unused_script(names: &[String], name: &str) -> bool {
    name.eq_ignore_ascii_case(ROOT_SCRIPT) && names.iter().any(|n| n.eq_ignore_ascii_case(SCRIPTS[0]))
}

/// The text of any file inside an archive, as UTF-8 with invalid bytes
/// replaced, whatever its language.
pub fn text(path: &Path) -> Option<String> {
    let (map, name) = split(path)?;
    let bytes = MpqArchive::open(map).ok()?.read_file(&name).ok()?;
    Some(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// The bytes of any file inside an archive.
pub fn bytes(path: &Path) -> Option<Vec<u8>> {
    let (map, name) = split(path)?;
    MpqArchive::open(map).ok()?.read_file(&name).ok()
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
    if crate::customtext::is_custom_text_file(path) {
        return crate::customtext::document(path, &bytes);
    }
    let text = crate::document::decode(bytes);
    Some(Document { language, version: 0, text, path: Some(path.to_path_buf()) })
}

#[cfg(test)]
#[path = "map.test.rs"]
pub(crate) mod tests;
