use super::*;
use crate::project::tests::TempDir;
use storm_rs::archive::MpqArchiveBuilder;

fn mpq(path: &Path, files: &[(&str, &str)]) {
    let mut b = MpqArchiveBuilder::new(path);
    for (name, text) in files {
        b = b.add_file(*name, text.as_bytes().to_vec());
    }
    b.build().unwrap();
}

fn text(game: &Game, name: &str) -> Option<String> {
    game.read(name).map(|(data, _)| String::from_utf8(data).unwrap())
}

#[test]
fn archives_stack_in_game_order_and_the_folder_wins() {
    let tmp = TempDir::new("game-layers");
    let wc3 = tmp.0.join("wc3");
    std::fs::create_dir_all(wc3.join("Scripts")).unwrap();
    // Lowercase on disk, as some installs have it.
    mpq(&wc3.join("war3.mpq"), &[("Scripts\\common.j", "war3"), ("a.txt", "war3"), ("b.txt", "war3")]);
    mpq(&wc3.join("War3x.mpq"), &[("Scripts\\common.j", "war3x"), ("a.txt", "war3x")]);
    mpq(&wc3.join("War3Patch.mpq"), &[("Scripts\\common.j", "patch")]);
    std::fs::write(wc3.join("Scripts/Blizzard.j"), "folder").unwrap();

    let (game, errors) = Game::open(std::slice::from_ref(&wc3));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(text(&game, "Scripts\\common.j").as_deref(), Some("patch"));
    assert_eq!(text(&game, "a.txt").as_deref(), Some("war3x"));
    assert_eq!(text(&game, "b.txt").as_deref(), Some("war3"));
    assert_eq!(text(&game, "scripts/blizzard.J").as_deref(), Some("folder"));
    assert_eq!(game.find("Scripts\\Blizzard.j"), Some(Source::File(wc3.join("Scripts/Blizzard.j"))));
    assert_eq!(game.find("SCRIPTS/COMMON.J"), Some(Source::Archive(wc3.join("War3Patch.mpq"))));
    assert_eq!(game.find("missing.txt"), None);
    assert_eq!(game.find("../wc3/Scripts/Blizzard.j"), None);

    std::fs::write(wc3.join("Scripts/common.j"), "loose").unwrap();
    let (game, _) = Game::open(std::slice::from_ref(&wc3));
    assert_eq!(text(&game, "Scripts\\common.j").as_deref(), Some("loose"));
}

#[test]
fn a_later_folder_overrides_an_earlier_one() {
    let tmp = TempDir::new("game-override");
    let (wc3, over) = (tmp.0.join("wc3"), tmp.0.join("over"));
    std::fs::create_dir_all(wc3.join("Scripts")).unwrap();
    std::fs::create_dir_all(over.join("Scripts")).unwrap();
    std::fs::write(wc3.join("Scripts/common.j"), "game").unwrap();
    std::fs::write(wc3.join("Scripts/Blizzard.j"), "game").unwrap();
    std::fs::write(over.join("Scripts/common.j"), "custom").unwrap();
    mpq(&over.join("War3Patch.mpq"), &[("Scripts\\Blizzard.j", "over-mpq")]);

    let (game, _) = Game::open(&[wc3.clone(), over.clone(), tmp.0.join("missing")]);
    assert_eq!(text(&game, "Scripts\\common.j").as_deref(), Some("custom"));
    assert_eq!(text(&game, "Scripts\\Blizzard.j").as_deref(), Some("over-mpq"));

    let (game, _) = Game::open(&[over, wc3]);
    assert_eq!(text(&game, "Scripts\\common.j").as_deref(), Some("game"));
}

#[test]
fn a_broken_archive_is_reported_and_skipped() {
    let tmp = TempDir::new("game-broken");
    std::fs::write(tmp.0.join("War3.mpq"), "not an archive").unwrap();
    std::fs::write(tmp.0.join("x.txt"), "x").unwrap();
    let (game, errors) = Game::open(std::slice::from_ref(&tmp.0));
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("War3.mpq"), "{errors:?}");
    assert_eq!(text(&game, "x.txt").as_deref(), Some("x"));
}

/// The real game, when it's on this machine: 1.26a, with UJAPI's
/// `common.j` over it.
#[test]
fn real_game_with_an_override() {
    let wc3 = PathBuf::from("/Volumes/TrascendSSD/Warcraft III/Warcraft III 1.26a Full (EN)");
    let ujapi = PathBuf::from("/Volumes/TrascendSSD/UJAPI");
    if !wc3.is_dir() || !ujapi.is_dir() {
        eprintln!("skipped: no game at {}", wc3.display());
        return;
    }
    let (game, errors) = Game::open(&[wc3.clone(), ujapi.clone()]);
    assert!(errors.is_empty(), "{errors:?}");

    let (blizzard, from) = game.read("Scripts\\Blizzard.j").expect("Blizzard.j");
    assert!(matches!(from, Source::Archive(_)), "{from:?}");
    assert!(String::from_utf8_lossy(&blizzard).contains("function InitBlizzard"));

    let (common, from) = game.read("Scripts\\common.j").expect("common.j");
    assert_eq!(from, Source::File(ujapi.join("Scripts/common.j")));
    assert_eq!(common, std::fs::read(ujapi.join("Scripts/common.j")).unwrap());

    let (game, _) = Game::open(std::slice::from_ref(&wc3));
    let (common, from) = game.read("Scripts\\common.j").expect("game's own common.j");
    assert!(matches!(from, Source::Archive(_)), "{from:?}");
    assert!(String::from_utf8_lossy(&common).contains("native ConvertRace"));
}
