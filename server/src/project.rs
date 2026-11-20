//! Projects. A folder with a `wc3.bni` file in it is a project, the way a
//! folder with `Cargo.toml` is a Rust package: every file in that folder
//! and below belongs to it and is read with its settings. The manifest is
//! an ordinary BNI file, parsed by `bni-rs`:
//!
//! ```ini
//! [project]
//! sources=src,lib,maps/*.w3x
//! exclude=**/old/**,test.j
//! game=C:/Games/Warcraft III
//! game=../my-common-j
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
    /// `[project] sources=`: the project's source folders and files,
    /// resolved against `root`, in the order written. Masks are in
    /// `source_masks`.
    pub sources: Vec<PathBuf>,
    /// `[project] sources=` masks (`maps/*.w3x`): the files they match
    /// (see `glob`) are sources too.
    pub source_masks: Vec<Glob>,
    /// `[project] exclude=`: masks of what isn't part of the project, even
    /// inside its sources.
    pub exclude: Vec<Glob>,
    /// `[project] game=`: the game folders, resolved against `root` (an
    /// absolute path stays as written), in the order written. A file in a
    /// later folder overrides the same file in an earlier one; see `game`.
    pub game: Vec<PathBuf>,
    /// `[lint] <rule>=off|warning|error`: rules switched or re-levelled
    /// from their defaults (`parser_rs::RULES`), by rule id.
    pub lint: HashMap<String, LintSetting>,
    /// `[lint] <group>=off|warning|error`: groups of rules
    /// (`parser_rs::GROUPS`), by group id. A rule set by name wins over
    /// its groups.
    pub lint_groups: HashMap<String, LintSetting>,
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

    /// Where the project's files are: its `sources` and the folders its
    /// source masks look in, or the whole root when it lists none.
    pub fn scopes(&self) -> Vec<PathBuf> {
        if self.sources.is_empty() && self.source_masks.is_empty() {
            return vec![self.root.clone()];
        }
        let mut scopes = self.sources.clone();
        for mask in &self.source_masks {
            if !scopes.contains(&mask.base) {
                scopes.push(mask.base.clone());
            }
        }
        scopes
    }

    /// Is `path` one of the project's files: the manifest itself, or a
    /// file inside one of its sources (or matching a source mask), not
    /// inside a hidden (`.name`) folder below it, and not excluded.
    /// Doesn't check which project is nearest; see [`find_manifest`].
    pub fn contains(&self, path: &Path) -> bool {
        if path == self.manifest() {
            return true;
        }
        let visible = |scope: &Path| {
            path.strip_prefix(scope).is_ok_and(|rel| {
                !rel.parent()
                    .is_some_and(|dirs| dirs.components().any(|c| is_hidden(c.as_os_str())))
            })
        };
        let listed = if self.sources.is_empty() && self.source_masks.is_empty() {
            visible(&self.root)
        } else {
            self.sources.iter().any(|s| visible(s))
                || self.source_masks.iter().any(|m| visible(&m.base) && m.matches(path))
        };
        listed && !self.exclude.iter().any(|m| m.matches(path))
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
    let mut project = Project {
        root: root.to_path_buf(),
        ..Project::default()
    };
    let mut issues = Vec::new();
    let mut dialect_line: Option<usize> = None;

    for section in &doc.sections {
        let items = &section.items;
        match section.name.text.as_str() {
            "project" => {
                for item in items {
                    match key_of(item, &mut issues) {
                        Some("sources") => read_sources(root, item, &mut project, &mut issues),
                        Some("exclude") => read_exclude(root, item, &mut project, &mut issues),
                        Some("game") => read_game(root, item, &mut project, &mut issues),
                        Some(_) => unknown_key(item, "project", &mut issues),
                        None => {}
                    }
                }
            }
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
            // An empty name is already a bni-rs lint warning.
            "" => {}
            other => issues.push(at_name(
                &section.name,
                Level::Warning,
                format!("unknown section [{other}] (expected [project], [jass] or [lint])"),
            )),
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
        "project" => "sources', 'exclude' or 'game",
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
            project.source_masks.push(mask);
            continue;
        }
        let path = resolve(root, text);
        if !path.exists() {
            issues.push(at_field(
                field,
                Level::Warning,
                format!("source path '{text}' does not exist"),
            ));
        }
        project.sources.push(path);
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
        if let Some((cached, project)) = self.cache.get(&manifest) {
            if *cached == stamp {
                return Some(project.clone());
            }
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
