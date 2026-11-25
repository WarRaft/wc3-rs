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

/// A map's triggers taken out with their code (`EXTRACT_MAP`, the game in
/// `WC3_GAME`) into a project of its own (`EXTRACT_OUT`): it builds, and
/// with no problem; `a_real_project_built` on it compares the rest.
#[test]
fn triggers_taken_out_with_their_code_build() {
    let (Some(map), Some(out)) = (std::env::var_os("EXTRACT_MAP"), std::env::var_os("EXTRACT_OUT")) else { return };
    let (map, out) = (PathBuf::from(map), PathBuf::from(out));
    let game: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("WC3_GAME").expect("WC3_GAME")).collect();
    let names = crate::names::names(&Lookup { game: game.clone(), roots: vec![] });
    let wtg = crate::map::bytes(&map.join("war3map.wtg")).unwrap();
    let wtg_rs::File::Classic(f) = wtg_rs::parse_any(&wtg, &names.triggers).unwrap() else { panic!("1.31's layout") };
    let script = map.join("war3map.wct");
    let wct = crate::customtext::document(&script, &crate::map::bytes(&script).unwrap()).unwrap().text;
    let made = crate::triggers::extract_files(&f, Some(&wct)).unwrap();
    let _ = std::fs::remove_dir_all(&out);
    // The categories' folders, an empty one too (as Extract makes them).
    for folder in &made.folders {
        std::fs::create_dir_all(out.join("triggers").join(folder)).unwrap();
    }
    for (p, t) in &made.files {
        let path = out.join("triggers").join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    }
    let custom = made.files.iter().filter(|(p, _)| p.ends_with(".j")).count();
    std::fs::write(out.join("wc3.bni"), format!("[project]\ngame={:?}\n[code]\nsources=triggers/\n[map]\nroot={:?}\n[build]\n", game[0], map)).unwrap();
    let built = build_at(&out, None).unwrap_or_else(|e| panic!("{e:#?}"));
    println!("{} files of code ({custom} .j), {} triggers: {built:#?}", made.files.len(), f.triggers.len());
    // The map built opens with the triggers it runs: World Editor's files of
    // the .trg files, the map's own again (the categories numbered anew).
    // The custom script's text: its code the same (the line breaks of a
    // piece that mixes `\r\n` and `\n` aren't kept through its text, which
    // Extract cuts it from).
    let text = |b: &[u8]| wct_rs::text::to_text(&wct_rs::parse(b).unwrap(), wct_rs::text::Script::Jass).unwrap();
    let back_wct = crate::map::bytes(&built.out.join("war3map.wct")).expect("war3map.wct written");
    assert_eq!(text(&back_wct), text(&crate::map::bytes(&script).unwrap()), "war3map.wct");
    let back = wtg_rs::parse_any(&crate::map::bytes(&built.out.join("war3map.wtg")).expect("war3map.wtg written"), &names.triggers).unwrap();
    let wtg_rs::File::Classic(back) = back else { panic!("written as 1.31's") };
    let named = |f: &wtg_rs::Triggers| -> Vec<(String, String)> {
        f.triggers.iter().map(|t| (t.name.clone(), f.categories.iter().find(|c| c.index == t.category).map_or(String::new(), |c| c.name.clone()))).collect()
    };
    assert_eq!(named(&back), named(&f), "each trigger in its category");
    assert_eq!(back.variables, f.variables);
    let mut back = back;
    for (t, o) in back.triggers.iter_mut().zip(&f.triggers) {
        t.category = o.category;
    }
    assert_eq!(back.triggers, f.triggers);
}

/// The code of an extracted project's triggers put beside them, from the
/// map they came from (`CODE_MAP`) into the project's triggers' folder
/// (`CODE_DIR`, its `wc3.bni` above for the game): only `.j` files beside
/// `.trg` files there, none replaced. For projects extracted before
/// Extract took the code out too.
#[test]
fn code_put_beside_extracted_triggers() {
    let (Some(map), Some(dir)) = (std::env::var_os("CODE_MAP"), std::env::var_os("CODE_DIR")) else { return };
    let (map, dir) = (PathBuf::from(map), PathBuf::from(dir));
    let manifest = crate::project::find_manifest(&dir.join("x")).unwrap();
    let (project, _) = crate::project::load(manifest.parent().unwrap(), &std::fs::read_to_string(&manifest).unwrap());
    let names = crate::names::names(&Lookup { game: project.game.clone(), roots: vec![map.clone()] });
    let wtg = crate::map::bytes(&map.join("war3map.wtg")).unwrap();
    let wtg_rs::File::Classic(f) = wtg_rs::parse_any(&wtg, &names.triggers).unwrap() else { panic!("1.31's layout") };
    let script = map.join("war3map.wct");
    let wct = crate::customtext::document(&script, &crate::map::bytes(&script).unwrap()).unwrap().text;
    let made = crate::triggers::extract_files(&f, Some(&wct)).unwrap();
    let (mut written, mut kept, mut missing) = (0, 0, Vec::new());
    for (p, t) in made.files.iter().filter(|(p, _)| p.ends_with(".j")) {
        let path = dir.join(p);
        let beside = path.with_extension("trg").is_file();
        if !beside {
            missing.push(p.clone());
        } else if path.exists() {
            kept += 1;
        } else {
            std::fs::write(&path, t).unwrap();
            written += 1;
        }
    }
    println!("{written} written, {kept} there already, no .trg for: {missing:?}");
    assert!(missing.is_empty());
}

#[test]
fn code_alone_builds_its_script_in_each_language() {
    let tmp = crate::project::tests::TempDir::new("code-alone");
    std::fs::create_dir_all(tmp.0.join("src")).unwrap();
    std::fs::write(tmp.0.join("src/main.j"), "native BJDebugMsg takes string s returns nothing\nfunction Half takes integer i returns integer\n    return i / 2\nendfunction\nfunction main takes nothing returns nothing\n    call BJDebugMsg(\"half: \" + I2S(Half(-7)))\nendfunction\nfunction config takes nothing returns nothing\nendfunction\nnative I2S takes integer i returns string\n").unwrap();
    std::fs::write(tmp.0.join("wc3.bni"), "[code]\nsources=src\n[build]\n[build]\nname=lua\nscript=lua\n").unwrap();
    // JASS: build/war3map.j.
    let j = build_at(&tmp.0, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(j.out, tmp.0.join("build").join("war3map.j"));
    let text = std::fs::read_to_string(&j.out).unwrap();
    assert!(text.contains("function main takes nothing returns nothing") && text.contains("function config"), "{text}");
    // Lua: build/war3map.lua, as Lua reads it.
    let l = build_at(&tmp.0, Some("lua")).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(l.out, tmp.0.join("build").join("war3map.lua"));
    let text = std::fs::read_to_string(&l.out).unwrap();
    assert!(text.contains("function Half(i)\n    do return wcb__div(i, 2) end\nend"), "{text}");
    assert!(text.contains("BJDebugMsg(\"half: \" .. (I2S(Half(-7)) or \"\"))"), "{text}");
}

#[test]
fn jasshelpers_externals_run_on_the_map_built() {
    let tmp = crate::project::tests::TempDir::new("externals");
    let root = &tmp.0;
    std::fs::create_dir_all(root.join("map")).unwrap();
    std::fs::write(root.join("map/war3map.w3i"), w3i_rs::Info { version: 25, ..Default::default() }.to_bytes().unwrap()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    // A tool: what it's given, and its input, into a file beside the project.
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::write(root.join("tools/tool.sh"), "printf '%s|' \"$@\" > \"$(dirname \"$0\")/args.txt\"\ncat > \"$(dirname \"$0\")/input.txt\"\ncase \"$3\" in fail) echo broke >&2; exit 3;; esac\n[ -f \"$4\" ] && cp \"$4\" \"$(dirname \"$0\")/file.txt\"\necho done\n").unwrap();
    let code = |text: &str| std::fs::write(root.join("src/main.j"), format!("// @dialect vjass\n{text}\nfunction main takes nothing returns nothing\nendfunction\nfunction config takes nothing returns nothing\nendfunction\n")).unwrap();
    std::fs::write(root.join("wc3.bni"), "[map]\nroot=map\n[code]\nsources=src\n[build]\n[external]\nTOOL=\"/bin/sh\",\"tools/tool.sh\"\n").unwrap();
    let read = |f: &str| std::fs::read_to_string(root.join("tools").join(f)).unwrap_or_default();

    // A block: its lines the tool's input; the map's path, the lookup paths, then its own arguments.
    code("//! externalblock TOOL w3u \"two words\"\n//! i first\n//! i\n//! endexternalblock");
    let b = build_at(root, None).unwrap_or_else(|e| panic!("{e:#?}"));
    let args: Vec<String> = read("args.txt").trim_end_matches('|').split('|').map(String::from).collect();
    assert_eq!(args[0], b.out.to_string_lossy());
    assert!(args[1].split(';').next().is_some_and(|p| p == root.to_string_lossy().replace('\\', "/")), "{args:?}");
    assert_eq!(&args[2..], ["w3u", "two words"]);
    assert_eq!(read("input.txt"), "first\n\n");
    assert!(b.notes.iter().any(|n| n.ends_with("`//! external TOOL` ran: done")), "{:?}", b.notes);

    // `$FILENAME$`: the block in a file (its extension), no input.
    code("//! externalblock extension=lua TOOL x $FILENAME$\n//! i print(1)\n//! endexternalblock");
    build_at(root, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(read("file.txt"), "print(1)\n");
    assert!(read("args.txt").contains(".lua|"), "{}", read("args.txt"));

    // A tool that fails, or isn't named: the build's error, at its line.
    code("//! external TOOL fail");
    let e = build_at(root, None).unwrap_err();
    assert!(e[0].ends_with("main.j:2: `//! external TOOL` failed (3): broke"), "{e:?}");
    code("//! external Other");
    let e = build_at(root, None).unwrap_err();
    assert!(e[0].ends_with("main.j:2: `//! external Other`: wc3.bni has no [external] Other= (the tool's program)"), "{e:?}");
}

#[test]
fn a_base_maps_copy_is_compacted() {
    let tmp = crate::project::tests::TempDir::new("compact");
    let root = &tmp.0;
    // The base: a big script the build replaces.
    let mut x: u32 = 7;
    let noise: Vec<u8> = (0..300_000).map(|_| { x ^= x << 13; x ^= x >> 17; x ^= x << 5; b'a' + (x % 26) as u8 }).collect();
    let old = [b"// old\n".to_vec(), noise].concat();
    let info = w3i_rs::Info { version: 25, ..Default::default() }.to_bytes().unwrap();
    crate::map::tests::make_map_bytes(&root.join("base.w3x"), &[("war3map.w3i", info), ("war3map.j", old)]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.j"), "function main takes nothing returns nothing\nendfunction\nfunction config takes nothing returns nothing\nendfunction\n").unwrap();
    std::fs::write(root.join("wc3.bni"), "[map]\nroot=base.w3x\n[code]\nsources=src\n[build]\n").unwrap();
    let b = build_at(root, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(b.base.as_deref(), Some(root.join("base.w3x").as_path()));
    assert!(!b.notes.iter().any(|n| n.contains("isn't compacted")), "{:?}", b.notes);
    // The old script's space given back: smaller than the base.
    let (base, out) = (std::fs::metadata(root.join("base.w3x")).unwrap().len(), std::fs::metadata(&b.out).unwrap().len());
    assert!(out < base, "{base} -> {out}");
    let a = storm_rs::archive::MpqArchive::open(&b.out).unwrap();
    assert!(String::from_utf8(a.read_file("war3map.j").unwrap()).unwrap().contains("function main takes nothing returns nothing"));
}

#[test]
fn a_protected_maps_own_script_is_kept() {
    let tmp = crate::project::tests::TempDir::new("protected");
    let root = &tmp.0;
    // A protected map: no triggers, its script in Scripts\war3map.j.
    let info = w3i_rs::Info { version: 25, ..Default::default() }.to_bytes().unwrap();
    let script = b"function main takes nothing returns nothing\n    call TheRealCode()\nendfunction\n".to_vec();
    crate::map::tests::make_map_bytes(&root.join("base.w3x"), &[("war3map.w3i", info), ("Scripts\\war3map.j", script.clone())]);
    std::fs::write(root.join("wc3.bni"), "[map]\nroot=base.w3x\n[build]\n").unwrap();
    let b = build_at(root, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!((b.script.as_str(), b.triggers.as_str()), ("Scripts\\war3map.j", "none"));
    assert!(b.notes.iter().any(|n| n.contains("its own script (Scripts\\war3map.j) is kept as it is")), "{:?}", b.notes);
    let a = storm_rs::archive::MpqArchive::open(&b.out).unwrap();
    assert_eq!(a.read_file("Scripts\\war3map.j").unwrap(), script);
    // None made in front of it: the game would run that one.
    assert!(a.read_file("war3map.j").is_err());
}
