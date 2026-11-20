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
