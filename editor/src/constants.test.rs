use super::*;

#[test]
fn a_key_set_on_its_line_added_to_its_section_or_a_new_one_and_taken_out() {
    let text = "[Misc]\r\nMaxHeroLevel=100\r\nHeroFactorXP=80,70\r\n\r\n";
    assert_eq!(get(text, "misc", "maxherolevel").as_deref(), Some("100"));
    // On its line, the rest as it was.
    assert_eq!(set(text, "Misc", "MaxHeroLevel", Some("20")), "[Misc]\r\nMaxHeroLevel=20\r\nHeroFactorXP=80,70\r\n\r\n");
    // Added after the section's last line.
    assert_eq!(set(text, "Misc", "MaxUnitLevel", Some("40")), "[Misc]\r\nMaxHeroLevel=100\r\nHeroFactorXP=80,70\r\nMaxUnitLevel=40\r\n\r\n");
    // Out.
    assert_eq!(set(text, "Misc", "MaxHeroLevel", None), "[Misc]\r\nHeroFactorXP=80,70\r\n\r\n");
    // Another section, at the end.
    assert_eq!(set(text, "TWN1", "DependencyOr", Some("htow")), "[Misc]\r\nMaxHeroLevel=100\r\nHeroFactorXP=80,70\r\n\r\n[TWN1]\r\nDependencyOr=htow\r\n");
    // An empty file: World Editor's line ends.
    assert_eq!(set("", "Misc", "GlobalExperience", Some("0")), "[Misc]\r\nGlobalExperience=0\r\n");
    // Its own line ends kept; a key in another section left.
    let unix = "[Misc]\nA=1\n[Other]\nA=2\n";
    assert_eq!(set(unix, "Misc", "A", Some("3")), "[Misc]\nA=3\n[Other]\nA=2\n");
}

#[test]
fn values_checked_by_their_types_and_limits() {
    let f = |ty: &str, min: &str, max: &str| Field { id: "x".into(), key: "K".into(), section: "Misc".into(), ty: ty.into(), name: "A field".into(), min: min.into(), max: max.into() };
    assert_eq!(check(&f("int", "1", "10000"), " 25 ").unwrap(), "25");
    assert!(check(&f("int", "1", "10000"), "0").unwrap_err().contains("1 at least"));
    assert!(check(&f("int", "", ""), "2.5").unwrap_err().contains("whole number"));
    assert_eq!(check(&f("bool", "", ""), "1").unwrap(), "1");
    assert!(check(&f("bool", "", ""), "yes").is_err());
    assert!(check(&f("unreal", "", ""), "-1").unwrap_err().contains("not below 0"));
    assert_eq!(check(&f("unrealList", "", ""), "1, 0.5 ,2").unwrap(), "1,0.5,2");
    assert_eq!(check(&f("unitList", "", ""), "htow, ogre").unwrap(), "htow,ogre");
    assert!(check(&f("unitList", "", ""), "htown").is_err());
    assert!(same("1", "1.0") && same("80, 70", "80,70.0") && !same("1,2", "1"));
}

#[test]
fn a_frame_files_string_list() {
    let fdf = "/* x */\nStringList {\n    ACCEPT   \"Accept\",\n    LOADING_WAITING_FOR_PLAYERS \"WAITING FOR OTHER PLAYERS\",\n    LAST \"a, b\"\n}\nOther { X \"no\", }\n";
    assert_eq!(string_list(fdf), [("ACCEPT".into(), "Accept".into()), ("LOADING_WAITING_FOR_PLAYERS".into(), "WAITING FOR OTHER PLAYERS".into()), ("LAST".into(), "a, b".into())]);
}
