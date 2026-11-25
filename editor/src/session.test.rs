use super::*;
use crate::testdir::{self, TestDir};

#[test]
fn opens_the_map_of_its_roots() {
    let dir = TestDir::new("session");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/war3map.w3i", &testdir::info());
    dir.write("map/war3map.wts", b"STRING 1\n{\nMy Map\n}\n");
    let s = Session::open(dir.path()).unwrap();
    let ServerMsg::Map(m) = s.map_msg() else { panic!() };
    assert_eq!((m.name.as_str(), m.author.as_str()), ("My Map", "Someone"));
    assert_eq!((m.width, m.height, m.tileset.as_str()), (3, 2, "L"));
    assert!(m.notes.is_empty(), "{:?}", m.notes);
}

#[test]
fn a_later_root_shadows_an_earlier_one() {
    let dir = TestDir::new("session-layers");
    dir.write("wc3.bni", b"[map]\nroot=base\nroot=over\n");
    dir.write("base/war3map.w3e", &testdir::terrain());
    let mut info = w3i_rs::parse(&testdir::info()).unwrap();
    info.name = "Over".into();
    dir.write("over/war3map.w3i", &info.to_bytes().unwrap());
    let ServerMsg::Map(m) = Session::open(dir.path()).unwrap().map_msg() else { panic!() };
    assert_eq!(m.name, "Over");
}

#[test]
fn says_why_it_cant_open() {
    let dir = TestDir::new("session-errors");
    assert!(Session::open(dir.path()).err().unwrap().contains("Can't read"));
    dir.write("wc3.bni", b"[project]\n");
    assert!(Session::open(dir.path()).err().unwrap().contains("no [map] root="));
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3i", &testdir::info());
    assert!(Session::open(dir.path()).err().unwrap().contains("no war3map.w3e"));
    // Without its info it opens, said.
    std::fs::remove_file(dir.path().join("map/war3map.w3i")).unwrap();
    dir.write("map/war3map.w3e", &testdir::terrain());
    let ServerMsg::Map(m) = Session::open(dir.path()).unwrap().map_msg() else { panic!() };
    assert_eq!(m.notes, ["The map has no war3map.w3i."]);
}
