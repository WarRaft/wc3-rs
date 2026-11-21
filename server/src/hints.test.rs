use super::*;

fn names() -> Names {
    let mut n = Names::default();
    n.weather.insert("RAhr".into(), "Ashenvale Rain (Heavy)".into());
    n.tilesets.insert('L', "Lordaeron Summer".into());
    n
}

#[test]
fn region_weather() {
    let text = "[w3r]\nversion=5\n[A]\nweather=RAhr\n[B]\nweather=0x00000000\n[C]\nweather=ABcd // x\n";
    let path = Path::new("war3map.w3r");
    assert_eq!(
        hints(text, path, &names(), 0, 20),
        r#"[[3,12,"Ashenvale Rain (Heavy)"],[5,18,"none"],[7,12,"not in TerrainArt\\Weather.slk"]]"#
    );
    assert_eq!(hints(text, path, &names(), 4, 5), r#"[[5,18,"none"]]"#);
    // Without the game's files, only `none`.
    assert_eq!(hints(text, path, &Names::default(), 0, 20), r#"[[5,18,"none"]]"#);
}

#[test]
fn tilesets() {
    assert_eq!(hints("[w3e]\ntileset=L\n", Path::new("war3map.w3e"), &names(), 0, 9), r#"[[1,9,"Lordaeron Summer"]]"#);
    let text = "[w3i]\ntileset=l\nlight_tileset=0\n[player]\ntileset=L\n";
    assert_eq!(
        hints(text, Path::new("x/war3map.W3I"), &names(), 0, 9),
        r#"[[1,9,"Lordaeron Summer"],[2,15,"the map's own"]]"#
    );
    assert_eq!(hints("[w3i]\ntileset=Q\n", Path::new("war3map.w3i"), &names(), 0, 9), r#"[[1,9,"not in UI\\WorldEditData.txt [TileSets]"]]"#);
    // The wrong text form for the file: nothing.
    assert_eq!(hints("[w3r]\ntileset=L\n", Path::new("war3map.w3e"), &names(), 0, 9), "[]");
    assert!(has_names(Path::new("a/war3map.W3R")) && !has_names(Path::new("war3map.w3c")));
}
