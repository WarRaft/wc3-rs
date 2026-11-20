use super::*;
use crate::project::tests::TempDir;

/// A tree under a temp root: `Games/Warcraft III/{Maps/, war3.mpq}`,
/// `src/a.j`, `.hidden/`.
fn tree(name: &str) -> TempDir {
    let tmp = TempDir::new(name);
    for dir in ["Games/Warcraft III/Maps", "Games/Other", "src", ".hidden"] {
        std::fs::create_dir_all(tmp.0.join(dir)).unwrap();
    }
    std::fs::write(tmp.0.join("Games/Warcraft III/war3.mpq"), "").unwrap();
    std::fs::write(tmp.0.join("src/a.j"), "").unwrap();
    tmp
}

/// (text under the link, target relative to `root`).
fn linked(root: &Path, src: &str) -> Vec<(String, String)> {
    links(root, src)
        .into_iter()
        .map(|l| {
            let line = src.lines().nth(l.line - 1).unwrap();
            let text: String = line.chars().skip(l.col - 1).take(l.end_col - l.col).collect();
            let target = l.target.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            (text, target)
        })
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn every_folder_along_a_path_is_a_link() {
    let tmp = tree("paths-links");
    let src = "[project]\ngame=Games/Warcraft III/Maps\ngame=\"./Games\\Other\" // quoted, backslash\nsources=src,missing/x\n[jass]\ngame=Games\n";
    assert_eq!(
        linked(&tmp.0, src),
        pairs(&[
            ("Games", "Games"),
            ("Warcraft III", "Games/Warcraft III"),
            ("Maps", "Games/Warcraft III/Maps"),
            (".", ""),
            ("Games", "Games"),
            ("Other", "Games/Other"),
            ("src", "src"),
        ])
    );
}

#[test]
fn absolute_paths_link_from_the_top() {
    let tmp = tree("paths-abs");
    let src = format!("[project]\ngame=\"{}/Games\"\n", tmp.0.display());
    let found = links(Path::new("/elsewhere"), &src);
    let last = found.last().unwrap();
    assert_eq!(last.target, tmp.0.join("Games"));
    // Every ancestor folder is a link too, `/` itself isn't.
    assert_eq!(found.len(), tmp.0.components().count());
}

/// The completion at the `|` in `line` (the second line of the manifest).
fn complete_at(root: &Path, line: &str) -> Option<(String, Vec<(String, bool)>)> {
    let col = line.chars().position(|c| c == '|').unwrap() + 1;
    let text = line.replace('|', "");
    let src = format!("[project]\n{text}\n");
    let c = complete(root, &src, 2, col)?;
    let replaced: String = text.chars().skip(c.col - 1).take(c.end_col - c.col).collect();
    Some((replaced, c.entries.into_iter().map(|e| (e.name, e.is_dir)).collect()))
}

#[test]
fn completes_folders_for_game() {
    let tmp = tree("paths-complete");
    let dirs = |names: &[&str]| names.iter().map(|n| (n.to_string(), true)).collect::<Vec<_>>();

    // Nothing typed yet: the project root's folders, not hidden ones.
    assert_eq!(complete_at(&tmp.0, "game=|"), Some((String::new(), dirs(&["Games", "src"]))));
    // Inside a folder, replacing the part the cursor is in.
    assert_eq!(
        complete_at(&tmp.0, "game=Games/Wa|rcraft III/Maps"),
        Some(("Warcraft III".into(), dirs(&["Other", "Warcraft III"])))
    );
    // After a separator; files don't count for game.
    assert_eq!(
        complete_at(&tmp.0, "game=\"Games/Warcraft III/|\""),
        Some((String::new(), dirs(&["Maps"])))
    );
    // A `.` shows hidden folders.
    assert_eq!(complete_at(&tmp.0, "game=.|"), Some((".".into(), dirs(&[".hidden", "Games", "src"]))));
    // A folder that isn't there: nothing.
    assert_eq!(complete_at(&tmp.0, "game=nope/|"), None);
    // Not a path setting, or before the `=`.
    assert_eq!(complete_at(&tmp.0, "dialect=|"), None);
    assert_eq!(complete_at(&tmp.0, "ga|me=x"), None);
}

#[test]
fn completes_files_too_for_sources() {
    let tmp = tree("paths-sources");
    assert_eq!(
        complete_at(&tmp.0, "sources=Games,src/|"),
        Some((String::new(), vec![("a.j".into(), false)]))
    );
    assert_eq!(
        complete_at(&tmp.0, "sources=|,src"),
        Some((String::new(), vec![("Games".into(), true), ("src".into(), true)]))
    );
}

#[test]
fn absolute_paths_complete_from_where_they_point() {
    let tmp = tree("paths-complete-abs");
    let line = format!("game={}/Games/|", tmp.0.display());
    let (_, entries) = complete_at(Path::new("/elsewhere"), &line).unwrap();
    assert_eq!(entries, vec![("Other".into(), true), ("Warcraft III".into(), true)]);
}

#[test]
fn queries_answer_in_the_open_manifest() {
    use crate::document::{Language, Position};
    use crate::index::{answer, QueryKind};
    let tmp = tree("paths-query");
    let manifest = tmp.0.join("wc3.bni");
    std::fs::write(&manifest, "").unwrap();
    let mut ws = crate::workspace::Workspace::default();
    ws.set_roots(vec![tmp.0.clone()]);
    let id = manifest.to_string_lossy().into_owned();
    // The editor's text, not the disk's.
    ws.open(id.clone(), Some(manifest.clone()), Language::Bni, 1, "[project]\ngame=Games/W\n".into());
    ws.take_reports();

    let links = answer(&ws, QueryKind::PathLinks, &id, Position::new(0, 0), "");
    let got: Vec<_> = links.locations.iter().map(|l| (l.range.start.character, l.range.end.character, l.id.clone())).collect();
    assert_eq!(got, vec![(5, 10, tmp.0.join("Games").to_string_lossy().into_owned())]);

    let done = answer(&ws, QueryKind::PathCompletion, &id, Position::new(1, 12), "");
    let got: Vec<_> = done
        .locations
        .iter()
        .map(|l| (l.name.as_str(), l.container.as_str(), l.range.start.character, l.range.end.character))
        .collect();
    assert_eq!(got, vec![("Other", "folder", 11, 12), ("Warcraft III", "folder", 11, 12)]);

    // Any other file: nothing.
    let a_j = tmp.0.join("src/a.j").to_string_lossy().into_owned();
    assert!(answer(&ws, QueryKind::PathLinks, &a_j, Position::new(0, 0), "").locations.is_empty());
}

#[test]
fn exclude_and_masks_link_and_complete_their_literal_part() {
    let tmp = tree("paths-masks");
    let src = "[project]\nsources=Games/*/Maps,src/**/*.j\nexclude=src/a.j\n";
    assert_eq!(linked(&tmp.0, src), pairs(&[("Games", "Games"), ("src", "src"), ("src", "src")]));
    assert_eq!(
        complete_at(&tmp.0, "exclude=src/|"),
        Some((String::new(), vec![("a.j".into(), false)]))
    );
    // Past a wildcard there's no one folder to list.
    assert_eq!(complete_at(&tmp.0, "sources=Games/*/|"), None);
}
