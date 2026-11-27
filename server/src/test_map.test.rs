use super::*;

#[test]
fn the_games_command_line_or_the_projects_command_filled_in() {
    let dir = std::env::temp_dir().join(format!("wc3-test-map-{}", std::process::id()));
    let game = dir.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(dir.join("extracted")).unwrap();
    std::fs::write(game.join("war3.EXE"), b"").unwrap();
    let src = format!("[project]\ngame={}\ngame={}\n", dir.join("extracted").display(), game.display());
    let (project, issues) = crate::project::load(&dir, &src);
    assert!(issues.iter().all(|i| i.level != parser_rs::Level::Error), "{issues:?}");
    // The last folder with War3.exe, in any case.
    assert_eq!(game_folder(&project), Some(game.clone()));
    let exe = game.join("war3.EXE").to_string_lossy().into_owned();
    // Windows: the game's own command line.
    let l = launch(&project, &game, true).unwrap();
    assert_eq!((l.program.as_str(), l.args.clone(), l.cwd.clone()), (exe.as_str(), vec!["-loadfile".to_string(), TEST_MAP.to_string()], game.clone()));
    // Elsewhere: the project's command, said when it has none.
    assert!(launch(&project, &game, false).unwrap_err().contains("[test] command="));
    let src = format!("{src}[test]\nwindow=1\nbuild=release\ncommand=\"wine\",\"{{exe}}\",\"-loadfile\",\"{{map}}\"\n");
    let (project, issues) = crate::project::load(&dir, &src);
    assert!(issues.iter().all(|i| i.level != parser_rs::Level::Error), "{issues:?}");
    assert_eq!(project.test.build.as_deref(), Some("release"));
    let l = launch(&project, &game, false).unwrap();
    assert_eq!((l.program.as_str(), l.args), ("wine", vec![exe, "-loadfile".into(), TEST_MAP.into(), "-window".into()]));
    // A game folder of its own (a wrapper's): that one.
    let wrapped = dir.join("wrapper/drive_c/Warcraft III");
    std::fs::create_dir_all(&wrapped).unwrap();
    std::fs::write(wrapped.join("War3.exe"), b"").unwrap();
    let (project, _) = crate::project::load(&dir, &format!("{src}[test]\ngame=wrapper/drive_c/Warcraft III\n"));
    assert_eq!(game_folder(&project), Some(wrapped));
    // A wrong setting said.
    let (_, issues) = crate::project::load(&dir, "[test]\nwindow=yes\nspeed=2\n");
    assert_eq!(issues.len(), 2, "{issues:?}");
    std::fs::remove_dir_all(&dir).ok();
}
