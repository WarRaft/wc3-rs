//! Building a project's map: a `[build]` of its `wc3.bni` (see the README,
//! "Building a map").
//!
//! - **The map's files** are its roots', each over the ones before (the
//!   last that has a file wins, as the lookup finds it), but for what's
//!   the project's and not the game's: the patches of the editor's files
//!   (`X.patch.txt`), `.trg` files, the project's code, `wc3.bni`, the
//!   map's script (made anew) and the archive's own files.
//! - **The script** (`war3map.j`) is `wc3_build::whole`'s: the map's part
//!   made of its files (through the project's lookup), the triggers, the
//!   map's custom script (the roots' `war3map.wct`, when they have one,
//!   whatever the triggers are: a trigger file's code of custom text
//!   beside it, `X.j`, over its piece there), the project's code,
//!   JassHelper's preprocessor
//!   run over all of it (`//! import "path"` found beside the importing
//!   file, else in the project's folder, else in its layers). The triggers are the
//!   project's `.trg` files when it has any (in the order of their paths),
//!   else the `war3map.wtg` of the roots; the code of a trigger of custom
//!   text is in the roots' `war3map.wct`, found by the trigger's name; a
//!   trigger file's `.j` beside it (the project's code) stands for its
//!   piece there.
//! - **The archive**: when the first root is a map, a copy of it with the
//!   rest written in (so a map's files nobody knows the names of, a
//!   protected map's, stay); else a new one, with the header of a map
//!   (`HM3W`: its name, flags and players from `war3map.w3i`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jass_rs::Dialect;
use storm_rs::archive::{MpqArchiveBuilder, create::MpqCreateOptions, folder_files};
use storm_rs::{AddFileOptions, MpqArchive, MpqArchiveWriter};
use wc3_build::whole::{Code, Input};
use wtg_rs::text::{Declared, Scope};

use crate::game::{Game, Lookup};
use crate::project::Project;

/// What a build made.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// The build's name.
    pub name: String,
    /// Where the map was written.
    pub out: PathBuf,
    /// The map the archive is a copy of, if it is.
    pub base: Option<PathBuf>,
    /// The map's files (the script among them).
    pub files: usize,
    /// The script's lines.
    pub script_lines: usize,
    /// Where the triggers came from: `.trg` files, `war3map.wtg`, none.
    pub triggers: String,
    /// What it says about the build: layers that couldn't be read, names
    /// left long.
    pub notes: Vec<String>,
}

/// The map's files, by their names in lowercase: the name to write,
/// where it comes from.
type Files = BTreeMap<String, (String, From)>;

/// Where a file of the map comes from.
#[derive(Debug, Clone, PartialEq)]
enum From {
    /// A file on disk, in a root folder.
    Disk(PathBuf),
    /// A root map (by its root's number), the file's name in it.
    Archive(usize, String),
}

/// The build `name` of `project` (its first without a name; a build of
/// its own without a `[build]`), made.
pub fn build(project: &Project, name: Option<&str>) -> Result<Built, Vec<String>> {
    let b = project.build_named(name).ok_or_else(|| vec![format!("the project has no build named '{}' ({})", name.unwrap_or(""), names(project))])?;
    if let Some(problem) = project.out_problem(&b) {
        return Err(vec![problem]);
    }
    let out = b.out(&project.root);
    if project.roots.is_empty() {
        return Err(vec!["the project has no map: [map] root= names its layers (a folder, or a map)".into()]);
    }
    let lookup = Lookup::of(project);
    let (game, mut notes) = lookup.open();
    let names = crate::names::names(&lookup);

    let (files, base) = map_files(project, &out)?;
    let (code, variables, initialization, triggers) = code(project, &game, &names)?;
    let read = |n: &str| game.read(n).map(|(b, _)| b);
    let read_patched = |n: &str| game.read_patched(n);
    // JassHelper's `//! import "path"`: beside the file importing it, else
    // in the project's folder, else in the project's layers (the map's
    // roots, the game) as a file of the map.
    let import = |from: &str, path: &str| -> Option<(String, String)> {
        let rel = path.replace('\\', "/");
        let candidates = [Path::new(from).parent().map(|d| d.join(&rel)), Some(project.root.join(&rel))];
        if let Some(p) = candidates.into_iter().flatten().find(|p| p.is_file()) {
            return std::fs::read(&p).ok().map(|b| (p.to_string_lossy().into_owned(), crate::document::decode(b)));
        }
        game.read(&path.replace('/', "\\")).map(|(b, _)| (path.to_string(), crate::document::decode(b)))
    };
    let input = Input { read: &read, read_patched: &read_patched, variables, initialization, code, uglify: b.uglify, import: &import };
    let script = wc3_build::whole::script(&input).map_err(|e| e.into_iter().map(|e| if e.origin.line > 0 { format!("{}:{}: {}", e.origin.file, e.origin.line, e.message) } else { format!("{}: {}", e.origin.file, e.message) }).collect::<Vec<_>>())?;
    notes.extend(script.notes);

    let written = write(project, &files, base.as_deref(), &script.text, &game, &out).map_err(|e| vec![e])?;
    Ok(Built { name: b.name, out, base, files: written, script_lines: script.text.lines().count(), triggers, notes })
}

/// The build `name` of the project of `path` (a file or folder in it:
/// the nearest `wc3.bni` in it or above), made; its settings' errors stop
/// it.
pub fn build_at(path: &Path, name: Option<&str>) -> Result<Built, Vec<String>> {
    let path = crate::project::normalize(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
    let probe = if path.is_dir() { path.join(crate::project::MANIFEST) } else { path.clone() };
    let manifest = crate::project::find_manifest(&probe).ok_or_else(|| vec![format!("{}: no {} here or above", path.display(), crate::project::MANIFEST)])?;
    let root = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
    let text = std::fs::read(&manifest).map(crate::document::decode).map_err(|e| vec![format!("{}: {e}", manifest.display())])?;
    let (project, issues) = crate::project::load(&root, &text);
    let errors: Vec<String> = issues.iter().filter(|i| i.level == parser_rs::Level::Error).map(|i| format!("{}:{}: {}", manifest.display(), i.line + 1, i.message)).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    build(&project, name)
}

fn names(project: &Project) -> String {
    if project.builds.is_empty() { "it has no [build]".into() } else { format!("it has {}", project.builds.iter().map(|b| if b.name.is_empty() { "the default".to_string() } else { format!("'{}'", b.name) }).collect::<Vec<_>>().join(", ")) }
}

/// Is `path` the project's code (`[code] sources`, not `exclude`)?
fn is_code(project: &Project, path: &Path) -> bool {
    project.sources.iter().any(|m| m.matches(path)) && !project.exclude.iter().any(|m| m.matches(path))
}

/// Is a file named `name` in the map the project's, not the map's?
fn left_out(name: &str) -> bool {
    let lower = name.to_ascii_lowercase().replace('/', "\\");
    let file = lower.rsplit('\\').next().unwrap_or(&lower);
    matches!(lower.as_str(), "(listfile)" | "(attributes)" | "(signature)" | "war3map.j" | "scripts\\war3map.j")
        || file == crate::project::MANIFEST
        || file.ends_with(".trg")
        || file.contains(".patch.")
}

/// The map's files, by their names in lowercase: the name to write, where
/// it comes from. And the map the archive starts as a copy of: the first
/// root, when it's a map.
fn map_files(project: &Project, out: &Path) -> Result<(Files, Option<PathBuf>), Vec<String>> {
    let mut files: BTreeMap<String, (String, From)> = BTreeMap::new();
    let mut errors = Vec::new();
    for (i, root) in project.roots.iter().enumerate() {
        if crate::map::is_map_name(root) && root.is_file() {
            match MpqArchive::open(root) {
                Ok(a) => {
                    for name in a.list_files() {
                        if !left_out(&name) {
                            files.insert(name.to_ascii_lowercase().replace('/', "\\"), (name.replace('/', "\\"), From::Archive(i, name)));
                        }
                    }
                }
                Err(e) => errors.push(format!("{}: {e}", root.display())),
            }
        } else if root.is_dir() {
            match folder_files(root) {
                Ok(list) => {
                    for (path, name) in list {
                        let hidden = path.strip_prefix(root).is_ok_and(|r| r.components().any(|c| crate::project::is_hidden(c.as_os_str())));
                        if hidden || path == out || crate::map::is_map_name(&path) || is_code(project, &path) || left_out(&name) {
                            continue;
                        }
                        files.insert(name.to_ascii_lowercase(), (name, From::Disk(path)));
                    }
                }
                Err(e) => errors.push(format!("{}: {e}", root.display())),
            }
        } else {
            errors.push(format!("{}: the root doesn't exist", root.display()));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let base = project.roots.first().filter(|r| crate::map::is_map_name(r)).cloned();
    Ok((files, base))
}

/// The project's code files of these extensions, in the order of their
/// paths.
fn code_files(project: &Project, extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mask in &project.sources {
        let mut stack = vec![mask.base.clone()];
        if mask.base.is_file() {
            stack.clear();
            if seen.insert(mask.base.clone()) {
                out.push(mask.base.clone());
            }
        }
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.file_name().is_some_and(crate::project::is_hidden) {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else if seen.insert(p.clone()) {
                    out.push(p);
                }
            }
        }
    }
    out.retain(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x))) && is_code(project, p));
    out.sort();
    out
}

type Made = (Vec<Code>, Vec<wtg_rs::jass::MapVariable>, Vec<String>, String);

/// The code: the triggers' JASS, the map's custom script and the
/// project's code; the triggers' variables, the triggers run at map
/// initialization, and where the triggers came from.
fn code(project: &Project, game: &Game, names: &crate::names::Names) -> Result<Made, Vec<String>> {
    let fallback = project.jass_dialect.unwrap_or(Dialect::Jass);
    // The map's custom script is JassHelper's (vJASS) unless the project
    // says it's plain JASS.
    let script_dialect = project.jass_dialect.unwrap_or(Dialect::VJass);
    let mut code = Vec::new();
    let mut errors = Vec::new();
    let read_text = |p: &Path| std::fs::read(p).map(crate::document::decode).map_err(|e| format!("{}: {e}", p.display()));

    // The custom script of the roots' map: its header, each trigger's by
    // its name in their `war3map.wtg`.
    let wtg = game.read("war3map.wtg").and_then(|(b, _)| match wtg_rs::parse_any(&b, &names.triggers) {
        Ok(wtg_rs::File::Classic(f)) => Some(f),
        _ => None,
    });
    let wct = game.read("war3map.wct").and_then(|(b, _)| wct_rs::parse(&b).ok());
    let mut custom: BTreeMap<String, String> = BTreeMap::new();
    if let (Some(wtg), Some(wct)) = (&wtg, &wct) {
        let all = wct.triggers.len() == wtg.triggers.len();
        let with_code = wtg.triggers.iter().filter(|t| all || t.is_comment == 0);
        for (t, c) in with_code.zip(&wct.triggers) {
            custom.insert(wtg_rs::jass::ident(&t.name), crate::document::decode(c.text().to_vec()));
        }
    }

    let trg = code_files(project, &["trg"]);
    let (variables, initialization, from) = if !trg.is_empty() {
        let texts: Vec<(PathBuf, String)> = trg.iter().filter_map(|p| read_text(p).map_err(|e| errors.push(e)).ok().map(|t| (p.clone(), t))).collect();
        let parsed: Vec<_> = texts.iter().map(|(_, t)| wtg_rs::text::trigger_from_text(t, Some(&names.triggers))).collect();
        let globals: Vec<(usize, Declared)> = parsed.iter().enumerate().flat_map(|(i, p)| p.variables.iter().filter(|d| d.scope == Scope::Global).map(move |d| (i, d.clone()))).collect();
        for (i, ((path, _), p)) in texts.iter().zip(&parsed).enumerate() {
            for e in &p.errors {
                errors.push(format!("{}:{}: {}", path.display(), e.line, e.message));
            }
            let mut visible: Vec<Declared> = globals.iter().filter(|(j, _)| *j != i).map(|(_, d)| d.clone()).collect();
            visible.extend(p.variables.iter().cloned());
            let me = path.to_string_lossy().into_owned();
            // The map's globals are declared (and set) by the map's part
            // (`InitGlobals`); a file declares only its own private ones.
            let own: Vec<Declared> = p.variables.iter().filter(|d| d.scope != Scope::Global).cloned().collect();
            let stem = path.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            code.push(Code { name: me.clone(), text: wtg_rs::jass::compile(p.trigger.as_ref(), &stem, &own, &visible, &names.trigger_ui), dialect: Dialect::Jass });
            // Custom text: its code beside it (`X.j`, among the project's
            // code: the project's file over the map's), else the map's
            // `war3map.wct`'s by the trigger's name.
            if let Some(t) = p.trigger.as_ref().filter(|t| t.is_custom != 0 && t.enabled != 0 && t.is_comment == 0)
                && !path.with_extension("j").is_file()
            {
                match custom.get(&wtg_rs::jass::ident(&t.name)) {
                    Some(c) => code.push(Code { name: format!("{me} (custom text)"), text: c.clone(), dialect: script_dialect }),
                    None => errors.push(format!(
                        "{}: trigger '{}' is custom text, and its code is neither beside it ({}) nor in the map's war3map.wct",
                        path.display(),
                        t.name,
                        path.with_extension("j").display()
                    )),
                }
            }
        }
        let variables = globals.iter().map(|(_, d)| wtg_rs::jass::map_variable(d, &names.trigger_ui)).collect();
        let triggers: Vec<wtg_rs::Trigger> = parsed.into_iter().filter_map(|p| p.trigger).collect();
        (variables, wtg_rs::jass::initialization_triggers(&triggers), format!("{} .trg files", trg.len()))
    } else if let Some(wtg) = &wtg {
        let text = wtg_rs::text::to_text(wtg).map_err(|e| vec![format!("war3map.wtg: {e}")])?;
        let file = wtg_rs::text::from_text(&text, None).file.ok_or_else(|| vec!["war3map.wtg: its text doesn't read".to_string()])?;
        for (nth, t) in file.triggers.iter().enumerate() {
            let name = format!("war3map.wtg: {}", t.name);
            code.push(Code { name: name.clone(), text: crate::triggers::wtg_jass(&text, nth + 1, names), dialect: Dialect::Jass });
            if t.is_custom != 0
                && t.enabled != 0
                && t.is_comment == 0
                && let Some(c) = custom.get(&wtg_rs::jass::ident(&t.name))
            {
                code.push(Code { name: format!("{name} (custom text)"), text: c.clone(), dialect: script_dialect });
            }
        }
        let variables = file.variables.iter().map(|v| wtg_rs::jass::map_variable(&Declared { variable: v.clone(), scope: Scope::Global }, &names.trigger_ui)).collect();
        (variables, wtg_rs::jass::initialization_triggers(&file.triggers), "war3map.wtg".into())
    } else {
        (Vec::new(), Vec::new(), "none".into())
    };
    // The map's custom script, before the triggers' code of custom text,
    // wherever the triggers are: a map's binary files work beside the
    // project's files (`war3map.wct` in the map, the triggers as `.trg`).
    if let Some(wct) = &wct {
        let header = crate::document::decode(wct.header.text().to_vec());
        if !header.trim().is_empty() {
            let at = code.iter().position(|c| c.name.ends_with("(custom text)")).unwrap_or(code.len());
            code.insert(at, Code { name: "war3map.wct".into(), text: header, dialect: script_dialect });
        }
    }
    // The project's code.
    for p in code_files(project, &["j", "vj", "zn"]) {
        match read_text(&p) {
            Ok(text) => code.push(Code { name: p.to_string_lossy().into_owned(), text, dialect: fallback }),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok((code, variables, initialization, from)) } else { Err(errors) }
}

/// Writes the map to `out` (through a file beside it, so a failed build
/// leaves the last map as it was): the number of files written.
fn write(project: &Project, files: &Files, base: Option<&Path>, script: &str, game: &Game, out: &Path) -> Result<usize, String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = out.with_extension("w3x.part");
    let _ = std::fs::remove_file(&tmp);
    let archives: BTreeMap<usize, MpqArchive> = files
        .values()
        .filter_map(|(_, f)| match f {
            From::Archive(i, _) => Some(*i),
            From::Disk(_) => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|i| MpqArchive::open(&project.roots[i]).map(|a| (i, a)).map_err(|e| format!("{}: {e}", project.roots[i].display())))
        .collect::<Result<_, _>>()?;
    let bytes = |f: &From| -> Result<Vec<u8>, String> {
        match f {
            From::Disk(p) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
            From::Archive(i, name) => archives[i].read_file(name).map_err(|e| format!("{}\\{name}: {e}", project.roots[*i].display())),
        }
    };
    let result = (|| -> Result<usize, String> {
        match base {
            Some(base) => {
                std::fs::copy(base, &tmp).map_err(|e| format!("{}: {e}", base.display()))?;
                let mut w = MpqArchiveWriter::open(&tmp).map_err(|e| format!("{}: {e}", base.display()))?;
                let had = MpqArchive::open(base).map_err(|e| format!("{}: {e}", base.display()))?;
                // What the base has that isn't the map's.
                for name in had.list_files().into_iter().filter(|n| left_out(n) && !n.starts_with('(')) {
                    w.remove_file(&name).map_err(|e| format!("{name}: {e}"))?;
                }
                for (name, from) in files.values() {
                    if matches!(from, From::Archive(0, _)) {
                        continue;
                    }
                    if had.has_file(name) {
                        w.remove_file(name).map_err(|e| format!("{name}: {e}"))?;
                    }
                    w.add_file(name, &bytes(from)?, &AddFileOptions::default()).map_err(|e| format!("{name}: {e}"))?;
                }
                w.add_file("war3map.j", script.as_bytes(), &AddFileOptions::default()).map_err(|e| format!("war3map.j: {e}"))?;
                w.finish().map_err(|e| format!("{}: {e}", out.display()))?;
                Ok(files.len() + 1)
            }
            None => {
                let mpq = out.with_extension("w3x.mpq");
                let _ = std::fs::remove_file(&mpq);
                let mut b = MpqArchiveBuilder::new(&mpq).options(MpqCreateOptions::v1().create_attributes(true).max_files(((files.len() + 1) * 2).max(64) as u32));
                for (name, from) in files.values() {
                    b = b.add_file(name.clone(), bytes(from)?);
                }
                b = b.add_file("war3map.j", script.as_bytes().to_vec());
                b.build().map_err(|e| format!("{}: {e}", out.display()))?;
                let archive = std::fs::read(&mpq).map_err(|e| format!("{}: {e}", mpq.display()));
                let _ = std::fs::remove_file(&mpq);
                let mut whole = header(game);
                whole.extend(archive?);
                std::fs::write(&tmp, whole).map_err(|e| format!("{}: {e}", tmp.display()))?;
                Ok(files.len() + 1)
            }
        }
    })();
    match result {
        Ok(n) => {
            std::fs::rename(&tmp, out).map_err(|e| format!("{}: {e}", out.display()))?;
            Ok(n)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// A map's header (`HM3W`, 512 bytes): its name, flags and players from
/// its `war3map.w3i` (a name `TRIGSTR_` from `war3map.wts`).
fn header(game: &Game) -> Vec<u8> {
    let info = game.read("war3map.w3i").and_then(|(b, _)| w3i_rs::parse(&b).ok()).unwrap_or_default();
    let mut name = info.name.clone();
    if name.starts_with("TRIGSTR_")
        && let Some((b, _)) = game.read("war3map.wts")
    {
        let doc = wts_rs::parse(&crate::document::decode(b));
        if let Some(t) = doc.lookup(&name) {
            name = t.to_string();
        }
    }
    let mut out = b"HM3W".to_vec();
    out.extend(0u32.to_le_bytes());
    out.extend(name.as_bytes());
    out.push(0);
    out.extend(info.flags.to_le_bytes());
    out.extend((info.players.len() as u32).to_le_bytes());
    out.resize(512, 0);
    out
}

#[cfg(test)]
#[path = "build.test.rs"]
mod tests;
