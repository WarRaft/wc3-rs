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
    write(&game, "UI/WorldEditStrings.txt", "[WorldEditStrings]\nWESTRING_WEATHER_RAIN=\"Rain (Heavy)\"\nWESTRING_LOCALE_L=Lordaeron Summer\nWESTRING_EAX_SPELLS=Spells\nWESTRING_SOUNDCHANNEL_COMBAT=Combat\nWESTRING_SNDPROPS_LOOPING=&Looping\nWESTRING_PLAYER_NEUTRAL=WESTRING_NEUTRAL\nWESTRING_NEUTRAL=Neutral\nWESTRING_AMP=Rock && Roll\n");
    write(&game, "UI/WorldEditData.txt", "[TileSets]\nL=WESTRING_LOCALE_L,TerrainArt\\Blight\\Lords_Blight\nAB=bad\n[Other]\nX=no\n[SoundChannels]\n05=1,WESTRING_SOUNDCHANNEL_COMBAT\nxx=1,No\n");
    write(&game, "UI/SoundInfo/EAXDefs.slk", "ID;PWXL;N;E\nC;X1;Y1;K\"EAXLabel\"\nC;X2;K\"DisplayText\"\nC;X1;Y2;K\"SpellsEAX\"\nC;X2;K\"WESTRING_EAX_SPELLS\"\nE\n");
    let lookup = Lookup { game: vec![game.clone()], roots: vec![] };
    let names = names(&lookup);
    assert_eq!(names.weather("RAhr"), Some("Rain (Heavy)"));
    assert_eq!(names.weather("XXxx"), Some("Plain Name"));
    assert_eq!(names.weather("none"), None);
    assert_eq!(names.tileset('L'), Some("Lordaeron Summer"));
    assert_eq!(names.tileset('l'), Some("Lordaeron Summer"));
    assert_eq!(names.tilesets.len(), 1);
    assert_eq!(names.eax("SpellsEAX"), Some("Spells"));
    assert_eq!(names.channel(5), Some("Combat"));
    assert_eq!(names.channels.len(), 1);
    // The editor's strings: access keys dropped, keys followed.
    assert_eq!(names.string("WESTRING_SNDPROPS_LOOPING").as_deref(), Some("Looping"));
    assert_eq!(names.string("WESTRING_PLAYER_NEUTRAL").as_deref(), Some("Neutral"));
    assert_eq!(names.string("WESTRING_AMP").as_deref(), Some("Rock & Roll"));
    assert_eq!(names.string("WESTRING_MISSING"), None);
    assert_eq!(names.say("WESTRING_MISSING", "fallback"), "fallback");

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

#[test]
fn the_editors_files_read_with_their_patches() {
    let tmp = TempDir::new("names-patches");
    let game = tmp.0.join("game");
    write(&game, "UI/TriggerData.txt", "[TriggerActions]\nDoNothing=0,nothing\n_DoNothing_Category=TC_NOTHING\n");
    write(&game, "UI/TriggerStrings.txt", "[TriggerActionStrings]\nDoNothing=\"Do Nothing\"\nDoNothing=\"Do nothing\"\n");
    write(&game, "UI/WorldEditStrings.txt", "[WorldEditStrings]\nWESTRING_WEATHER_RAIN=Rain\n");
    // A root adds a function, its phrase and a string, and renames one.
    let root = tmp.0.join("map");
    write(&root, "UI/TriggerData.patch.txt", "[TriggerActions]\nMyAction=0,unit,real\n_MyAction_Category=TC_UNIT\n");
    write(&root, "UI/TriggerStrings.patch.txt", "[TriggerActionStrings]\nMyAction=\"My Action\"\nMyAction=\"Hit \",~Unit,\" for \",~Amount\n");
    write(&root, "UI/WorldEditStrings.patch.txt", "[WorldEditStrings]\nWESTRING_WEATHER_RAIN=Downpour\nWESTRING_MINE=Mine\n");
    let lookup = Lookup { game: vec![game.clone()], roots: vec![root.clone()] };
    let names = names(&lookup);
    assert_eq!(names.triggers.params(wtg_rs::FunctionKind::Action, "MyAction"), Some(2));
    assert_eq!(names.triggers.params(wtg_rs::FunctionKind::Action, "DoNothing"), Some(0));
    let ui = names.trigger_ui.function(wtg_rs::FunctionKind::Action, "MyAction").unwrap();
    assert_eq!(ui.title, "My Action");
    assert_eq!(ui.args, ["unit", "real"]);
    assert_eq!(names.string("WESTRING_WEATHER_RAIN").as_deref(), Some("Downpour"));
    assert_eq!(names.string("WESTRING_MINE").as_deref(), Some("Mine"));

    // A patch changing on disk is a change of the names.
    let mut cache = Cache::default();
    cache.get(&lookup);
    assert!(cache.disk_changed(&[game.join("UI/TriggerData.patch.txt")]));
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
    assert_eq!(names.eax("DefaultEAXON"), Some("Default"));
    assert_eq!(names.eax("HeroAcksEAX"), Some("Hero Speech"));
    assert_eq!(names.channel(14), Some("Fire"));
    assert_eq!(names.ground.get("Ldrt").map(String::as_str), Some("Dirt"));
    assert_eq!(names.cliffs.get("CLdi").map(String::as_str), Some("Dirt Cliff"));
    assert_eq!(names.string("WESTRING_SNDPROPS_3DSOUND").as_deref(), Some("3D sound"));
    assert_eq!(names.string("WESTRING_RACE_NIGHTELF").as_deref(), Some("Night Elf"));

}

#[test]
fn names_without_markup() {
    assert_eq!(plain("|cff804040Zombie|r"), "Zombie");
    assert_eq!(plain("|CFF36544aGhoul|R (strong)"), "Ghoul (strong)");
    assert_eq!(plain("a|nb || |cfz"), "a b || |cfz");
}

/// How long the names of a real game and map take (`WC3_GAME`, `MAP`).
#[test]
#[ignore]
fn names_load_time() {
    let lookup = crate::game::Lookup {
        game: std::env::split_paths(&std::env::var_os("WC3_GAME").expect("WC3_GAME")).collect(),
        roots: vec![std::path::PathBuf::from(std::env::var("MAP").expect("MAP"))],
    };
    for _ in 0..2 {
        let t = std::time::Instant::now();
        let n = names(&lookup);
        println!("{:?}: {} unit names", t.elapsed(), n.objects.of.get(&w3abdhqtu_rs::Kind::Unit).map_or(0, |u| u.len()));
    }
}
