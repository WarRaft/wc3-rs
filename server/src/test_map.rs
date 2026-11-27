//! Test Map, as World Editor's: the project's map built, copied into the
//! game's folder as `Maps\Test\WorldEditTestMap.w3x` (where World Editor
//! puts it), and the game run with it (`War3.exe -loadfile <map>`, the
//! game's command line). The game's folder is the last of `[project]
//! game=` that has `War3.exe`, or `[test] game=` (a wrapper's: Porting
//! Kit's `drive_c`). `[test]` says which build, whether in a window, and
//! on a system that can't run the game itself (macOS, Linux) the command
//! that does (`command="wine","{exe}","-loadfile","{map}"`).

use std::path::{Path, PathBuf};

use crate::project::Project;

/// Where the map goes, as the game's folder has it.
pub const TEST_MAP: &str = "Maps\\Test\\WorldEditTestMap.w3x";

/// What Test Map runs: the program, its arguments, in which folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// The game's folder: `[test] game`, else the last of the project's
/// that has `War3.exe`.
pub fn game_folder(project: &Project) -> Option<PathBuf> {
    if let Some(g) = &project.test.game {
        return exe_in(g).map(|_| g.clone());
    }
    project.game.iter().rev().find(|g| exe_in(g).is_some()).cloned()
}

/// `War3.exe` in `folder`, in any case.
fn exe_in(folder: &Path) -> Option<PathBuf> {
    std::fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("war3.exe")))
}

/// What runs the game in `game` with the test map: `[test] command`
/// filled in, else (on Windows) `War3.exe -loadfile`; `-window` with
/// `window=1`. `windows`: whether this system runs the game itself.
pub fn launch(project: &Project, game: &Path, windows: bool) -> Result<Launch, String> {
    let exe = exe_in(game).ok_or_else(|| format!("{}: no War3.exe", game.display()))?;
    let fill = |a: &str| a.replace("{game}", &game.to_string_lossy()).replace("{exe}", &exe.to_string_lossy()).replace("{map}", TEST_MAP);
    let mut argv: Vec<String> = if !project.test.command.is_empty() {
        project.test.command.iter().map(|a| fill(a)).collect()
    } else if windows {
        vec![exe.to_string_lossy().into_owned(), "-loadfile".into(), TEST_MAP.into()]
    } else {
        return Err("Test Map runs the Windows game, which this system can't run by itself: say how in wc3.bni, [test] command=\"wine\",\"{exe}\",\"-loadfile\",\"{map}\" (or your wrapper's)".into());
    };
    if project.test.window {
        argv.push("-window".into());
    }
    let program = argv.remove(0);
    Ok(Launch { program, args: argv, cwd: game.to_path_buf() })
}

/// Test Map of the project at `path`: built (`name`, else `[test] build`,
/// else the default), copied in as [`TEST_MAP`], the game started (not
/// waited for). What was done, or what's wrong.
pub fn run(path: &Path, name: Option<&str>) -> Result<String, Vec<String>> {
    let path = crate::project::normalize(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
    let probe = if path.is_dir() { path.join(crate::project::MANIFEST) } else { path.clone() };
    let manifest = crate::project::find_manifest(&probe).ok_or_else(|| vec![format!("{}: no {} here or above", path.display(), crate::project::MANIFEST)])?;
    let root = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
    let text = std::fs::read(&manifest).map(crate::document::decode).map_err(|e| vec![format!("{}: {e}", manifest.display())])?;
    let (project, _) = crate::project::load(&root, &text);
    let game = game_folder(&project).ok_or_else(|| match &project.test.game {
        Some(g) => vec![format!("{}: no War3.exe ([test] game=)", g.display())],
        None => vec![format!("No game folder of {} has War3.exe ([project] game=)", manifest.display())],
    })?;
    let launch = launch(&project, &game, cfg!(windows)).map_err(|e| vec![e])?;
    let built = crate::build::build_at(&root, name.or(project.test.build.as_deref()))?;
    if !built.errors.is_empty() {
        return Err(built.errors.iter().map(|e| format!("the script: {e}")).collect());
    }
    let to = game.join(TEST_MAP.replace('\\', std::path::MAIN_SEPARATOR_STR));
    std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| vec![format!("{}: {e}", to.display())])?;
    std::fs::copy(&built.out, &to).map_err(|e| vec![format!("{} → {}: {e}", built.out.display(), to.display())])?;
    std::process::Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| vec![format!("{}: {e}", launch.program)])?;
    Ok(format!("{} → {}; {} {}", built.out.display(), to.display(), launch.program, launch.args.join(" ")))
}

#[cfg(test)]
#[path = "test_map.test.rs"]
mod tests;
