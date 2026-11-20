use super::*;
use crate::map::tests::make_map;
use crate::project::tests::TempDir;

fn setup(name: &str) -> (TempDir, PathBuf, MapFs) {
    let tmp = TempDir::new(name);
    let map = tmp.0.join("Test.w3x");
    make_map(&map, &[("war3map.j", "// j\n"), ("UI\\a.fdf", "a"), ("UI\\Sub\\b.txt", "b"), ("war3map.w3i", "i")]);
    (tmp, map, MapFs::default())
}

fn dir(fs: &mut MapFs, path: &Path) -> Vec<String> {
    fs.read_dir(path)
        .unwrap()
        .into_iter()
        .map(|(n, k)| if k == Kind::Dir { format!("{n}/") } else { n })
        .collect()
}

#[test]
fn a_map_reads_like_a_folder() {
    let (_tmp, map, mut fs) = setup("mapfs-read");
    assert_eq!(dir(&mut fs, &map), vec!["UI/", "war3map.j", "war3map.w3i"]);
    assert_eq!(dir(&mut fs, &map.join("ui")), vec!["Sub/", "a.fdf"]);
    assert_eq!(fs.stat(&map).unwrap().kind, Kind::Dir);
    let a = fs.stat(&map.join("UI/A.FDF")).unwrap();
    assert_eq!((a.kind, a.size, a.readonly), (Kind::File, 1, false));
    assert_eq!(fs.read(&map.join("UI/Sub/b.txt")).unwrap(), b"b");
    assert_eq!(fs.stat(&map.join("nope")), Err(FsError::NotFound));
    assert_eq!(fs.read_dir(&map.join("war3map.j")), Err(FsError::NotADirectory));
    assert_eq!(fs.read(&map.join("UI")), Err(FsError::IsADirectory));
}

#[test]
fn writes_add_replace_and_keep_the_rest() {
    let (_tmp, map, mut fs) = setup("mapfs-write");
    let before = fs.stat(&map.join("war3map.w3i")).unwrap().mtime;
    fs.write(&map.join("war3map.j"), b"// new\n".to_vec(), false, true).unwrap();
    fs.write(&map.join("New/c.j"), b"c".to_vec(), true, false).unwrap();
    assert_eq!(fs.read(&map.join("war3map.j")).unwrap(), b"// new\n");
    assert_eq!(fs.read(&map.join("new/C.J")).unwrap(), b"c");
    assert_eq!(fs.read(&map.join("UI/a.fdf")).unwrap(), b"a");
    assert_eq!(dir(&mut fs, &map), vec!["New/", "UI/", "war3map.j", "war3map.w3i"]);
    // Writing one file doesn't make the others look changed.
    assert_eq!(fs.stat(&map.join("war3map.w3i")).unwrap().mtime, before);
    // A fresh look at the map agrees.
    assert_eq!(dir(&mut MapFs::default(), &map), vec!["New/", "UI/", "war3map.j", "war3map.w3i"]);

    assert_eq!(fs.write(&map.join("war3map.j"), vec![], true, false), Err(FsError::Exists));
    assert_eq!(fs.write(&map.join("missing.j"), vec![], false, true), Err(FsError::NotFound));
    assert_eq!(fs.write(&map.join("UI"), vec![], true, true), Err(FsError::IsADirectory));
    assert_eq!(fs.write(&map.join("war3map.j/x"), vec![], true, true), Err(FsError::NotADirectory));
    assert!(!map.with_file_name(".Test.w3x.wc3-saving").exists());
}

#[test]
fn delete_and_rename_files_and_folders() {
    let (_tmp, map, mut fs) = setup("mapfs-delete");
    fs.delete(&map.join("war3map.w3i"), false).unwrap();
    assert_eq!(fs.stat(&map.join("war3map.w3i")), Err(FsError::NotFound));
    assert!(matches!(fs.delete(&map.join("UI"), false), Err(FsError::Other(_))));
    fs.rename(&map.join("UI"), &map.join("Frames"), false).unwrap();
    assert_eq!(dir(&mut fs, &map.join("Frames")), vec!["Sub/", "a.fdf"]);
    assert_eq!(fs.read(&map.join("Frames/Sub/b.txt")).unwrap(), b"b");
    assert_eq!(fs.stat(&map.join("UI")), Err(FsError::NotFound));
    // A new spelling of the same name.
    fs.rename(&map.join("war3map.j"), &map.join("WAR3MAP.J"), false).unwrap();
    assert_eq!(dir(&mut fs, &map), vec!["Frames/", "WAR3MAP.J"]);
    assert_eq!(fs.rename(&map.join("Frames/a.fdf"), &map.join("war3map.j"), false), Err(FsError::Exists));
    fs.delete(&map.join("Frames"), true).unwrap();
    assert_eq!(dir(&mut fs, &map), vec!["WAR3MAP.J"]);
    assert!(matches!(fs.delete(&map, true), Err(FsError::NoPermissions(_))));
}

#[test]
fn moving_between_maps() {
    let (tmp, map, mut fs) = setup("mapfs-move");
    let other = tmp.0.join("Other.w3m");
    make_map(&other, &[("war3map.j", "// other\n")]);
    fs.rename(&map.join("UI/a.fdf"), &other.join("UI/a.fdf"), false).unwrap();
    assert_eq!(fs.read(&other.join("UI/a.fdf")).unwrap(), b"a");
    assert_eq!(fs.stat(&map.join("UI/a.fdf")), Err(FsError::NotFound));
}

#[test]
fn empty_folders_live_until_a_file_fills_them() {
    let (_tmp, map, mut fs) = setup("mapfs-mkdir");
    fs.create_dir(&map.join("Empty")).unwrap();
    assert_eq!(fs.create_dir(&map.join("empty")), Err(FsError::Exists));
    assert_eq!(dir(&mut fs, &map), vec!["Empty/", "UI/", "war3map.j", "war3map.w3i"]);
    fs.write(&map.join("Empty/x.txt"), b"x".to_vec(), true, false).unwrap();
    assert_eq!(dir(&mut fs, &map.join("Empty")), vec!["x.txt"]);
    fs.create_dir(&map.join("Gone")).unwrap();
    fs.delete(&map.join("Gone"), false).unwrap();
    assert_eq!(fs.stat(&map.join("Gone")), Err(FsError::NotFound));
}

#[test]
fn the_games_archives_are_read_only() {
    let tmp = TempDir::new("mapfs-mpq");
    let mpq = tmp.0.join("War3Patch.mpq");
    make_map(&mpq, &[("Scripts\\Blizzard.j", "// b\n")]);
    let mut fs = MapFs::default();
    assert!(fs.stat(&mpq.join("Scripts/Blizzard.j")).unwrap().readonly);
    assert!(matches!(fs.write(&mpq.join("Scripts/Blizzard.j"), vec![], true, true), Err(FsError::NoPermissions(_))));
    assert_eq!(fs.read(&mpq.join("Scripts/Blizzard.j")).unwrap(), b"// b\n");
}

#[test]
fn an_unused_top_script_is_noted() {
    let tmp = TempDir::new("mapfs-note");
    let map = tmp.0.join("Both.w3x");
    make_map(&map, &[("war3map.j", "// top\n"), ("Scripts\\war3map.j", "// scripts\n")]);
    let mut fs = MapFs::default();
    assert!(fs.stat(&map.join("war3map.j")).unwrap().note.contains("unused"));
    assert!(fs.stat(&map.join("Scripts/war3map.j")).unwrap().note.is_empty());
}

/// The real maps, when they're on this machine: a copy written to keeps
/// every file.
#[test]
fn real_maps_survive_a_write() {
    let maps = Path::new("/Volumes/TrascendSSD/Work/maps");
    let Ok(found) = std::fs::read_dir(maps) else {
        eprintln!("skipped: no maps");
        return;
    };
    let tmp = TempDir::new("mapfs-real");
    for project in found.flatten() {
        for file in std::fs::read_dir(project.path()).into_iter().flatten().flatten() {
            let src = file.path();
            if !crate::map::is_map(&src) {
                continue;
            }
            let copy = tmp.0.join(src.file_name().unwrap());
            std::fs::copy(&src, &copy).unwrap();
            let mut fs = MapFs::default();
            if fs.stat(&copy).unwrap().readonly {
                eprintln!("read-only: {}", src.display());
                continue;
            }
            let before = MpqArchive::open(&copy).unwrap();
            let names = crate::map::names_in(&before);
            let data: Vec<Vec<u8>> = names.iter().map(|n| before.read_file(n).unwrap()).collect();
            drop(before);
            fs.write(&copy.join("wc3-test.txt"), b"hello".to_vec(), true, false).unwrap();
            let after = MpqArchive::open(&copy).unwrap();
            for (n, d) in names.iter().zip(&data) {
                assert_eq!(&after.read_file(n).unwrap(), d, "{n} in {}", src.display());
            }
            assert_eq!(after.read_file("wc3-test.txt").unwrap(), b"hello");
        }
    }
}
