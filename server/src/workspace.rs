//! The project model, in memory. Every file of every known project is
//! here, open in the editor or not: its text (the editor's while it's
//! open, the disk's otherwise), its language and its project. Whenever a
//! file changes, it's re-analyzed and the editor gets a report, so the
//! problems of the whole project are known without opening anything.
//!
//! - **Which projects.** Every `wc3.bni` under the workspace folders, and
//!   the project of any file opened from elsewhere.
//! - **Which files.** A project's `sources` (or its whole root when it
//!   lists none), files with a known extension (`Language::from_path`),
//!   outside hidden folders, whose nearest `wc3.bni` is this project's
//!   (a nested project owns its own files). Plus the manifest itself,
//!   and the scripts inside every map (`.w3x`, `.w3m`) found the same
//!   way, each at a path inside the map's (see `map`).
//! - **Game files.** The game's scripts (`game::SCRIPTS`: `common.j`,
//!   `Blizzard.j`, `common.ai`) from each project's `game=` folders, at the path they're
//!   found at: on disk, or inside an archive
//!   (`War3Patch.mpq/Scripts/Blizzard.j`). Their declarations are visible
//!   to the project's files the game runs with them: `Blizzard.j` to map
//!   scripts, `common.ai` to AI scripts (`.ai`), `common.j` to both; they get no diagnostics (they're the game's,
//!   not the project's) and can't be renamed into. Projects with the same
//!   `game=` folders share them.
//! - **Visibility.** A file sees the files of its own project and that
//!   project's game files (see [`Workspace::scope`]), so two projects with
//!   different game folders don't see each other's `common.j`. An AI
//!   script (`.ai`) runs on its own: it sees no other project file, and
//!   none sees it. A map's own `Scripts\Blizzard.j` (often an optimized
//!   copy) replaces the game's for that map's files, gets no diagnostics,
//!   and no other file sees it.
//! - **Identity.** A file is identified by its path on disk, the form the
//!   editor uses for files too; a document with no file (untitled) by the
//!   editor's URI.
//!
//! Text from disk is read as UTF-8, with invalid bytes replaced: old maps
//! often store scripts in a legacy code page.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parser_rs::Occurrence;

use crate::analysis;
use crate::document::{Document, Language};
use crate::map;
use crate::names::Names;
use crate::project::{self, Project, Projects};
use crate::protocol::ServerMsg;
use w3abdhqtu_rs::Kind;
use crate::objects::Meta as Metadata;

struct Entry {
    doc: Document,
    /// The editor owns the text; disk changes don't touch it.
    open: bool,
    /// From the last analysis; empty until the file is first analyzed.
    symbols: Vec<Occurrence>,
    /// The root of the project it was last analyzed with.
    project: Option<PathBuf>,
    /// One of the game's scripts (see the module docs).
    game: bool,
    /// For the text form of object data, the game's metadata it was last
    /// checked with (empty outside a project with a game).
    objects: Option<Arc<Metadata>>,
    /// For the text form of regions, terrain or map info, the names its
    /// ids show as (`names`), from its project's layers.
    names: Option<Arc<Names>>,
    /// For the text form of terrain, what its file's points say (how many
    /// use each texture), read from the file as it is on disk.
    usage: Option<Arc<w3e_rs::text::Usage>>,
    /// A JASS file that parses: its program, to check its names.
    program: Option<Arc<jass_rs::Program>>,
    /// What a file of code declares for the rest of its project.
    declarations: Option<Arc<jass_rs::names::Declarations>>,
    /// Its own diagnostics (parse errors, lint), from its last analysis.
    own: Vec<analysis::Diagnostic>,
    /// Its names checked against its project ([`Workspace::check_names`]).
    named: Vec<analysis::Diagnostic>,
    /// Its highlighting, from its last analysis, sent again with a new
    /// check of its names.
    tokens: Vec<u32>,
    /// The settings it was last analyzed with (its `[lint]` for its names).
    settings: Option<Arc<Project>>,
}

impl Entry {
    fn new(doc: Document, open: bool) -> Entry {
        Entry {
            doc,
            open,
            symbols: Vec::new(),
            project: None,
            game: false,
            objects: None,
            names: None,
            usage: None,
            program: None,
            declarations: None,
            own: Vec::new(),
            named: Vec::new(),
            tokens: Vec::new(),
            settings: None,
        }
    }

    fn game(doc: Document, open: bool) -> Entry {
        Entry { game: true, ..Entry::new(doc, open) }
    }
}

/// What a file is read with: its project's settings, and the game's
/// metadata when it's the text form of object data.
type Settings = (Option<Arc<Project>>, Option<Arc<Metadata>>);
/// A file to analyze, by id, with its [`Settings`].
type Job<'a> = (&'a String, &'a Entry, Option<Arc<Project>>, Option<Arc<Metadata>>);

#[derive(Default)]
pub struct Workspace {
    roots: Vec<PathBuf>,
    files: HashMap<String, Entry>,
    projects: Projects,
    /// Manifests of the projects loaded so far.
    manifests: HashSet<PathBuf>,
    /// The maps in the model, and the ids of all their source files,
    /// those the project excludes too: a manifest change re-checks them
    /// without reading the map again.
    maps: HashMap<PathBuf, Vec<String>>,
    /// The maps among `maps` that are a `[map] root=` of their project:
    /// their files go through the project's whole lookup. Any other map
    /// stands alone over the game (see `lookup`).
    root_maps: HashSet<PathBuf>,
    /// The game files of each set of `game=` folders, in their order.
    games: HashMap<crate::game::Lookup, Vec<String>>,
    /// The game files each project (by root) sees.
    project_games: HashMap<PathBuf, Vec<String>>,
    /// The game files of the project the last name query came from: a
    /// game file shared by projects with different `game=` folders (the
    /// game's `Blizzard.j` under a custom `common.j` and under the stock
    /// one) is read with these when they include it, so it sees one file
    /// per game path, the one that project sees.
    context: std::sync::Mutex<Option<Vec<String>>>,
    /// The game's metadata for the text forms of object data.
    objects: crate::objects::Cache,
    /// The names ids show as, for the text forms of regions, terrain and
    /// map info.
    names: crate::names::Cache,
    /// What terrain files' points say, by path (`terrain_usage`).
    usage: HashMap<PathBuf, Option<Arc<w3e_rs::text::Usage>>>,
    /// Files of the model that a text open in the editor stands for, by
    /// that text's id: a custom script (`war3map.wct`) opened as text
    /// under its own id, whose disk version leaves the model while it's
    /// open, so its code is there once.
    shadowed: HashMap<String, String>,
    /// Files to analyze and report.
    dirty: HashSet<String>,
    /// Files the editor should forget: they left the model.
    cleared: Vec<String>,
}

/// The id of a file on disk.
pub fn path_id(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Workspace {
    /// The workspace folders. Loads every project found in them.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
        self.sync_disk();
    }

    /// The editor opened a document. A file from a project not loaded yet
    /// loads that project too.
    pub fn open(&mut self, id: String, path: Option<PathBuf>, language: Language, version: i32, text: String) {
        let new_project = path
            .as_deref()
            .and_then(project::find_manifest)
            .is_some_and(|m| !self.manifests.contains(&m));
        // A custom script's text is JASS, or Lua in a Lua map.
        let language = match path.as_deref() {
            Some(p) if crate::customtext::is_custom_text_file(p) => crate::customtext::language(p),
            _ => language,
        };
        // The text stands for a file of the model with the same path (a
        // custom script opened as text, under the editor's own id): that
        // one leaves until the text is closed.
        if let Some(disk) = path.as_deref().map(path_id).filter(|d| *d != id)
            && self.files.get(&disk).is_some_and(|e| !e.open)
        {
            self.remove(&disk);
            self.shadowed.insert(id.clone(), disk);
        }
        let doc = Document { language, version, text, path };
        let entry = if self.is_game(&id) { Entry::game(doc, true) } else { Entry::new(doc, true) };
        self.files.insert(id.clone(), entry);
        self.dirty.insert(id);
        if new_project {
            self.sync_disk();
        }
    }

    pub fn edit(&mut self, id: &str, version: i32, changes: &[(crate::document::Range, String)]) {
        if let Some(entry) = self.files.get_mut(id).filter(|e| e.open) {
            for (range, text) in changes {
                entry.doc.apply(*range, text);
            }
            entry.doc.version = version;
            self.dirty.insert(id.to_string());
        }
    }

    /// The editor switched the document's language; `None` means one the
    /// server doesn't analyze, which is the same as closing it.
    pub fn set_language(&mut self, id: &str, language: Option<Language>) {
        match language {
            Some(language) => {
                if let Some(entry) = self.files.get_mut(id) {
                    entry.doc.language = language;
                    self.dirty.insert(id.to_string());
                }
            }
            None => self.close(id),
        }
    }

    /// The editor closed a document. A project file goes back to its text
    /// on disk and stays in the model; anything else leaves it.
    pub fn close(&mut self, id: &str) {
        // A text that stood for a file of the model: the file comes back.
        if let Some(disk) = self.shadowed.remove(id) {
            self.remove(id);
            if let Some(doc) = load(Path::new(&disk)) {
                self.files.insert(disk.clone(), Entry::new(doc, false));
                self.dirty.insert(disk);
            }
            return;
        }
        let Some(entry) = self.files.get(id) else {
            return;
        };
        let path = entry.doc.path.clone();
        if self.is_game(id)
            && let Some(doc) = path.as_deref().and_then(load)
        {
            self.files.insert(id.to_string(), Entry::game(doc, false));
            self.dirty.insert(id.to_string());
            return;
        }
        let member = path.filter(|p| self.member(p).is_some());
        match member.and_then(|p| load(&p)) {
            Some(doc) => {
                self.files.insert(id.to_string(), Entry::new(doc, false));
                self.dirty.insert(id.to_string());
            }
            None => self.remove(id),
        }
    }

    /// Files changed on disk (created, modified or deleted). A manifest
    /// change reloads every project, since `sources` may have changed and
    /// settings apply to all files. A change to a game folder's archive or
    /// script (`game::affects`) reads its game files again. Open files keep
    /// the editor's text.
    pub fn disk_changed(&mut self, paths: &[PathBuf]) {
        // The game's metadata changed: check the object-data texts again.
        // A terrain file (or its map) changed: count its points again.
        let before = self.usage.len();
        self.usage.retain(|file, _| !paths.iter().any(|p| file == p || file.starts_with(p)));
        if self.usage.len() != before {
            let texts = self.files.iter().filter(|(_, e)| e.usage.is_some());
            let ids: Vec<String> = texts.map(|(id, _)| id.clone()).collect();
            self.dirty.extend(ids);
        }
        // The names changed: show the texts' hints again.
        if self.names.disk_changed(paths) {
            let texts = self.files.iter().filter(|(_, e)| e.names.is_some());
            let ids: Vec<String> = texts.map(|(id, _)| id.clone()).collect();
            self.dirty.extend(ids);
        }
        if self.objects.disk_changed(paths) {
            let texts = self.files.iter().filter(|(_, e)| {
                e.doc.language == Language::Bni
                    && e.doc.path.as_deref().is_some_and(|p| Kind::from_path(&p.to_string_lossy()).is_some())
            });
            let ids: Vec<String> = texts.map(|(id, _)| id.clone()).collect();
            self.dirty.extend(ids);
        }
        if paths.iter().any(|p| project::is_manifest(p)) {
            self.sync_disk();
            self.dirty.extend(self.files.keys().cloned());
            return;
        }
        // A game folder's archive or script changed: the game files of
        // every set of folders with it in are read again.
        let mut game_changed = false;
        for path in paths {
            let stale: Vec<crate::game::Lookup> =
                self.games.keys().filter(|lookup| lookup.scripts_affected_by(path)).cloned().collect();
            for stack in stale {
                self.games.remove(&stack);
                game_changed = true;
            }
        }
        if game_changed {
            self.sync_disk();
        }
        for path in paths {
            if self.is_game(&path_id(path)) || crate::game::is_archive_name(path) {
                continue;
            }
            if map::is_map_name(path) {
                self.reload_map(path);
                continue;
            }
            let id = path_id(path);
            if self.files.get(&id).is_some_and(|e| e.open) || self.is_shadowed(&id) {
                continue;
            }
            match self.member(path).and_then(|_| load(path)) {
                Some(doc) => {
                    self.files.insert(id.clone(), Entry::new(doc, false));
                    self.dirty.insert(id);
                }
                None => {
                    if self.files.contains_key(&id) {
                        self.remove(&id);
                    }
                }
            }
        }
    }

    /// Is the model empty of work?
    pub fn is_idle(&self) -> bool {
        self.dirty.is_empty() && self.cleared.is_empty()
    }

    /// Analyzes every changed file, in parallel, keeps its symbols for the
    /// index, and returns the reports in a stable order: cleared files
    /// first (empty reports), then the rest by id. Files not open in the
    /// editor get diagnostics only: tokens are for highlighting what's on
    /// screen.
    pub fn take_reports(&mut self) -> Vec<ServerMsg> {
        let mut out: Vec<ServerMsg> = std::mem::take(&mut self.cleared)
            .into_iter()
            .map(|id| ServerMsg::File { id, version: 0, diagnostics: Vec::new(), tokens: Vec::new() })
            .collect();

        let mut ids: Vec<String> = std::mem::take(&mut self.dirty).into_iter().collect();
        ids.sort();
        let settings: Vec<Settings> = ids
            .iter()
            .map(|id| {
                let Some(path) = self.files.get(id).and_then(|e| e.doc.path.clone()) else {
                    return (None, None);
                };
                let bni = self.files[id].doc.language == Language::Bni;
                let objects = bni.then(|| self.object_metadata(&path)).flatten();
                if bni && crate::hints::has_names(&path) {
                    let names = self.names(&path);
                    let usage = crate::terrain::is_terrain_file(&path).then(|| self.terrain_usage(&path)).flatten();
                    let entry = self.files.get_mut(id).expect("listed");
                    entry.names = Some(names);
                    entry.usage = usage;
                }
                (self.settings_for(&path), objects)
            })
            .collect();
        let mut jobs: Vec<Job> = Vec::new();
        for (id, (project, objects)) in ids.iter().zip(settings) {
            if let Some(entry) = self.files.get(id) {
                jobs.push((id, entry, project, objects));
            }
        }
        let results = parallel_map(&jobs, |(id, entry, project, objects)| {
            let a = analysis::analyze(&entry.doc, project.as_deref(), entry.open, objects.as_deref());
            ((*id).clone(), entry.doc.version, a)
        });
        let quiet: HashSet<String> = ids.iter().filter(|id| self.is_map_script(id)).cloned().collect();
        let mut roots: HashMap<String, Option<Arc<Project>>> = jobs.iter().map(|(id, _, p, _)| ((*id).clone(), p.clone())).collect();
        let mut objects: HashMap<String, Option<Arc<Metadata>>> =
            jobs.iter().map(|(id, _, _, o)| ((*id).clone(), o.clone())).collect();
        // What any file declares for the others changed (or files left):
        // every JASS file's names are checked again, else the analyzed ones'.
        let mut changed = !out.is_empty();
        let mut analyzed = Vec::new();
        for (id, _, mut a) in results {
            if let Some(entry) = self.files.get_mut(&id) {
                entry.symbols = a.symbols;
                entry.settings = roots.remove(&id).flatten();
                entry.project = entry.settings.as_ref().map(|p| p.root.clone());
                entry.objects = objects.remove(&id).flatten();
                if entry.game || quiet.contains(&id) {
                    a.diagnostics.clear();
                }
                changed |= entry.declarations != a.declarations;
                entry.program = a.program;
                entry.declarations = a.declarations;
                entry.own = a.diagnostics;
                entry.tokens = a.tokens;
                analyzed.push(id);
            }
        }
        let recheck: Vec<String> = if changed {
            self.files.iter().filter(|(_, e)| e.program.is_some()).map(|(id, _)| id.clone()).collect()
        } else {
            analyzed.iter().filter(|id| self.files.get(*id).is_some_and(|e| e.program.is_some())).cloned().collect()
        };
        let named = self.check_names(&recheck);
        let analyzed: HashSet<String> = analyzed.into_iter().collect();
        let mut ids: Vec<String> = analyzed.iter().cloned().collect();
        for (id, diags) in named {
            let entry = self.files.get_mut(&id).expect("checked");
            // Sent again only when they changed.
            if entry.named != diags && !analyzed.contains(&id) {
                ids.push(id.clone());
            }
            entry.named = diags;
        }
        ids.sort();
        ids.dedup();
        for id in ids {
            let e = &self.files[&id];
            let mut diagnostics = e.own.clone();
            diagnostics.extend(e.named.iter().cloned());
            diagnostics.sort_by_key(|d| (d.range.start.line, d.range.start.character));
            out.push(ServerMsg::File { id: id.clone(), version: e.doc.version, diagnostics, tokens: e.tokens.clone() });
        }
        out
    }

    /// The names of the JASS files `ids` checked against what the files
    /// each one sees declare (its project's, the game's scripts it runs
    /// with: [`Workspace::scope`]), in parallel: `jass_rs::names`. Only
    /// where the game's `common.j` is seen (without it every native would
    /// be unknown); never in the game's scripts or a map's own script,
    /// whose problems aren't reported. Each id with its diagnostics.
    fn check_names(&self, ids: &[String]) -> Vec<(String, Vec<analysis::Diagnostic>)> {
        let jobs: Vec<(&String, &Entry)> = ids.iter().filter_map(|id| self.files.get(id).map(|e| (id, e))).collect();
        parallel_map(&jobs, |(id, e)| {
            let Some(program) = e.program.as_deref().filter(|_| !e.game && !self.is_map_script(id)) else {
                return ((*id).clone(), Vec::new());
            };
            let mut env = Vec::new();
            let mut common = false;
            for (fid, _, _) in self.scope(id).filter(|(fid, _, _)| fid != id) {
                common |= self.files[fid].game && crate::game::Script::of(Path::new(fid)) == Some(crate::game::Script::Common);
                if let Some(d) = self.files[fid].declarations.as_deref() {
                    env.push(d);
                }
            }
            if !common {
                return ((*id).clone(), Vec::new());
            }
            ((*id).clone(), analysis::names(&e.doc, e.settings.as_deref(), program, &env))
        })
    }

    /// The set of game files read together with the game file `id` (or,
    /// for `None`, with a file outside every project): the last name
    /// query's project's, if it has `id`; else the first set (by `game=`
    /// folders and map roots) that has it. Each game path appears once in a set, so a
    /// name never resolves to two of the game's files.
    fn game_set(&self, id: Option<&str>) -> Option<&Vec<String>> {
        let has = |ids: &Vec<String>| id.is_none_or(|id| ids.iter().any(|g| g == id));
        let context = self.context.lock().ok().and_then(|c| c.clone());
        if let Some(ctx) = context.filter(|c| has(c)) {
            return self.games.values().find(|ids| **ids == ctx);
        }
        let mut sets: Vec<(&crate::game::Lookup, &Vec<String>)> =
            self.games.iter().filter(|(_, ids)| has(ids)).collect();
        sets.sort_by(|a, b| (&a.0.game, &a.0.roots, a.1).cmp(&(&b.0.game, &b.0.roots, b.1)));
        sets.first().map(|(_, ids)| *ids)
    }

    /// A name query comes from `id`: if it's a project's file, its
    /// project's game files become the ones a shared game file is read
    /// with (see `context`).
    pub fn note_context(&self, id: &str) {
        let Some(entry) = self.files.get(id).filter(|e| !e.game) else {
            return;
        };
        let Some(ids) = entry.project.as_ref().and_then(|r| self.project_games.get(r)) else {
            return;
        };
        if let Ok(mut c) = self.context.lock() {
            *c = Some(ids.clone());
        }
    }

    /// Is `id` one of the game's scripts?
    pub fn is_game(&self, id: &str) -> bool {
        self.games.values().any(|ids| ids.iter().any(|g| g == id))
    }

    /// The files `id` sees (see the module docs): its project's files and
    /// the game files it's run with (`game::Script::sees`: an AI script
    /// gets `common.ai`, any other `Blizzard.j`, both `common.j`); for a
    /// game file, the game files it comes with; for a file with no
    /// project, every file. An AI script runs on its own, so among the
    /// project's files it sees only itself, and the other files don't see
    /// it.
    pub fn scope<'a>(&'a self, id: &str) -> impl Iterator<Item = (&'a str, &'a Document, &'a [Occurrence])> + 'a {
        use crate::game::Script;
        let me = self.files.get(id);
        let viewer = if me.is_some_and(|e| e.game) {
            Script::of(Path::new(id)).unwrap_or(Script::Common)
        } else {
            Script::for_file(Path::new(id))
        };
        let sees = move |fid: &str| Script::of(Path::new(fid)).is_none_or(|s| viewer.sees(s));
        // Among non-game files: an AI script only itself, anything else
        // anything but AI scripts.
        let me_id = id.to_string();
        let shares = move |fid: &str| match viewer {
            Script::Ai => fid == me_id,
            _ => Script::for_file(Path::new(fid)) != Script::Ai,
        };
        // A game file sees the one set of game files it's read with.
        let game: Option<HashSet<&str>> = if me.is_some_and(|e| e.game) {
            Some(self.game_set(Some(id)).into_iter().flatten().map(String::as_str).collect())
        } else {
            None
        };
        // A file outside every project: one set of game files too.
        let loose_games: HashSet<&str> = if me.is_some_and(|e| !e.game && e.project.is_none()) {
            self.game_set(None).into_iter().flatten().map(String::as_str).collect()
        } else {
            HashSet::new()
        };
        // A map that isn't a root: its own copies of the game's scripts
        // replace the game's for its files, and no other file sees them.
        let mine: Vec<&str> = self.map_of(id).map(|m| self.map_scripts(m)).unwrap_or_default();
        let mine_kinds: Vec<Script> = mine.iter().filter_map(|f| Script::of(Path::new(f))).collect();
        let project = me.and_then(|e| e.project.clone()).filter(|_| game.is_none());
        let project_games: HashSet<&str> = project
            .as_ref()
            .and_then(|r| self.project_games.get(r))
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        self.files.iter().filter_map(move |(fid, e)| {
            let visible = match (&game, &project) {
                (Some(stack), _) => stack.contains(fid.as_str()) && sees(fid),
                (None, Some(root)) => {
                    let replaced = Script::of(Path::new(fid)).is_some_and(|s| mine_kinds.contains(&s));
                    (project_games.contains(fid.as_str()) && sees(fid) && !replaced)
                        || (mine.contains(&fid.as_str()) && sees(fid))
                        || (!e.game && e.project.as_ref() == Some(root) && shares(fid) && !self.is_map_script(fid))
                }
                (None, None) => {
                    if e.game {
                        loose_games.contains(fid.as_str()) && sees(fid)
                    } else {
                        shares(fid)
                    }
                }
            };
            visible.then_some((fid.as_str(), &e.doc, e.symbols.as_slice()))
        })
    }

    /// The map `id` is inside, if any.
    fn map_of(&self, id: &str) -> Option<&Path> {
        self.maps.keys().find(|m| Path::new(id).starts_with(m)).map(PathBuf::as_path)
    }

    /// The own copies of the game's scripts (`Scripts\common.j`,
    /// `Scripts\Blizzard.j`, `Scripts\common.ai`) of a map that isn't a
    /// root, in the model. A root map's are found by the project's lookup
    /// like any of its files.
    fn map_scripts(&self, map: &Path) -> Vec<&str> {
        if self.root_maps.contains(map) {
            return Vec::new();
        }
        self.maps.get(map).into_iter().flatten()
            .filter(|id| is_script_in(map, id) && self.files.contains_key(*id))
            .map(String::as_str)
            .collect()
    }

    /// Is `id` the own copy of one of the game's scripts of a map that
    /// isn't a root? Maps often carry an optimized `Blizzard.j`, which the
    /// game loads instead of its own.
    pub fn is_map_script(&self, id: &str) -> bool {
        self.map_of(id).is_some_and(|m| !self.root_maps.contains(m) && is_script_in(m, id))
    }

    /// One file's text and symbols.
    pub fn file(&self, id: &str) -> Option<(&Document, &[Occurrence])> {
        self.files.get(id).map(|e| (&e.doc, e.symbols.as_slice()))
    }

    /// The text form of object data `id` and the game's metadata it was
    /// last checked with; `None` for any other file.
    pub fn object_text(&self, id: &str) -> Option<(&Document, &Metadata)> {
        let entry = self.files.get(id)?;
        Some((&entry.doc, entry.objects.as_deref()?))
    }

    /// Every file's id, text and symbols, in no particular order.
    pub fn all_files(&self) -> impl Iterator<Item = (&str, &Document, &[Occurrence])> {
        self.files.iter().map(|(id, e)| (id.as_str(), &e.doc, e.symbols.as_slice()))
    }

    /// A map changed on disk (or appeared, or went away): its files are
    /// read again. Open ones keep the editor's text.
    fn reload_map(&mut self, path: &Path) {
        let old = self.maps.remove(path).unwrap_or_default();
        let mut all = Vec::new();
        let mut kept = Vec::new();
        if map::is_map(path)
            && let Some(project) = self.member(path)
        {
            for (inner, doc) in map::files(path) {
                let id = path_id(&inner);
                all.push(id.clone());
                if !project.contains(&inner) {
                    continue;
                }
                kept.push(id.clone());
                if self.files.get(&id).is_some_and(|e| e.open) || self.is_shadowed(&id) {
                    continue;
                }
                self.files.insert(id.clone(), Entry::new(doc, false));
                self.dirty.insert(id);
            }
        }
        for id in old {
            if !kept.contains(&id) && self.files.get(&id).is_some_and(|e| !e.open) {
                self.remove(&id);
            }
        }
        if !all.is_empty() {
            self.maps.insert(path.to_path_buf(), all);
        }
    }

    /// Does a text open in the editor stand for the file `id`?
    fn is_shadowed(&self, id: &str) -> bool {
        self.shadowed.values().any(|d| d == id)
    }

    fn remove(&mut self, id: &str) {
        self.files.remove(id);
        self.dirty.remove(id);
        self.cleared.push(id.to_string());
    }

    /// The project `path` is a file of, if any (see the module docs):
    /// its nearest project, or a loaded project that lists it in
    /// `sources` from outside its own root. A map counts as a file too.
    fn member(&mut self, path: &Path) -> Option<Arc<Project>> {
        if Language::from_path(path).is_none() && !map::is_map_name(path) {
            return None;
        }
        self.owner(path)
    }

    /// The project whose `sources` take in `path`, whatever kind of file
    /// it is: its nearest project, or a loaded project that lists it from
    /// outside its own root. A file in a map belongs where the map does.
    fn owner(&mut self, path: &Path) -> Option<Arc<Project>> {
        if let Some(project) = self.projects.for_file(path)
            && project.contains(path)
        {
            return Some(project);
        }
        let manifests: Vec<PathBuf> = self.manifests.iter().cloned().collect();
        manifests.iter().find_map(|m| {
            let project = self.projects.for_file(m)?;
            (!path.starts_with(&project.root) && project.contains(path)).then_some(project)
        })
    }

    /// The settings `path` is read with: its project's, or, for a file
    /// that isn't one of a project's files (outside `sources`, say), those
    /// of the nearest `wc3.bni` above it.
    fn settings_for(&mut self, path: &Path) -> Option<Arc<Project>> {
        self.member(path).or_else(|| self.projects.for_file(path))
    }

    /// The `game=` folders of the project `path` belongs to, in their
    /// order: the one whose `sources` take it in, else the nearest
    /// `wc3.bni` above it (a map next to its manifest belongs to that
    /// project, `sources` or not; see `settings_for`). None for a file
    /// with no `wc3.bni` above it.
    pub fn game_folders(&mut self, path: &Path) -> Vec<PathBuf> {
        self.settings_for(path).map(|p| p.game.clone()).unwrap_or_default()
    }

    /// The game's metadata for the object-data file `path` (`.w3a` ...),
    /// from its project's game folders (`game_folders`): empty outside a
    /// project. `None` when `path` isn't object data.
    pub fn object_metadata(&mut self, path: &Path) -> Option<Arc<Metadata>> {
        let kinds: Vec<Kind> = if crate::objects::is_collection_file(path) {
            w3o_rs::KINDS.to_vec()
        } else {
            vec![Kind::from_path(&path.to_string_lossy())?]
        };
        let lookup = self.lookup(path);
        Some(Arc::new(Metadata(kinds.into_iter().map(|k| (k, self.objects.get(&lookup, k))).collect())))
    }

    /// The names (`names::Names`) the ids in the text form at `path` show
    /// as, from its project's layers.
    pub fn names(&mut self, path: &Path) -> Arc<Names> {
        let lookup = self.lookup(path);
        self.names.get(&lookup)
    }

    /// The text of file `id`, the names its ids show as, and for terrain
    /// what its points say, when it's the text form of regions, sounds,
    /// terrain or map info.
    pub fn names_text(&self, id: &str) -> Option<(&Document, &Names, Option<&w3e_rs::text::Usage>)> {
        let entry = self.files.get(id)?;
        Some((&entry.doc, entry.names.as_deref()?, entry.usage.as_deref()))
    }

    /// What the points of the terrain file at `path` say, read once from
    /// the file (on disk or in its map) until it changes on disk.
    fn terrain_usage(&mut self, path: &Path) -> Option<Arc<w3e_rs::text::Usage>> {
        self.usage
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let bytes = if path.is_file() { std::fs::read(path).ok() } else { map::bytes(path) }?;
                let terrain = w3e_rs::parse(&bytes).ok()?;
                Some(Arc::new(w3e_rs::text::Usage::of(&terrain)))
            })
            .clone()
    }

    /// Where the files `path` is read with are looked up: its project's
    /// game folders and map roots (`game::Lookup`); nothing outside a
    /// project.
    ///
    /// A file inside a map that isn't one of the project's roots (a map
    /// kept in the repository to take things from) is read with that map
    /// alone over the game: the game folders, then the map.
    pub fn lookup(&mut self, path: &Path) -> crate::game::Lookup {
        match self.settings_for(path) {
            Some(project) => crate::game::Lookup::for_file(&project, path),
            None => crate::game::Lookup::default(),
        }
    }

    /// Every folder whose changes matter: the workspace folders, and the
    /// roots, sources and game folders of every loaded project (which may
    /// lie outside them). Folders inside another listed one are left out.
    pub fn watch_paths(&mut self) -> Vec<PathBuf> {
        let mut paths = self.roots.clone();
        let manifests: Vec<PathBuf> = self.manifests.iter().cloned().collect();
        for m in manifests {
            if let Some(project) = self.projects.for_file(&m) {
                paths.push(project.root.clone());
                paths.extend(project.scopes().into_iter().filter(|s| s.is_dir()));
                paths.extend(project.game.iter().filter(|g| g.is_dir()).cloned());
            }
        }
        paths.sort();
        paths.dedup();
        let mut out: Vec<PathBuf> = Vec::new();
        for p in paths {
            if !out.iter().any(|kept| p.starts_with(kept)) {
                out.push(p);
            }
        }
        out
    }

    /// Brings the disk side of the model up to date: finds the projects,
    /// loads their files that aren't loaded yet, and drops files that are
    /// no longer part of any project (unless they're open).
    fn sync_disk(&mut self) {
        let mut manifests: HashSet<PathBuf> =
            self.manifests.iter().filter(|m| m.is_file()).cloned().collect();
        for root in &self.roots {
            walk(root, &mut |p| {
                if project::is_manifest(p) {
                    manifests.insert(p.to_path_buf());
                }
            });
        }
        for entry in self.files.values().filter(|e| e.open) {
            if let Some(m) = entry.doc.path.as_deref().and_then(project::find_manifest) {
                manifests.insert(m);
            }
        }

        let mut wanted: HashMap<String, PathBuf> = HashMap::new();
        let mut maps: Vec<(PathBuf, Arc<Project>)> = Vec::new();
        for manifest in &manifests {
            let Some(project) = self.projects.for_file(manifest) else {
                continue;
            };
            wanted.insert(path_id(manifest), manifest.clone());
            for scope in project.scopes() {
                walk(&scope, &mut |p| {
                    let owned = !p.starts_with(&project.root)
                        || project::find_manifest(p).as_deref() == Some(manifest.as_path());
                    if !owned || !project.contains(p) {
                        return;
                    }
                    if Language::from_path(p).is_some() {
                        wanted.entry(path_id(p)).or_insert_with(|| p.to_path_buf());
                    } else if map::is_map_name(p) && !maps.iter().any(|(m, _)| m == p) {
                        maps.push((p.to_path_buf(), project.clone()));
                    }
                });
            }
        }
        self.manifests = manifests;

        // A map already in the model isn't read again: its files are known,
        // and the ones the project (still) wants are loaded below if they
        // aren't yet. A new map is read once.
        let mut found_maps = HashMap::new();
        self.root_maps = maps.iter().filter(|(m, p)| p.roots.contains(m)).map(|(m, _)| m.clone()).collect();
        for (path, project) in maps {
            let ids = match self.maps.remove(&path) {
                Some(ids) => ids,
                None => {
                    let mut ids = Vec::new();
                    for (inner, doc) in map::files(&path) {
                        let id = path_id(&inner);
                        if project.contains(&inner) && !self.files.contains_key(&id) && !self.is_shadowed(&id) {
                            self.files.insert(id.clone(), Entry::new(doc, false));
                            self.dirty.insert(id.clone());
                        }
                        ids.push(id);
                    }
                    ids
                }
            };
            for id in ids.iter().filter(|id| project.contains(Path::new(id))) {
                wanted.insert(id.clone(), PathBuf::from(id));
            }
            if !ids.is_empty() {
                found_maps.insert(path, ids);
            }
        }
        self.maps = found_maps;

        // The game files of every project, shared by projects with the same
        // `game=` folders; each set is read once.
        let mut games = HashMap::new();
        let mut project_games = HashMap::new();
        for manifest in &self.manifests.clone() {
            let Some(project) = self.projects.for_file(manifest) else {
                continue;
            };
            let lookup = crate::game::Lookup::of(&project);
            if lookup.is_empty() {
                continue;
            }
            let ids: Vec<String> = match games.get(&lookup).or_else(|| self.games.get(&lookup)) {
                Some(ids) => ids.clone(),
                None => crate::game::scripts(&lookup)
                    .into_iter()
                    .map(|(path, doc)| {
                        let id = path_id(&path);
                        if !self.files.get(&id).is_some_and(|e| e.open) {
                            self.files.insert(id.clone(), Entry::game(doc, false));
                            self.dirty.insert(id.clone());
                        }
                        id
                    })
                    .collect(),
            };
            for id in &ids {
                wanted.insert(id.clone(), PathBuf::from(id));
            }
            games.insert(lookup, ids.clone());
            project_games.insert(project.root.clone(), ids);
        }
        self.games = games;
        self.project_games = project_games;
        // A file may have become a game file, or stopped being one.
        let game_ids: HashSet<String> = self.games.values().flatten().cloned().collect();
        for (id, entry) in self.files.iter_mut() {
            let game = game_ids.contains(id);
            if entry.game != game {
                entry.game = game;
                self.dirty.insert(id.clone());
            }
        }

        for (id, path) in &wanted {
            if self.files.contains_key(id) || self.is_shadowed(id) {
                continue;
            }
            if let Some(doc) = load(path) {
                let entry = if self.is_game(id) { Entry::game(doc, false) } else { Entry::new(doc, false) };
                self.files.insert(id.clone(), entry);
                self.dirty.insert(id.clone());
            }
        }
        let gone: Vec<String> = self
            .files
            .iter()
            .filter(|(id, e)| !e.open && !wanted.contains_key(*id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.remove(&id);
        }
    }
}

/// Is `id` the file `Scripts\Blizzard.j` (any case) inside `map`?
/// Is `id` one of the game's scripts (`game::SCRIPTS`) at its place in
/// `map`, in any case?
fn is_script_in(map: &Path, id: &str) -> bool {
    Path::new(id).strip_prefix(map).is_ok_and(|rel| {
        let rel: Vec<String> = rel.iter().map(|p| p.to_string_lossy().to_lowercase()).collect();
        crate::game::SCRIPTS
            .iter()
            .any(|s| s.to_lowercase().split('\\').map(String::from).collect::<Vec<_>>() == rel)
    })
}

/// A file's disk version (from inside its map for a file in a map), or
/// `None` if it can't be read or has no known language.
fn load(path: &Path) -> Option<Document> {
    if !path.exists() {
        return map::load(path);
    }
    let language = Language::from_path(path)?;
    let bytes = std::fs::read(path).ok()?;
    if crate::customtext::is_custom_text_file(path) {
        return crate::customtext::document(path, &bytes);
    }
    let text = crate::document::decode(bytes);
    Some(Document { language, version: 0, text, path: Some(path.to_path_buf()) })
}

/// Calls `f` for `path` if it's a file, or for every file below it if it's
/// a folder, skipping hidden folders and not following symlinked ones.
fn walk(path: &Path, f: &mut impl FnMut(&Path)) {
    if path.is_file() {
        f(path);
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let child = entry.path();
        if kind.is_dir() {
            if !project::is_hidden(&entry.file_name()) {
                walk(&child, f);
            }
        } else if kind.is_file() || (kind.is_symlink() && child.is_file()) {
            f(&child);
        }
    }
}

/// `items.iter().map(f)`, spread over the CPU cores when there's more
/// than one item.
fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len());
    if threads <= 1 {
        return items.iter().map(f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| s.spawn(move || part.iter().map(f).collect::<Vec<_>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("analysis panicked")).collect()
    })
}

#[cfg(test)]
#[path = "workspace.test.rs"]
mod tests;
