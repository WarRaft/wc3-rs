//! Maps as folders: the file system the editor shows under the `wc3map:`
//! scheme. A path is `<archive>/<inner path>`, the archive (a map, or a
//! game `.mpq`) a folder and the files in it files, so the editor can
//! list, open, copy, add, rename and delete them like any others.
//!
//! - **Which files.** The names [`map::names_in`] finds: the archive's
//!   `(listfile)`, the files a map keeps at its top and the game's own
//!   file names. Folders are what those names have in common (`UI\a.fdf`
//!   makes `UI`); an MPQ has none of its own, so a folder made empty here
//!   lives only until the server stops.
//! - **Case** doesn't matter, as in MPQ itself; a file keeps the spelling
//!   it was found under.
//! - **Writing** goes through `storm-rs`'s in-place writer on a copy of the
//!   archive next to it. The copy replaces the archive only once it reads
//!   back right: every byte of the old files where it was, every file
//!   written readable and equal, every file kept still there. The writer
//!   appends after the files and moves the tables to the end, so an
//!   archive whose tables aren't already after its files (some protectors
//!   put them first) is read-only, as is every `.mpq`: those are the
//!   game's.
//! - **Times.** A file's time is the archive's when it was first read,
//!   or when this file was last written here; writing one file doesn't
//!   make the others look changed. A change to the archive from outside
//!   makes them all new.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use storm_rs::archive::{AddFileOptions, MpqArchiveWriter};
use storm_rs::MpqArchive;

use crate::map;
use crate::protocol::{fs_flags, FsOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

/// Why an operation failed; the editor shows each as its own
/// `FileSystemError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    Exists,
    NotADirectory,
    IsADirectory,
    NoPermissions(String),
    Other(String),
}

pub type FsResult<T> = Result<T, FsError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub kind: Kind,
    pub size: u64,
    /// Milliseconds since 1970.
    pub mtime: u64,
    pub readonly: bool,
    /// Something to show next to the file: why it doesn't count.
    pub note: String,
}

/// What's known about one archive.
struct Listing {
    /// The archive file's size and time when read, to see changes made
    /// from outside.
    stamp: (u64, SystemTime),
    /// The time every file has unless written here since.
    base: u64,
    /// Files, in the spelling found, and their sizes, by lowercase name.
    names: Vec<String>,
    sizes: HashMap<String, u64>,
    /// Lowercase name → time written here.
    written: HashMap<String, u64>,
    /// Why it can't be written, if it can't.
    readonly: Option<String>,
}

/// One change to an archive.
enum Change {
    Put(String, Vec<u8>),
    Remove(String),
}

#[derive(Default)]
pub struct MapFs {
    listings: HashMap<PathBuf, Listing>,
    /// Folders made here that hold no file yet: archive → lowercase
    /// inner path → spelling.
    empty_dirs: HashMap<PathBuf, HashMap<String, String>>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn stamp(archive: &Path) -> FsResult<(u64, SystemTime)> {
    let meta = std::fs::metadata(archive).map_err(|_| FsError::NotFound)?;
    Ok((meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// `inner`'s parent, `""` at the top.
fn parent(inner: &str) -> &str {
    inner.rfind('\\').map_or("", |i| &inner[..i])
}

/// Is `name` inside the folder `dir` (lowercase both), at any depth?
fn under(name: &str, dir: &str) -> bool {
    dir.is_empty() || (name.len() > dir.len() && name.starts_with(dir) && name.as_bytes()[dir.len()] == b'\\')
}

/// The archive `path` is in and the path inside it (`\`-separated, `""`
/// for the archive itself).
pub fn locate(path: &Path) -> Option<(PathBuf, String)> {
    if map::is_archive(path) {
        return Some((path.to_path_buf(), String::new()));
    }
    let (archive, name) = map::split(path)?;
    Some((archive.to_path_buf(), name))
}

/// Why the archive can't be written, if it can't.
fn readonly_reason(archive: &Path, mpq: &MpqArchive) -> Option<String> {
    let is_mpq = archive.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mpq"));
    if is_mpq {
        return Some("the game's archives are read-only".into());
    }
    if mpq.het_table.is_some() || mpq.bet_table.is_some() || !mpq.hi_block_table.is_empty() {
        return Some("this archive's format can't be written".into());
    }
    let end = data_end(mpq);
    if (mpq.header.hash_table_pos as u64) < end || (mpq.header.block_table_pos as u64) < end {
        return Some("this map's tables come before its files (a protected map?), so it can't be written".into());
    }
    None
}

/// Where the archive's file data ends, relative to the MPQ start.
fn data_end(mpq: &MpqArchive) -> u64 {
    mpq.block_table
        .iter()
        .filter(|b| b.flags & storm_rs::constants::MPQ_FILE_EXISTS != 0)
        .map(|b| b.file_pos as u64 + b.compressed_size as u64)
        .max()
        .unwrap_or(0)
}

impl MapFs {
    /// The archive's listing, read again if the archive changed.
    fn listing(&mut self, archive: &Path) -> FsResult<&mut Listing> {
        let now = stamp(archive)?;
        let fresh = self.listings.get(archive).is_some_and(|l| l.stamp == now);
        if !fresh {
            let mpq = MpqArchive::open(archive).map_err(|e| FsError::Other(format!("can't read {}: {e}", archive.display())))?;
            let names = map::names_in(&mpq);
            let sizes = names
                .iter()
                .map(|n| (lower(n), mpq.get_file_size(n).unwrap_or(0) as u64))
                .collect();
            let readonly = readonly_reason(archive, &mpq);
            self.listings.insert(
                archive.to_path_buf(),
                Listing { stamp: now, base: ms(now.1), names, sizes, written: HashMap::new(), readonly },
            );
        }
        Ok(self.listings.get_mut(archive).expect("just inserted"))
    }

    /// The spelling of the file `inner` (any case), if it's a file.
    fn file(&mut self, archive: &Path, inner: &str) -> FsResult<Option<String>> {
        let key = lower(inner);
        Ok(self.listing(archive)?.names.iter().find(|n| lower(n) == key).cloned())
    }

    /// Is `inner` a folder: the archive itself, a folder some file is in,
    /// or one made here?
    fn is_dir(&mut self, archive: &Path, inner: &str) -> FsResult<bool> {
        let key = lower(inner);
        if key.is_empty() {
            return Ok(true);
        }
        if self.listing(archive)?.names.iter().any(|n| under(&lower(n), &key)) {
            return Ok(true);
        }
        Ok(self
            .empty_dirs
            .get(archive)
            .is_some_and(|dirs| dirs.keys().any(|d| *d == key || under(d, &key))))
    }

    pub fn stat(&mut self, path: &Path) -> FsResult<Stat> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        if let Some(name) = self.file(&archive, &inner)? {
            let listing = self.listing(&archive)?;
            let key = lower(&name);
            let note = if map::is_unused_script(&listing.names, &name) {
                "unused: the game runs Scripts\\war3map.j".to_string()
            } else {
                String::new()
            };
            return Ok(Stat {
                kind: Kind::File,
                size: listing.sizes.get(&key).copied().unwrap_or(0),
                mtime: listing.written.get(&key).copied().unwrap_or(listing.base),
                readonly: listing.readonly.is_some(),
                note,
            });
        }
        if self.is_dir(&archive, &inner)? {
            let listing = self.listing(&archive)?;
            return Ok(Stat {
                kind: Kind::Dir,
                size: 0,
                mtime: listing.base,
                readonly: listing.readonly.is_some(),
                note: listing.readonly.clone().unwrap_or_default(),
            });
        }
        Err(FsError::NotFound)
    }

    /// What's directly in the folder `path`: names and kinds, folders
    /// first, then by name.
    pub fn read_dir(&mut self, path: &Path) -> FsResult<Vec<(String, Kind)>> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        if !self.is_dir(&archive, &inner)? {
            return Err(if self.file(&archive, &inner)?.is_some() { FsError::NotADirectory } else { FsError::NotFound });
        }
        let dir = lower(&inner);
        let prefix_len = if dir.is_empty() { 0 } else { dir.len() + 1 };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut add = |rest: &str, is_file: bool, out: &mut Vec<(String, Kind)>| {
            let (name, kind) = match rest.find('\\') {
                Some(i) => (&rest[..i], Kind::Dir),
                None if is_file => (rest, Kind::File),
                None => (rest, Kind::Dir),
            };
            if !name.is_empty() && seen.insert(lower(name)) {
                out.push((name.to_string(), kind));
            }
        };
        let names = self.listing(&archive)?.names.clone();
        for n in &names {
            if under(&lower(n), &dir) {
                add(&n[prefix_len..], true, &mut out);
            }
        }
        if let Some(dirs) = self.empty_dirs.get(&archive) {
            for (key, spelling) in dirs {
                if under(key, &dir) {
                    add(&spelling[prefix_len..], false, &mut out);
                }
            }
        }
        out.sort_by(|a, b| (a.1 == Kind::File).cmp(&(b.1 == Kind::File)).then_with(|| lower(&a.0).cmp(&lower(&b.0))));
        Ok(out)
    }

    pub fn read(&mut self, path: &Path) -> FsResult<Vec<u8>> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        let Some(name) = self.file(&archive, &inner)? else {
            return Err(if self.is_dir(&archive, &inner)? { FsError::IsADirectory } else { FsError::NotFound });
        };
        let mpq = MpqArchive::open(&archive).map_err(|e| FsError::Other(e.to_string()))?;
        mpq.read_file(&name).map_err(|e| FsError::Other(format!("can't read {name}: {e}")))
    }

    /// Fails unless every folder above `inner` is a folder (or missing).
    fn check_parents(&mut self, archive: &Path, inner: &str) -> FsResult<()> {
        let mut p = parent(inner);
        while !p.is_empty() {
            if self.file(archive, p)?.is_some() {
                return Err(FsError::NotADirectory);
            }
            p = parent(p);
        }
        Ok(())
    }

    pub fn write(&mut self, path: &Path, data: Vec<u8>, create: bool, overwrite: bool) -> FsResult<()> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        if inner.is_empty() {
            return Err(FsError::IsADirectory);
        }
        let existing = self.file(&archive, &inner)?;
        if existing.is_none() && self.is_dir(&archive, &inner)? {
            return Err(FsError::IsADirectory);
        }
        match existing {
            Some(_) if !overwrite => return Err(FsError::Exists),
            None if !create => return Err(FsError::NotFound),
            _ => {}
        }
        self.check_parents(&archive, &inner)?;
        let name = existing.unwrap_or(inner);
        self.apply(&archive, vec![Change::Put(name, data)])
    }

    pub fn create_dir(&mut self, path: &Path) -> FsResult<()> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        if self.file(&archive, &inner)?.is_some() || self.is_dir(&archive, &inner)? {
            return Err(FsError::Exists);
        }
        self.check_parents(&archive, &inner)?;
        if let Some(reason) = &self.listing(&archive)?.readonly {
            return Err(FsError::NoPermissions(reason.clone()));
        }
        self.empty_dirs.entry(archive).or_default().insert(lower(&inner), inner);
        Ok(())
    }

    pub fn delete(&mut self, path: &Path, recursive: bool) -> FsResult<()> {
        let (archive, inner) = locate(path).ok_or(FsError::NotFound)?;
        if inner.is_empty() {
            return Err(FsError::NoPermissions("delete the map file itself to remove the map".into()));
        }
        if let Some(name) = self.file(&archive, &inner)? {
            return self.apply(&archive, vec![Change::Remove(name)]);
        }
        if !self.is_dir(&archive, &inner)? {
            return Err(FsError::NotFound);
        }
        let key = lower(&inner);
        let files: Vec<String> = self.listing(&archive)?.names.iter().filter(|n| under(&lower(n), &key)).cloned().collect();
        if !files.is_empty() && !recursive {
            return Err(FsError::Other(format!("{inner} isn't empty")));
        }
        if !files.is_empty() {
            self.apply(&archive, files.into_iter().map(Change::Remove).collect())?;
        }
        if let Some(dirs) = self.empty_dirs.get_mut(&archive) {
            dirs.retain(|d, _| *d != key && !under(d, &key));
        }
        Ok(())
    }

    pub fn rename(&mut self, from: &Path, to: &Path, overwrite: bool) -> FsResult<()> {
        let (src, src_inner) = locate(from).ok_or(FsError::NotFound)?;
        let (dst, dst_inner) = locate(to).ok_or(FsError::NotFound)?;
        if src_inner.is_empty() || dst_inner.is_empty() {
            return Err(FsError::NoPermissions("a map is renamed as a file on disk".into()));
        }
        let same = src == dst;
        // Each file to move: (old name, new name).
        let moves: Vec<(String, String)> = if let Some(name) = self.file(&src, &src_inner)? {
            vec![(name, dst_inner.clone())]
        } else if self.is_dir(&src, &src_inner)? {
            let key = lower(&src_inner);
            if same && under(&lower(&dst_inner), &key) {
                return Err(FsError::Other("can't move a folder into itself".into()));
            }
            let names = self.listing(&src)?.names.clone();
            names
                .into_iter()
                .filter(|n| under(&lower(n), &key))
                .map(|n| {
                    let rest = n[src_inner.len()..].to_string();
                    (n, format!("{dst_inner}{rest}"))
                })
                .collect()
        } else {
            return Err(FsError::NotFound);
        };
        // Only a different spelling of the same name: nothing is in the way.
        let case_only = same && lower(&src_inner) == lower(&dst_inner);
        if !case_only {
            if self.file(&dst, &dst_inner)?.is_some() {
                if !overwrite {
                    return Err(FsError::Exists);
                }
                if moves.len() > 1 || self.is_dir(&src, &src_inner)? {
                    return Err(FsError::NotADirectory);
                }
            } else if self.is_dir(&dst, &dst_inner)? {
                if !overwrite {
                    return Err(FsError::Exists);
                }
                return Err(FsError::IsADirectory);
            }
            self.check_parents(&dst, &dst_inner)?;
        }

        let mpq = MpqArchive::open(&src).map_err(|e| FsError::Other(e.to_string()))?;
        let mut puts = Vec::new();
        let mut removes = Vec::new();
        for (old, new) in &moves {
            let data = mpq.read_file(old).map_err(|e| FsError::Other(format!("can't read {old}: {e}")))?;
            removes.push(Change::Remove(old.clone()));
            puts.push(Change::Put(new.clone(), data));
        }
        drop(mpq);
        if same {
            // Removing first lets a new spelling of the same name in.
            removes.extend(puts);
            self.apply(&src, removes)?;
        } else {
            self.apply(&dst, puts)?;
            self.apply(&src, removes)?;
        }
        // An empty folder moved along.
        if let Some(dirs) = self.empty_dirs.get_mut(&src) {
            let key = lower(&src_inner);
            let moved: Vec<(String, String)> = dirs
                .iter()
                .filter(|(d, _)| **d == key || under(d, &key))
                .map(|(d, s)| (d.clone(), format!("{dst_inner}{}", &s[src_inner.len()..])))
                .collect();
            for (d, _) in &moved {
                dirs.remove(d);
            }
            let target = self.empty_dirs.entry(dst).or_default();
            for (_, s) in moved {
                target.insert(lower(&s), s);
            }
        }
        Ok(())
    }

    /// Makes the changes to `archive`, safely (see the module docs).
    fn apply(&mut self, archive: &Path, changes: Vec<Change>) -> FsResult<()> {
        let listing = self.listing(archive)?;
        if let Some(reason) = &listing.readonly {
            return Err(FsError::NoPermissions(reason.clone()));
        }
        let kept: Vec<String> = listing
            .names
            .iter()
            .filter(|n| {
                !changes.iter().any(|c| match c {
                    Change::Put(name, _) | Change::Remove(name) => name.eq_ignore_ascii_case(n),
                })
            })
            .cloned()
            .collect();
        let file_name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("map");
        let temp = archive.with_file_name(format!(".{file_name}.wc3-saving"));
        let result = write_copy(archive, &temp, &changes, &kept).and_then(|()| {
            std::fs::rename(&temp, archive).map_err(|e| FsError::Other(format!("can't replace {}: {e}", archive.display())))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result?;

        // The archive is read again next time; what was written here keeps
        // its own time, the rest the time they had.
        let old = self.listings.remove(archive).expect("listed above");
        let listing = self.listing(archive)?;
        listing.base = old.base;
        listing.written = old.written;
        let now = now_ms();
        for change in &changes {
            match change {
                Change::Put(name, _) => listing.written.insert(lower(name), now),
                Change::Remove(name) => listing.written.remove(&lower(name)),
            };
        }
        // A folder that now holds a file isn't an empty one any more.
        if let Some(dirs) = self.empty_dirs.get_mut(archive) {
            for change in &changes {
                if let Change::Put(name, _) = change {
                    let name = lower(name);
                    dirs.retain(|d, _| !under(&name, d));
                }
            }
        }
        Ok(())
    }
}

/// Copies `archive` to `temp`, makes the changes there and checks the
/// result (see the module docs).
fn write_copy(archive: &Path, temp: &Path, changes: &[Change], kept: &[String]) -> FsResult<()> {
    let fail = |what: &str, e: &dyn std::fmt::Display| FsError::Other(format!("{what}: {e}"));
    let before = MpqArchive::open(archive).map_err(|e| fail("can't read the map", &e))?;
    let start = before.mpq_offset + before.header.header_size as u64;
    let end = before.mpq_offset + data_end(&before);
    drop(before);
    std::fs::copy(archive, temp).map_err(|e| fail("can't copy the map", &e))?;

    let mut writer = MpqArchiveWriter::open(temp).map_err(|e| fail("can't open the map for writing", &e))?;
    for change in changes {
        match change {
            Change::Put(name, data) => writer.add_file(name, data, &AddFileOptions::default()),
            Change::Remove(name) => writer.remove_file(name),
        }
        .map_err(|e| fail("can't change the map", &e))?;
    }
    writer.finish().map_err(|e| fail("can't finish writing the map", &e))?;

    // The old files' bytes are where they were...
    let old = std::fs::read(archive).map_err(|e| fail("can't check the map", &e))?;
    let new = std::fs::read(temp).map_err(|e| fail("can't check the map", &e))?;
    let range = start as usize..end as usize;
    if new.len() < range.end || old.get(range.clone()) != new.get(range) {
        return Err(FsError::Other("writing would have damaged the map's files; nothing was changed".into()));
    }
    // ...the new ones read back, the removed ones are gone, the rest are there.
    let after = MpqArchive::open(temp).map_err(|e| fail("the written map doesn't open", &e))?;
    let mut put: HashMap<String, &[u8]> = HashMap::new();
    let mut removed = HashSet::new();
    for change in changes {
        match change {
            Change::Put(name, data) => {
                removed.remove(&lower(name));
                put.insert(lower(name), data);
            }
            Change::Remove(name) => {
                put.remove(&lower(name));
                removed.insert(lower(name));
            }
        }
    }
    for change in changes {
        if let Change::Put(name, _) = change {
            if let Some(data) = put.get(&lower(name)) {
                if after.read_file(name).ok().as_deref() != Some(*data) {
                    return Err(FsError::Other(format!("{name} didn't read back as written; nothing was changed")));
                }
            }
        }
    }
    if let Some(name) = removed.iter().find(|n| after.has_file(n)) {
        return Err(FsError::Other(format!("{name} is still there after removing it; nothing was changed")));
    }
    if let Some(name) = kept.iter().find(|n| !after.has_file(n)) {
        return Err(FsError::Other(format!("{name} went missing; nothing was changed")));
    }
    Ok(())
}

/// The outcome of one `Fs` message.
#[derive(Default)]
pub struct Answer {
    pub error: Option<FsError>,
    pub stat: Option<Stat>,
    pub entries: Vec<(String, Kind)>,
    pub data: Vec<u8>,
    /// Did an archive change?
    pub changed: bool,
}

/// Runs one `Fs` message (see `protocol::ClientMsg::Fs`).
pub fn answer(fs: &mut MapFs, op: Option<FsOp>, path: &Path, to: &Path, flags: u8, data: Vec<u8>) -> Answer {
    let flag = |f: u8| flags & f != 0;
    let mut out = Answer::default();
    let done: FsResult<()> = match op {
        None => Err(FsError::Other("unknown operation".into())),
        Some(FsOp::Stat) => fs.stat(path).map(|s| out.stat = Some(s)),
        Some(FsOp::ReadDir) => fs.read_dir(path).map(|e| out.entries = e),
        Some(FsOp::Read) => fs.read(path).map(|d| out.data = d),
        Some(FsOp::Write) => fs.write(path, data, flag(fs_flags::CREATE), flag(fs_flags::OVERWRITE)),
        Some(FsOp::Delete) => fs.delete(path, flag(fs_flags::RECURSIVE)),
        Some(FsOp::Rename) => fs.rename(path, to, flag(fs_flags::OVERWRITE)),
        Some(FsOp::CreateDir) => fs.create_dir(path),
    };
    out.changed = done.is_ok() && matches!(op, Some(FsOp::Write | FsOp::Delete | FsOp::Rename));
    out.error = done.err();
    out
}

#[cfg(test)]
#[path = "mapfs.test.rs"]
mod tests;
