use super::*;
use crate::testdir::TestDir;

#[test]
fn one_editor_per_project_until_it_ends() {
    let dir = TestDir::new("lock");
    let project = TestDir::new("lock-project");
    let first = Lock::take_in(dir.path(), project.path()).unwrap();
    let second = Lock::take_in(dir.path(), project.path());
    assert!(second.unwrap_err().contains("open in another window"));
    // Another project isn't held by it.
    let other = TestDir::new("lock-other");
    assert!(Lock::take_in(dir.path(), other.path()).is_ok());
    drop(first);
    assert!(Lock::take_in(dir.path(), project.path()).is_ok());
}

#[test]
fn the_same_folder_however_written() {
    let dir = TestDir::new("lock-paths");
    let project = TestDir::new("lock-paths-project");
    std::fs::create_dir(project.path().join("sub")).unwrap();
    let plain = path_in(dir.path(), project.path());
    assert_eq!(path_in(dir.path(), &project.path().join("sub").join("..")), plain);
    let upper = PathBuf::from(project.path().to_string_lossy().to_uppercase());
    assert_eq!(path_in(dir.path(), &upper), plain);
}
