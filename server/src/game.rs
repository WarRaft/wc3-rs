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
            let text = crate::document::decode(bytes);
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

/// Does a change to `path` change what the game folder `folder` gives:
/// is it one of its archives ([`ARCHIVES`], directly in it) or one of the
/// game's scripts ([`SCRIPTS`]) in it, in any case?
pub fn affects(folder: &Path, path: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let (folder, path) = (lower(folder), lower(path));
    let Some(rel) = path.strip_prefix(&folder).and_then(|r| r.strip_prefix('/')) else {
        return false;
    };
    ARCHIVES.iter().any(|a| rel == a.to_lowercase())
        || SCRIPTS.iter().any(|s| rel == s.replace('\\', "/").to_lowercase())
}

/// Is `path` named like one of the game's archives?
pub fn is_archive_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| ARCHIVES.iter().any(|a| n.eq_ignore_ascii_case(a)))
}

/// The game's archives, in the order it opens them: each one over the
/// ones before.
pub const ARCHIVES: [&str; 4] = storm_rs::archive::WAR3_ARCHIVES;

pub use storm_rs::archive::Source;

/// The game's files, layered: see the module docs. Built on
/// `storm_rs::archive::Layers`, with the game's archives in its order.
#[derive(Debug, Default)]
pub struct Game(storm_rs::archive::Layers);

impl Game {
    /// Opens the game folders, lowest first. Never fails: an archive that
    /// can't be opened is left out and reported (`path: error`), and the
    /// rest still apply. A folder that doesn't exist adds nothing.
    pub fn open(folders: &[PathBuf]) -> (Game, Vec<String>) {
        let (layers, errors) = storm_rs::archive::Layers::open(folders, &ARCHIVES);
        (Game(layers), errors)
    }

    /// Where `name` comes from: the topmost layer that has it.
    #[cfg(test)]
    pub fn find(&self, name: &str) -> Option<Source> {
        self.0.find(name)
    }

    /// The contents of `name` from the topmost layer that has it, and
    /// where it came from.
    pub fn read(&self, name: &str) -> Option<(Vec<u8>, Source)> {
        self.0.read(name)
    }
}

#[cfg(test)]
#[path = "game.test.rs"]
mod tests;
