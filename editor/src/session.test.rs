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
    // No game: only the tables it hasn't.
    assert!(m.notes.iter().all(|n| n.starts_with("No TerrainArt")), "{:?}", m.notes);
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
    assert_eq!(m.notes[0], "The map has no war3map.w3i.");
}

#[test]
fn the_terrain_drawn_with_the_tables_of_the_roots() {
    let dir = TestDir::new("session-draw");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/TerrainArt/Terrain.slk", &testdir::slk(&[&["tileID", "dir", "file"], &["Ldrt", "T", "Dirt"]]));
    dir.write("map/TerrainArt/CliffTypes.slk", &testdir::slk(&[&["cliffID", "groundTile"], &["CLdi", "Ldrt"]]));
    dir.write("map/T/Dirt.blp", &testdir::tga(8, 4, |_, _| [9, 9, 9, 255]));
    let mut s = Session::open(dir.path()).unwrap();
    let msgs = s.draw();
    let [ServerMsg::Ground(g), ServerMsg::Water(w), ServerMsg::Cliffs(c), ServerMsg::Terrain(t), ServerMsg::Objects(o)] = &msgs[..] else { panic!("{} messages", msgs.len()) };
    // Nothing placed.
    assert!(o.models.is_empty() && o.notes.is_empty());
    // No cliff on it; its one cliff texture isn't in the roots.
    assert!(c.meshes.is_empty() && c.texture_paths.len() == 1 && !c.texture_paths[0].1.is_empty());
    assert_eq!(g.textures.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["Ldrt", "blight"]);
    assert!(g.textures[0].error.is_empty() && g.textures[0].extended);
    assert!(g.textures[1].error.contains("blight"), "{}", g.textures[1].error);
    assert!(w.error.contains("Water.slk"), "{}", w.error);
    assert_eq!((t.width, t.height, t.tiles.len()), (3, 2, 2));
    // Every message goes over the wire and back.
    for m in &msgs {
        let frame = m.encode();
        let (kind, body) = crate::protocol::read_frame(&mut &frame[..]).unwrap().unwrap();
        assert_eq!(&ServerMsg::decode(kind, &body).unwrap().unwrap(), m);
    }
}

/// A real map with a real game: `WC3_PROJECT` (a folder with a wc3.bni
/// naming them); skipped without it.
#[test]
fn a_real_map_draws_with_its_textures() {
    let Some(root) = std::env::var_os("WC3_PROJECT") else { return };
    let start = std::time::Instant::now();
    let mut s = Session::open(Path::new(&root)).unwrap();
    let msgs = s.draw();
    eprintln!("drawn in {:?}; notes: {:?}", start.elapsed(), s.notes);
    for m in &msgs {
        match m {
            ServerMsg::Ground(g) => {
                for t in &g.textures {
                    eprintln!("  {} {} {}+{} {}", t.id, t.path, t.first, t.count, t.error);
                }
                assert!(g.textures.iter().all(|t| t.error.is_empty()));
            }
            ServerMsg::Water(w) => {
                eprintln!("  water: {} frames of {}, {:?} {}", w.frames.count, w.frames.size, w.colors, w.error);
                assert!(w.error.is_empty());
            }
            ServerMsg::Cliffs(c) => {
                let placed: usize = c.meshes.iter().map(|m| m.instances.len() / 4).sum();
                eprintln!("  cliffs: {} models, {placed} placed, textures {:?}, notes {:?}", c.meshes.len(), c.texture_paths, c.notes);
                assert!(c.meshes.iter().all(|m| m.error.is_empty()) && c.texture_paths.iter().all(|(_, e)| e.is_empty()));
            }
            ServerMsg::Objects(o) => {
                let placed: usize = o.models.iter().map(|m| m.instances.len() / crate::placed::INSTANCE).sum();
                let pixels: usize = o.textures.iter().map(|t| t.rgba.len()).sum();
                eprintln!("  objects: {} models, {placed} placed, {} textures ({pixels} bytes), notes {:?}", o.models.len(), o.textures.len(), o.notes);
                for e in o.models.iter().filter(|m| !m.error.is_empty()).map(|m| &m.error).chain(o.textures.iter().filter(|t| !t.error.is_empty()).map(|t| &t.error)).take(10) {
                    eprintln!("    {e}");
                }
            }
            ServerMsg::Terrain(t) => eprintln!("  terrain {} x {}, {} cliff tiles", t.width, t.height, t.tile_flags.iter().filter(|f| **f & 1 != 0).count()),
            _ => {}
        }
        eprintln!("  {} bytes", m.encode().len());
    }
}
