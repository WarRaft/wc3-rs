use super::*;

/// A Reforged install (`CASC_GAME`; skipped without it): its mods in
/// order, a file SD and HD, through the game's layers.
#[test]
fn a_reforged_folder_as_the_game() {
    let Some(folder) = std::env::var_os("CASC_GAME").map(PathBuf::from) else { return };
    assert!(is_storage(&folder));
    let sd = Mods::open(&folder, false, None).unwrap();
    let hd = Mods::open(&folder, true, None).unwrap();
    eprintln!("{:?}\n{:?}", sd.prefixes, hd.prefixes);
    assert_eq!(sd.prefixes.last().unwrap(), "War3.w3mod:");
    assert!(hd.prefixes.iter().any(|p| p == "War3.w3mod:_HD.w3mod:"));
    let footman = "Units\\Human\\Footman\\Footman.mdx";
    let (s, h) = (sd.read(footman).unwrap(), hd.read(footman).unwrap());
    assert!(h.len() > s.len() * 4, "{} {}", s.len(), h.len());
    assert_eq!(hd.find(footman), Some(Source::Storage(folder.clone(), "War3.w3mod:_HD.w3mod:Units/Human/Footman/Footman.mdx".into())));
    // Through `Lookup`: the tables and the editor's strings.
    let lookup = crate::game::Lookup { game: vec![folder.clone()], hd: true, ..Default::default() };
    let (game, errors) = lookup.open();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(game.read("Units\\UnitData.slk").unwrap().0.starts_with(b"ID;"));
    assert!(game.read_patched("UI\\WorldEditStrings.txt").is_some_and(|t| t.contains("WESTRING")));
}
