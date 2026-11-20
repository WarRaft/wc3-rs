//! The game's files, from the folders a project's `game=` lines name
//! (`Project::game`). A game file is named the way the game names it,
//! `Scripts\common.j`; `/` works too, and case doesn't matter.
//!
//! Each folder is searched the way Warcraft III searches its own: the
//! archives in the order the game opens them, each over the ones before,
//! then the folder itself over all of them:
//!
//! 1. `War3.mpq`
//! 2. `War3x.mpq`
//! 3. `War3xLocal.mpq`
//! 4. `War3Patch.mpq`
//! 5. the folder (`<folder>/Scripts/common.j`)
//!
//! A later `game=` folder sits over an earlier one, archives and files
//! alike, so a folder with only `Scripts/common.j` in it replaces just
//! that file of the game below it. Archives a folder doesn't have are
//! skipped; names match whatever their case on disk (`war3.mpq`).

use std::path::{Path, PathBuf};

use storm_rs::MpqArchive;

/// The game's scripts the project's files build on: their declarations
/// (natives, `Blizzard.j`'s functions and globals, `common.ai`'s AI API)
/// are visible to the project's files, each to the scripts the game runs
/// it with (see [`Script::sees`]).
pub const SCRIPTS: [&str; 3] = ["Scripts\\common.j", "Scripts\\Blizzard.j", "Scripts\\common.ai"];

/// Which of the game's scripts a file is, or which ones a file is run
/// with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    /// `common.j`: the natives, under every script.
    Common,
    /// `Blizzard.j`: map scripts run with it (and `common.j`).
    Blizzard,
    /// `common.ai`: AI scripts (`.ai`) run with it (and `common.j`), not
    /// with `Blizzard.j`.
    Ai,
}

impl Script {
    /// The game script `path` is, by its file name.
    pub fn of(path: &Path) -> Option<Script> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        match name.as_str() {
            "common.j" => Some(Script::Common),
            "blizzard.j" => Some(Script::Blizzard),
            "common.ai" => Some(Script::Ai),
            _ => None,
        }
    }

    /// The scripts a project file is run with: an AI script (`.ai`) with
    /// `common.ai`, anything else with `Blizzard.j`.
    pub fn for_file(path: &Path) -> Script {
        let ai = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("ai"));
        if ai { Script::Ai } else { Script::Blizzard }
    }

    /// Does code run with `self` see the game script `other`? `common.j`
    /// is under everything; `Blizzard.j` and `common.ai` don't see each
    /// other. As a viewer, `Common` (`common.j` itself) sees all three.
    pub fn sees(self, other: Script) -> bool {
        self == Script::Common || other == Script::Common || self == other
    }
}

/// The game's scripts ([`SCRIPTS`]) from the game folders `folders`, each
/// at the path it's found at: on disk, or inside an archive
/// (`War3Patch.mpq/Scripts/Blizzard.j`, see `map`). Text is read as
/// UTF-8, invalid bytes replaced.
pub fn scripts(folders: &[PathBuf]) -> Vec<(PathBuf, crate::document::Document)> {
    let (game, errors) = Game::open(folders);
    for e in errors {
        eprintln!("wc3-server: {e}");
    }
    SCRIPTS
        .iter()
        .filter_map(|name| {
            let (bytes, source) = game.read(name)?;
            let path = match source {
                Source::Archive(archive) => crate::map::inner_path(&archive, name),
                Source::File(path) => path,
            };
            let text = String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
            let doc = crate::document::Document {
                language: crate::document::Language::Jass,
                version: 0,
                text,
                path: Some(path.clone()),
            };
            Some((path, doc))
        })
        .collect()
}

/// The game's archives, in the order it opens them: each one over the
/// ones before.
pub const ARCHIVES: [&str; 4] = ["War3.mpq", "War3x.mpq", "War3xLocal.mpq", "War3Patch.mpq"];

/// One place a game file can come from.
#[derive(Debug)]
enum Layer {
    Archive(Box<MpqArchive>),
    Folder(PathBuf),
}

/// Where a game file was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// In this archive.
    Archive(PathBuf),
    /// This file on disk.
    File(PathBuf),
}

/// The game's files, layered: see the module docs.
#[derive(Debug, Default)]
pub struct Game {
    /// Lowest first; a lookup walks them from the top.
    layers: Vec<Layer>,
}

impl Game {
    /// Opens the game folders, lowest first. Never fails: an archive that
    /// can't be opened is left out and reported (`path: error`), and the
    /// rest still apply. A folder that doesn't exist adds nothing.
    pub fn open(folders: &[PathBuf]) -> (Game, Vec<String>) {
        let mut game = Game::default();
        let mut errors = Vec::new();
        for folder in folders {
            for name in ARCHIVES {
                let Some(path) = find_entry(folder, name) else { continue };
                if !path.is_file() {
                    continue;
                }
                match MpqArchive::open(&path) {
                    Ok(archive) => game.layers.push(Layer::Archive(Box::new(archive))),
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                }
            }
            if folder.is_dir() {
                game.layers.push(Layer::Folder(folder.clone()));
            }
        }
        (game, errors)
    }

    /// Where `name` comes from: the topmost layer that has it.
    #[cfg(test)]
    pub fn find(&self, name: &str) -> Option<Source> {
        let name = mpq_name(name)?;
        self.layers.iter().rev().find_map(|layer| match layer {
            Layer::Archive(a) => a.has_file(&name).then(|| Source::Archive(PathBuf::from(&a.path))),
            Layer::Folder(dir) => find_file(dir, &name).map(Source::File),
        })
    }

    /// The contents of `name` from the topmost layer that has it, and
    /// where it came from. A layer that has the file but can't read it
    /// is skipped, so the layer below it answers.
    pub fn read(&self, name: &str) -> Option<(Vec<u8>, Source)> {
        let name = mpq_name(name)?;
        self.layers.iter().rev().find_map(|layer| match layer {
            Layer::Archive(a) => {
                if !a.has_file(&name) {
                    return None;
                }
                let data = a.read_file(&name).ok()?;
                Some((data, Source::Archive(PathBuf::from(&a.path))))
            }
            Layer::Folder(dir) => {
                let path = find_file(dir, &name)?;
                let data = std::fs::read(&path).ok()?;
                Some((data, Source::File(path)))
            }
        })
    }
}

/// `name` as an archive names it: `\`-separated, without empty, `.` or
/// `..` parts. `None` when nothing is left or it tries to climb out.
fn mpq_name(name: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in name.split(['\\', '/']) {
        match part {
            "" | "." => {}
            ".." => return None,
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("\\"))
}

/// The file `name` (`\`-separated) under `dir`, matching each part in
/// any case.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut path = dir.to_path_buf();
    for part in name.split('\\') {
        path = find_entry(&path, part)?;
    }
    path.is_file().then_some(path)
}

/// The entry `name` of `dir`: that exact name, or else the one entry
/// whose name matches it in any case.
fn find_entry(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .map(|e| e.path())
}

#[cfg(test)]
#[path = "game.test.rs"]
mod tests;
