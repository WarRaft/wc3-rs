use super::*;

/// The problems of a script: names and types over the game's scripts, and
/// the rules of `pure`.
fn problems(text: &str, env: &jass_rs::names::Declarations) -> Vec<String> {
    let program = jass_rs::parse(text).unwrap_or_else(|e| panic!("line {}: {}", e.line, e.message));
    let mut out: Vec<String> = jass_rs::names::check(&program, &[env]).into_iter().map(|d| format!("{}: {}", d.rule, d.message)).collect();
    out.extend(
        jass_rs::lint(&program)
            .into_iter()
            .filter(|d| parser_rs::find_rule(d.rule).is_some_and(|r| r.groups.contains(&parser_rs::group::PURE)) || d.rule == parser_rs::rule::DUPLICATE_DECLARATION)
            .map(|d| format!("{}: {}", d.rule, d.message)),
    );
    out
}

/// A real project built (`WC3_PROJECT`, a folder of one, `WC3_BUILD` its
/// build): its script has no problem the first root's `war3map.j` hasn't,
/// and every file of that map is in the map built, the same but for what
/// the build makes or leaves out.
#[test]
fn a_real_project_built() {
    let Some(dir) = std::env::var_os("WC3_PROJECT") else { return };
    let built = build_at(Path::new(&dir), std::env::var("WC3_BUILD").ok().as_deref()).unwrap_or_else(|e| panic!("{e:#?}"));
    println!("{built:#?}");
    let out = MpqArchive::open(&built.out).unwrap();
    let script = String::from_utf8_lossy(&out.read_file("war3map.j").unwrap()).into_owned();
    let manifest = crate::project::find_manifest(&Path::new(&dir).join(crate::project::MANIFEST)).unwrap();
    let (project, _) = crate::project::load(manifest.parent().unwrap(), &std::fs::read_to_string(&manifest).unwrap());
    let (game, _) = Lookup { game: project.game.clone(), roots: vec![] }.open();
    let mut env = jass_rs::names::Declarations::default();
    for name in ["Scripts\\common.j", "Scripts\\Blizzard.j"] {
        let text = crate::document::decode(game.read(name).unwrap().0);
        env.extend(&jass_rs::names::Declarations::of(&jass_rs::parse(&text).unwrap()));
    }
    let ours = problems(&script, &env);
    let Some(base) = &built.base else { return };
    let base = MpqArchive::open(base).unwrap();
    let theirs = base.read_file("war3map.j").ok().map(|b| problems(&String::from_utf8_lossy(&b), &env)).unwrap_or_default();
    let only_ours: Vec<&String> = ours.iter().filter(|p| !theirs.contains(p)).collect();
    println!("problems: {} of ours, {} of the map's war3map.j", ours.len(), theirs.len());
    assert!(only_ours.is_empty(), "{:#?}", &only_ours[..only_ours.len().min(20)]);
    let mut same = 0;
    for name in base.list_files() {
        if left_out(&name) {
            continue;
        }
        assert_eq!(out.read_file(&name).ok(), base.read_file(&name).ok(), "{name}");
        same += 1;
    }
    println!("{same} files of the base alike");
}
