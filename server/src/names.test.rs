use super::*;
use crate::project::tests::TempDir;

const WEATHER: &str = "ID;PWXL;N;E\nC;X1;Y1;K\"effectID\"\nC;X2;K\"name\"\nC;X1;Y2;K\"RAhr\"\nC;X2;K\"WESTRING_WEATHER_RAIN\"\nC;X1;Y3;K\"XXxx\"\nC;X2;K\"Plain Name\"\nE\n";

fn write(dir: &std::path::Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn names_come_from_the_layers_and_the_last_wins() {
    let tmp = TempDir::new("names");
    let game = tmp.0.join("game");
    write(&game, "TerrainArt/Weather.slk", WEATHER);
    write(&game, "UI/WorldEditStrings.txt", "[WorldEditStrings]\nWESTRING_WEATHER_RAIN=\"Rain (Heavy)\"\nWESTRING_LOCALE_L=Lordaeron Summer\n");
    write(&game, "UI/WorldEditData.txt", "[TileSets]\nL=WESTRING_LOCALE_L,TerrainArt\\Blight\\Lords_Blight\nAB=bad\n[Other]\nX=no\n");
    let lookup = Lookup { game: vec![game.clone()], roots: vec![] };
    let names = names(&lookup);
    assert_eq!(names.weather("RAhr"), Some("Rain (Heavy)"));
    assert_eq!(names.weather("XXxx"), Some("Plain Name"));
    assert_eq!(names.weather("none"), None);
    assert_eq!(names.tileset('L'), Some("Lordaeron Summer"));
    assert_eq!(names.tileset('l'), Some("Lordaeron Summer"));
    assert_eq!(names.tilesets.len(), 1);

    // A root replaces the strings: the names change with it.
    let root = tmp.0.join("map");
    write(&root, "ui/worldeditstrings.txt", "[WorldEditStrings]\nWESTRING_WEATHER_RAIN=Downpour\n");
    let lookup = Lookup { game: vec![game.clone()], roots: vec![root.clone()] };
    let over = super::names(&lookup);
    assert_eq!(over.weather("RAhr"), Some("Downpour"));
    // The whole file is replaced, not merged.
    assert_eq!(over.tileset('L'), Some("WESTRING_LOCALE_L"));

    let mut cache = Cache::default();
    assert_eq!(cache.get(&lookup).weather("RAhr"), Some("Downpour"));
    assert!(!cache.disk_changed(&[game.join("Maps/x.w3x")]));
    assert!(cache.disk_changed(&[root.join("anything.txt")]));
    cache.get(&lookup);
    assert!(cache.disk_changed(&[game.join("UI/WorldEditData.txt")]));
    assert!(super::names(&Lookup::default()).weather.is_empty());
}

/// The names the game's own files give: `WC3_GAME` is a game folder.
#[test]
#[ignore]
fn the_games_names() {
    let game = std::env::var("WC3_GAME").expect("WC3_GAME");
    let names = names(&Lookup { game: vec![game.into()], roots: vec![] });
    println!("{} weathers, {} tilesets", names.weather.len(), names.tilesets.len());
    assert_eq!(names.weather("RAhr"), Some("Ashenvale Rain (Heavy)"));
    assert_eq!(names.weather("WNcw"), Some("Wind (Heavy)"));
    assert_eq!(names.tileset('L'), Some("Lordaeron Summer"));
    assert_eq!(names.tileset('Q'), Some("Village Fall"));
}
