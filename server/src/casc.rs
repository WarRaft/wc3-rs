//! A Reforged install as a game folder: its CASC storage (`casc-rs`) laid
//! over the rest as one layer of `game::Game`, its mods in the order the
//! game reads them (the first that has a file wins):
//!
//! 1. with HD (`[project] hd=1`): `War3.w3mod:_HD.w3mod:_Locales/<locale>.w3mod:`,
//!    `War3.w3mod:_HD.w3mod:`;
//! 2. `War3.w3mod:_Locales/<locale>.w3mod:`;
//! 3. `War3.w3mod:`, the game's files.
//!
//! A picture (`.blp`, an HD model's `.tif` ...) the storage hasn't is
//! read as its `.dds` (Reforged's
//! textures are DDS; the game reads the one for the other).
//!
//! The locale: `[project] locale=` (`ruRU`), else the one installed (the
//! locale mod with the most files on disk). A storage opened once is
//! kept (opening one reads its indices, encoding and root).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use storm_rs::archive::{Source, Store};

/// Is `folder` a Reforged install: a `.build.info` and its `Data`?
pub fn is_storage(folder: &Path) -> bool {
    folder.join(".build.info").is_file() && folder.join("Data").is_dir()
}

/// An opened storage and the locale installed in it.
struct Opened {
    storage: Arc<casc_rs::Storage>,
    installed: Option<String>,
}

/// The storage of `folder`, opened once.
fn open(folder: &Path) -> Result<Arc<Opened>, String> {
    static OPEN: OnceLock<Mutex<HashMap<PathBuf, Arc<Opened>>>> = OnceLock::new();
    let mut open = OPEN.get_or_init(Default::default).lock().map_err(|_| "the storages' lock is poisoned".to_string())?;
    if let Some(o) = open.get(folder) {
        return Ok(o.clone());
    }
    let storage = casc_rs::Storage::open(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for e in &storage.entries {
        // `War3.w3mod:_Locales/ruRU.w3mod:...`, on disk.
        let Some(rest) = e.name.strip_prefix("War3.w3mod:_Locales/") else { continue };
        let Some((locale, _)) = rest.split_once(".w3mod:") else { continue };
        if storage.has(&e.name) {
            *counts.entry(locale.to_string()).or_default() += 1;
        }
    }
    let installed = counts.into_iter().max_by_key(|(_, n)| *n).map(|(l, _)| l);
    let o = Arc::new(Opened { storage: Arc::new(storage), installed });
    open.insert(folder.to_path_buf(), o.clone());
    Ok(o)
}

/// A storage's files for the game: its mods over each other.
pub struct Mods {
    storage: Arc<casc_rs::Storage>,
    folder: PathBuf,
    /// The mods' prefixes, the first first.
    pub prefixes: Vec<String>,
}

impl std::fmt::Debug for Mods {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Mods({}, {:?})", self.folder.display(), self.prefixes)
    }
}

impl Mods {
    /// The storage of `folder`, its mods for `hd` and `locale` (`None`:
    /// the one installed).
    pub fn open(folder: &Path, hd: bool, locale: Option<&str>) -> Result<Mods, String> {
        let o = open(folder)?;
        let locale = locale.map(str::to_string).or_else(|| o.installed.clone());
        let mut prefixes = Vec::new();
        if hd {
            if let Some(l) = &locale {
                prefixes.push(format!("War3.w3mod:_HD.w3mod:_Locales/{l}.w3mod:"));
            }
            prefixes.push("War3.w3mod:_HD.w3mod:".into());
        }
        if let Some(l) = &locale {
            prefixes.push(format!("War3.w3mod:_Locales/{l}.w3mod:"));
        }
        prefixes.push("War3.w3mod:".into());
        Ok(Mods { storage: o.storage.clone(), folder: folder.to_path_buf(), prefixes })
    }

    /// The name of `name` in the first mod that has it on disk; a picture
    /// (`.blp`, `.tif`, `.tga`, `.png`, `.jpg`: HD models name `.tif`s)
    /// the storage hasn't, its `.dds` (Reforged keeps its textures so, and
    /// the game reads the one for the other).
    fn full(&self, name: &str) -> Option<String> {
        let name = name.replace('\\', "/");
        let find = |name: &str| self.prefixes.iter().map(|p| format!("{p}{name}")).find(|n| self.storage.has(n));
        find(&name).or_else(|| {
            let dot = name.rfind('.')?;
            let ext = name[dot..].to_ascii_lowercase();
            [".blp", ".tif", ".tga", ".png", ".jpg"].contains(&ext.as_str()).then_some(())?;
            find(&format!("{}.dds", &name[..dot]))
        })
    }
}

impl Store for Mods {
    fn find(&self, name: &str) -> Option<Source> {
        self.full(name).map(|n| Source::Storage(self.folder.clone(), n))
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.storage.read(&self.full(name)?).ok()
    }
}

#[cfg(test)]
#[path = "casc.test.rs"]
mod tests;
