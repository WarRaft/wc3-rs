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

#[test]
fn scripts_inside_maps_are_project_files() {
    let tmp = project("ws-maps", &[("wc3.bni", "[project]\nsources=.\n"), ("a.j", OK), (".hidden/x.j", BAD)]);
    let map = tmp.0.join("Test.w3x");
    crate::map::tests::make_map(&map, &[("war3map.j", BAD), ("Scripts\\lib.j", OK), ("war3map.w3i", "x")]);
    std::fs::write(tmp.0.join(".hidden/Hidden.w3x"), "").unwrap();
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(
        names(&tmp.0, &r),
        vec!["Test.w3x/Scripts/lib.j", "Test.w3x/war3map.j", "a.j", "wc3.bni"]
    );
    assert_eq!(r[1].2, 1, "war3map.j is analyzed: {r:?}");
    let script = map.join("war3map.j");
    assert_eq!(ws.file(&path_id(&script)).unwrap().0.path.as_deref(), Some(script.as_path()));

    // Opened and closed: back to the map's text.
    ws.open(path_id(&script), Some(script.clone()), Language::Jass, 1, OK.into());
    assert_eq!(reports(&mut ws)[0].2, 0);
    ws.close(&path_id(&script));
    assert_eq!(reports(&mut ws), vec![(path_id(&script), 0, 1, 0)]);

    // The map is saved again: its files are read again, gone ones dropped.
    crate::map::tests::make_map(&map, &[("war3map.j", OK)]);
    ws.disk_changed(std::slice::from_ref(&map));
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["Test.w3x/Scripts/lib.j", "Test.w3x/war3map.j"]);
    assert_eq!((r[1].1, r[1].2), (0, 0));
    assert!(ws.file(&path_id(&map.join("Scripts/lib.j"))).is_none());

    // A manifest change keeps the map's files without reading it again.
    std::fs::write(tmp.0.join("wc3.bni"), "[project]\nsources=.\n[jass]\ndialect=vjass\n").unwrap();
    ws.disk_changed(&[tmp.0.join("wc3.bni")]);
    assert_eq!(
        names(&tmp.0, &reports(&mut ws)),
        vec!["Test.w3x/war3map.j", "a.j", "wc3.bni"]
    );

    // The map is deleted.
    std::fs::remove_file(&map).unwrap();
    ws.disk_changed(std::slice::from_ref(&map));
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["Test.w3x/war3map.j"]);
    assert!(ws.file(&path_id(&script)).is_none());
}

#[test]
fn sources_can_name_a_map() {
    let tmp = project("ws-map-sources", &[("wc3.bni", "[project]\nsources=Main.w3m\n"), ("a.j", OK)]);
    crate::map::tests::make_map(&tmp.0.join("Main.w3m"), &[("war3map.j", OK)]);
    crate::map::tests::make_map(&tmp.0.join("Other.w3x"), &[("war3map.j", OK)]);
    let mut ws = init(&tmp.0);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["Main.w3m/war3map.j", "wc3.bni"]);
}

#[test]
fn excluded_files_and_map_scripts_leave_the_model() {
    let tmp = project("ws-exclude", &[
        ("wc3.bni", "[project]\nsources=.\nexclude=test.j,**/old/**\n"),
        ("a.j", OK),
        ("test.j", BAD),
        ("lib/old/b.j", BAD),
    ]);
    crate::map::tests::make_map(&tmp.0.join("Map.w3x"), &[("war3map.j", OK), ("war3map.lua", "local x = 1\n")]);
    let mut ws = init(&tmp.0);
    assert_eq!(
        names(&tmp.0, &reports(&mut ws)),
        vec!["Map.w3x/war3map.j", "Map.w3x/war3map.lua", "a.j", "wc3.bni"]
    );

    // Excluding a map's script drops just that file.
    std::fs::write(tmp.0.join("wc3.bni"), "[project]\nsources=.\nexclude=test.j,**/old/**,**/war3map.lua\n").unwrap();
    ws.disk_changed(&[tmp.0.join("wc3.bni")]);
    let r = reports(&mut ws);
    assert_eq!(r[0], (path_id(&tmp.0.join("Map.w3x/war3map.lua")), 0, 0, 0), "cleared first: {r:?}");
    assert!(ws.file(&path_id(&tmp.0.join("Map.w3x/war3map.lua"))).is_none());

    // Source masks: only the matching maps.
    std::fs::write(tmp.0.join("wc3.bni"), "[project]\nsources=*.w3x\n").unwrap();
    ws.disk_changed(&[tmp.0.join("wc3.bni")]);
    reports(&mut ws);
    let mut ids: Vec<_> = ws.all_files().map(|(id, _, _)| id.to_string()).collect();
    ids.sort();
    assert_eq!(
        names(&tmp.0, &ids.into_iter().map(|id| (id, 0, 0, 0)).collect::<Vec<_>>()),
        vec!["Map.w3x/war3map.j", "Map.w3x/war3map.lua", "wc3.bni"]
    );
}

#[test]
fn game_scripts_are_seen_by_their_projects_only() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const COMMON: &str = "native CreateTimer takes nothing returns timer\n";
    const BLIZZARD: &str = "function BJDebugMsg takes string msg returns nothing\n    local integer unused\nendfunction\n";
    const USE: &str = "function F takes nothing returns nothing\n    call BJDebugMsg(\"x\")\n    call CreateTimer()\nendfunction\n";

    let tmp = project("ws-game", &[
        ("game/wc3/README", ""),
        ("game/ujapi/Scripts/common.j", "native CreateTimer takes nothing returns timer\nnative UjapiOnly takes nothing returns nothing\n"),
        ("a/wc3.bni", "[project]\ngame=../game/wc3\n"),
        ("a/main.j", USE),
        ("b/wc3.bni", "[project]\ngame=../game/wc3\ngame=../game/ujapi\n"),
        ("b/main.j", USE),
    ]);
    let patch = tmp.0.join("game/wc3/War3Patch.mpq");
    crate::map::tests::make_map(&patch, &[("Scripts\\common.j", COMMON), ("Scripts\\Blizzard.j", BLIZZARD)]);
    let mut ws = init(&tmp.0);
    let raw = ws.take_reports();
    let r: Vec<(String, usize)> = raw
        .into_iter()
        .map(|m| match m {
            ServerMsg::File { id, diagnostics, .. } => (id, diagnostics.len()),
            _ => unreachable!(),
        })
        .collect();
    let common = path_id(&patch.join("Scripts/common.j"));
    let blizzard = path_id(&patch.join("Scripts/Blizzard.j"));
    let ujapi = path_id(&tmp.0.join("game/ujapi/Scripts/common.j"));
    // Loaded once each, without diagnostics (Blizzard.j's unused local).
    for id in [&common, &blizzard, &ujapi] {
        let found: Vec<_> = r.iter().filter(|x| &x.0 == id).collect();
        assert_eq!(found, vec![&(id.clone(), 0)], "{r:?}");
        assert!(ws.is_game(id));
    }

    let definition = |file: &str, line: u32, ch: u32| -> Vec<String> {
        let id = path_id(&tmp.0.join(file));
        answer(&ws, QueryKind::Definition, &id, Position::new(line, ch), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    // `a` gets the game's common.j, `b` UJAPI's, which sits over it.
    assert_eq!(definition("a/main.j", 2, 10), vec![common.clone()]);
    assert_eq!(definition("b/main.j", 2, 10), vec![ujapi.clone()]);
    // Blizzard.j is the same file for both.
    assert_eq!(definition("a/main.j", 1, 10), vec![blizzard.clone()]);
    assert_eq!(definition("b/main.j", 1, 10), vec![blizzard.clone()]);
    // References of a project's function don't reach the other project.
    let refs = answer(&ws, QueryKind::References, &path_id(&tmp.0.join("a/main.j")), Position::new(0, 10), "");
    assert!(refs.locations.iter().all(|l| l.id.contains("/a/")), "{:?}", refs.locations);

    // The game's functions can't be renamed.
    let why = answer(&ws, QueryKind::PrepareRename, &path_id(&tmp.0.join("a/main.j")), Position::new(1, 10), "").text;
    assert!(why.contains("the game's Blizzard.j"), "{why}");

    // Opened and closed, a game file stays one.
    ws.open(blizzard.clone(), Some(PathBuf::from(&blizzard)), Language::Jass, 1, BLIZZARD.into());
    ws.close(&blizzard);
    let r = reports(&mut ws);
    assert_eq!(r, vec![(blizzard.clone(), 0, 0, 0)]);
    assert!(ws.is_game(&blizzard));

    // Without game= the files leave.
    std::fs::write(tmp.0.join("a/wc3.bni"), "").unwrap();
    std::fs::write(tmp.0.join("b/wc3.bni"), "").unwrap();
    ws.disk_changed(&[tmp.0.join("a/wc3.bni")]);
    reports(&mut ws);
    assert!(ws.file(&common).is_none() && ws.file(&ujapi).is_none());
}

#[test]
fn ai_scripts_see_common_ai_and_map_scripts_blizzard_j() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const COMMON: &str = "native CreateTimer takes nothing returns timer\n";
    const BLIZZARD: &str = "function BJDebugMsg takes string msg returns nothing\nendfunction\n";
    const COMMON_AI: &str = "native GetUnitCountDone takes integer unitid returns integer\nfunction InitAI takes nothing returns nothing\n    call CreateTimer()\nendfunction\n";
    const CALLS: &str = "function F takes nothing returns nothing\n    call CreateTimer()\n    call BJDebugMsg(\"x\")\n    call InitAI()\nendfunction\n";

    let tmp = project("ws-game-ai", &[
        ("game/README", ""),
        ("p/wc3.bni", "[project]\ngame=../game\nsources=.\n"),
        ("p/main.j", CALLS),
        ("p/ai/elf.ai", CALLS),
    ]);
    let patch = tmp.0.join("game/War3Patch.mpq");
    crate::map::tests::make_map(
        &patch,
        &[("Scripts\\common.j", COMMON), ("Scripts\\Blizzard.j", BLIZZARD), ("Scripts\\common.ai", COMMON_AI)],
    );
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let in_game = |name: &str| path_id(&patch.join("Scripts").join(name));
    let definition = |id: &str, line: u32| -> Vec<String> {
        answer(&ws, QueryKind::Definition, id, Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    let main = path_id(&tmp.0.join("p/main.j"));
    let ai = path_id(&tmp.0.join("p/ai/elf.ai"));
    // A map script: common.j and Blizzard.j, not common.ai.
    assert_eq!(definition(&main, 1), vec![in_game("common.j")]);
    assert_eq!(definition(&main, 2), vec![in_game("Blizzard.j")]);
    assert!(definition(&main, 3).is_empty());
    // An AI script: common.j and common.ai, not Blizzard.j.
    assert_eq!(definition(&ai, 1), vec![in_game("common.j")]);
    assert!(definition(&ai, 2).is_empty());
    assert_eq!(definition(&ai, 3), vec![in_game("common.ai")]);
    // common.ai itself sees common.j.
    assert_eq!(definition(&in_game("common.ai"), 2), vec![in_game("common.j")]);
    assert!(ws.is_game(&in_game("common.ai")));
}

#[test]
fn an_ai_script_runs_on_its_own() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    let tmp = project("ws-ai-alone", &[
        ("wc3.bni", ""),
        ("main.j", "function MapOnly takes nothing returns nothing\n    call Shared()\n    call AiOnly()\nendfunction\n"),
        ("lib.j", "function Shared takes nothing returns nothing\nendfunction\n"),
        ("elf.ai", "function AiOnly takes nothing returns nothing\nendfunction\nfunction main takes nothing returns nothing\n    call AiOnly()\n    call MapOnly()\n    call OtherAi()\nendfunction\n"),
        ("orc.ai", "function OtherAi takes nothing returns nothing\nendfunction\nfunction AiOnly takes nothing returns nothing\nendfunction\n"),
    ]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let id = |f: &str| path_id(&tmp.0.join(f));
    let definition = |file: &str, line: u32| -> Vec<String> {
        answer(&ws, QueryKind::Definition, &id(file), Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    // Map scripts see each other, not AI scripts.
    assert_eq!(definition("main.j", 1), vec![id("lib.j")]);
    assert!(definition("main.j", 2).is_empty());
    // An AI script sees itself only: its own AiOnly, not orc.ai's.
    assert_eq!(definition("elf.ai", 3), vec![id("elf.ai")]);
    assert!(definition("elf.ai", 4).is_empty());
    assert!(definition("elf.ai", 5).is_empty());
    // References of an AI script's function stay in it.
    let refs = answer(&ws, QueryKind::References, &id("elf.ai"), Position::new(0, 10), "");
    assert!(refs.locations.iter().all(|l| l.id == id("elf.ai")), "{:?}", refs.locations);
    assert_eq!(refs.locations.len(), 2);
}

#[test]
fn game_folders_are_watched_and_reread() {
    let tmp = project("ws-game-watch", &[
        ("game/Maps/README", ""),
        ("over/README", ""),
        ("p/wc3.bni", "[project]\ngame=../game\ngame=../over\n"),
        ("p/main.j", OK),
    ]);
    let patch = tmp.0.join("game/War3Patch.mpq");
    crate::map::tests::make_map(&patch, &[("Scripts\\common.j", "native A takes nothing returns nothing\n")]);
    let mut ws = init(&tmp.0.join("p"));
    reports(&mut ws);
    let in_mpq = path_id(&patch.join("Scripts/common.j"));
    assert!(ws.watch_paths().contains(&tmp.0.join("game")));
    assert!(ws.watch_paths().contains(&tmp.0.join("over")));

    // The archive is saved again: common.j is read again.
    crate::map::tests::make_map(&patch, &[("Scripts\\common.j", "native B takes nothing returns nothing\n")]);
    ws.disk_changed(std::slice::from_ref(&patch));
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["game/War3Patch.mpq/Scripts/common.j"]);
    assert!(ws.file(&in_mpq).unwrap().0.text.contains("native B"));

    // A loose common.j in a folder above replaces it.
    let loose = tmp.0.join("over/Scripts/common.j");
    std::fs::create_dir_all(loose.parent().unwrap()).unwrap();
    std::fs::write(&loose, "native C takes nothing returns nothing\n").unwrap();
    ws.disk_changed(std::slice::from_ref(&loose));
    assert_eq!(
        names(&tmp.0, &reports(&mut ws)),
        vec!["game/War3Patch.mpq/Scripts/common.j", "over/Scripts/common.j"]
    );
    assert!(ws.file(&in_mpq).is_none());
    assert!(ws.is_game(&path_id(&loose)));

    // Edited, then deleted: back to the archive's.
    std::fs::write(&loose, "native D takes nothing returns nothing\n").unwrap();
    ws.disk_changed(std::slice::from_ref(&loose));
    reports(&mut ws);
    assert!(ws.file(&path_id(&loose)).unwrap().0.text.contains("native D"));
    std::fs::remove_file(&loose).unwrap();
    ws.disk_changed(std::slice::from_ref(&loose));
    reports(&mut ws);
    assert!(ws.file(&path_id(&loose)).is_none());
    assert!(ws.file(&in_mpq).unwrap().0.text.contains("native B"));

    // Anything else in a game folder doesn't reload it.
    ws.disk_changed(&[tmp.0.join("game/Maps/A.w3x")]);
    assert!(reports(&mut ws).is_empty());
}

#[test]
fn one_file_per_game_path_whichever_project_led_there() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    let tmp = project("ws-game-one", &[
        ("game/Scripts/common.j", "constant integer STATE = 1\n"),
        ("game/Scripts/Blizzard.j", "function BJ takes nothing returns integer\n    return STATE\nendfunction\n"),
        ("ujapi/Scripts/COMMON.J", "constant integer STATE = 2\n"),
        ("noblizz/Scripts/blizzard.j", ""),
        ("a/wc3.bni", "[project]\ngame=../game\n"),
        ("a/main.j", "function F takes nothing returns nothing\n    call BJ()\nendfunction\n"),
        ("b/wc3.bni", "[project]\ngame=../game\ngame=../ujapi\n"),
        ("b/main.j", "function F takes nothing returns nothing\n    call BJ()\nendfunction\n"),
        ("c/wc3.bni", "[project]\ngame=../game\ngame=../noblizz\n"),
        ("c/main.j", "function F takes nothing returns nothing\n    call BJ()\nendfunction\n"),
    ]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let id = |f: &str| path_id(&tmp.0.join(f));
    let definition = |file: &str, line: u32, ch: u32| -> Vec<String> {
        answer(&ws, QueryKind::Definition, &id(file), Position::new(line, ch), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    let blizzard = "game/Scripts/Blizzard.j";
    // Opened on its own: Blizzard.j sees one common.j.
    assert_eq!(definition(blizzard, 1, 12).len(), 1);
    // Reached from b: b's common.j (UJAPI's, any case).
    assert_eq!(definition("b/main.j", 1, 10), vec![id(blizzard)]);
    assert_eq!(definition(blizzard, 1, 12), vec![id("ujapi/Scripts/COMMON.J")]);
    // Reached from a: the game's.
    assert_eq!(definition("a/main.j", 1, 10), vec![id(blizzard)]);
    assert_eq!(definition(blizzard, 1, 12), vec![id("game/Scripts/common.j")]);
    // An empty blizzard.j above it takes Blizzard.j's functions away.
    assert!(definition("c/main.j", 1, 10).is_empty());
    assert!(ws.is_game(&id("noblizz/Scripts/blizzard.j")));
}

#[test]
fn a_maps_own_blizzard_j_replaces_the_games_for_that_map() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const CALLS: &str = "function F takes nothing returns nothing\n    call Stock()\n    call Optimized()\n    call N()\nendfunction\n";
    let tmp = project("ws-map-blizzard", &[
        ("game/Scripts/common.j", "native N takes nothing returns nothing\n"),
        ("game/Scripts/Blizzard.j", "function Stock takes nothing returns nothing\nendfunction\n"),
        ("p/wc3.bni", "[project]\ngame=../game\nsources=.\n"),
        ("p/main.j", CALLS),
    ]);
    let a = tmp.0.join("p/A.w3x");
    crate::map::tests::make_map(&a, &[
        ("war3map.j", CALLS),
        ("scripts\\BLIZZARD.J", "function Optimized takes nothing returns nothing\n    local integer unused\n    call N()\nendfunction\n"),
    ]);
    crate::map::tests::make_map(&tmp.0.join("p/B.w3x"), &[("war3map.j", CALLS)]);
    let mut ws = init(&tmp.0.join("p"));
    let raw = ws.take_reports();
    let own = path_id(&a.join("scripts/BLIZZARD.J"));
    let quiet = raw.iter().any(|m| matches!(m, ServerMsg::File { id, diagnostics, .. } if *id == own && diagnostics.is_empty()));
    assert!(quiet, "the map's Blizzard.j is reported without diagnostics");
    assert!(ws.is_map_blizzard(&own));

    let definition = |file: &Path, line: u32| -> Vec<String> {
        answer(&ws, QueryKind::Definition, &path_id(file), Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    let stock = path_id(&tmp.0.join("game/Scripts/Blizzard.j"));
    let common = path_id(&tmp.0.join("game/Scripts/common.j"));
    // In map A: its own Blizzard.j, not the game's.
    assert!(definition(&a.join("war3map.j"), 1).is_empty());
    assert_eq!(definition(&a.join("war3map.j"), 2), vec![own.clone()]);
    assert_eq!(definition(&a.join("war3map.j"), 3), vec![common.clone()]);
    // Map B and the project's own files: the game's, never A's.
    for file in [tmp.0.join("p/B.w3x/war3map.j"), tmp.0.join("p/main.j")] {
        assert_eq!(definition(&file, 1), vec![stock.clone()], "{}", file.display());
        assert!(definition(&file, 2).is_empty(), "{}", file.display());
    }
    // The map's Blizzard.j sees common.j.
    assert_eq!(definition(Path::new(&own), 2), vec![common]);
}
