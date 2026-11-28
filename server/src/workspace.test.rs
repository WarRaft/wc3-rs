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
fn without_sources_only_the_manifest_is_loaded() {
    let tmp = project("ws-no-sources", &[("wc3.bni", "[jass]\ndialect=vjass\n"), ("a.j", OK)]);
    crate::map::tests::make_map(&tmp.0.join("Map.w3x"), &[("war3map.j", OK)]);
    let mut ws = init(&tmp.0);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["wc3.bni"]);
}

#[test]
fn sources_dot_is_the_whole_folder() {
    let tmp = project("ws-whole", &[
        ("wc3.bni", "[code]\nsources=.\n"),
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
        ("wc3.bni", "[code]\nsources=src\n"),
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
        ("wc3.bni", "[code]\nsources=.\n"),
        ("a.j", OK),
        ("inner/wc3.bni", "[code]\nsources=.\n[jass]\ndialect=vjass\n"),
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
    let tmp = project("ws-open", &[("wc3.bni", "[code]\nsources=.\n"), ("a.j", OK), ("b.j", BAD)]);
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
    let tmp = project("ws-close", &[("wc3.bni", "[code]\nsources=.\n"), ("a.j", OK)]);
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
    let tmp = project("ws-disk", &[("wc3.bni", "[code]\nsources=.\n"), ("a.j", OK)]);
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
    let tmp = project("ws-open-wins", &[("wc3.bni", "[code]\nsources=.\n"), ("a.j", OK)]);
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
        ("wc3.bni", "[code]\nsources=.\n"),
        ("src/a.j", "globals\n    integer library = 1\nendglobals"),
        ("tools/b.j", OK),
    ]);
    let mut ws = init(&tmp.0);
    let r = reports(&mut ws);
    assert_eq!(names(&tmp.0, &r), vec!["src/a.j", "tools/b.j", "wc3.bni"]);
    assert_eq!(r[0].2, 0);

    // Narrow the sources and switch to vJASS.
    let manifest = tmp.0.join("wc3.bni");
    std::fs::write(&manifest, "[code]\nsources=src\n[jass]\ndialect=vjass\n").unwrap();
    ws.disk_changed(&[manifest]);
    let r = reports(&mut ws);
    // tools/b.j left (cleared, reported first); src/a.j is now an error.
    assert_eq!(names(&tmp.0, &r), vec!["tools/b.j", "src/a.j", "wc3.bni"]);
    assert_eq!(r[1].2, 1);
}

#[test]
fn sources_outside_the_root_are_loaded_and_watched() {
    let tmp = project("ws-outside", &[
        ("game/wc3.bni", "[code]\nsources=src,../shared\n"),
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
    let tmp = project("ws-latin1", &[("wc3.bni", "[code]\nsources=.\n")]);
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
    let mut all = vec![("wc3.bni", "[code]\nsources=.\n")];
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
    let tmp = project("ws-maps", &[("wc3.bni", "[code]\nsources=.\n"), ("a.j", OK), (".hidden/x.j", BAD)]);
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
    std::fs::write(tmp.0.join("wc3.bni"), "[code]\nsources=.\n[jass]\ndialect=vjass\n").unwrap();
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
    let tmp = project("ws-map-sources", &[("wc3.bni", "[code]\nsources=Main.w3m\n"), ("a.j", OK)]);
    crate::map::tests::make_map(&tmp.0.join("Main.w3m"), &[("war3map.j", OK)]);
    crate::map::tests::make_map(&tmp.0.join("Other.w3x"), &[("war3map.j", OK)]);
    let mut ws = init(&tmp.0);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["Main.w3m/war3map.j", "wc3.bni"]);
}

#[test]
fn excluded_files_and_map_scripts_leave_the_model() {
    let tmp = project("ws-exclude", &[
        ("wc3.bni", "[code]\nsources=.\nexclude=test.j,**/old/**\n"),
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
    std::fs::write(tmp.0.join("wc3.bni"), "[code]\nsources=.\nexclude=test.j,**/old/**,**/war3map.lua\n").unwrap();
    ws.disk_changed(&[tmp.0.join("wc3.bni")]);
    let r = reports(&mut ws);
    assert_eq!(r[0], (path_id(&tmp.0.join("Map.w3x/war3map.lua")), 0, 0, 0), "cleared first: {r:?}");
    assert!(ws.file(&path_id(&tmp.0.join("Map.w3x/war3map.lua"))).is_none());

    // Source masks: only the matching maps.
    std::fs::write(tmp.0.join("wc3.bni"), "[code]\nsources=*.w3x\n").unwrap();
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
        ("a/wc3.bni", "[project]\ngame=../game/wc3\n[code]\nsources=.\n"),
        ("a/main.j", USE),
        ("b/wc3.bni", "[project]\ngame=../game/wc3\ngame=../game/ujapi\n[code]\nsources=.\n"),
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
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
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
        ("wc3.bni", "[code]\nsources=.\n"),
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
        ("p/wc3.bni", "[project]\ngame=../game\ngame=../over\n[code]\nsources=.\n"),
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
        ("a/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
        ("a/main.j", "function F takes nothing returns nothing\n    call BJ()\nendfunction\n"),
        ("b/wc3.bni", "[project]\ngame=../game\ngame=../ujapi\n[code]\nsources=.\n"),
        ("b/main.j", "function F takes nothing returns nothing\n    call BJ()\nendfunction\n"),
        ("c/wc3.bni", "[project]\ngame=../game\ngame=../noblizz\n[code]\nsources=.\n"),
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
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
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
    assert!(ws.is_map_script(&own));

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

#[test]
fn game_folders_of_a_file() {
    let tmp = project(
        "ws-game-folders",
        &[("p/wc3.bni", "[project]\ngame=wc3\ngame=ujapi\n[code]\nsources=maps/*.w3x,src\n"), ("p/wc3/x", "")],
    );
    let mut ws = Workspace::default();
    let p = tmp.0.join("p");
    let both = vec![p.join("wc3"), p.join("ujapi")];
    // Every game folder, in order; in a map of the project too.
    assert_eq!(ws.game_folders(&p.join("maps/a.w3x/war3map.w3u")), both);
    assert_eq!(ws.game_folders(&p.join("src/war3map.w3a")), both);
    // Below the manifest but not in `sources`: still the project's, as a
    // map next to its `wc3.bni` is.
    assert_eq!(ws.game_folders(&p.join("war3map.w3a")), both);
    assert_eq!(ws.game_folders(&p.join("other.w3x/war3map.w3a")), both);
    // No `wc3.bni` above: no game.
    assert_eq!(ws.game_folders(&tmp.0.join("war3map.w3u")), Vec::<PathBuf>::new());
}

#[test]
fn object_data_files_are_not_analyzed() {
    // Binary object data in the project's sources isn't text: nothing to
    // report for it.
    let tmp = project("ws-objects", &[("wc3.bni", "[code]\nsources=.\n"), ("war3map.w3a", "\u{3}\0\0\0garbage"), ("a.j", OK)]);
    let mut ws = init(&tmp.0);
    assert_eq!(names(&tmp.0, &reports(&mut ws)), vec!["a.j", "wc3.bni"]);
}

#[test]
fn object_text_is_checked_with_its_projects_game() {
    let meta = "C;X1;Y1;K\"ID\"\nC;X2;K\"type\"\nC;X3;K\"field\"\nC;X1;Y2;K\"umvs\"\nC;X2;K\"int\"\nC;X3;K\"Speed\"\n";
    let tmp = project(
        "ws-object-text",
        &[
            ("p/wc3.bni", "[project]\ngame=wc3\n[code]\nsources=src\n[lint]\nobject-value-range=off\n"),
            ("p/wc3/Units/UnitMetaData.slk", meta),
        ],
    );
    let p = tmp.0.join("p");
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    // The editor's text form of src/war3map.w3u: its id is the text's,
    // its path the file's.
    let text = "[w3u]\nversion=2\n[hfoo]\numvs=1.5\n";
    ws.open("wc3obj:/x".into(), Some(p.join("src/war3map.w3u")), Language::Bni, 1, text.into());
    let found: Vec<_> = ws
        .take_reports()
        .into_iter()
        .filter_map(|m| match m {
            ServerMsg::File { id, diagnostics, .. } if id == "wc3obj:/x" => Some(diagnostics),
            _ => None,
        })
        .flatten()
        .map(|d| (d.rule, d.range.start.line))
        .collect();
    assert_eq!(found, vec![("object-value-type", 3)]);

    // Below the manifest but outside `sources`: still the project's game.
    ws.open("wc3obj:/y".into(), Some(p.join("war3map.w3u")), Language::Bni, 1, text.into());
    let found = ws.take_reports().into_iter().any(|m| matches!(m, ServerMsg::File { id, diagnostics, .. } if id == "wc3obj:/y" && !diagnostics.is_empty()));
    assert!(found);
    // No `wc3.bni` above: no game, no checks.
    ws.open("wc3obj:/z".into(), Some(tmp.0.join("war3map.w3u")), Language::Bni, 1, text.into());
    let found = ws.take_reports().into_iter().any(|m| matches!(m, ServerMsg::File { id, diagnostics, .. } if id == "wc3obj:/z" && !diagnostics.is_empty()));
    assert!(!found);
}

#[test]
fn hover_on_an_object_field() {
    let meta = "C;X1;Y1;K\"ID\"\nC;X2;K\"type\"\nC;X3;K\"field\"\nC;X1;Y2;K\"umvs\"\nC;X2;K\"int\"\nC;X3;K\"Speed\"\n";
    let tmp = project("ws-object-hover", &[("p/wc3.bni", "[project]\ngame=wc3\n[code]\nsources=src\n"), ("p/wc3/Units/UnitMetaData.slk", meta)]);
    let mut ws = init(&tmp.0);
    ws.open("wc3obj:/x".into(), Some(tmp.0.join("p/src/war3map.w3u")), Language::Bni, 1, "[w3u]\nversion=2\n[hfoo]\numvs=1\n".into());
    ws.take_reports();
    let answer = crate::index::answer(&ws, crate::index::QueryKind::Hover, "wc3obj:/x", crate::document::Position::new(3, 1), "");
    assert_eq!(answer.text, "**Speed** `umvs`\n\n`int`: a whole number");
}

#[test]
fn a_root_replaces_the_games_scripts_like_any_file() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const CALLS: &str = "function F takes nothing returns nothing\n    call Stock()\n    call Ours()\n    call N()\nendfunction\n";
    let tmp = project("ws-root-scripts", &[
        ("game/Scripts/common.j", "native N takes nothing returns nothing\n"),
        ("game/Scripts/Blizzard.j", "function Stock takes nothing returns nothing\nendfunction\n"),
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=src\n[map]\nroot=map\n"),
        ("p/map/scripts/BLIZZARD.J", "function Ours takes nothing returns nothing\nendfunction\n"),
        ("p/src/main.j", CALLS),
    ]);
    let mut ws = init(&tmp.0.join("p"));
    reports(&mut ws);
    let ours = path_id(&tmp.0.join("p/map/scripts/BLIZZARD.J"));
    let common = path_id(&tmp.0.join("game/Scripts/common.j"));
    assert!(ws.is_game(&ours), "the root's Blizzard.j is the project's game script");
    assert!(!ws.is_game(&path_id(&tmp.0.join("game/Scripts/Blizzard.j"))));
    let main = path_id(&tmp.0.join("p/src/main.j"));
    let definition = |ws: &Workspace, line: u32| -> Vec<String> {
        answer(ws, QueryKind::Definition, &main, Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    assert!(definition(&ws, 1).is_empty(), "the game's Blizzard.j is shadowed");
    assert_eq!(definition(&ws, 2), vec![ours.clone()]);
    assert_eq!(definition(&ws, 3), vec![common]);

    // Deleted from the root: the game's comes back.
    std::fs::remove_file(tmp.0.join("p/map/scripts/BLIZZARD.J")).unwrap();
    ws.disk_changed(&[tmp.0.join("p/map/scripts/BLIZZARD.J")]);
    reports(&mut ws);
    assert!(ws.is_game(&path_id(&tmp.0.join("game/Scripts/Blizzard.j"))));
    assert_eq!(definition(&ws, 1), vec![path_id(&tmp.0.join("game/Scripts/Blizzard.j"))]);
}

#[test]
fn a_map_in_the_roots_goes_through_the_whole_chain_any_other_stands_alone() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const CALLS: &str = "function F takes nothing returns nothing\n    call Stock()\n    call Base()\n    call Kept()\n    call N()\n    call M()\nendfunction\n";
    let tmp = project("ws-root-map", &[
        ("game/Scripts/common.j", "native N takes nothing returns nothing\n"),
        ("game/Scripts/Blizzard.j", "function Stock takes nothing returns nothing\nendfunction\n"),
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=src,kept\n[map]\nroot=base.w3x\nroot=map\n"),
        ("p/map/README", ""),
        ("p/src/main.j", CALLS),
    ]);
    let base = tmp.0.join("p/base.w3x");
    crate::map::tests::make_map(&base, &[
        ("war3map.j", CALLS),
        ("Scripts\\Blizzard.j", "function Base takes nothing returns nothing\nendfunction\n"),
    ]);
    std::fs::create_dir_all(tmp.0.join("p/kept")).unwrap();
    let kept = tmp.0.join("p/kept/Old.w3x");
    crate::map::tests::make_map(&kept, &[
        ("war3map.j", CALLS),
        ("Scripts\\Blizzard.j", "function Kept takes nothing returns nothing\nendfunction\n"),
        ("Scripts\\common.j", "native M takes nothing returns nothing\n"),
    ]);
    let mut ws = init(&tmp.0.join("p"));
    reports(&mut ws);
    let base_blizzard = path_id(&base.join("Scripts/Blizzard.j"));
    let kept_blizzard = path_id(&kept.join("Scripts/Blizzard.j"));
    let kept_common = path_id(&kept.join("Scripts/common.j"));
    let definition = |ws: &Workspace, file: &Path, line: u32| -> Vec<String> {
        answer(ws, QueryKind::Definition, &path_id(file), Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    // The root map's Blizzard.j is the project's: every file sees it,
    // the code in src and the root map's own script alike.
    assert!(ws.is_game(&base_blizzard));
    assert!(!ws.is_map_script(&base_blizzard));
    for file in [tmp.0.join("p/src/main.j"), base.join("war3map.j")] {
        assert!(definition(&ws, &file, 1).is_empty(), "{}", file.display());
        assert_eq!(definition(&ws, &file, 2), vec![base_blizzard.clone()], "{}", file.display());
        assert!(definition(&ws, &file, 3).is_empty(), "{}", file.display());
    }
    // A map that isn't a root: the game, then the map itself. Its own
    // Blizzard.j and common.j replace the game's for its scripts only.
    assert!(ws.is_map_script(&kept_blizzard) && ws.is_map_script(&kept_common));
    let old = kept.join("war3map.j");
    assert!(definition(&ws, &old, 1).is_empty());
    assert!(definition(&ws, &old, 2).is_empty(), "the root map isn't in its chain");
    assert_eq!(definition(&ws, &old, 3), vec![kept_blizzard]);
    assert!(definition(&ws, &old, 4).is_empty(), "its common.j replaces the game's");
    assert_eq!(definition(&ws, &old, 5), vec![kept_common]);
    assert!(definition(&ws, &tmp.0.join("p/src/main.j"), 5).is_empty());

    // Lookups for other files follow the same two rules.
    let game = vec![tmp.0.join("game")];
    let lookup = ws.lookup(&kept.join("war3map.w3r"));
    assert_eq!((lookup.game, lookup.roots), (game.clone(), vec![kept.clone()]));
    let full = vec![base.clone(), tmp.0.join("p/map")];
    let lookup = ws.lookup(&base.join("war3map.w3r"));
    assert_eq!((lookup.game.clone(), lookup.roots.clone()), (game.clone(), full.clone()));
    let lookup = ws.lookup(&tmp.0.join("p/map/war3map.w3r"));
    assert_eq!((lookup.game.clone(), lookup.roots.clone()), (game.clone(), full));
    // A map the project doesn't name at all, only opened: the short chain.
    let other = tmp.0.join("p/other/Other.W3M");
    let lookup = ws.lookup(&other.join("war3map.w3r"));
    assert_eq!((lookup.game, lookup.roots), (game, vec![other]));
}

#[test]
fn terrain_text_counts_its_files_points_until_the_file_changes() {
    use w3e_rs::{Point, Rawcode, Terrain, Version, ZERO_HEIGHT};
    let tmp = project("ws-terrain-usage", &[("p/wc3.bni", "[map]\nroot=map\n"), ("p/map/README", "")]);
    let path = tmp.0.join("p/map/war3map.w3e");
    let file = |grass: usize| {
        let v = Version::of(11);
        let points = (0..4)
            .map(|i| {
                let mut p = Point { height: ZERO_HEIGHT, layer: 0xF0, ..Point::default() };
                p.set_texture(v, u16::from(i < grass));
                p
            })
            .collect();
        let t = Terrain {
            version: 11,
            tileset: b'L',
            custom_tilesets: 0,
            ground: vec![Rawcode(*b"Ldrt"), Rawcode(*b"Lgrs")],
            cliffs: vec![],
            width: 2,
            height: 2,
            offset_x: 0.0,
            offset_y: 0.0,
            points,
        };
        t.to_bytes().unwrap()
    };
    std::fs::write(&path, file(1)).unwrap();
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let text = String::from_utf8(crate::terrain::convert(crate::objects::TO_TEXT, &path.to_string_lossy(), &file(1)).unwrap()).unwrap();
    ws.open("wc3obj:/t".into(), Some(path.clone()), Language::Bni, 1, text);
    ws.take_reports();
    let usage = |ws: &Workspace| ws.names_text("wc3obj:/t").and_then(|(_, _, u)| u.cloned()).unwrap();
    assert_eq!(usage(&ws).ground, [3, 1]);

    // Saved again with more grass: counted again once the disk says so.
    std::fs::write(&path, file(3)).unwrap();
    ws.take_reports();
    assert_eq!(usage(&ws).ground, [3, 1], "the cache answers until the change is reported");
    ws.disk_changed(std::slice::from_ref(&path));
    ws.take_reports();
    assert_eq!(usage(&ws).ground, [1, 3]);
}

#[test]
fn a_roots_custom_script_is_code_of_the_project() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    let tmp = project("ws-wct", &[
        ("p/wc3.bni", "[code]\nsources=src\n[map]\nroot=map\nroot=base.w3x\n"),
        ("p/map/README", ""),
        ("p/src/main.j", "function F takes nothing returns nothing\n    call Helper()\n    call FromBase()\nendfunction\n"),
    ]);
    let wct = |name: &str| {
        wct_rs::CustomText {
            version: 1,
            header: wct_rs::Code::new(format!("function {name} takes nothing returns nothing\r\nendfunction\r\n").as_bytes()),
            ..wct_rs::CustomText::default()
        }
        .to_bytes()
        .unwrap()
    };
    let disk = tmp.0.join("p/map/war3map.wct");
    std::fs::write(&disk, wct("Helper")).unwrap();
    let base = tmp.0.join("p/base.w3x");
    crate::map::tests::make_map_bytes(&base, &[("war3map.wct", wct("FromBase"))]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let main = path_id(&tmp.0.join("p/src/main.j"));
    let definition = |ws: &Workspace, line: u32| -> Vec<String> {
        answer(ws, QueryKind::Definition, &main, Position::new(line, 10), "")
            .locations
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    // Both custom scripts are in the model, as JASS.
    assert_eq!(definition(&ws, 1), vec![path_id(&disk)]);
    assert_eq!(definition(&ws, 2), vec![path_id(&base.join("war3map.wct"))]);
    assert!(ws.file(&path_id(&disk)).unwrap().0.text.starts_with("// @wct version=1\n"));

    // Opened as text: the text stands for the file, once.
    let text = ws.file(&path_id(&disk)).unwrap().0.text.clone().replace("Helper", "Helper2");
    ws.open("wc3obj:/w".into(), Some(disk.clone()), Language::Jass, 1, text);
    reports(&mut ws);
    assert!(ws.file(&path_id(&disk)).is_none());
    assert!(definition(&ws, 1).is_empty(), "the open text renamed it");
    ws.disk_changed(std::slice::from_ref(&disk));
    reports(&mut ws);
    assert!(ws.file(&path_id(&disk)).is_none(), "the disk doesn't bring it back while open");
    // Closed: back to the file.
    ws.close("wc3obj:/w");
    reports(&mut ws);
    assert_eq!(definition(&ws, 1), vec![path_id(&disk)]);
    assert!(ws.file("wc3obj:/w").is_none());
}

#[test]
fn names_are_checked_across_the_project() {
    const COMMON: &str = "type unit extends handle\nnative KillUnit takes unit u returns nothing\n";
    let tmp = project("ws-names", &[
        ("game/README", ""),
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
        ("p/a.j", "function Helper takes integer n returns nothing\nendfunction\n"),
        ("p/b.j", "function F\n    Helper(1, 2)\n    Missing()\n    KillUnit(null)\nendfunction\n"),
    ]);
    crate::map::tests::make_map(&tmp.0.join("game/War3Patch.mpq"), &[("Scripts\\common.j", COMMON)]);
    let mut ws = init(&tmp.0);
    let b = path_id(&tmp.0.join("p/b.j"));
    let messages = |ws: &mut Workspace| -> HashMap<String, Vec<String>> {
        ws.take_reports()
            .into_iter()
            .map(|m| match m {
                ServerMsg::File { id, diagnostics, .. } => (id, diagnostics.into_iter().map(|d| format!("{}: {}", d.rule, d.message)).collect()),
                _ => unreachable!(),
            })
            .collect()
    };
    let r = messages(&mut ws);
    // Helper of a.j, KillUnit of the game's common.j; Missing nowhere.
    assert_eq!(
        r[&b],
        ["argument-count: 'Helper' takes 1 argument, not 2", "unknown-name: the function 'Missing' is declared nowhere"],
        "{r:?}"
    );
    // a.j changes: b.j is checked again and reported, its text unchanged.
    let a = path_id(&tmp.0.join("p/a.j"));
    ws.open(a.clone(), Some(tmp.0.join("p/a.j")), Language::Jass, 2, "function Helper takes integer n, integer m returns nothing\nendfunction\nfunction Missing\nendfunction\n".into());
    let r = messages(&mut ws);
    assert_eq!(r.get(&b), Some(&Vec::new()), "{r:?}");
    // A change that declares nothing new reports a.j alone.
    ws.open(a.clone(), Some(tmp.0.join("p/a.j")), Language::Jass, 3, "function Helper takes integer n, integer m returns nothing\n    KillUnit(null)\nendfunction\nfunction Missing\nendfunction\n".into());
    let r = messages(&mut ws);
    assert_eq!(r.keys().collect::<Vec<_>>(), [&a], "{r:?}");
}

#[test]
fn names_are_not_checked_without_the_games_common_j() {
    let tmp = project("ws-names-none", &[("p/wc3.bni", "[code]\nsources=.\n"), ("p/b.j", "function F\n    Missing()\nendfunction\n")]);
    let mut ws = init(&tmp.0);
    for m in ws.take_reports() {
        let ServerMsg::File { diagnostics, .. } = m else { unreachable!() };
        assert!(!diagnostics.iter().any(|d| d.rule == parser_rs::rule::UNKNOWN_NAME), "{diagnostics:?}");
    }
}

#[test]
fn frame_files_link_to_each_other_and_the_games() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const TOC: &str = "UI\\FrameDef\\GlobalStrings.fdf\r\nUI\\FrameDef\\UI\\Dialog.fdf\r\n";
    const STRINGS: &str = "StringList {\n    OK \"Okay\",\n}\n";
    const DIALOG: &str = "IncludeFile \"UI\\FrameDef\\UI\\Templates.fdf\",\nFrame \"BACKDROP\" \"Dialog\" {\n}\n";
    const TEMPLATES: &str = "Frame \"GLUETEXTBUTTON\" \"ButtonTemplate\" {\n    Width 0.1,\n}\n";
    const MINE: &str = concat!(
        "IncludeFile \"UI\\FrameDef\\UI\\Templates.fdf\",\n",
        "Frame \"GLUETEXTBUTTON\" \"MyButton\" INHERITS \"ButtonTemplate\" {\n",
        "    Text \"OK\",\n",
        "    ControlBackdrop \"MyBack\",\n",
        "    Frame \"BACKDROP\" \"MyBack\" INHERITS \"Nope\" {\n",
        "    }\n",
        "}\n",
    );
    let tmp = project("ws-fdf", &[("game/README", ""), ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"), ("p/UI/FrameDef/My.fdf", MINE)]);
    let patch = tmp.0.join("game/War3Patch.mpq");
    crate::map::tests::make_map(&patch, &[
        ("UI\\FrameDef\\FrameDef.toc", TOC),
        ("UI\\FrameDef\\GlobalStrings.fdf", STRINGS),
        ("UI\\FrameDef\\UI\\Dialog.fdf", DIALOG),
        ("UI\\FrameDef\\UI\\Templates.fdf", TEMPLATES),
        ("UI\\FrameDef\\UI\\Unlisted.fdf", TEMPLATES),
    ]);
    let mut ws = init(&tmp.0);
    let reported = reports(&mut ws);
    let in_game = |name: &str| path_id(&patch.join(name));
    let mine = path_id(&tmp.0.join("p/UI/FrameDef/My.fdf"));
    // The game's: the listed ones and what they include, not the others.
    for name in ["UI/FrameDef/GlobalStrings.fdf", "UI/FrameDef/UI/Dialog.fdf", "UI/FrameDef/UI/Templates.fdf"] {
        assert!(ws.is_game(&in_game(name)), "{name}");
    }
    assert!(ws.file(&in_game("UI/FrameDef/UI/Unlisted.fdf")).is_none());
    // One problem: the frame inherited that's declared nowhere.
    assert_eq!(reported.iter().find(|r| r.0 == mine).map(|r| r.2), Some(1), "{reported:?}");
    let ask = |kind: QueryKind, line: u32, character: u32| answer(&ws, kind, &mine, Position::new(line, character), "");
    let ids = |a: crate::index::Answer| a.locations.into_iter().map(|l| (l.id, l.range.start.line, l.range.start.character)).collect::<Vec<_>>();
    // INHERITS "ButtonTemplate" → the game's template; Text "OK" → its key.
    assert_eq!(ids(ask(QueryKind::Definition, 1, 50)), [(in_game("UI/FrameDef/UI/Templates.fdf"), 0, 24)]);
    assert_eq!(ids(ask(QueryKind::Definition, 2, 11)), [(in_game("UI/FrameDef/GlobalStrings.fdf"), 1, 4)]);
    // A frame of the file: its declaration, its references.
    assert_eq!(ids(ask(QueryKind::References, 3, 22)), [(mine.clone(), 3, 21), (mine.clone(), 4, 22)]);
    assert!(ask(QueryKind::Hover, 3, 22).text.contains("Frame \"BACKDROP\" \"MyBack\""));
    // A string that names nothing: no hover.
    assert_eq!(ask(QueryKind::Hover, 4, 41).text, "");
    // The outline: the frames, a child in its parent.
    let outline: Vec<(String, String)> = ask(QueryKind::DocumentSymbols, 0, 0).locations.into_iter().map(|l| (l.name, l.container)).collect();
    assert_eq!(outline, [("MyButton".to_string(), String::new()), ("MyBack".to_string(), "MyButton".to_string())]);
    // IncludeFile's path → the game's file.
    assert_eq!(ids(ask(QueryKind::PathLinks, 0, 0)), [(in_game("UI/FrameDef/UI/Templates.fdf"), 0, 13)]);
    // Renaming the file's own frame renames its uses; the game's can't be.
    let renamed = answer(&ws, QueryKind::Rename, &mine, Position::new(3, 22), "MyBackdrop");
    assert_eq!(renamed.text, "");
    assert_eq!(renamed.locations.len(), 2);
    assert!(!answer(&ws, QueryKind::Rename, &mine, Position::new(1, 50), "Other").text.is_empty());
    // In a string: every frame and key, once, replacing the string's text.
    let completion = ask(QueryKind::Completion, 1, 46);
    let names: Vec<&str> = completion.locations.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["ButtonTemplate", "Dialog", "MyBack", "MyButton", "OK"]);
    let r = completion.locations[0].range;
    assert_eq!((r.start.line, r.start.character, r.end.character), (1, 44, 58));
    // Not in a string, or in a comment: nothing.
    assert!(ask(QueryKind::Completion, 1, 2).locations.is_empty());
}

#[test]
fn code_strings_name_frames_by_their_text() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const TOC: &str = "UI\\FrameDef\\UI\\Templates.fdf\r\n";
    const TEMPLATES: &str = "Frame \"GLUETEXTBUTTON\" \"ButtonTemplate\" {\n}\nStringList {\n    ACCEPT \"Accept\",\n}\n";
    const MINE: &str = "Frame \"BACKDROP\" \"MyBack\" {\n}\n";
    const MAIN: &str = concat!(
        "function F takes nothing returns nothing\n",
        "    call Make(\"ButtonTemplate\", Other(\"MyBack\"))\n",
        "    call Print(\"MyBack is a frame\", \"\")\n",
        "endfunction\n",
        "//! zinc\n",
        "library L { function G() { Make(\"MyBack\"); } }\n",
        "//! endzinc\n",
    );
    let tmp = project("ws-code-frames", &[
        ("game/README", ""),
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
        ("p/UI/FrameDef/My.fdf", MINE),
        ("p/main.j", &format!("// @dialect vjass\n{MAIN}")),
        ("p/z.zn", "library Z { function H() { Make(\"MyBack\"); } }\n"),
        ("p/a.lua", "local f = Make('ACCEPT', \"MyBack\")\n"),
        ("p/b.as", "void H() { Make(\"ButtonTemplate\"); }\n"),
    ]);
    let patch = tmp.0.join("game/War3Patch.mpq");
    crate::map::tests::make_map(&patch, &[("UI\\FrameDef\\FrameDef.toc", TOC), ("UI\\FrameDef\\UI\\Templates.fdf", TEMPLATES)]);
    let mut ws = init(&tmp.0);
    reports(&mut ws);
    let file = |name: &str| path_id(&tmp.0.join("p").join(name));
    let templates = path_id(&patch.join("UI/FrameDef/UI/Templates.fdf"));
    let ids = |a: crate::index::Answer| a.locations.into_iter().map(|l| (l.id, l.range.start.line, l.range.start.character)).collect::<Vec<_>>();
    let def = |ws: &Workspace, name: &str, line: u32, character: u32| ids(answer(ws, QueryKind::Definition, &file(name), Position::new(line, character), ""));
    // Any string whose text is a name, whatever it's passed to, in every language.
    assert_eq!(def(&ws, "main.j", 2, 18), [(templates.clone(), 0, 24)]);
    assert_eq!(def(&ws, "main.j", 2, 42), [(file("UI/FrameDef/My.fdf"), 0, 18)]);
    assert_eq!(def(&ws, "main.j", 6, 34), [(file("UI/FrameDef/My.fdf"), 0, 18)]);
    assert_eq!(def(&ws, "z.zn", 0, 33), [(file("UI/FrameDef/My.fdf"), 0, 18)]);
    assert_eq!(def(&ws, "a.lua", 0, 17), [(templates.clone(), 3, 4)]);
    assert_eq!(def(&ws, "a.lua", 0, 28), [(file("UI/FrameDef/My.fdf"), 0, 18)]);
    assert_eq!(def(&ws, "b.as", 0, 19), [(templates.clone(), 0, 24)]);
    // A string that isn't a name: nothing.
    assert!(def(&ws, "main.j", 3, 18).is_empty());
    // From the frame: every use, renamed with it.
    let uses = ids(answer(&ws, QueryKind::References, &file("UI/FrameDef/My.fdf"), Position::new(0, 20), ""));
    assert_eq!(uses.len(), 5, "{uses:?}");
    let renamed = answer(&ws, QueryKind::Rename, &file("UI/FrameDef/My.fdf"), Position::new(0, 20), "MyBackdrop");
    assert_eq!((renamed.text.as_str(), renamed.locations.len()), ("", 5));
    // In any string of code: every frame and key (asked for: Ctrl+Space).
    let names: Vec<String> = answer(&ws, QueryKind::Completion, &file("main.j"), Position::new(3, 37), "").locations.into_iter().map(|l| l.name).collect();
    assert_eq!(names, ["ACCEPT", "ButtonTemplate", "MyBack"]);
    // The frame renamed in its file: its uses follow, as soon as it's analyzed.
    ws.open(file("UI/FrameDef/My.fdf"), Some(tmp.0.join("p/UI/FrameDef/My.fdf")), crate::document::Language::Fdf, 1, MINE.replace("MyBack", "Gone"));
    reports(&mut ws);
    assert!(def(&ws, "z.zn", 0, 33).is_empty());
}

#[test]
fn toc_files_list_frame_files() {
    use crate::document::Position;
    use crate::index::{answer, QueryKind};
    const TOC: &str = "UI\\FrameDef\\UI\\Templates.fdf\r\n";
    const TEMPLATES: &str = "Frame \"GLUETEXTBUTTON\" \"ButtonTemplate\" {\n}\n";
    const MY_TOC: &str = "UI\\FrameDef\\UI\\Templates.fdf\nwar3mapImported\\My.fdf\nwar3mapImported\\Nope.fdf";
    const MAIN: &str = "function F takes nothing returns nothing\n    call BlzLoadTOCFile(\"war3mapImported\\\\My.toc\")\nendfunction\n";
    let tmp = project("ws-toc", &[
        ("game/README", ""),
        ("p/wc3.bni", "[project]\ngame=../game\n[code]\nsources=.\n"),
        ("p/war3mapImported/My.toc", MY_TOC),
        ("p/war3mapImported/My.fdf", "Frame \"BACKDROP\" \"MyBack\" INHERITS \"ButtonTemplate\" {\n}\n"),
        ("p/main.j", MAIN),
    ]);
    let patch = tmp.0.join("game/War3Patch.mpq");
    crate::map::tests::make_map(&patch, &[("UI\\FrameDef\\FrameDef.toc", TOC), ("UI\\FrameDef\\UI\\Templates.fdf", TEMPLATES)]);
    let mut ws = init(&tmp.0);
    let reported = ws.take_reports();
    let file = |name: &str| path_id(&tmp.0.join("p").join(name));
    // The game's list is in the model, a list too.
    let game_toc = path_id(&patch.join("UI/FrameDef/FrameDef.toc"));
    assert!(ws.is_game(&game_toc));
    assert_eq!(ws.file(&game_toc).map(|(d, _)| d.language), Some(crate::document::Language::Toc));
    // The project's list: a file that's nowhere, and its last line unread.
    let diags: Vec<(u32, &str)> = reported
        .iter()
        .find_map(|m| match m {
            ServerMsg::File { id, diagnostics, .. } if *id == file("war3mapImported/My.toc") => Some(diagnostics.iter().map(|d| (d.range.start.line, d.rule)).collect()),
            _ => None,
        })
        .expect("reported");
    assert_eq!(diags, [(2, "toc-last-line"), (2, "missing-file")]);
    let links = |id: &str| -> Vec<(u32, String)> {
        answer(&ws, QueryKind::PathLinks, id, Position::new(0, 0), "").locations.into_iter().map(|l| (l.range.start.line, l.id)).collect()
    };
    // Each line → its file: the game's, the project's.
    assert_eq!(links(&file("war3mapImported/My.toc")), [(0, path_id(&patch.join("UI/FrameDef/UI/Templates.fdf"))), (1, file("war3mapImported/My.fdf"))]);
    // A path in a string of code → the file.
    assert_eq!(links(&file("main.j")), [(1, file("war3mapImported/My.toc"))]);
}
