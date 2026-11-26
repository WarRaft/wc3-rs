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
//! A Reforged install (a CASC storage: `.build.info`, `Data`) is a game
//! folder too: its storage one layer, its mods inside it as the game
//! reads them (HD and the locale over the game's files: `crate::casc`).
//!
//! A later `game=` folder sits over an earlier one, archives and files
//! alike, so a folder with only `Scripts/common.j` in it replaces just
//! that file of the game below it. Archives a folder doesn't have are
//! skipped; names match whatever their case on disk (`war3.mpq`).
//!
//! Over all of them go the map's roots (`[map] root=`, [`Lookup`]): each
//! a folder or a map, each over the ones before, the way the game loads a
//! map's files over its own. Every file the server looks up for a project
//! (`UI\WorldEditStrings.txt`, `TerrainArt\Weather.slk`, the metadata
//! tables) goes through the same layers, and the last one that has it
//! wins: any of them can replace (shadow) any file below it.
//!
//! A few BNI files are read **with patches** ([`Game::read_patched`]):
//! `UI\TriggerData.patch.txt` beside `UI\TriggerData.txt` in any layer
//! changes it by meaning (replaces its keys, adds keys and sections:
//! `bni_rs::patch`) instead of replacing it whole. Which files: the ones
//! whose reader asks for it (the editor's data and strings,
//! `names::PATCHED`); everything else (tables, binaries, scripts) is read
//! as it is.

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

/// The game's scripts ([`SCRIPTS`]) looked up through `lookup` like any
/// other file (the game folders, then the map's roots; the last that has
/// one wins), each at the path it's found at: on disk, or inside an
/// archive (`War3Patch.mpq/Scripts/Blizzard.j`, see `map`). Text is read
/// as UTF-8, invalid bytes replaced.
///
/// The game loads `common.j` and `Blizzard.j` under every map script,
/// `common.j` and `common.ai` under every AI script. Nothing about them
/// needs a lookup of its own: a map script that wants a `common.ai`
/// function declares it as a `native` in its own code, and a root that
/// replaces one of them replaces it as it would any file. What such a
/// replacement breaks is for the build to find.
pub fn scripts(lookup: &Lookup) -> Vec<(PathBuf, crate::document::Document)> {
    let (game, errors) = lookup.open();
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
                Source::Storage(folder, inner) => folder.join(inner),
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

/// Where a project looks its files up: its game folders, then its map's
/// roots, lowest first (see the module docs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Lookup {
    pub game: Vec<PathBuf>,
    pub roots: Vec<PathBuf>,
    /// A Reforged game folder's HD files over its others (`[project]
    /// hd=1`), and its locale (`locale=`; `None`: the one installed):
    /// see `crate::casc`.
    pub hd: bool,
    pub locale: Option<String>,
}

impl Lookup {
    /// A project's: its `game=` folders and its `[map] root=`s.
    pub fn of(project: &crate::project::Project) -> Lookup {
        Lookup {
            game: project.game.clone(),
            roots: project.roots.clone(),
            hd: project.hd,
            locale: project.locale.clone(),
        }
    }

    /// Where the files `path` (a file of `project`) is read with are
    /// looked up: the project's layers ([`of`](Self::of)), except for a
    /// file inside a map that isn't one of its roots (a map kept in the
    /// repository to take things from), read with that map alone over
    /// the game: the game folders, then the map.
    pub fn for_file(project: &crate::project::Project, path: &Path) -> Lookup {
        let path = crate::project::normalize(path);
        // By the path, so it holds for a map the project names nowhere.
        let map = path.ancestors().skip(1).find(|a| crate::map::is_map_name(a));
        match map.filter(|m| !project.roots.iter().any(|r| r == m)) {
            Some(map) => Lookup { game: project.game.clone(), roots: vec![map.to_path_buf()], hd: project.hd, locale: project.locale.clone() },
            None => Lookup::of(project),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.game.is_empty() && self.roots.is_empty()
    }

    /// Opens every layer. Never fails: a game archive or a root that can't
    /// be opened is left out and reported (`path: error`), and the rest
    /// still apply; a game folder or root that doesn't exist adds nothing.
    pub fn open(&self) -> (Game, Vec<String>) {
        let (mut game, mut errors) = Game::open_with(&self.game, self.hd, self.locale.as_deref());
        for root in self.roots.iter().filter(|r| r.exists()) {
            if let Err(e) = game.0.push(root) {
                errors.push(e);
            }
        }
        (game, errors)
    }

    /// Does a change to `path` change the game's scripts ([`scripts`]):
    /// an archive or a script of a game folder ([`affects`]), a root map
    /// itself, or a script in a root folder?
    pub fn scripts_affected_by(&self, path: &Path) -> bool {
        self.game.iter().any(|folder| affects(folder, path))
            || self.roots.iter().any(|root| path == root || affects(root, path))
    }

    /// Does a change to `path` change what these layers give of the files
    /// the caller reads: in a game folder, what `in_game` says (a change to
    /// one of its archives, or to one of those files in it); in a root,
    /// anything (the root map itself, or a file under a root folder).
    pub fn affected_by(&self, path: &Path, in_game: impl Fn(&Path, &Path) -> bool) -> bool {
        self.game.iter().any(|folder| in_game(folder, path)) || self.roots.iter().any(|root| path.starts_with(root))
    }
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
        Game::open_with(folders, false, None)
    }

    /// [`open`](Self::open), a Reforged folder (`crate::casc`) its
    /// storage's mods for `hd` and `locale`.
    pub fn open_with(folders: &[PathBuf], hd: bool, locale: Option<&str>) -> (Game, Vec<String>) {
        let mut layers = storm_rs::archive::Layers::default();
        let mut errors = Vec::new();
        for folder in folders {
            if crate::casc::is_storage(folder) {
                match crate::casc::Mods::open(folder, hd, locale) {
                    Ok(mods) => layers.push_store(Box::new(mods)),
                    Err(e) => errors.push(e),
                }
                continue;
            }
            let (l, e) = storm_rs::archive::Layers::open(std::slice::from_ref(folder), &ARCHIVES);
            layers.extend(l);
            errors.extend(e);
        }
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

    /// The text of BNI file `name` (`UI\TriggerData.txt`) changed by its
    /// patches ([`patch_name`]: `UI\TriggerData.patch.txt`), each by
    /// meaning (`bni_rs::patch`), lowest first: those in the layer `name`
    /// comes from and in the layers over it. A patch below that layer
    /// changed a file that layer replaces, so it's shadowed with that
    /// file. Without `name` anywhere, every patch goes on an empty file.
    /// `None` when there is neither. Only for the files a caller chooses
    /// to read so (see the module docs).
    pub fn read_patched(&self, name: &str) -> Option<String> {
        let base = self.0.holders(name).last().copied();
        let mut text = match base {
            Some(layer) => crate::document::decode(self.0.read_in(layer, name)?.0),
            None => String::new(),
        };
        let patch = patch_name(name);
        let patches: Vec<usize> = self.0.holders(&patch).into_iter().filter(|&l| base.is_none_or(|b| l >= b)).collect();
        if base.is_none() && patches.is_empty() {
            return None;
        }
        for layer in patches {
            if let Some((bytes, _)) = self.0.read_in(layer, &patch) {
                text = bni_rs::patch(&text, &crate::document::decode(bytes));
            }
        }
        Some(text)
    }
}

/// The name of the patch to `name`: `.patch` before its extension
/// (`UI\TriggerData.txt`: `UI\TriggerData.patch.txt`), or after a name
/// without one.
pub fn patch_name(name: &str) -> String {
    let file = name.rfind(['\\', '/']).map_or(0, |i| i + 1);
    match name[file..].rfind('.').filter(|&i| i > 0) {
        Some(dot) => format!("{}.patch{}", &name[..file + dot], &name[file + dot..]),
        None => format!("{name}.patch"),
    }
}

#[cfg(test)]
#[path = "game.test.rs"]
mod tests;
