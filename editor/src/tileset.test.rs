use super::*;
use crate::testdir::slk;

#[test]
fn rows_by_id_columns_by_name_in_any_case() {
    let t = Slk::parse(std::str::from_utf8(&slk(&[&["tileID", "dir", "file"], &["Ldrt", "TerrainArt\\L", "Lords_Dirt"]])).unwrap());
    assert_eq!(t.get("Ldrt", "DIR"), Some("TerrainArt\\L"));
    assert_eq!(t.get("Ldrt", "nothing"), None);
    assert_eq!(t.get("Lgrs", "dir"), None);
}

#[test]
fn a_tilesets_water() {
    let text = slk(&[
        &["waterID", "height", "texFile", "numTex", "texRate", "Smin_R", "Smin_G", "Smin_B", "Smin_A", "Dmax_A"],
        &["LSha", "-0.7", "Water\\W", "45", "15", "255", "128", "0", "10", "300"],
    ]);
    let w = water(&Slk::parse(std::str::from_utf8(&text).unwrap()), 'L').unwrap();
    assert_eq!((w.height, w.texture.as_str(), w.count, w.rate), (-0.7, "Water\\W", 45, 15.0));
    assert_eq!(w.colors[0], [255, 128, 0, 10]);
    // Past 255: 255; not there: 0.
    assert_eq!(w.colors[3], [0, 0, 0, 255]);
    assert!(water(&Slk::parse(std::str::from_utf8(&text).unwrap()), 'X').is_none());
}

#[test]
fn paths_get_blp_unless_they_have_an_extension() {
    assert_eq!(with_extension("TerrainArt\\Blight\\Ashen_Blight"), "TerrainArt\\Blight\\Ashen_Blight.blp");
    assert_eq!(with_extension("a.b\\c.tga"), "a.b\\c.tga");
    assert_eq!(with_extension("a.b\\c"), "a.b\\c.blp");
}
