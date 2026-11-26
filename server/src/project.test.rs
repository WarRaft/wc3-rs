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
    let src = "[code]\nsources=src\n[jass]\ndialect=vJASS // case-insensitive\n";
    let (project, issues) = load(&tmp.0, src);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.root, tmp.0);
    assert_eq!(project.jass_dialect, Some(Dialect::VJass));
    assert_eq!(project.scopes(), vec![tmp.0.join("src")]);
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
            (1, "unknown section [jas] (expected [project], [code], [map], [build], [jass], [lint] or [external])"),
            (4, "unknown setting 'source' in [project] (expected 'game', 'hd', 'de' or 'locale')"),
            (5, "setting without a name"),
        ]
    );
    assert!(issues.iter().all(|i| i.level == Level::Warning));
}

#[test]
fn missing_and_empty_source_paths_are_warnings() {
    let tmp = TempDir::new("missing");
    let (project, issues) = load(&tmp.0, "[code]\nsources=nope,,\"with space\"\n");
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
    let (project, _) = load(Path::new("/p"), "[code]\nsources=./src,a/../lib\n");
    assert_eq!(project.scopes(), vec![PathBuf::from("/p/src"), PathBuf::from("/p/lib")]);
}

#[test]
fn a_project_contains_its_scopes_and_manifest_but_not_hidden_folders() {
    let (whole, _) = load(Path::new("/p"), "[code]\nsources=.\n");
    assert!(whole.contains(Path::new("/p/a.j")));
    assert!(whole.contains(Path::new("/p/deep/er/a.j")));
    assert!(whole.contains(Path::new("/p/m/Map.w3x/war3map.j")), "a map is a folder like any other");
    assert!(!whole.contains(Path::new("/p/.git/a.j")));
    assert!(!whole.contains(Path::new("/q/a.j")));

    let (src, _) = load(Path::new("/p"), "[code]\nsources=src\n");
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
            (2, Level::Warning, "unknown lint rule or group 'unused-locals' (they are listed in the wc3-rs README)"),
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
fn a_group_switches_its_rules_and_a_rule_setting_wins() {
    use parser_rs::{rule, Diagnostic};
    let diags = || {
        vec![
            Diagnostic::error(rule::IMPLICIT_SIGNATURE, "implicit", 1),
            Diagnostic::warning(rule::UNUSED_LOCAL, "unused", 2),
        ]
    };
    // Off by default.
    let rules: Vec<_> = apply_lint(diags(), None).iter().map(|d| d.rule).collect();
    assert_eq!(rules, vec![rule::UNUSED_LOCAL]);

    let (project, issues) = load(Path::new("/p"), "[lint]
pure=warning
");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.lint_groups.get("pure"), Some(&LintSetting::On(Level::Warning)));
    assert!(project.lint.is_empty());
    let got: Vec<_> = apply_lint(diags(), Some(&project)).iter().map(|d| (d.rule, d.level)).collect();
    assert_eq!(got, vec![(rule::IMPLICIT_SIGNATURE, Level::Warning), (rule::UNUSED_LOCAL, Level::Warning)]);

    // The rule's own setting wins, before or after the group.
    for src in ["[lint]
implicit-signature=off
pure=error
", "[lint]
pure=error
implicit-signature=off
"] {
        let (project, _) = load(Path::new("/p"), src);
        let rules: Vec<_> = apply_lint(diags(), Some(&project)).iter().map(|d| d.rule).collect();
        assert_eq!(rules, vec![rule::UNUSED_LOCAL], "{src}");
    }
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

#[test]
fn backslashes_separate_path_parts() {
    let tmp = TempDir::new("backslash");
    std::fs::create_dir_all(tmp.0.join("a/b")).unwrap();
    let (project, issues) = load(&tmp.0, "[project]\ngame=.\\a\n[code]\nsources=a\\b\n");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(project.scopes(), vec![tmp.0.join("a/b")]);
    assert_eq!(project.game, vec![tmp.0.join("a")]);
}

#[test]
fn source_masks_and_excludes() {
    let tmp = TempDir::new("masks");
    for dir in ["src/old", "maps"] {
        std::fs::create_dir_all(tmp.0.join(dir)).unwrap();
    }
    for f in ["src/a.j", "src/old/b.j", "maps/A.w3x", "test.j", "c.j"] {
        std::fs::write(tmp.0.join(f), "").unwrap();
    }
    let src = "[code]\nsources=src,maps/*.w3x,nope/*.j,test.j\nexclude=**/OLD/**,test.j\nexclude=\n";
    let (project, issues) = load(&tmp.0, src);
    let got: Vec<_> = issues.iter().map(|i| (i.line, i.message.as_str())).collect();
    assert_eq!(got, vec![(2, "source mask 'nope/*.j' matches nothing"), (4, "empty exclude mask")]);
    assert_eq!(project.sources.len(), 4);
    assert_eq!(project.exclude.len(), 2);
    assert_eq!(project.scopes(), vec![tmp.0.join("src"), tmp.0.join("maps"), tmp.0.join("nope"), tmp.0.join("test.j")]);

    let has = |rel: &str| project.contains(&tmp.0.join(rel));
    assert!(has("src/a.j"));
    assert!(!has("src/old/b.j"), "excluded");
    assert!(has("maps/A.w3x/war3map.j"), "inside a map the mask matches");
    assert!(!has("maps/other.j"), "under a mask's folder but not matching it");
    assert!(!has("test.j"), "exclude wins over sources");
    assert!(!has("c.j"));
    assert!(has("wc3.bni"));
}

#[test]
fn without_sources_a_project_has_only_its_manifest() {
    let (project, issues) = load(Path::new("/p"), "[code]\nexclude=test.j\n");
    assert!(issues.is_empty(), "{issues:?}");
    assert!(project.contains(Path::new("/p/wc3.bni")));
    assert!(!project.contains(Path::new("/p/a.j")));
    assert!(!project.contains(Path::new("/p/m/Map.w3x/war3map.j")));
    assert!(project.scopes().is_empty());
}

#[test]
fn every_source_is_a_mask_and_covers_what_is_inside_what_it_matches() {
    for sources in ["sources=.", "sources=m", "sources=M/map.W3X", "sources=**/*.w3x", "sources=m/*"] {
        let (project, _) = load(Path::new("/p"), &format!("[code]\n{sources}\n"));
        assert!(project.contains(Path::new("/p/m/Map.w3x/war3map.j")), "{sources}");
    }
    let (project, _) = load(Path::new("/p"), "[code]\nsources=**/*.j\nexclude=**/*.w3x\n");
    assert!(project.contains(Path::new("/p/a/b.j")));
    assert!(!project.contains(Path::new("/p/m/Map.w3x/war3map.j")));
}


#[test]
fn reads_the_map_roots_in_order_and_they_are_sources() {
    let tmp = TempDir::new("roots");
    std::fs::create_dir_all(tmp.0.join("map/Units")).unwrap();
    std::fs::create_dir_all(tmp.0.join("icons")).unwrap();
    std::fs::write(tmp.0.join("base.W3X"), "").unwrap();
    std::fs::write(tmp.0.join("notes.txt"), "").unwrap();
    let src = "[map]\nroot=base.W3X\nroot=./icons\nroot=map\nroot=nope\nroot=notes.txt\nroot=map\nroot=\nroot=a,b\nsize=1\n[map]\n";
    let (project, issues) = load(&tmp.0, src);
    assert_eq!(
        project.roots,
        ["base.W3X", "icons", "map", "nope", "notes.txt"].map(|r| tmp.0.join(r)).to_vec()
    );
    let got: Vec<(usize, Level, &str)> = issues.iter().map(|i| (i.line, i.level, i.message.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (5, Level::Warning, "root 'nope' does not exist"),
            (6, Level::Warning, "root 'notes.txt' is a file but not a map: a root is a folder or a map (.w3x, .w3m)"),
            (7, Level::Warning, "root 'map' is already a root above"),
            (8, Level::Error, "'root' needs a value: a folder or a map (.w3x, .w3m)"),
            (9, Level::Error, "'root' takes a single path (quote a path that contains a comma; add another 'root=' line for another root)"),
            (10, Level::Warning, "unknown setting 'size' in [map] (expected 'root')"),
            (11, Level::Warning, "[map] is already at line 1: a project is one map, so these roots are added to it"),
        ]
    );
    // What's in a root is the project's.
    assert!(project.contains(&tmp.0.join("map/Units/UnitFunc.txt")));
    assert!(project.contains(&tmp.0.join("base.W3X/war3map.j")));
    assert!(!project.contains(&tmp.0.join("other.j")));
}

#[test]
fn builds_have_unique_names() {
    let src = "[build]\nname=debug\n[build]\nname=release\nname=x\n[build]\nname=debug\n[build]\nfast=1\n[build]\nuglify=1\n";
    let (project, issues) = load(Path::new("/p"), src);
    // The one without a name is the default; a second one is refused.
    assert_eq!(project.builds.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["debug", "release", ""]);
    let got: Vec<(usize, &str)> = issues.iter().map(|i| (i.line, i.message.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (5, "a build has one 'name'"),
            (7, "there's already a build named 'debug'"),
            (9, "unknown setting 'fast' in [build] (expected 'name', 'out', 'script', 'uglify', 'inline', 'dead_code' or 'string_hash')"),
            (10, "there's a default build already (a [build] without a 'name'): name this one"),
        ]
    );
    // Asked for by no name: the default; by its name, that one.
    assert_eq!(project.build_named(None).unwrap().name, "");
    assert_eq!(project.build_named(Some("release")).unwrap().name, "release");
    assert!(project.build_named(Some("nope")).is_none());
    assert_eq!(project.builds[2].out(&project), PathBuf::from("/p/build/war3map.j"));
    // No default: the first.
    let (two, _) = load(Path::new("/p"), "[build]\nname=a\n[build]\nname=b\n");
    assert_eq!(two.build_named(None).unwrap().name, "a");
}

#[test]
fn a_builds_map_is_written_outside_the_roots_and_the_code() {
    let tmp = TempDir::new("build-out");
    std::fs::create_dir_all(tmp.0.join("map")).unwrap();
    std::fs::create_dir_all(tmp.0.join("src")).unwrap();
    let (_, issues) = load(&tmp.0, "[map]\nroot=map\n[code]\nsources=src\n[build]\nname=a\nout=map/a.w3x\n[build]\nname=b\nout=src/b.w3x\n[build]\nname=c\n");
    let errors: Vec<(usize, &str)> = issues.iter().filter(|i| i.level == Level::Error).map(|i| (i.line, i.message.as_str())).collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].0, 7);
    assert!(errors[0].1.starts_with("build 'a' writes ") && errors[0].1.contains("inside the map's root"), "{errors:?}");
    assert_eq!(errors[1].0, 10);
    assert!(errors[1].1.contains("among the project's code"), "{errors:?}");
    // The project's folder as a root: the default place is inside it.
    let (_, issues) = load(&tmp.0, "[map]\nroot=.\n[build]\n");
    assert!(issues.iter().any(|i| i.message.starts_with("the default build writes")), "{:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
}

#[test]
fn code_settings_belong_in_code() {
    let tmp = TempDir::new("code-section");
    std::fs::create_dir_all(tmp.0.join("src")).unwrap();
    let (project, issues) = load(&tmp.0, "[project]\nsources=src\nexclude=x\n[code]\nsources=src\nexclude=old\ngame=x\n");
    let got: Vec<(usize, Level, &str)> = issues.iter().map(|i| (i.line, i.level, i.message.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (2, Level::Error, "'sources' is code's: it goes in [code] (the map's files are its [map] root= lines)"),
            (3, Level::Error, "'exclude' is code's: it goes in [code] (the map's files are its [map] root= lines)"),
            (7, Level::Warning, "unknown setting 'game' in [code] (expected 'sources' or 'exclude')"),
        ]
    );
    assert_eq!(project.sources.len(), 1);
    assert_eq!(project.exclude.len(), 1);
}

#[test]
fn exclude_is_for_code_and_leaves_the_roots_alone() {
    let (project, _) = load(Path::new("/p"), "[code]\nsources=src\nexclude=**/old/**\n[map]\nroot=map\n");
    assert!(!project.contains(Path::new("/p/src/old/a.j")));
    assert!(project.contains(Path::new("/p/map/old/a.j")));
    assert!(!project.contains(Path::new("/p/maps/Kept.w3x/war3map.j")), "a map neither code nor root isn't the project's");
}

#[test]
fn a_build_says_where_and_how() {
    let src = "[build]\nname=release\nout=out/map.w3x\nuglify=1\ninline=1\ndead_code=1\nstring_hash=1\n[build]\nname=debug\nuglify=true\ninline=yes\noptimize=1\n";
    let (project, issues) = load(Path::new("/p"), src);
    assert_eq!(project.builds[0], Build { name: "release".into(), out: Some(PathBuf::from("/p/out/map.w3x")), uglify: true, inline: true, dead_code: true, string_hash: true, script: Default::default() });
    assert_eq!(project.builds[1].out(&project), PathBuf::from("/p/build/war3map.j"));
    assert_eq!(issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>(), ["'uglify' is 1 or 0", "'inline' is 1 or 0", "'optimize' is each by its name now: inline=1, dead_code=1 (and string_hash=1)"]);
}

#[test]
fn externals_name_their_programs() {
    let (p, issues) = load(Path::new("/proj"), "[external]\nObjectMerger=\"wine\",\"tools/ObjectMerger.exe\"\nLocal=\"bin/merge\"\nEmpty=\nObjectMerger=\"other\"\n");
    // A program with a path in the project's folder; one by name as it is.
    assert_eq!(p.externals, [
        ("ObjectMerger".to_string(), vec!["wine".to_string(), "tools/ObjectMerger.exe".to_string()]),
        ("Local".to_string(), vec![Path::new("/proj").join("bin/merge").to_string_lossy().into_owned()]),
    ]);
    let said: Vec<(usize, &str)> = issues.iter().map(|i| (i.line, i.message.as_str())).collect();
    assert_eq!(said, [
        (4, "'Empty' needs its program: Empty=\"path/to/tool\" (and arguments, comma-separated)"),
        (5, "'ObjectMerger' is named already: the first is run"),
    ]);
}

#[test]
fn a_reforged_games_hd_and_locale() {
    let (p, issues) = load(Path::new("/p"), "[project]\nhd=1\nlocale=enUS\n");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!((p.hd, p.locale.as_deref()), (true, Some("enUS")));
    let (p, _) = load(Path::new("/p"), "[project]\n");
    assert_eq!((p.hd, p.locale), (false, None));
    let (p, _) = load(Path::new("/p"), "[project]\nde=1\n");
    assert_eq!((p.hd, p.de), (false, true));
    let (_, issues) = load(Path::new("/p"), "[project]\nhd=yes\nde=true\nlocale=russian\n");
    assert_eq!(issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>(), ["'hd' is 1 or 0", "'de' is 1 or 0", "'locale' is a locale's four letters: ruRU, enUS ..."]);
}
