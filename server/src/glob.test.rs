use super::*;

fn m(mask: &str, path: &str) -> bool {
    Glob::new(Path::new("/p"), mask).matches(Path::new(path))
}

#[test]
fn names_and_wildcards() {
    assert!(m("test.j", "/p/test.j"));
    assert!(!m("test.j", "/p/sub/test.j"));
    assert!(m("*.j", "/p/a.j"));
    assert!(!m("*.j", "/p/a.zn"));
    assert!(!m("*.j", "/p/sub/a.j"));
    assert!(m("a?.j", "/p/ab.j"));
    assert!(m("a?.j", "/p/aж.j"));
    assert!(!m("a?.j", "/p/a.j"));
    assert!(m("*.bak.j", "/p/x.bak.j"));
}

#[test]
fn double_star_crosses_folders() {
    assert!(m("**/old/**", "/p/old/a.j"));
    assert!(m("**/old/**", "/p/x/y/old/z/a.j"));
    assert!(m("**/old", "/p/a/old/b.j"), "a folder covers what's in it");
    assert!(m("**/*.j", "/p/a.j"));
    assert!(m("src/**/*.j", "/p/src/a/b/c.j"));
    assert!(!m("src/**/*.j", "/p/lib/c.j"));
    assert!(!m("**/old/**", "/p/older/a.j"));
}

#[test]
fn case_and_separators_dont_matter() {
    assert!(m("SRC\\*.J", "/p/src/a.j"));
    assert!(m("**/War3Map.lua", "/p/Map.w3x/war3map.lua"));
}

#[test]
fn files_inside_a_matching_map_or_folder_match() {
    assert!(m("*.w3x", "/p/Map.w3x/war3map.j"));
    assert!(m("maps/*", "/p/maps/a/b.j"));
}

#[test]
fn relative_parts_and_absolute_masks() {
    assert!(m("../shared/*.j", "/shared/a.j"));
    assert!(m("/abs/**/*.j", "/abs/x/a.j"));
    assert!(!m("/abs/**/*.j", "/p/abs/x/a.j"));
}

#[test]
fn the_base_is_the_folder_before_the_first_wildcard() {
    let base = |mask: &str| Glob::new(Path::new("/p"), mask).base;
    assert_eq!(base("src/**/*.j"), PathBuf::from("/p/src"));
    assert_eq!(base("*.w3x"), PathBuf::from("/p"));
    assert_eq!(base("../shared/a?.j"), PathBuf::from("/shared"));
    assert!(is_mask("a*") && is_mask("a?") && !is_mask("src/a.j"));
}
