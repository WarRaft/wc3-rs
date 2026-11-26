use super::*;

/// A Reforged install (`CASC_GAME`; skipped without it): its mods in
/// order, a file SD and HD, through the game's layers.
#[test]
fn a_reforged_folder_as_the_game() {
    let Some(folder) = std::env::var_os("CASC_GAME").map(PathBuf::from) else { return };
    assert!(is_storage(&folder));
    let sd = Mods::open(&folder, Graphics::Sd, None, None).unwrap();
    let hd = Mods::open(&folder, Graphics::Hd, None, None).unwrap();
    eprintln!("{:?}\n{:?}", sd.prefixes, hd.prefixes);
    assert_eq!(sd.prefixes.last().unwrap(), "War3.w3mod:_Deprecated.w3mod:");
    assert!(hd.prefixes.iter().any(|p| p == "War3.w3mod:_HD.w3mod:"));
    let footman = "Units\\Human\\Footman\\Footman.mdx";
    let (s, h) = (sd.read(footman).unwrap(), hd.read(footman).unwrap());
    assert!(h.len() > s.len() * 4, "{} {}", s.len(), h.len());
    assert_eq!(hd.find(footman), Some(Source::Storage(folder.clone(), "War3.w3mod:_HD.w3mod:Units/Human/Footman/Footman.mdx".into())));
    // A texture asked for as BLP: its DDS, decoded.
    let (bytes, source) = (hd.read("Textures\\Footman.blp").unwrap(), hd.find("Textures\\Footman.blp").unwrap());
    assert!(matches!(&source, Source::Storage(_, n) if n.ends_with(".dds")), "{source:?}");
    let t = crate::texture::decode(&bytes);
    assert!(t.error.is_empty() && !t.mips[0].rgba.is_empty(), "{}", t.error);
    eprintln!("Footman: {} {}x{}, {} mips", t.encoding, t.width, t.height, t.mips.len());
    // Through `Lookup`: the tables and the editor's strings.
    let lookup = crate::game::Lookup { game: vec![folder.clone()], hd: true, ..Default::default() };
    let (game, errors) = lookup.open();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(game.read("Units\\UnitData.slk").unwrap().0.starts_with(b"ID;"));
    assert!(game.read_patched("UI\\WorldEditStrings.txt").is_some_and(|t| t.contains("WESTRING")));
    // 2.0: Lordaeron Capital's ground, in HD and in Definitive alone; a
    // tileset's own (Lordaeron Summer's bridge) through its mod.
    let grass = "TerrainArt\\LordaeronCapital\\LordC_GrassTrim_Diffuse.dds";
    for g in [Graphics::Hd, Graphics::De] {
        let m = Mods::open(&folder, g, Some('L'), None).unwrap();
        assert!(m.find(grass).is_some(), "{g:?}");
        assert!(m.find("ReplaceableTextures\\Bridges\\ElvenBridge_Diffuse.dds").is_some_and(|s| matches!(&s, Source::Storage(_, n) if n.contains("_Tilesets/L.w3mod:"))), "{g:?}");
    }
    assert!(sd.find(grass).is_none());
}

/// The mods' order, as HiveWE reads them: graphics' tileset, locale,
/// itself; the game's tileset, locale, itself; deprecated last.
#[test]
fn the_mods_in_order() {
    assert_eq!(
        prefixes(Graphics::De, Some('L'), Some("ruRU")),
        [
            "War3.w3mod:_DE.w3mod:_Tilesets/L.w3mod:",
            "War3.w3mod:_DE.w3mod:_Locales/ruRU.w3mod:",
            "War3.w3mod:_DE.w3mod:",
            "War3.w3mod:_Tilesets/L.w3mod:",
            "War3.w3mod:_Locales/ruRU.w3mod:",
            "War3.w3mod:",
            "War3.w3mod:_Deprecated.w3mod:",
        ]
    );
    assert_eq!(prefixes(Graphics::Sd, None, None), ["War3.w3mod:", "War3.w3mod:_Deprecated.w3mod:"]);
    assert_eq!(prefixes(Graphics::Hd, None, None)[0], "War3.w3mod:_HD.w3mod:");
    assert_eq!(Graphics::of(true, true), Graphics::De);
}

/// Every DDS on disk decoded (`CASC_GAME`; ignored: it decodes them all).
#[test]
#[ignore]
fn every_dds_decodes() {
    let Some(folder) = std::env::var_os("CASC_GAME").map(PathBuf::from) else { return };
    let o = open(&folder).unwrap();
    let (mut read, mut failed) = (0, Vec::new());
    for e in &o.storage.entries {
        if !e.name.to_ascii_lowercase().ends_with(".dds") || !o.storage.has(&e.name) {
            continue;
        }
        let t = crate::texture::decode(&o.storage.read(&e.name).unwrap());
        if !t.error.is_empty() || t.mips.first().is_none_or(|m| m.rgba.is_empty()) {
            failed.push(format!("{}: {}", e.name, t.error));
        } else {
            read += 1;
        }
    }
    eprintln!("{read} decoded, {} failed", failed.len());
    for f in failed.iter().take(10) {
        eprintln!("  {f}");
    }
    assert!(failed.is_empty());
}
