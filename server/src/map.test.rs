use super::*;
use crate::project::tests::TempDir;
use storm_rs::archive::MpqArchiveBuilder;

/// A map with `files` in it, at `path`.
pub(crate) fn make_map(path: &Path, files: &[(&str, &str)]) {
    let mut b = MpqArchiveBuilder::new(path);
    for (name, text) in files {
        b = b.add_file(*name, text.as_bytes().to_vec());
    }
    b.build().unwrap();
}

#[test]
fn reads_the_scripts_of_a_map() {
    let tmp = TempDir::new("map-files");
    let map = tmp.0.join("Test.w3x");
    make_map(
        &map,
        &[
            ("war3map.j", "function main takes nothing returns nothing\nendfunction\n"),
            ("Scripts\\lib.zn", "library L {}\n"),
            ("war3map.w3i", "binary"),
        ],
    );
    let files = files(&map);
    let paths: Vec<&Path> = files.iter().map(|(p, _)| p.as_path()).collect();
    assert_eq!(paths, vec![map.join("Scripts").join("lib.zn"), map.join("war3map.j")]);
    assert_eq!(files[1].1.language, Language::Jass);
    assert!(files[1].1.text.starts_with("function main"));
    assert_eq!(files[1].1.path.as_deref(), Some(map.join("war3map.j").as_path()));

    let doc = load(&map.join("Scripts/lib.zn")).unwrap();
    assert_eq!(doc.language, Language::Zinc);
    assert_eq!(doc.text, "library L {}\n");
    assert!(load(&map.join("missing.j")).is_none());
    assert!(load(&tmp.0.join("not-a-map/war3map.j")).is_none());
}

#[test]
fn splits_a_path_inside_a_map() {
    let tmp = TempDir::new("map-split");
    let map = tmp.0.join("A.W3M");
    make_map(&map, &[("war3map.j", "")]);
    let inner = map.join("Scripts").join("x.j");
    let (found, name) = split(&inner).unwrap();
    assert_eq!(found, map.as_path());
    assert_eq!(name, "Scripts\\x.j");
    assert_eq!(inner_path(&map, "Scripts\\x.j"), inner);
    assert!(split(&map).is_none());
    assert!(split(&tmp.0.join("war3map.j")).is_none());
}

#[test]
fn a_file_that_is_not_a_map_has_no_files() {
    let tmp = TempDir::new("map-broken");
    let map = tmp.0.join("Broken.w3x");
    std::fs::write(&map, "not an archive").unwrap();
    assert!(files(&map).is_empty());
}

/// The maps in the `maps` repo, when it's on this machine.
#[test]
fn real_maps() {
    let dir = PathBuf::from("/Volumes/TrascendSSD/Work/maps/TheCastle");
    let Some(map) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.flatten().map(|e| e.path()).find(|p| is_map_name(p))
    }) else {
        eprintln!("skipped: no maps at {}", dir.display());
        return;
    };
    let files = files(&map);
    let script = files.iter().find(|(p, _)| p.ends_with("war3map.j")).expect("war3map.j");
    assert!(script.1.text.contains("function main"), "{}", &script.1.text[..200.min(script.1.text.len())]);
}

#[test]
fn lists_every_file_of_a_map_and_reads_any_as_text() {
    let tmp = TempDir::new("map-names");
    let map = tmp.0.join("Test.w3x");
    make_map(&map, &[("war3map.j", "j"), ("UI\\a.fdf", "fdf"), ("war3map.w3i", "bin"), ("Units\\b.mdx", "x")]);
    assert_eq!(names(&map), vec!["UI\\a.fdf", "Units\\b.mdx", "war3map.j", "war3map.w3i"]);
    assert_eq!(text(&map.join("UI/a.fdf")).as_deref(), Some("fdf"));
    assert_eq!(text(&map.join("war3map.w3i")).as_deref(), Some("bin"));
    assert!(names(&tmp.0.join("missing.w3x")).is_empty());
}

#[test]
fn a_map_without_a_listfile_still_shows_the_game_files_it_replaces() {
    use storm_rs::archive::MpqCreateOptions;
    let tmp = TempDir::new("map-protected");
    let map = tmp.0.join("Protected.w3x");
    MpqArchiveBuilder::new(&map)
        .options(MpqCreateOptions::default().create_listfile(false))
        .add_file("war3map.j", b"function main takes nothing returns nothing\nendfunction\n".to_vec())
        .add_file("Scripts\\Blizzard.j", b"function F takes nothing returns nothing\nendfunction\n".to_vec())
        .add_file("UI\\FrameDef\\UI\\EscMenuTemplates.fdf", b"x".to_vec())
        .add_file("secret.j", b"".to_vec())
        .build()
        .unwrap();
    let all = names(&map);
    assert!(all.iter().any(|n| n.eq_ignore_ascii_case("Scripts\\Blizzard.j")), "{all:?}");
    assert!(all.iter().any(|n| n.eq_ignore_ascii_case("UI\\FrameDef\\UI\\EscMenuTemplates.fdf")), "{all:?}");
    assert!(all.iter().any(|n| n == "war3map.j"));
    assert!(!all.iter().any(|n| n == "secret.j"), "unknown names can't be found");
    let scripts: Vec<PathBuf> = files(&map).into_iter().map(|(p, _)| p).collect();
    assert_eq!(scripts.len(), 2, "{scripts:?}");
}

#[test]
fn the_script_in_scripts_wins_over_the_one_at_the_top() {
    let tmp = TempDir::new("map-scripts-order");
    let map = tmp.0.join("Both.w3x");
    MpqArchiveBuilder::new(&map)
        .options(storm_rs::archive::MpqCreateOptions::default().create_listfile(false))
        .add_file("war3map.j", b"// top\n".to_vec())
        .add_file("Scripts\\war3map.j", b"// scripts\n".to_vec())
        .add_file("war3map.w3i", b"x".to_vec())
        .add_file("war3mapMisc.txt", b"x".to_vec())
        .build()
        .unwrap();
    let all = names(&map);
    assert_eq!(all, vec!["Scripts\\war3map.j", "war3map.j", "war3map.w3i", "war3mapMisc.txt"]);
    assert!(is_unused_script(&all, "war3map.j") && !is_unused_script(&all, "Scripts\\war3map.j"));
    let scripts: Vec<PathBuf> = files(&map).into_iter().map(|(p, _)| p).collect();
    // `war3mapMisc.txt` is BNI, a source too.
    assert_eq!(scripts, vec![map.join("Scripts").join("war3map.j"), map.join("war3mapMisc.txt")]);

    // Without one in `Scripts`, the top one is the script.
    let top = tmp.0.join("Top.w3x");
    make_map(&top, &[("war3map.j", "// top\n")]);
    assert!(!is_unused_script(&names(&top), "war3map.j"));
    assert_eq!(files(&top).len(), 1);
}
