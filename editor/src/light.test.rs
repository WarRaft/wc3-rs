use super::*;

/// The game's day and night of Lordaeron (`WC3_GAME`; skipped without it):
/// the sun high at noon, down at midnight, the night bluer.
#[test]
fn lordaeron_through_a_day() {
    let Some(folder) = std::env::var_os("WC3_GAME") else { return };
    let game = wc3_server::game::Lookup { game: vec![folder.into()], roots: vec![], ..Default::default() }.open().0;
    let (terrain, unit, notes) = load(&game, 'L');
    assert!(notes.is_empty(), "{notes:?}");
    assert!(terrain.path.contains("DNCLordaeronTerrain") && unit.path.contains("DNCLordaeronUnit"));
    let at = |h: usize| &terrain.samples[h * 2 * SAMPLE..][..SAMPLE];
    for h in [0, 6, 12, 18] {
        eprintln!("{h:02}:00 dir {:?} colour {:?} ambient {:?}", &at(h)[..3], &at(h)[3..6], &at(h)[6..9]);
    }
    let (noon, midnight) = (at(12), at(0));
    assert!(noon[2] < -0.3, "the sun from above at noon: {noon:?}");
    assert!(noon[3] > midnight[3], "brighter at noon");
    assert!(midnight[5] >= midnight[3], "bluer at night: {midnight:?}");
}

