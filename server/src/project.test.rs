use super::*;
use jass_rs::Dialect;

/// A fresh, empty folder under the system temp dir, removed on drop.
pub(crate) struct TempDir(pub PathBuf);

impl TempDir {
    pub(crate) fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "wc3-server-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reads_dialect_and_sources() {
    let tmp = TempDir::new("reads");
    std::fs::create_dir(tmp.0.join("src")).unwrap();
    let src = "[project]\nsources=src\n\n[jass]\ndialect=vJASS // case-insensitive\n";
    let (project, issues) = load(&tmp.0, src);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.root, tmp.0);
    assert_eq!(project.jass_dialect, Some(Dialect::VJass));
    assert_eq!(project.sources, vec![tmp.0.join("src")]);
}

#[test]
fn empty_manifest_is_a_project_with_defaults() {
    let (project, issues) = load(Path::new("/p"), "");
    assert!(issues.is_empty());
    assert_eq!(project.jass_dialect, None);
    assert!(project.sources.is_empty());
}

#[test]
fn unknown_dialect_is_an_error_at_the_value() {
    let (project, issues) = load(Path::new("/p"), "[jass]\ndialect=cjass\n");
    assert_eq!(project.jass_dialect, None);
    assert_eq!(issues.len(), 1);
    let i = &issues[0];
    assert_eq!(i.level, Level::Error);
    assert_eq!((i.line, i.col, i.end_col), (2, 9, 14));
    assert!(i.message.contains("unknown JASS dialect 'cjass'"));
}

#[test]
fn dialect_needs_exactly_one_value() {
    let (_, issues) = load(Path::new("/p"), "[jass]\ndialect=\n");
    assert!(issues[0].message.contains("needs a value"));
    let (_, issues) = load(Path::new("/p"), "[jass]\ndialect=jass,vjass\n");
    assert!(issues[0].message.contains("single value"));
}

#[test]
fn first_dialect_wins_and_repeats_are_reported() {
    let (project, issues) = load(Path::new("/p"), "[jass]\ndialect=vjass\ndialect=jass\n");
    assert_eq!(project.jass_dialect, Some(Dialect::VJass));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].line, 3);
    assert!(issues[0].message.contains("already set at line 2"));
}

#[test]
fn unknown_sections_and_keys_are_warnings() {
    let src = "[jas]\ndialect=vjass\n[project]\nsource=src\n=x\n";
    let (project, issues) = load(Path::new("/p"), src);
    assert_eq!(project.jass_dialect, None);
    let messages: Vec<_> = issues.iter().map(|i| (i.line, i.message.as_str())).collect();
    assert_eq!(
        messages,
        vec![
            (1, "unknown section [jas] (expected [project], [jass] or [lint])"),
            (4, "unknown setting 'source' in [project] (expected 'sources' or 'game')"),
            (5, "setting without a name"),
        ]
    );
    assert!(issues.iter().all(|i| i.level == Level::Warning));
}

#[test]
fn missing_and_empty_source_paths_are_warnings() {
    let tmp = TempDir::new("missing");
    let (project, issues) = load(&tmp.0, "[project]\nsources=nope,,\"with space\"\n");
    assert_eq!(project.sources.len(), 2);
    let messages: Vec<_> = issues.iter().map(|i| i.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "source path 'nope' does not exist",
            "empty source path",
            "source path 'with space' does not exist",
        ]
    );
}

#[test]
fn finds_the_nearest_manifest_above_a_file() {
    let tmp = TempDir::new("find");
    let nested = tmp.0.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(find_manifest(&nested.join("x.j")), None);

    std::fs::write(tmp.0.join(MANIFEST), "").unwrap();
    assert_eq!(find_manifest(&nested.join("x.j")), Some(tmp.0.join(MANIFEST)));

    // A nested project shadows the outer one for its own files.
    std::fs::write(tmp.0.join("a").join(MANIFEST), "").unwrap();
    assert_eq!(find_manifest(&nested.join("x.j")), Some(tmp.0.join("a").join(MANIFEST)));
}

#[test]
fn projects_reload_a_changed_manifest() {
    let tmp = TempDir::new("reload");
    let file = tmp.0.join("x.j");
    let manifest = tmp.0.join(MANIFEST);
    let mut projects = Projects::default();
    assert_eq!(projects.for_file(&file), None);

    std::fs::write(&manifest, "[jass]\ndialect=vjass\n").unwrap();
    assert_eq!(projects.for_file(&file).unwrap().jass_dialect, Some(Dialect::VJass));

    // A different size is a new stamp even within the same mtime tick.
    std::fs::write(&manifest, "[jass]\n").unwrap();
    assert_eq!(projects.for_file(&file).unwrap().jass_dialect, None);

    std::fs::remove_file(&manifest).unwrap();
    assert_eq!(projects.for_file(&file), None);
}

#[test]
fn source_paths_are_normalized() {
    let (project, _) = load(Path::new("/p"), "[project]\nsources=./src,a/../lib\n");
    assert_eq!(project.sources, vec![PathBuf::from("/p/src"), PathBuf::from("/p/lib")]);
}

#[test]
fn a_project_contains_its_scopes_and_manifest_but_not_hidden_folders() {
    let whole = Project { root: "/p".into(), ..Project::default() };
    assert!(whole.contains(Path::new("/p/a.j")));
    assert!(whole.contains(Path::new("/p/deep/er/a.j")));
    assert!(!whole.contains(Path::new("/p/.git/a.j")));
    assert!(!whole.contains(Path::new("/q/a.j")));

    let src = Project { root: "/p".into(), sources: vec!["/p/src".into()], ..Project::default() };
    assert!(src.contains(Path::new("/p/src/a.j")));
    assert!(src.contains(Path::new("/p/wc3.bni")));
    assert!(!src.contains(Path::new("/p/other/a.j")));
    assert!(!src.contains(Path::new("/p/src/.cache/a.j")));
}

#[test]
fn reads_lint_settings() {
    let src = "[lint]\ninfinite-loop=warning\nunused-local=OFF\nempty-branch=error\n";
    let (project, issues) = load(Path::new("/p"), src);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.lint.get("infinite-loop"), Some(&LintSetting::On(Level::Warning)));
    assert_eq!(project.lint.get("unused-local"), Some(&LintSetting::Off));
    assert_eq!(project.lint.get("empty-branch"), Some(&LintSetting::On(Level::Error)));
}

#[test]
fn lint_settings_are_checked() {
    let src = "[lint]\nunused-locals=off\nunused-local=maybe\nempty-branch=\n";
    let (project, issues) = load(Path::new("/p"), src);
    assert!(project.lint.is_empty());
    let got: Vec<_> = issues.iter().map(|i| (i.line, i.level, i.message.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (2, Level::Warning, "unknown lint rule 'unused-locals' (the rules are listed in the wc3-rs README)"),
            (3, Level::Error, "unknown lint setting 'maybe' (expected off, warning or error)"),
            (4, Level::Error, "'empty-branch' needs one value: off, warning or error"),
        ]
    );
}

#[test]
fn lint_defaults_and_project_settings_apply() {
    use parser_rs::{rule, Diagnostic};
    let diags = || {
        vec![
            Diagnostic::warning(rule::INFINITE_LOOP, "loop", 1),
            Diagnostic::warning(rule::UNUSED_LOCAL, "unused", 2),
            Diagnostic::warning(rule::EMPTY_BRANCH, "empty", 3),
        ]
    };
    // No project: only the default-off rule goes.
    let rules: Vec<_> = apply_lint(diags(), None).iter().map(|d| d.rule).collect();
    assert_eq!(rules, vec![rule::UNUSED_LOCAL, rule::EMPTY_BRANCH]);
    // A project turns it on, another rule off, and re-levels a third.
    let (project, _) = load(Path::new("/p"), "[lint]\ninfinite-loop=warning\nunused-local=off\nempty-branch=error\n");
    let got: Vec<_> = apply_lint(diags(), Some(&project)).iter().map(|d| (d.rule, d.level)).collect();
    assert_eq!(got, vec![(rule::INFINITE_LOOP, Level::Warning), (rule::EMPTY_BRANCH, Level::Error)]);
}

#[test]
fn reads_the_game_folders_in_order() {
    let tmp = TempDir::new("game");
    std::fs::create_dir_all(tmp.0.join("wc3")).unwrap();
    std::fs::create_dir_all(tmp.0.join("over")).unwrap();
    let abs = tmp.0.join("wc3");
    let src = format!("[project]\ngame=\"{}\"\ngame=./over\n", abs.display());
    let (project, issues) = load(&tmp.0, &src);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.game, vec![abs, tmp.0.join("over")]);
}

#[test]
fn game_checks() {
    let tmp = TempDir::new("game-checks");
    std::fs::write(tmp.0.join("file"), "").unwrap();

    let (project, issues) = load(&tmp.0, "[project]\ngame=nope\n");
    assert_eq!(project.game, vec![tmp.0.join("nope")]);
    assert_eq!(issues[0].level, Level::Warning);
    assert_eq!((issues[0].line, issues[0].col, issues[0].end_col), (2, 6, 10));
    assert!(issues[0].message.contains("game folder 'nope' does not exist"));

    let (_, issues) = load(&tmp.0, "[project]\ngame=file\n");
    assert!(issues[0].message.contains("is not a folder"));

    let (project, issues) = load(&tmp.0, "[project]\ngame=\n");
    assert!(project.game.is_empty());
    assert_eq!(issues[0].level, Level::Error);
    assert!(issues[0].message.contains("needs a value"));

    let (_, issues) = load(&tmp.0, "[project]\ngame=a,b\n");
    assert!(issues[0].message.contains("single path"));
}
