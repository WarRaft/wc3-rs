use super::*;

fn names() -> Names {
    let mut n = Names::default();
    n.weather.insert("RAhr".into(), "Ashenvale Rain (Heavy)".into());
    n.tilesets.insert('L', "Lordaeron Summer".into());
    n.ground.insert("Ldrt".into(), "Dirt".into());
    n.ground.insert("Lgrs".into(), "Grass".into());
    n.cliffs.insert("CLdi".into(), "Lordaeron Cliff (Dirt)".into());
    n.eax.insert("DefaultEAXON".into(), "Default".into());
    n.eax.insert("SpellsEAX".into(), "Spells".into());
    n
}

#[test]
fn sound_eax_channel_flags() {
    let text = "[w3s]\nversion=1\n[gg_snd_A]\neax=\"SpellsEAX\"\nchannel=5\nflags=0x00000017\n[gg_snd_B]\neax=\"\"\nchannel=default\nflags=0\neax=\"Nope\"\nchannel=15\n";
    assert_eq!(
        hints(text, Path::new("war3map.w3s"), &names(), None, 0, 20),
        concat!(
            r#"[[3,15,"Spells"],[4,9,"Combat"],[5,16,"looping, 3D sound, stop when out of range, imported"],"#,
            r#"[7,6,"Default"],[8,15,"default"],[10,10,"not in UI\\SoundInfo\\EAXDefs.slk"],[11,10,"not one of the game's channels (0 to 14)"]]"#
        )
    );
}

#[test]
fn region_weather() {
    let text = "[w3r]\nversion=5\n[A]\nweather=RAhr\n[B]\nweather=0x00000000\n[C]\nweather=ABcd // x\n";
    let path = Path::new("war3map.w3r");
    assert_eq!(
        hints(text, path, &names(), None, 0, 20),
        r#"[[3,12,"Ashenvale Rain (Heavy)"],[5,18,"none"],[7,12,"not in TerrainArt\\Weather.slk"]]"#
    );
    assert_eq!(hints(text, path, &names(), None, 4, 5), r#"[[5,18,"none"]]"#);
    // Without the game's files, only `none`.
    assert_eq!(hints(text, path, &Names::default(), None, 0, 20), r#"[[5,18,"none"]]"#);
}

#[test]
fn tilesets() {
    assert_eq!(hints("[w3e]\ntileset=L\n", Path::new("war3map.w3e"), &names(), None, 0, 9), r#"[[1,9,"Lordaeron Summer"]]"#);
    assert_eq!(
        hints("[w3e]\nground=Ldrt,Lgrs\ncliffs=CLdi\n", Path::new("war3map.w3e"), &names(), None, 0, 9),
        r#"[[1,16,"Dirt, Grass"],[2,11,"Lordaeron Cliff (Dirt)"]]"#
    );
    // A code the files don't name shows as it is; with the file's points,
    // how many use each.
    let usage = w3e_rs::text::Usage { ground: vec![7, 0], cliffs: vec![2] };
    let text = "[w3e]\nversion=11\nground=Ldrt,Xxxx\nwidth=3\noffset_x=-128\n";
    assert_eq!(
        hints(text, Path::new("war3map.w3e"), &names(), Some(&usage), 0, 9),
        r#"[[1,10,"up to 1.31"],[2,16,"Dirt 7, Xxxx 0"],[3,7,"2 cells"],[4,13,"-128 to 128 across"]]"#
    );
    let text = "[w3i]\ntileset=l\nlight_tileset=0\n[player]\ntileset=L\n";
    assert_eq!(
        hints(text, Path::new("x/war3map.W3I"), &names(), None, 0, 9),
        r#"[[1,9,"Lordaeron Summer"],[2,15,"the map's own"]]"#
    );
    assert_eq!(hints("[w3i]\ntileset=Q\n", Path::new("war3map.w3i"), &names(), None, 0, 9), r#"[[1,9,"not in UI\\WorldEditData.txt [TileSets]"]]"#);
    // The wrong text form for the file: nothing.
    assert_eq!(hints("[w3r]\ntileset=L\n", Path::new("war3map.w3e"), &names(), None, 0, 9), "[]");
    assert!(has_names(Path::new("a/war3map.W3R")) && !has_names(Path::new("war3map.w3c")));
}

#[test]
fn the_games_words_where_it_has_them() {
    let mut n = names();
    for (k, v) in [
        ("WESTRING_RACE_ORC", "Орк"),
        ("WESTRING_ALLIED", "Союзники"),
        ("WESTRING_NONE", "Нет"),
        ("WESTRING_PLAYER", "Игрок"),
        ("WESTRING_SNDPROPS_LOOPING", "&Повтор"),
        ("WESTRING_TECHTYPE_ITEM", "Предмет"),
    ] {
        n.strings.insert(k.into(), v.into());
    }
    n.channels.insert(5, "Бой".into());
    let text = "[w3i]\nversion=25\nflags=0x80000044\nfog_style=1\n[player]\nnumber=0\ntype=1\nrace=2\nally_high=0x00000006\n[force]\nflags=0x0000000B\nplayers=0\n[unit_table]\ncolumns=0,2\n";
    assert_eq!(
        hints(text, Path::new("war3map.w3i"), &n, None, 0, 30),
        concat!(
            r#"[[1,10,"The Frozen Throne"],[2,16,"melee map, use custom forces, 0x80000000"],[3,11,"linear"],"#,
            r#"[5,8,"Игрок 1"],[6,6,"user"],[7,6,"Орк"],[8,20,"players 2, 3"],"#,
            r#"[10,16,"Союзники, allied victory, shared vision"],[13,11,"unit, Предмет"]]"#
        )
    );
    let sound = "[w3s]\n[gg_snd_A]\nchannel=5\nflags=1\n";
    assert_eq!(hints(sound, Path::new("war3map.w3s"), &n, None, 0, 9), r#"[[2,9,"Бой"],[3,7,"Повтор"]]"#);
    let region = "[w3r]\n[A]\nweather=0x00000000\n";
    assert_eq!(hints(region, Path::new("war3map.w3r"), &n, None, 0, 9), r#"[[2,18,"Нет"]]"#);
}

#[test]
fn imports_say_where_the_file_is() {
    let text = "[imp]\nversion=1\nfile=8,\"Icon.blp\"\nfile=13,\"Armor\\Crab.mdx\"\n";
    assert_eq!(
        hints(text, Path::new("war3map.imp"), &names(), None, 0, 9),
        r#"[[2,17,"war3mapImported\\Icon.blp (unknown 0x08)"],[3,24,"Armor\\Crab.mdx (custom path, unknown 0x04, unknown 0x08)"]]"#
    );
}

#[test]
fn minimap_kinds_and_colors() {
    let text = "[mmp]\nversion=0\n[icon]\nkind=2\ncolor=255,3,3,255\n";
    assert_eq!(hints(text, Path::new("war3map.mmp"), &names(), None, 0, 9), r#"[[3,6,"start location"]]"#);
    assert_eq!(colors(text, Path::new("x/war3map.MMP")), "[[4,6,4,17,255,3,3,255]]");
    assert!(has_colors(Path::new("war3map.w3r")) && !has_colors(Path::new("war3map.w3s")));
}

#[test]
fn trigger_categories_by_index() {
    let text = "[wtg]\n\n[category]\nindex=38\nname=\"Cinematics, \\\"intro\\\"\"\n\n[trigger]\nname=\"A\"\ncategory=38\n\n[trigger]\ncategory=5\n";
    let path = Path::new("/m/war3map.wtg");
    assert!(has_names(path));
    assert_eq!(hints(text, path, &Names::default(), None, 0, 20), r#"[[8,11,"Cinematics, \"intro\""],[11,10,"no category 5"]]"#);
    // Only the lines asked for.
    assert_eq!(hints(text, path, &Names::default(), None, 10, 20), r#"[[11,10,"no category 5"]]"#);
}
