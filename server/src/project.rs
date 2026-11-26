//! Projects. A folder with a `wc3.bni` file in it is a project, the way a
//! folder with `Cargo.toml` is a Rust package: every file in that folder
//! and below belongs to it and is read with its settings. The manifest is
//! an ordinary BNI file, parsed by `bni-rs`:
//!
//! ```ini
//! [project]
//! game=C:/Games/Warcraft III
//! game=../my-common-j
//!
//! [code]
//! sources=src,lib
//! exclude=**/old/**,test.j
//!
//! [map]
//! root=base.w3x
//! root=../shared/map
//! root=map
//!
//! [build]
//! name=debug
//!
//! [jass]
//! dialect=vjass
//!
//! [lint]
//! pure=error
//! infinite-loop=warning
//! unused-local=off
//! ```
//!
//! The format is documented in the `wc3-rs` README ("Projects").

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use bni_rs::ast::{Field, Item, Name};
use parser_rs::Level;

use crate::glob::{self, Glob};

/// The manifest's file name.
pub const MANIFEST: &str = "wc3.bni";

/// A project's settings, read from its manifest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Project {
    /// The folder that holds `wc3.bni`.
    pub root: PathBuf,
    /// `[jass] dialect=`: the dialect of `.j` files whose header declares
    /// none. A header always wins.
    pub jass_dialect: Option<jass_rs::Dialect>,
    /// `[code] sources=`: the project's code, as masks resolved against
    /// `root`, in the order written. A plain path is a mask without
    /// wildcards. A file is code when a mask matches it or a folder (or
    /// map) it's in; see `glob`. The map's files are the project's by its
    /// `roots`, not by these.
    pub sources: Vec<Glob>,
    /// `[code] exclude=`: masks of what isn't code, even inside its
    /// sources. It doesn't touch the roots.
    pub exclude: Vec<Glob>,
    /// The roots as masks: everything in each is the project's.
    root_masks: Vec<Glob>,
    /// `[project] game=`: the game folders, resolved against `root` (an
    /// absolute path stays as written), in the order written. A file in a
    /// later folder overrides the same file in an earlier one; see `game`.
    pub game: Vec<PathBuf>,
    /// `[project] hd=1|0`: a Reforged game folder's HD files over its
    /// others (`crate::casc`); 0 by default.
    pub hd: bool,
    /// `[project] de=1|0`: 2.0's Definitive graphics (`_DE.w3mod`) in
    /// HD's place, drawn as HD.
    pub de: bool,
    /// `[project] locale=`: a Reforged game folder's locale (`ruRU`);
    /// the one installed by default.
    pub locale: Option<String>,
    /// `[map] root=`: the map's roots, resolved against `root` (an
    /// absolute path stays as written), in the order written. Each is a
    /// folder or a map whose top is the map's top (`war3map.w3i`,
    /// `Units\UnitFunc.txt`), each over the ones before, and all over the
    /// game's folders: see `game::Lookup`. A project is one map.
    /// Everything in a root is the project's, as code is.
    pub roots: Vec<PathBuf>,
    /// `[build]`: the build scenarios, in order (see `crate::build`).
    pub builds: Vec<Build>,
    /// `[lint] <rule>=off|warning|error`: rules switched or re-levelled
    /// from their defaults (`parser_rs::RULES`), by rule id.
    pub lint: HashMap<String, LintSetting>,
    /// `[lint] <group>=off|warning|error`: groups of rules
    /// (`parser_rs::GROUPS`), by group id. A rule set by name wins over
    /// its groups.
    pub lint_groups: HashMap<String, LintSetting>,
    /// `[external] NAME="program","arg",...`: the tools JassHelper's
    /// `//! external NAME` runs on the map built, each its program and the
    /// arguments before the ones the build gives (a program with a `/` in
    /// it resolved against `root`; `"wine","tools/ObjectMerger.exe"` runs
    /// a Windows tool), in the order written.
    pub externals: Vec<(String, Vec<String>)>,
}

/// A `[build]`: a scenario of building the project's map. Any number of
/// them, each its own section; one without a name is the default (one at
/// most), what a build asked for by no name builds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Build {
    /// `name=`: unique in the project; empty for the default build.
    pub name: String,
    /// `out=`: where the map is written, resolved against the project's
    /// root; `build/<name>.w3x` when left out.
    pub out: Option<PathBuf>,
    /// `uglify=1`: the script's names made short.
    pub uglify: bool,
    /// `inline=1`: one-line functions' calls inlined (`wc3_build::inline`).
    pub inline: bool,
    /// `dead_code=1`: what nothing reachable uses left out
    /// (`wc3_build::dead_code`).
    pub dead_code: bool,
    /// `string_hash=1`: `StringHash` of a literal worked out
    /// (`wc3_build::string_hash`).
    pub string_hash: bool,
    /// `script=jass|lua|as`: the language of the map's script.
    pub script: wc3_build::jass::Script,
}

impl Build {
    /// Where the map is written: `build/<name>.w3x`; for a project of
    /// code alone (no `[map] root`), its script, `build/war3map.<ext>`.
    pub fn out(&self, project: &Project) -> PathBuf {
        if let Some(out) = &self.out {
            return out.clone();
        }
        if project.roots.is_empty() {
            return project.root.join("build").join(format!("war3map.{}", self.script.extension()));
        }
        let name = if self.name.is_empty() { "map" } else { self.name.as_str() };
        project.root.join("build").join(format!("{name}.w3x"))
    }

    /// What this build is called to people: its name, or "the default build".
    pub fn title(&self) -> String {
        if self.name.is_empty() { "the default build".into() } else { format!("build '{}'", self.name) }
    }
}

impl Project {
    /// The build asked for by `name`: that one; without a name, the
    /// default (the `[build]` without one), else the first; without any
    /// `[build]`, a default one.
    pub fn build_named(&self, name: Option<&str>) -> Option<Build> {
        match name.filter(|n| !n.is_empty()) {
            Some(n) => self.builds.iter().find(|b| b.name == n).cloned(),
            None => Some(self.builds.iter().find(|b| b.name.is_empty()).or(self.builds.first()).cloned().unwrap_or_default()),
        }
    }

    /// Why the map of `b` can't be written where it goes: inside a root
    /// of the map, or among the project's code, it would be read as the
    /// map's (or the code's) by the next build. `None` when it can.
    pub fn out_problem(&self, b: &Build) -> Option<String> {
        let out = normalize(&b.out(self));
        if let Some(r) = self.roots.iter().find(|r| out == normalize(r) || (r.is_dir() && out.starts_with(normalize(r)))) {
            return Some(format!("{} writes {} inside the map's root {}: the next build would take it as one of the map's files; give it an 'out' elsewhere", b.title(), out.display(), r.display()));
        }
        let code = self.sources.iter().any(|m| m.matches(&out)) && !self.exclude.iter().any(|m| m.matches(&out));
        code.then(|| format!("{} writes {} among the project's code ([code] sources): give it an 'out' elsewhere, or exclude it", b.title(), out.display()))
    }
}

/// What a project does with a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSetting {
    Off,
    /// On, at this level, whatever level the rule reports at by default.
    On(Level),
}

impl LintSetting {
    fn from_name(name: &str) -> Option<LintSetting> {
        match name.to_ascii_lowercase().as_str() {
            "off" => Some(LintSetting::Off),
            "warning" => Some(LintSetting::On(Level::Warning)),
            "error" => Some(LintSetting::On(Level::Error)),
            _ => None,
        }
    }
}

/// Applies the project's lint settings (or none) to `diags`: drops the
/// rules that are off, by the project's choice or by default, and sets the
/// level of those it re-levels. A rule's own setting comes first, then
/// that of a group it belongs to, then its default.
pub fn apply_lint(diags: Vec<parser_rs::Diagnostic>, project: Option<&Project>) -> Vec<parser_rs::Diagnostic> {
    diags
        .into_iter()
        .filter_map(|mut d| {
            let rule = parser_rs::find_rule(d.rule);
            let setting = project.and_then(|p| {
                p.lint.get(d.rule).or_else(|| {
                    rule.and_then(|r| r.groups.iter().find_map(|g| p.lint_groups.get(*g)))
                })
            });
            let setting = setting.copied();
            let enabled = rule.is_none_or(|r| r.enabled);
            match setting {
                Some(LintSetting::Off) => None,
                Some(LintSetting::On(level)) => {
                    d.level = level;
                    Some(d)
                }
                None => enabled.then_some(d),
            }
        })
        .collect()
}

/// A problem in the manifest's settings, with the exact columns of the
/// offending name or value (1-based, codepoint-counted, end-exclusive).
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub level: Level,
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
}

impl Project {
    /// The manifest's path.
    pub fn manifest(&self) -> PathBuf {
        self.root.join(MANIFEST)
    }

    /// Where the project's files are: the folders its `sources` look in,
    /// and its roots.
    pub fn scopes(&self) -> Vec<PathBuf> {
        let mut scopes: Vec<PathBuf> = Vec::new();
        for mask in self.sources.iter().chain(&self.root_masks) {
            if !scopes.contains(&mask.base) {
                scopes.push(mask.base.clone());
            }
        }
        scopes
    }

    /// Is `path` one of the project's files: the manifest itself, or a
    /// file one of its `sources` masks matches (itself or a folder or
    /// map it's in), not
    /// inside a hidden (`.name`) folder below it, and not excluded.
    /// Doesn't check which project is nearest; see [`find_manifest`].
    pub fn contains(&self, path: &Path) -> bool {
        if path == self.manifest() {
            return true;
        }
        // No hidden folder below the mask's base; `matches` has checked
        // the base itself, in any case.
        let visible = |base: &Path| {
            let parts: Vec<_> = path.components().skip(base.components().count()).collect();
            !parts.split_last().is_some_and(|(_, dirs)| dirs.iter().any(|c| is_hidden(c.as_os_str())))
        };
        let code = self.sources.iter().any(|m| m.matches(path) && visible(&m.base))
            && !self.exclude.iter().any(|m| m.matches(path));
        code || self.root_masks.iter().any(|m| m.matches(path) && visible(&m.base))
    }
}

/// Hidden folders (`.git`, `.idea`, `.vscode`, ...) are never part of a
/// project.
pub fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| n.starts_with('.') && n != "." && n != "..")
}

/// Resolves `.` and `..` lexically, so `root/./src` and `root/a/../src`
/// are the same path as `root/src`, the form the editor and a directory
/// walk produce.
pub fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The path a setting's value `text` names: against `root`, an absolute
/// path as written. `\` separates parts like `/` (Warcraft III paths are
/// often written the Windows way), and `.` / `..` resolve.
pub fn resolve(root: &Path, text: &str) -> PathBuf {
    let text = if cfg!(windows) { text.to_string() } else { text.replace('\\', "/") };
    normalize(&root.join(text))
}

/// Is `path` a project manifest?
pub fn is_manifest(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == MANIFEST)
}

/// Reads the manifest text `src` of the project rooted at `root`. Never
/// fails: settings that can't be used are skipped and reported as issues,
/// and the rest still apply. BNI syntax errors and BNI lint aren't
/// repeated here; those come from `bni-rs` itself.
pub fn load(root: &Path, src: &str) -> (Project, Vec<Issue>) {
    let doc = bni_rs::parse(src);
    let mut build_places: Vec<(usize, usize, usize)> = Vec::new();
    let mut project = Project {
        root: root.to_path_buf(),
        ..Project::default()
    };
    let mut issues = Vec::new();
    let mut dialect_line: Option<usize> = None;
    let mut map_line: Option<usize> = None;

    for section in &doc.sections {
        let items = &section.items;
        match section.name.text.as_str() {
            "project" => {
                for item in items {
                    match key_of(item, &mut issues) {
                        Some("game") => read_game(root, item, &mut project, &mut issues),
                        Some(name @ ("hd" | "de")) => {
                            let key = item.key.as_ref().expect("key_of returned a key");
                            let value = match item.fields.as_slice() {
                                [field] if field.text().trim() == "1" => Some(true),
                                [field] if field.text().trim() == "0" => Some(false),
                                _ => None,
                            };
                            match (value, name) {
                                (Some(v), "hd") => project.hd = v,
                                (Some(v), _) => project.de = v,
                                (None, _) => issues.push(at_name(key, Level::Error, format!("'{name}' is 1 or 0"))),
                            }
                        }
                        Some("locale") => {
                            let key = item.key.as_ref().expect("key_of returned a key");
                            match item.fields.as_slice() {
                                [field] if field.text().trim().len() == 4 => project.locale = Some(field.text().trim().to_string()),
                                _ => issues.push(at_name(key, Level::Error, "'locale' is a locale's four letters: ruRU, enUS ...".into())),
                            }
                        }
                        Some(key @ ("sources" | "exclude")) => issues.push(at_name(
                            item.key.as_ref().expect("key_of returned a key"),
                            Level::Error,
                            format!("'{key}' is code's: it goes in [code] (the map's files are its [map] root= lines)"),
                        )),
                        Some(_) => unknown_key(item, "project", &mut issues),
                        None => {}
                    }
                }
            }
            "code" => {
                for item in items {
                    match key_of(item, &mut issues) {
                        Some("sources") => read_sources(root, item, &mut project, &mut issues),
                        Some("exclude") => read_exclude(root, item, &mut project, &mut issues),
                        Some(_) => unknown_key(item, "code", &mut issues),
                        None => {}
                    }
                }
            }
            "map" => {
                if let Some(first) = map_line {
                    issues.push(at_name(
                        &section.name,
                        Level::Warning,
                        format!("[map] is already at line {first}: a project is one map, so these roots are added to it"),
                    ));
                }
                map_line.get_or_insert(section.line);
                for item in items {
                    match key_of(item, &mut issues) {
                        Some("root") => read_root(root, item, &mut project, &mut issues),
                        Some(_) => unknown_key(item, "map", &mut issues),
                        None => {}
                    }
                }
            }
            "build" => read_build(section, &mut project, &mut issues, &mut build_places),
            "jass" => {
                for item in items {
                    match key_of(item, &mut issues) {
                        Some("dialect") => {
                            if let Some(first) = dialect_line {
                                issues.push(at_name(
                                    item.key.as_ref().unwrap(),
                                    Level::Warning,
                                    format!("'dialect' is already set at line {first}; this one is ignored"),
                                ));
                                continue;
                            }
                            dialect_line = Some(item.line);
                            project.jass_dialect = read_dialect(item, &mut issues);
                        }
                        Some(_) => unknown_key(item, "jass", &mut issues),
                        None => {}
                    }
                }
            }
            "lint" => {
                for item in items {
                    if let Some(rule) = key_of(item, &mut issues) {
                        read_lint(item, rule, &mut project, &mut issues);
                    }
                }
            }
            "external" => {
                for item in items {
                    let Some(tool) = key_of(item, &mut issues) else { continue };
                    let mut argv: Vec<String> = item.fields.iter().map(|f| f.text().trim().to_string()).collect();
                    if argv.first().is_none_or(|p| p.is_empty()) {
                        issues.push(at_name(item.key.as_ref().expect("a key"), Level::Error, format!("'{tool}' needs its program: {tool}=\"path/to/tool\" (and arguments, comma-separated)")));
                        continue;
                    }
                    if argv[0].contains(['/', '\\']) && Path::new(&argv[0]).is_relative() {
                        argv[0] = root.join(&argv[0]).to_string_lossy().into_owned();
                    }
                    if project.externals.iter().any(|(t, _)| t == tool) {
                        issues.push(at_name(item.key.as_ref().expect("a key"), Level::Warning, format!("'{tool}' is named already: the first is run")));
                        continue;
                    }
                    project.externals.push((tool.to_string(), argv));
                }
            }
            // An empty name is already a bni-rs lint warning.
            "" => {}
            other => issues.push(at_name(
                &section.name,
                Level::Warning,
                format!("unknown section [{other}] (expected [project], [code], [map], [build], [jass], [lint] or [external])"),
            )),
        }
    }
    // Where each build writes its map, once the roots and the code are known.
    for (b, (line, col, end_col)) in project.builds.iter().zip(&build_places) {
        if let Some(message) = project.out_problem(b) {
            issues.push(Issue { level: Level::Error, message, line: *line, col: *col, end_col: *end_col });
        }
    }
    (project, issues)
}

/// The item's key, or `None` (with an issue) for a keyless `=value` line.
fn key_of<'a>(item: &'a Item, issues: &mut Vec<Issue>) -> Option<&'a str> {
    match &item.key {
        Some(key) => Some(key.text.as_str()),
        None => {
            issues.push(Issue {
                level: Level::Warning,
                message: "setting without a name".into(),
                line: item.line,
                col: item.eq.start_col,
                end_col: item.eq.end_col,
            });
            None
        }
    }
}

fn unknown_key(item: &Item, section: &str, issues: &mut Vec<Issue>) {
    let key = item.key.as_ref().expect("key_of returned a key");
    let expected = match section {
        "project" => "game', 'hd', 'de' or 'locale",
        "code" => "sources' or 'exclude",
        "map" => "root",
        "build" => "name', 'out', 'script', 'uglify', 'inline', 'dead_code' or 'string_hash",
        _ => "dialect",
    };
    issues.push(at_name(
        key,
        Level::Warning,
        format!("unknown setting '{}' in [{section}] (expected '{expected}')", key.text),
    ));
}

/// `<rule>=off|warning|error`, or `<group>=...` for every rule of a
/// group. Set twice: the last one wins, as lint settings are often
/// appended to override earlier ones.
fn read_lint(item: &Item, rule: &str, project: &mut Project, issues: &mut Vec<Issue>) {
    let key = item.key.as_ref().expect("key_of returned a key");
    let is_group = parser_rs::find_group(rule).is_some();
    if !is_group && parser_rs::find_rule(rule).is_none() {
        issues.push(at_name(
            key,
            Level::Warning,
            format!("unknown lint rule or group '{rule}' (they are listed in the wc3-rs README)"),
        ));
        return;
    }
    let [field] = item.fields.as_slice() else {
        issues.push(at_name(key, Level::Error, format!("'{rule}' needs one value: off, warning or error")));
        return;
    };
    let value = field.text();
    match LintSetting::from_name(value.trim()) {
        Some(setting) => {
            let map = if is_group { &mut project.lint_groups } else { &mut project.lint };
            map.insert(rule.to_string(), setting);
        }
        None => issues.push(at_field(
            field,
            Level::Error,
            format!("unknown lint setting '{value}' (expected off, warning or error)"),
        )),
    }
}

fn read_dialect(item: &Item, issues: &mut Vec<Issue>) -> Option<jass_rs::Dialect> {
    let [field] = item.fields.as_slice() else {
        let message = if item.fields.is_empty() {
            "'dialect' needs a value: jass or vjass"
        } else {
            "'dialect' takes a single value: jass or vjass"
        };
        issues.push(at_name(item.key.as_ref().unwrap(), Level::Error, message.into()));
        return None;
    };
    let name = field.text();
    let dialect = jass_rs::Dialect::from_name(name.trim());
    if dialect.is_none() {
        issues.push(at_field(
            field,
            Level::Error,
            format!("unknown JASS dialect '{name}' (expected jass or vjass)"),
        ));
    }
    dialect
}

/// `game=<folder>`: one path, absolute or relative to the project root;
/// each `game=` line adds a folder on top of the earlier ones. A folder
/// that doesn't exist is only a warning: the setting is kept, so a
/// manifest shared between machines still loads.
fn read_game(root: &Path, item: &Item, project: &mut Project, issues: &mut Vec<Issue>) {
    let key = item.key.as_ref().expect("key_of returned a key");
    let [field] = item.fields.as_slice() else {
        let message = if item.fields.is_empty() {
            "'game' needs a value: the Warcraft III folder"
        } else {
            "'game' takes a single path (quote a path that contains a comma; add another 'game=' line for another folder)"
        };
        issues.push(at_name(key, Level::Error, message.into()));
        return;
    };
    let text = field.text();
    let text = text.trim();
    if text.is_empty() {
        issues.push(at_field(field, Level::Error, "'game' needs a value: the Warcraft III folder".into()));
        return;
    }
    let path = resolve(root, text);
    if !path.exists() {
        issues.push(at_field(field, Level::Warning, format!("game folder '{text}' does not exist")));
    } else if !path.is_dir() {
        issues.push(at_field(field, Level::Warning, format!("game path '{text}' is not a folder")));
    }
    project.game.push(path);
}

/// `root=<folder or map>`: one path, absolute or relative to the project
/// root; each `root=` line goes on top of the earlier ones. Everything
/// in it is the project's. One that doesn't exist is only a warning, like
/// a game folder.
fn read_root(root: &Path, item: &Item, project: &mut Project, issues: &mut Vec<Issue>) {
    let key = item.key.as_ref().expect("key_of returned a key");
    let [field] = item.fields.as_slice() else {
        let message = if item.fields.is_empty() {
            "'root' needs a value: a folder or a map (.w3x, .w3m)"
        } else {
            "'root' takes a single path (quote a path that contains a comma; add another 'root=' line for another root)"
        };
        issues.push(at_name(key, Level::Error, message.into()));
        return;
    };
    let text = field.text();
    let text = text.trim();
    if text.is_empty() {
        issues.push(at_field(field, Level::Error, "'root' needs a value: a folder or a map (.w3x, .w3m)".into()));
        return;
    }
    let path = resolve(root, text);
    if project.roots.contains(&path) {
        issues.push(at_field(field, Level::Warning, format!("root '{text}' is already a root above")));
        return;
    }
    if !path.exists() {
        issues.push(at_field(field, Level::Warning, format!("root '{text}' does not exist")));
    } else if path.is_file() && !crate::map::is_archive(&path) {
        issues.push(at_field(
            field,
            Level::Warning,
            format!("root '{text}' is a file but not a map: a root is a folder or a map (.w3x, .w3m)"),
        ));
    }
    project.root_masks.push(Glob::new(root, text));
    project.roots.push(path);
}

/// `[build]`: one build scenario, named by `name=`, unique.
fn read_build(section: &bni_rs::ast::Section, project: &mut Project, issues: &mut Vec<Issue>, places: &mut Vec<(usize, usize, usize)>) {
    let mut name: Option<String> = None;
    let mut build = Build::default();
    // Where a problem of the build is shown: its `out=`, else its header.
    let mut place = (section.name.span.start_line, section.name.span.start_col, section.name.span.end_col);
    for item in &section.items {
        match key_of(item, issues) {
            Some("out") => {
                let key = item.key.as_ref().expect("key_of returned a key");
                match item.fields.as_slice() {
                    [field] if !field.text().trim().is_empty() => {
                        if build.out.is_some() {
                            issues.push(at_name(key, Level::Warning, "'out' set twice; the last one is used".into()));
                        }
                        build.out = Some(resolve(&project.root, field.text().trim()));
                        let at = at_field(field, Level::Error, String::new());
                        place = (at.line, at.col, at.end_col);
                    }
                    _ => issues.push(at_name(key, Level::Error, "'out' needs a single path".into())),
                }
            }
            Some("script") => {
                let key = item.key.as_ref().expect("key_of returned a key");
                match item.fields.as_slice() {
                    [field] => match wc3_build::jass::Script::from_name(field.text().trim()) {
                        Some(s) => build.script = s,
                        None => issues.push(at_field(field, Level::Error, "'script' is jass, lua or as".into())),
                    },
                    _ => issues.push(at_name(key, Level::Error, "'script' is jass, lua or as".into())),
                }
            }
            Some("uglify") => {
                let key = item.key.as_ref().expect("key_of returned a key");
                match item.fields.as_slice() {
                    [field] if field.text().trim() == "1" => build.uglify = true,
                    [field] if field.text().trim() == "0" => build.uglify = false,
                    _ => issues.push(at_name(key, Level::Error, "'uglify' is 1 or 0".into())),
                }
            }
            Some(flag @ ("inline" | "dead_code" | "string_hash")) => {
                let key = item.key.as_ref().expect("key_of returned a key");
                let on = match item.fields.as_slice() {
                    [field] if field.text().trim() == "1" => true,
                    [field] if field.text().trim() == "0" => false,
                    _ => {
                        issues.push(at_name(key, Level::Error, format!("'{flag}' is 1 or 0")));
                        continue;
                    }
                };
                match flag {
                    "inline" => build.inline = on,
                    "dead_code" => build.dead_code = on,
                    _ => build.string_hash = on,
                }
            }
            Some("optimize") => issues.push(at_name(
                item.key.as_ref().expect("key_of returned a key"),
                Level::Warning,
                "'optimize' is each by its name now: inline=1, dead_code=1 (and string_hash=1)".into(),
            )),
            Some("name") => {
                let key = item.key.as_ref().expect("key_of returned a key");
                if name.is_some() {
                    issues.push(at_name(key, Level::Error, "a build has one 'name'".into()));
                    continue;
                }
                let [field] = item.fields.as_slice() else {
                    issues.push(at_name(key, Level::Error, "'name' needs a single value".into()));
                    continue;
                };
                let text = field.text().trim().to_string();
                if text.is_empty() {
                    issues.push(at_field(field, Level::Error, "'name' needs a value".into()));
                } else if project.builds.iter().any(|b| b.name == text) {
                    issues.push(at_field(field, Level::Error, format!("there's already a build named '{text}'")));
                    name = Some(text);
                } else {
                    name = Some(text);
                }
            }
            Some(_) => unknown_key(item, "build", issues),
            None => {}
        }
    }
    match name {
        Some(n) if !project.builds.iter().any(|b| b.name == n) => {
            project.builds.push(Build { name: n, ..build });
            places.push(place);
        }
        Some(_) => {}
        // No name: the default build, one at most.
        None if project.builds.iter().any(|b| b.name.is_empty()) => {
            issues.push(at_name(&section.name, Level::Error, "there's a default build already (a [build] without a 'name'): name this one".into()))
        }
        None => {
            project.builds.push(build);
            places.push(place);
        }
    }
}

fn read_sources(root: &Path, item: &Item, project: &mut Project, issues: &mut Vec<Issue>) {
    for field in &item.fields {
        let text = field.text();
        let text = text.trim();
        if text.is_empty() {
            issues.push(at_field(field, Level::Warning, "empty source path".into()));
            continue;
        }
        if glob::is_mask(text) {
            let mask = Glob::new(root, text);
            if !matches_anything(&mask) {
                issues.push(at_field(field, Level::Warning, format!("source mask '{text}' matches nothing")));
            }
            project.sources.push(mask);
            continue;
        }
        let mask = Glob::new(root, text);
        if !mask.base.exists() {
            issues.push(at_field(
                field,
                Level::Warning,
                format!("source path '{text}' does not exist"),
            ));
        }
        project.sources.push(mask);
    }
}

/// Does anything under the mask's base folder match it?
fn matches_anything(mask: &Glob) -> bool {
    fn walk(dir: &Path, mask: &Glob) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|e| {
            let path = e.path();
            mask.matches(&path)
                || (e.file_type().is_ok_and(|t| t.is_dir()) && !is_hidden(&e.file_name()) && walk(&path, mask))
        })
    }
    walk(&mask.base, mask)
}

/// `exclude=<mask>,...`: what isn't part of the project. A mask that
/// matches nothing isn't reported: there may be nothing to exclude yet.
fn read_exclude(root: &Path, item: &Item, project: &mut Project, issues: &mut Vec<Issue>) {
    if item.fields.is_empty() {
        let key = item.key.as_ref().expect("key_of returned a key");
        issues.push(at_name(key, Level::Warning, "empty exclude mask".into()));
    }
    for field in &item.fields {
        let text = field.text();
        let text = text.trim();
        if text.is_empty() {
            issues.push(at_field(field, Level::Warning, "empty exclude mask".into()));
            continue;
        }
        project.exclude.push(Glob::new(root, text));
    }
}

fn at_name(name: &Name, level: Level, message: String) -> Issue {
    Issue {
        level,
        message,
        line: name.span.start_line,
        col: name.span.start_col,
        end_col: name.span.end_col,
    }
}

fn at_field(field: &Field, level: Level, message: String) -> Issue {
    // An empty field is zero-width; widen it so the editor shows it.
    let end_col = field.span.end_col.max(field.span.start_col + 1);
    Issue {
        level,
        message,
        line: field.span.start_line,
        col: field.span.start_col,
        end_col,
    }
}

/// The manifest of the project `file` belongs to: the nearest `wc3.bni`
/// in the file's folder or any folder above it.
pub fn find_manifest(file: &Path) -> Option<PathBuf> {
    file.parent()?
        .ancestors()
        .map(|dir| dir.join(MANIFEST))
        .find(|manifest| manifest.is_file())
}

/// What identifies one saved version of a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// Loaded projects, keyed by manifest path. A manifest is read again
/// only when its modification time or size changes, so a lookup per
/// request costs a few `stat` calls. Settings come from the manifest as
/// saved on disk, not from unsaved edits in the editor.
#[derive(Default)]
pub struct Projects {
    cache: HashMap<PathBuf, (Stamp, Arc<Project>)>,
}

impl Projects {
    /// The project `file` belongs to, or `None` when there's no
    /// `wc3.bni` above it (or it can't be read).
    pub fn for_file(&mut self, file: &Path) -> Option<Arc<Project>> {
        let manifest = find_manifest(file)?;
        let meta = std::fs::metadata(&manifest).ok()?;
        let stamp = Stamp {
            modified: meta.modified().ok(),
            len: meta.len(),
        };
        if let Some((cached, project)) = self.cache.get(&manifest)
            && *cached == stamp
        {
            return Some(project.clone());
        }
        let src = std::fs::read_to_string(&manifest).ok()?;
        let root = manifest.parent()?.to_path_buf();
        let project = Arc::new(load(&root, &src).0);
        self.cache.insert(manifest, (stamp, project.clone()));
        Some(project)
    }
}

#[cfg(test)]
#[path = "project.test.rs"]
pub(crate) mod tests;
