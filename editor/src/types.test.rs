use super::*;
use crate::testdir::{self, TestDir};

#[test]
fn the_games_tables_and_profile_then_the_maps_object_data() {
    let dir = TestDir::new("types");
    dir.write("Units/UnitUI.slk", &testdir::slk(&[&["unitUIID", "file", "modelScale"], &["hfoo", "units\\human\\Footman\\Footman", "1"]]));
    dir.write("Units/ItemFunc.txt", b"[ratc]\nfile=Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl\n");
    dir.write("Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "file", "numVar"], &["LTlt", "Doodads\\Tree", "_"]]));
    // The map's: a custom footman, its model changed. Without the
    // metadata at hand, `umdl` names no key: the change is left out.
    let game = wc3_server::game::Lookup { game: vec![], roots: vec![dir.path().to_path_buf()], ..Default::default() }.open().0;
    let t = Types::load(&game);
    assert_eq!(t.get(Kind::Unit, "hfoo", "FILE"), Some("units\\human\\Footman\\Footman"));
    assert_eq!(t.number(Kind::Unit, "hfoo", "modelscale"), Some(1.0));
    assert_eq!(t.get(Kind::Item, "ratc", "file"), Some("Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl"));
    // `_` is no value.
    assert_eq!(t.get(Kind::Doodad, "LTlt", "numVar"), None);
    assert!(t.has(Kind::Doodad, "LTlt") && !t.has(Kind::Destructable, "LTlt"));
}

/// With the game: a map's custom unit takes its standard one's model,
/// or its own (`WC3_GAME`, a game folder; skipped without it).
#[test]
fn custom_objects_with_the_games_metadata() {
    let Some(folder) = std::env::var_os("WC3_GAME") else { return };
    let dir = TestDir::new("types-game");
    use parser_rs::bin::Rawcode;
    use w3abdhqtu_rs::{Mod, Object, Set};
    let object = |id: &[u8; 4], mods: Vec<Mod>| Object { base: Rawcode(*b"hfoo"), id: Rawcode(*id), sets: vec![Set { flags: 0, mods }] };
    let model = Mod { field: Rawcode(*b"umdl"), level: 0, column: 0, value: Value::Str("Units\\Mine.mdl".into()), end: Rawcode::NONE };
    let mut data = w3abdhqtu_rs::ObjectData::new(Kind::Unit);
    data.customs.push(object(b"h000", vec![model]));
    data.customs.push(object(b"h001", vec![]));
    dir.write("war3map.w3u", &data.to_bytes().unwrap());
    let lookup = wc3_server::game::Lookup { game: vec![folder.into()], roots: vec![dir.path().to_path_buf()], ..Default::default() };
    let t = Types::load(&lookup.open().0);
    assert_eq!(t.get(Kind::Unit, "h000", "file"), Some("Units\\Mine.mdl"));
    assert_eq!(t.get(Kind::Unit, "h001", "file"), t.get(Kind::Unit, "hfoo", "file"));
    assert!(t.get(Kind::Unit, "hfoo", "file").unwrap().to_lowercase().contains("footman"));
}
