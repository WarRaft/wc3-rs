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
            (1, "unknown section [jas] (expected [project] or [jass])"),
            (4, "unknown setting 'source' in [project] (expected 'sources')"),
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
