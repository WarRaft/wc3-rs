use super::*;
use crate::testdir::{self, TestDir};

fn meta(rows: &[&[&str]]) -> Metadata {
    Metadata::from_tables(&String::from_utf8(testdir::slk(rows)).unwrap(), &[])
}

#[test]
fn the_games_values_by_key_level_and_index() {
    let dir = TestDir::new("objdata-standard");
    dir.write("Units/AbilityData.slk", &testdir::slk(&[&["alias", "code", "levels", "Cool1", "Cool2", "DataA1"], &["AHbz", "AHbz", "3", "6", "7", "12"]]));
    dir.write("Units/HumanAbilityFunc.txt", b"[AHbz]\nButtonpos=0,2\n");
    dir.write("Units/HumanAbilityStrings.txt", b"[AHbz]\nName=Blizzard\nTip=\"One\",\"Two\",\"Three\"\n");
    let (game, _) = Game::open(&[dir.path().to_path_buf()]);
    let profile = Standard::profile(&game);
    let s = Standard::load(&game, Kind::Ability, &profile);
    assert_eq!(s.ids, ["AHbz"]);
    let m = meta(&[&["ID", "field", "repeat", "data", "index"], &["acdn", "Cool", "1", "", ""], &["Hbz1", "Data", "1", "1", ""], &["anam", "Name", "", "", ""], &["atp1", "Tip", "1", "", ""], &["abpy", "Buttonpos", "", "", "1"]]);
    let f = |id: &[u8; 4]| m.get(Rawcode(*id)).unwrap();
    assert_eq!(s.value("AHbz", f(b"acdn"), 2).as_deref(), Some("7"));
    assert_eq!(s.value("AHbz", f(b"Hbz1"), 1).as_deref(), Some("12"));
    assert_eq!(s.value("AHbz", f(b"anam"), 0).as_deref(), Some("Blizzard"));
    assert_eq!(s.value("AHbz", f(b"atp1"), 2).as_deref(), Some("Two"));
    assert_eq!(s.value("AHbz", f(b"abpy"), 0).as_deref(), Some("2"));
    assert_eq!(s.get("AHbz", "LEVELS"), Some("3"));
}

#[test]
fn a_field_set_and_taken_out_new_ids_and_values_in_limits() {
    let mut data = ObjectData::new(Kind::Unit);
    let (hfoo, hp) = (Rawcode(*b"hfoo"), Rawcode(*b"uhpm"));
    set(&mut data, hfoo, hp, 0, 0, Some(Value::Int(500)));
    assert_eq!(data.originals.len(), 1);
    set(&mut data, hfoo, hp, 0, 0, Some(Value::Int(600)));
    assert_eq!(data.originals[0].mods().map(|m| m.value.clone()).collect::<Vec<_>>(), [Value::Int(600)]);
    set(&mut data, hfoo, hp, 0, 0, None);
    assert!(data.originals.is_empty(), "its last change out: the game's");
    let taken = [Rawcode(*b"h000"), Rawcode(*b"h001")];
    assert_eq!(new_id(Kind::Unit, hfoo, |id| taken.contains(&id)), Some(Rawcode(*b"h002")));
    assert_eq!(new_id(Kind::Unit, Rawcode(*b"Hpal"), |_| false), Some(Rawcode(*b"H000")));
    assert_eq!(new_id(Kind::Ability, Rawcode(*b"AHbz"), |_| false), Some(Rawcode(*b"A000")));
    let m = meta(&[&["ID", "field", "type", "minVal", "maxVal"], &["uhpm", "HP", "int", "1", "100000"], &["umvs", "spd", "unreal", "0", "522"], &["ucol", "collision", "bool", "", ""]]);
    let f = |id: &[u8; 4]| m.get(Rawcode(*id));
    assert_eq!(to_value("250", f(b"uhpm")), Ok(Value::Int(250)));
    assert!(to_value("0", f(b"uhpm")).unwrap_err().contains("1 to 100000"));
    assert!(to_value("2.5", f(b"uhpm")).unwrap_err().contains("whole"));
    assert_eq!(to_value("300.5", f(b"umvs")), Ok(Value::Unreal(300.5)));
    assert!(to_value("2", f(b"ucol")).is_err());
    assert_eq!(to_value("hi", None), Ok(Value::Str("hi".into())));
    assert_eq!(shown(&Value::Real(2.0)), "2");
}
