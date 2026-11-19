use super::*;
use crate::project::tests::TempDir;
use parser_rs::Level;

/// A project on disk: `files` are (relative path, text).
fn project(name: &str, files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new(name);
    for (rel, text) in files {
        let path = tmp.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    tmp
}

/// id → (version, error count, token count) of every report.
fn reports(ws: &mut Workspace) -> Vec<(String, i32, usize, usize)> {
    ws.take_reports()
        .into_iter()
        .map(|msg| {
            let ServerMsg::File { id, version, diagnostics, tokens } = msg else {
                panic!("only file reports");
            };
            let errors = diagnostics.iter().filter(|d| d.level == Level::Error).count();
            (id, version, errors, tokens.len())
        })
        .collect()
}

/// The ids of `reports` relative to `root`, for short assertions.
fn names(root: &Path, reports: &[(String, i32, usize, usize)]) -> Vec<String> {
    reports
        .iter()
        .map(|r| Path::new(&r.0).strip_prefix(root).map_or(r.0.clone(), |p| p.to_string_lossy().into_owned()))
        .collect()
}

const OK: &str = "function F takes nothing returns nothing\nendfunction";
const BAD: &str = "function";

fn init(root: &Path) -> Workspace {
    let mut ws = Workspace::default();
    ws.set_roots(vec![root.to_path_buf()]);
    ws
}

#[test]
fn loads_the_whole_root_when_there_are_no_sources() {
    let tmp = project("ws-whole", &[
        ("wc3.bni", ""),
        ("a.j", OK),
        ("deep/b.zn", "library L {}"),
        ("notes.txt", "not code"),
        (".git/c.j", BAD),
    ]);
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["a.j", "deep/b.zn", "wc3.bni"]);
    assert!(r.iter().all(|(_, version, errors, tokens)| *version == 0 && *errors == 0 && *tokens == 0));
}

#[test]
fn sources_limit_the_files_and_the_manifest_is_always_one() {
    let tmp = project("ws-sources", &[
        ("wc3.bni", "[project]\nsources=src\n"),
        ("src/a.j", OK),
        ("src/.cache/x.j", BAD),
        ("tools/b.j", BAD),
    ]);
    let mut ws = init(&tmp.0);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["src/a.j", "wc3.bni"]);
}

#[test]
fn a_nested_project_owns_its_files() {
    let tmp = project("ws-nested", &[
        ("wc3.bni", ""),
        ("a.j", OK),
        ("inner/wc3.bni", "[jass]\ndialect=vjass\n"),
        ("inner/b.j", "globals\n    integer library = 1\nendglobals"),
    ]);
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["a.j", "inner/b.j", "inner/wc3.bni", "wc3.bni"]);
    // b.j is read with the inner project's vJASS.
    assert_eq!(r[1].2, 1);
}

#[test]
fn files_without_a_project_are_not_loaded() {
    let tmp = project("ws-none", &[("a.j", BAD)]);
    let mut ws = init(&tmp.0);
    assert!(reports(&mut ws).is_empty());
}

#[test]
fn opening_a_file_from_an_unknown_project_loads_that_project() {
    let tmp = project("ws-open", &[("wc3.bni", ""), ("a.j", OK), ("b.j", BAD)]);
    let mut ws = Workspace::default(); // no workspace folders at all
    let a = tmp.0.join("a.j");
    ws.open(path_id(&a), Some(a.clone()), Language::Jass, 3, OK.into());
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["a.j", "b.j", "wc3.bni"]);
    // The open one has the editor's version and highlighting.
    assert_eq!((r[0].1, r[0].3 > 0), (3, true));
    assert_eq!((r[1].1, r[1].2, r[1].3), (0, 1, 0));
}

#[test]
fn closing_a_project_file_reverts_it_to_disk() {
    let tmp = project("ws-close", &[("wc3.bni", ""), ("a.j", OK)]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let a = tmp.0.join("a.j");
    ws.open(path_id(&a), Some(a.clone()), Language::Jass, 2, BAD.into());
    assert_eq!(reports(&mut ws), vec![(path_id(&a), 2, 1, 5)]);
    ws.close(&path_id(&a));
    // Still reported, from disk now: no error, no tokens.
    assert_eq!(reports(&mut ws), vec![(path_id(&a), 0, 0, 0)]);
}

#[test]
fn closing_anything_else_clears_it() {
    let mut ws = Workspace::default();
    ws.open("untitled:1".into(), None, Language::Jass, 1, BAD.into());
    reports(&mut ws);
    ws.close("untitled:1");
    assert_eq!(reports(&mut ws), vec![("untitled:1".into(), 0, 0, 0)]);
    assert!(ws.is_idle());
}

#[test]
fn disk_changes_update_add_and_drop_files() {
    let tmp = project("ws-disk", &[("wc3.bni", ""), ("a.j", OK)]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let (a, b) = (tmp.0.join("a.j"), tmp.0.join("b.j"));

    std::fs::write(&a, BAD).unwrap();
    std::fs::write(&b, OK).unwrap();
    ws.disk_changed(&[a.clone(), b.clone(), tmp.0.join("x.txt")]);
    assert_eq!(reports(&mut ws), vec![(path_id(&a), 0, 1, 0), (path_id(&b), 0, 0, 0)]);

    std::fs::remove_file(&a).unwrap();
    ws.disk_changed(std::slice::from_ref(&a));
    assert_eq!(reports(&mut ws), vec![(path_id(&a), 0, 0, 0)]);
    assert!(ws.is_idle());
}

#[test]
fn the_editor_text_wins_over_the_disk_while_open() {
    let tmp = project("ws-open-wins", &[("wc3.bni", ""), ("a.j", OK)]);
    let mut ws = init(&tmp.0);
    let a = tmp.0.join("a.j");
    ws.open(path_id(&a), Some(a.clone()), Language::Jass, 5, OK.into());
    reports(&mut ws);
    std::fs::write(&a, BAD).unwrap();
    ws.disk_changed(std::slice::from_ref(&a));
    assert!(ws.is_idle());
}

#[test]
fn a_manifest_change_reloads_projects_and_re_analyzes_everything() {
    let tmp = project("ws-manifest", &[
        ("wc3.bni", ""),
        ("src/a.j", "globals\n    integer library = 1\nendglobals"),
        ("tools/b.j", OK),
    ]);
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["src/a.j", "tools/b.j", "wc3.bni"]);
    assert_eq!(r[0].2, 0);

    // Narrow the sources and switch to vJASS.
    let manifest = tmp.0.join("wc3.bni");
    std::fs::write(&manifest, "[project]\nsources=src\n[jass]\ndialect=vjass\n").unwrap();
    ws.disk_changed(&[manifest]);
    let r = reports(&mut ws);
    // tools/b.j left (cleared, reported first); src/a.j is now an error.
    assert_eq!(names(&tmp.0, &r), vec!["tools/b.j", "src/a.j", "wc3.bni"]);
    assert_eq!(r[1].2, 1);
}

#[test]
fn sources_outside_the_root_are_loaded_and_watched() {
    let tmp = project("ws-outside", &[
        ("game/wc3.bni", "[project]\nsources=src,../shared\n"),
        ("game/src/a.j", OK),
        ("shared/lib.j", OK),
    ]);
    let game = tmp.0.join("game");
    let mut ws = init(&game);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["game/src/a.j", "game/wc3.bni", "shared/lib.j"]);
    assert_eq!(ws.watch_paths(), vec![game.clone(), tmp.0.join("shared")]);

    // A change there is picked up, not mistaken for a stray file.
    let lib = tmp.0.join("shared/lib.j");
    std::fs::write(&lib, BAD).unwrap();
    ws.disk_changed(std::slice::from_ref(&lib));
    assert_eq!(reports(&mut ws), vec![(path_id(&lib), 0, 1, 0)]);
}

#[test]
fn legacy_encodings_are_read_lossily() {
    let tmp = project("ws-latin1", &[("wc3.bni", "")]);
    std::fs::write(tmp.0.join("a.j"), b"// caf\xe9\nfunction F takes nothing returns nothing\nendfunction").unwrap();
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["a.j", "wc3.bni"]);
    assert_eq!(r[0].2, 0);
}

#[test]
fn many_files_are_analyzed_in_parallel_in_a_stable_order() {
    let files: Vec<(String, &str)> = (0..40).map(|i| (format!("f{i:02}.j"), if i % 2 == 0 { OK } else { BAD })).collect();
    let refs: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let mut all = vec![("wc3.bni", "")];
    all.extend(refs);
    let tmp = project("ws-many", &all);
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(r.len(), 41);
    for (i, report) in r.iter().take(40).enumerate() {
        assert!(report.0.ends_with(&format!("f{i:02}.j")));
        assert_eq!(report.2, i % 2);
    }
}
