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
    let [ServerMsg::Ground(g), ServerMsg::Water(w), ServerMsg::Cliffs(c), ServerMsg::Terrain(t), ServerMsg::Objects(o), ServerMsg::Instances(_), ServerMsg::Overlays(v), ServerMsg::Palette(_), ServerMsg::ObjectPalette(_), ServerMsg::Sounds(_), ServerMsg::Follow(f), ServerMsg::Light(l), ServerMsg::State(st)] = &msgs[..] else { panic!("{} messages", msgs.len()) };
    assert_eq!(st, &StateMsg::default());
    // No game: no day and night (the view's own light), said.
    assert!(l.terrain.is_empty() && l.notes.iter().any(|n| n.contains("TerrainLights")), "{:?}", l.notes);
    // The pieces with a text form, where they are (the terrain) or would
    // be, at the top root (the map has none of the others).
    assert_eq!(f.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["war3map.w3e", "war3map.doo", "war3mapUnits.doo", "war3map.w3r", "war3map.w3c", "war3map.w3s", "war3map.w3u", "war3map.w3t", "war3map.w3b", "war3map.w3d", "war3map.w3a", "war3map.w3h", "war3map.w3q", "war3map.wts", "war3map.imp", "war3map.w3i", "war3mapMisc.txt", "war3mapSkin.txt"]);
    assert!(f[1].1.ends_with("map/war3map.doo"), "{f:?}");
    // Nothing placed; the minimap a pixel a corner.
    assert!(o.models.is_empty() && o.notes.is_empty());
    assert_eq!((v.minimap.width, v.minimap.height, v.minimap.data.len()), (3, 2, 24));
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
                let animated: Vec<_> = o.models.iter().filter_map(|m| m.anim.as_ref()).collect();
                let floats: usize = animated.iter().map(|a| a.matrices.len()).sum();
                eprintln!("  animated: {} models, {} matrix bytes, frames max {:?}", animated.len(), floats * 4, animated.iter().map(|a| a.frames).max());
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

/// A brush where a ray from above meets the ground, at (1, 0.5).
fn raise(s: &mut Session, start: bool) -> Vec<ServerMsg> {
    let brush = crate::edit::Brush { tool: crate::edit::Tool::Raise, size: 4.0, strength: 1.0, texture: None, cliff: -1, flags: 0, variation: -1 };
    s.brush([1.0, 0.5, 10.0], [0.0, 0.0, -1.0], &brush, start)
}

#[test]
fn a_brush_then_undo_then_save_where_the_terrain_came_from() {
    let dir = TestDir::new("session-edit");
    dir.write("wc3.bni", b"[map]\nroot=base\nroot=over\n");
    dir.write("base/war3map.w3e", &testdir::terrain());
    std::fs::create_dir_all(dir.path().join("over")).unwrap();
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let msgs = raise(&mut s, true);
    // The terrain again (no new ground textures), and the state.
    // (and what's placed, on it as it is now).
    assert!(matches!(&msgs[..], [ServerMsg::Cliffs(c), ServerMsg::Terrain(t), ServerMsg::Instances(_), ServerMsg::State(st)] if c.textures.count == 0 && t.heights[1] > 0.0 && st.dirty && !st.undo), "{msgs:?}");
    s.stroke_end();
    let msgs = s.undo(false);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.redo && !st.undo));
    s.undo(true);
    // Saved into base, where it came from, not over it; the pathing made
    // with it (the map had none: into the top root), the overlays again.
    let saved = s.save();
    assert!(
        matches!(&saved[..], [ServerMsg::Overlays(_), ServerMsg::State(st)] if !st.dirty && st.message.contains("base") && st.message.contains("over/war3map.wpm")),
        "{saved:?}"
    );
    let back = w3e_rs::parse(&std::fs::read(dir.path().join("base/war3map.w3e")).unwrap()).unwrap();
    assert!(back.point(1, 0).unwrap().height > 0x2000);
    assert!(!dir.path().join("over/war3map.w3e").exists());
}

#[test]
fn a_terrain_from_the_game_saves_into_the_top_root() {
    let dir = TestDir::new("session-edit-game");
    dir.write("wc3.bni", b"[project]\ngame=game\n\n[map]\nroot=map\n");
    dir.write("game/war3map.w3e", &testdir::terrain());
    std::fs::create_dir_all(dir.path().join("map")).unwrap();
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    raise(&mut s, true);
    s.save();
    assert!(dir.path().join("map/war3map.w3e").exists());
}

#[test]
fn things_placed_moved_turned_deleted_undone_and_saved() {
    let dir = TestDir::new("session-things");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "file", "numVar"], &["LTrc", "Doodads\\Rock", "1"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    // Placed at (1, 0.5) cells: (0, 0) in the game's units.
    let msgs = s.place(0, "LTrc", 0, [1.0, 0.5, 10.0], down);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.dirty && st.undo && st.message.is_empty()), "{msgs:?}");
    let d = &s.placed.doodads.as_ref().unwrap().doodads;
    assert_eq!((d.len(), d[0].placement.x, d[0].placement.y, d[0].number), (1, 0.0, 0.0, 0));
    // Moved in a drag of two steps: one undo for it.
    s.move_thing(0, 0, [1.5, 0.5, 10.0], down, true);
    s.move_thing(0, 0, [2.0, 0.5, 10.0], down, false);
    s.stroke_end();
    assert_eq!(s.placed.doodads.as_ref().unwrap().doodads[0].placement.x, 128.0);
    s.rotate(0, 0, std::f32::consts::PI);
    assert!((s.placed.doodads.as_ref().unwrap().doodads[0].placement.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    s.undo(false);
    s.undo(false);
    assert_eq!(s.placed.doodads.as_ref().unwrap().doodads[0].placement.x, 0.0);
    s.undo(true);
    s.undo(true);
    assert!(matches!(s.delete(0, 9).last(), Some(ServerMsg::State(st)) if st.message.contains("no thing 9")));
    s.delete(0, 0);
    assert!(s.placed.doodads.as_ref().unwrap().doodads.is_empty());
    s.undo(false);
    // Saved: the doodads' file made, into the root, and the pathing; the
    // terrain untouched.
    let saved = s.save();
    assert!(
        matches!(saved.last(), Some(ServerMsg::State(st)) if st.message.contains("war3map.doo") && st.message.contains("war3map.wpm") && !st.message.contains("w3e")),
        "{saved:?}"
    );
    let back = doo_rs::parse_doodads_any(&std::fs::read(dir.path().join("map/war3map.doo")).unwrap()).unwrap();
    assert_eq!(back.doodads.len(), 1);
    assert_eq!(back.doodads[0].placement.x, 128.0);
}

#[test]
fn regions_drawn_moved_resized_renamed_deleted_and_pathing_painted() {
    let dir = TestDir::new("session-regions");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    // A pathing map: 8 by 4 cells, nothing forbidden.
    let pathing = wpm_rs::Pathing { version: 0, width: 8, height: 4, cells: vec![0; 32] };
    dir.write("map/war3map.wpm", &pathing.to_bytes().unwrap());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    // Cells (0, 0) to (1.3, 0.8): -128, -64 to 38.4, 38.4: on the grid of 32.
    s.region_drag(true, 0, -1, ([0.0, 0.0, 9.0], down), ([0.5, 0.5, 9.0], down));
    s.region_drag(false, 0, -1, ([0.0, 0.0, 9.0], down), ([1.3, 0.8, 9.0], down));
    s.stroke_end();
    let r = &s.placed.regions.as_ref().unwrap().regions;
    assert_eq!((r.len(), r[0].name.as_str()), (1, "Region 000"));
    assert_eq!([r[0].left, r[0].bottom, r[0].right, r[0].top], [-128.0, -64.0, 32.0, 32.0]);
    // Moved a cell right; its top right corner to (1.5, 1) cells.
    s.region_drag(true, 1, 0, ([0.5, 0.5, 9.0], down), ([1.5, 0.5, 9.0], down));
    let r = &s.placed.regions.as_ref().unwrap().regions[0];
    assert_eq!([r.left, r.right], [0.0, 160.0]);
    s.region_drag(true, 2, 0, ([1.9, 0.9, 9.0], down), ([1.5, 1.0, 9.0], down));
    let r = &s.placed.regions.as_ref().unwrap().regions[0];
    assert_eq!([r.left, r.bottom, r.right, r.top], [0.0, -64.0, 64.0, 64.0]);
    s.region_rename(0, "Spawn");
    assert!(matches!(s.region_rename(0, " ").last(), Some(ServerMsg::State(st)) if st.message.contains("needs a name")));
    // A click on the ground in it says it.
    let picked = s.pick([1.0, 0.5, 9.0], down);
    assert_eq!(picked.region, Some(0));
    assert!(picked.text.contains("in Spawn"), "{}", picked.text);
    // Pathing: no building painted where the brush is.
    let brush = crate::edit::Brush { tool: crate::edit::Tool::PathingOn, size: 1.0, strength: 0.0, texture: None, cliff: -1, flags: 8, variation: -1 };
    let msgs = s.brush([1.0, 0.5, 9.0], down, &brush, true);
    assert!(matches!(&msgs[..], [ServerMsg::Overlays(o), ServerMsg::State(st)] if o.pathing.data.contains(&8) && st.dirty));
    s.stroke_end();
    s.region_delete(0);
    assert!(s.placed.regions.as_ref().unwrap().regions.is_empty());
    s.undo(false);
    assert_eq!(s.placed.regions.as_ref().unwrap().regions[0].name, "Spawn");
    // Saved: both files, the terrain untouched.
    let saved = s.save();
    assert!(matches!(&saved[..], [ServerMsg::State(st)] if st.message.contains("war3map.w3r") && st.message.contains("war3map.wpm") && !st.message.contains("w3e")), "{saved:?}");
    let back = wpm_rs::parse(&std::fs::read(dir.path().join("map/war3map.wpm")).unwrap()).unwrap();
    assert!(back.cells.contains(&8));
}

#[test]
fn the_pathing_made_again_on_save_with_the_paint_over_it() {
    let dir = TestDir::new("session-pathing");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    // Painted: no walking around (1, 0.5) cells.
    let paint = crate::edit::Brush { tool: crate::edit::Tool::PathingOn, size: 0.5, strength: 0.0, texture: None, cliff: -1, flags: wpm_rs::NO_WALK, variation: -1 };
    // The map had no war3map.wpm: a brush has nothing to paint until a
    // save made one.
    assert!(matches!(s.brush([1.0, 0.5, 9.0], down, &paint, true).last(), Some(ServerMsg::State(st)) if st.message.contains("no war3map.wpm")));
    let raise = crate::edit::Brush { tool: crate::edit::Tool::Raise, size: 2.0, strength: 0.1, texture: None, cliff: -1, flags: 0, variation: -1 };
    s.brush([1.0, 0.5, 9.0], down, &raise, true);
    s.stroke_end();
    s.save();
    let made = wpm_rs::parse(&std::fs::read(dir.path().join("map/war3map.wpm")).unwrap()).unwrap();
    assert_eq!((made.width, made.height), (8, 4));
    // Dry land everywhere: "no water" on every cell, nothing forbidden.
    assert!(made.cells.iter().all(|c| *c == wpm_rs::NO_WATER));
    s.brush([1.0, 0.5, 9.0], down, &paint, true);
    s.stroke_end();
    // The terrain changed again, then saved: the paint kept over it.
    s.brush([1.0, 0.5, 9.0], down, &raise, true);
    s.stroke_end();
    s.save();
    let again = wpm_rs::parse(&std::fs::read(dir.path().join("map/war3map.wpm")).unwrap()).unwrap();
    assert!(again.cells.iter().any(|c| c & wpm_rs::NO_WALK != 0) && again.cells.iter().any(|c| c & wpm_rs::NO_WALK == 0));
}

#[test]
fn a_change_on_disk_taken_in_unless_the_editor_has_its_own() {
    let dir = TestDir::new("session-disk");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    // Regions written by someone else: read again, said.
    let mut regions = w3r_rs::Regions::new();
    regions.regions.push(w3r_rs::Region::new("Outside", 0.0, 0.0, 64.0, 64.0, 0));
    dir.write("map/war3map.w3r", &regions.to_bytes().unwrap());
    let msgs = s.disk_changed(&[dir.path().join("map/war3map.w3r")]);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("Read again") && st.message.contains("war3map.w3r")), "{msgs:?}");
    assert_eq!(s.placed.regions.as_ref().unwrap().regions[0].name, "Outside");
    // A change elsewhere: nothing.
    assert!(s.disk_changed(&[dir.path().join("elsewhere.txt")]).is_empty());
    // The editor's own save isn't a change on disk.
    s.region_rename(0, "Ours");
    s.save();
    assert!(s.disk_changed(&[dir.path().join("map/war3map.w3r")]).is_empty());
    // Changed by both: the editor's kept, said.
    s.region_rename(0, "Ours again");
    regions.regions[0].name = "Theirs".into();
    dir.write("map/war3map.w3r", &regions.to_bytes().unwrap());
    let msgs = s.disk_changed(&[dir.path().join("map/war3map.w3r")]);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("your changes to it kept")), "{msgs:?}");
    assert_eq!(s.placed.regions.as_ref().unwrap().regions[0].name, "Ours again");
}

#[test]
fn cameras_made_from_the_view_dragged_renamed_deleted_undone_saved_and_in_step_with_their_text() {
    let dir = TestDir::new("session-cameras");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let ground = crate::placed::terrain_at(s.drawn.as_ref().unwrap(), 2.0, 1.0);
    // The view looking north (horizontal 0) down at -0.977 radians from
    // 10 cells, at (2, 1) cells, a layer over the ground: the corner is
    // at -128, -64.
    let msgs = s.camera_view(-1, [2.0, 1.0, ground + 1.0, 10.0, 0.0, -0.977, 50.0, 0.0]);
    assert!(matches!(&msgs[..], [ServerMsg::Overlays(o), ServerMsg::State(st)] if o.cameras.len() == 1 && o.camera_lines.len() > 0 && st.dirty && st.undo), "{msgs:?}");
    let c = &s.placed.cameras.as_ref().unwrap().cameras[0];
    assert_eq!(c.name, "Camera 001");
    assert_eq!([c.target_x, c.target_y, c.z_offset, c.distance, c.rotation, c.field_of_view], [128.0, 64.0, 128.0, 1280.0, 90.0, 50.0]);
    assert_eq!(c.angle_of_attack, 304.02);
    // Drawn where the view was: its eye up and to the south of its target.
    let o = s.overlays.as_ref().unwrap();
    let cam = &o.cameras[0];
    assert!((cam.target[0] - 2.0).abs() < 1e-4 && (cam.target[1] - 1.0).abs() < 1e-4);
    assert!(cam.eye[1] < cam.target[1] && cam.eye[2] > cam.target[2]);
    assert_eq!(cam.view, [90.0, 304.02, 1280.0, 50.0, 128.0, 0.0]);
    s.camera_view(-1, [3.0, 1.0, ground, 10.0, std::f32::consts::FRAC_PI_2, -0.5, 70.0, -0.1]);
    let c = &s.placed.cameras.as_ref().unwrap().cameras[1];
    assert_eq!((c.name.as_str(), c.rotation, c.roll), ("Camera 002", 0.0, 354.27));
    // The second set to another view: east (horizontal -π/2 looks west).
    s.camera_view(1, [3.0, 1.0, ground, 5.0, -std::f32::consts::FRAC_PI_2, -0.5, 70.0, 0.0]);
    let c = &s.placed.cameras.as_ref().unwrap().cameras[1];
    assert_eq!((c.name.as_str(), c.rotation, c.distance, c.roll), ("Camera 002", 180.0, 640.0, 0.0));
    // The first's target dragged to (1, 0.5) cells, its height kept.
    let down = [0.0, 0.0, -1.0];
    s.camera_drag(true, 0, [1.0, 0.5, 9.0], down);
    s.camera_drag(false, 0, [1.5, 0.5, 9.0], down);
    s.stroke_end();
    let c = &s.placed.cameras.as_ref().unwrap().cameras[0];
    assert_eq!([c.target_x, c.target_y, c.z_offset], [64.0, 0.0, 128.0]);
    s.undo(false);
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras[0].target_x, 128.0);
    s.camera_set(0, "field_of_view", 60.0);
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras[0].field_of_view, 60.0);
    assert_eq!(s.overlays.as_ref().unwrap().cameras[0].view[3], 60.0);
    assert!(matches!(s.camera_set(0, "field_of_view", 200.0).last(), Some(ServerMsg::State(st)) if st.message.contains("field of view can't be 200")));
    assert!(matches!(s.camera_set(0, "distance", 0.0).last(), Some(ServerMsg::State(st)) if st.message.contains("distance can't be 0")));
    assert!(matches!(s.camera_set(0, "nothing", 1.0).last(), Some(ServerMsg::State(st)) if st.message.contains("no nothing")));
    s.undo(false);
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras[0].field_of_view, 50.0);
    s.camera_rename(0, "Intro");
    assert!(matches!(s.camera_rename(0, " ").last(), Some(ServerMsg::State(st)) if st.message.contains("needs a name")));
    s.camera_delete(1);
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras.len(), 1);
    assert!(matches!(s.camera_delete(5).last(), Some(ServerMsg::State(st)) if st.message.contains("no camera")));
    s.undo(false);
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras.len(), 2);
    // Saved: a new war3map.w3c, 1.27a's layout (no war3map.w3i).
    let saved = s.save();
    assert!(matches!(&saved[..], [ServerMsg::State(st)] if st.message.contains("war3map.w3c")), "{saved:?}");
    let back = w3c_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3c")).unwrap(), w3c_rs::Layout::default()).unwrap();
    assert_eq!(back.cameras, s.placed.cameras.as_ref().unwrap().cameras);
    assert_eq!(back.cameras[0].name, "Intro");
    // Its text form: typed, taken in; the editor's change given to it.
    let path = dir.path().join("map/war3map.w3c").display().to_string();
    assert!(s.follow().iter().any(|(n, p)| n == "war3map.w3c" && *p == path));
    let text = String::from_utf8(wc3_server::cameras::convert(wc3_server::objects::TO_TEXT, &path, &back.to_bytes().unwrap()).unwrap()).unwrap();
    assert!(s.text_changed(&path, &text).is_empty());
    let typed = text.replace("[Intro]", "[Typed]");
    assert_ne!(typed, text);
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(&msgs[..], [ServerMsg::Overlays(o), .., ServerMsg::State(st)] if o.cameras[0].name == "Typed" && st.undo), "{msgs:?}");
    s.camera_rename(1, "Outro");
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { text, .. }] if text.contains("[Outro]")), "{out:?}");
    assert!(!s.edits.dirty.cameras);
    // Set to a view looking at the ground: its height kept (a cell over
    // it), from the view's eye along its direction, nearer.
    let height = s.placed.cameras.as_ref().unwrap().cameras[0].z_offset;
    assert_eq!(height, 128.0);
    let (v, hz, d) = (-0.977f32, 0.3f32, 10.0f32);
    let dir = [v.cos() * hz.sin(), v.cos() * hz.cos(), v.sin()];
    let ground = crate::placed::terrain_at(s.drawn.as_ref().unwrap(), 2.0, 1.0);
    let eye = [2.0 - dir[0] * d, 1.0 - dir[1] * d, ground - dir[2] * d];
    s.camera_view(0, [2.0, 1.0, ground, d, hz, v, 70.0, 0.0]);
    let c = &s.placed.cameras.as_ref().unwrap().cameras[0];
    assert_eq!(c.z_offset, 128.0);
    assert!(c.distance < d * 128.0, "{}", c.distance);
    let cam = &s.overlays.as_ref().unwrap().cameras[0];
    assert!((0..3).all(|k| (cam.eye[k] - eye[k]).abs() < 0.01), "{:?} {eye:?}", cam.eye);
    let over = cam.target[2] - crate::placed::terrain_at(s.drawn.as_ref().unwrap(), cam.target[0], cam.target[1]);
    assert!((over - 1.0).abs() < 0.01, "{over}");
    // Off the map before the line is at its height: the view's target, said.
    let msgs = s.camera_view(0, [2.0, 1.0, ground, d, hz, 0.3, 70.0, 0.0]);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("never looks")), "{msgs:?}");
    assert_eq!(s.placed.cameras.as_ref().unwrap().cameras[0].z_offset, 0.0);
}

#[test]
fn a_text_form_open_follows_the_editor_and_the_editor_it() {
    let dir = TestDir::new("session-text");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut regions = w3r_rs::Regions::new();
    regions.regions.push(w3r_rs::Region::new("A", 0.0, 0.0, 64.0, 64.0, 0));
    dir.write("map/war3map.w3r", &regions.to_bytes().unwrap());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let path = dir.path().join("map/war3map.w3r").display().to_string();
    let text = String::from_utf8(wc3_server::regions::convert(wc3_server::objects::TO_TEXT, &path, &regions.to_bytes().unwrap()).unwrap()).unwrap();
    // Opened as it is: nothing to take in.
    assert!(s.text_changed(&path, &text).is_empty());
    // Typed in it: taken in (an undo), the editor not the one with it unsaved.
    let typed = text.replace("[A]", "[Typed]");
    assert_ne!(typed, text);
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if !st.dirty && st.undo), "{msgs:?}");
    assert_eq!(s.placed.regions.as_ref().unwrap().regions[0].name, "Typed");
    // An error in it: said, nothing taken.
    let msgs = s.text_changed(&path, &typed.replace("left=0", "left=what"));
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("line")), "{msgs:?}");
    assert_eq!(s.placed.regions.as_ref().unwrap().regions[0].name, "Typed");
    s.text_changed(&path, &typed);
    // Changed in the editor: given to the document; it's the document's to save.
    s.region_rename(0, "Renamed");
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { path: p, text }] if *p == path && text.contains("Renamed")), "{out:?}");
    assert!(!s.edits.dirty.regions);
    // Its echo: nothing.
    let ServerMsg::TextOut { text: given, .. } = &out[0] else { unreachable!() };
    assert!(s.text_changed(&path, given).is_empty());
    // Save: VS Code saves it.
    s.edits.dirty.regions = true;
    let saved = s.save();
    assert!(saved.iter().any(|m| matches!(m, ServerMsg::SaveDocs(p) if p == &vec![path.clone()])), "{saved:?}");
    // Closed: the editor's own again.
    s.text_closed(&path);
    s.region_rename(0, "Mine");
    assert!(s.sync_docs().is_empty() && s.edits.dirty.regions);
}

#[test]
fn the_light_the_shadows_and_the_fog_of_the_map() {
    let dir = TestDir::new("session-light");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    // Its info: the fog on (0x2000), blue, green, red, alpha as stored.
    let mut info = w3i_rs::parse(&testdir::info()).unwrap();
    info.flags |= 0x2000;
    info.fog = w3i_rs::Fog { style: 1, start_z: 1000.0, end_z: 3000.0, density: 0.5, color: [10, 20, 30, 255] };
    dir.write("map/war3map.w3i", &info.to_bytes().unwrap());
    // A shadow on the first cell.
    let mut shd = vec![0u8; 8 * 4];
    shd[0] = 255;
    dir.write("map/war3map.shd", &shd);
    let s = Session::open(dir.path()).unwrap();
    let l = s.light();
    assert_eq!(l.fog, Some((1, 1000.0, 3000.0, 0.5, [30, 20, 10, 255])));
    assert_eq!((l.shadows.width, l.shadows.height, l.shadows.data[0], l.shadows.data[1]), (8, 4, 255, 0));
}

/// A terrain of 32 by 32 cells, Lordaeron's dirt.
fn terrain_32() -> Vec<u8> {
    let mut t = w3e_rs::parse(&testdir::terrain()).unwrap();
    t.width = 33;
    t.height = 33;
    t.points = vec![t.points[0]; 33 * 33];
    t.to_bytes().unwrap()
}

#[test]
fn the_map_resized_its_tiles_set_and_undone_saved_with_its_info_and_shadows() {
    let dir = TestDir::new("session-resize");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &terrain_32());
    let mut info = w3i_rs::parse(&testdir::info()).unwrap();
    info.camera_complements = [2, 2, 2, 2];
    dir.write("map/war3map.w3i", &info.to_bytes().unwrap());
    dir.write("map/war3map.shd", &vec![255u8; 128 * 128]);
    let mut cams = w3c_rs::Cameras::new(w3c_rs::Layout::Classic);
    cams.cameras = vec![w3c_rs::Camera::new("Kept", 0.0, 0.0), w3c_rs::Camera::new("Cut", 3000.0, 0.0)];
    dir.write("map/war3map.w3c", &cams.to_bytes().unwrap());
    dir.write("map/TerrainArt/Terrain.slk", &testdir::slk(&[&["tileID", "dir", "file"], &["Ldrt", "T", "Dirt"], &["Lgrs", "T", "Grass"]]));
    dir.write("map/TerrainArt/CliffTypes.slk", &testdir::slk(&[&["cliffID", "groundTile"], &["CLdi", "Ldrt"], &["CLgr", "Lgrs"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    assert_eq!(s.overlays().cameras.len(), 2);
    let settings = s.map_settings();
    assert_eq!((settings.width, settings.margins, settings.map_tiles.clone(), settings.max_tiles), (33, [2, 2, 2, 2], vec!["Ldrt".to_string()], 16));
    assert_eq!(settings.tiles.iter().map(|t| (t.id.as_str(), t.cliff)).collect::<Vec<_>>(), [("Ldrt", true), ("Lgrs", true)]);

    // Bigger on the left: all of it again, the margins grown.
    let msgs = s.resize(crate::resize::Sides { left: 8, ..Default::default() }, None);
    assert!(matches!(&msgs[0], ServerMsg::Map(m) if m.width == 41), "{:?}", &msgs[0]);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.dirty && !st.undo && st.message.contains("40 by 32")));
    assert!(msgs.iter().any(|m| matches!(m, ServerMsg::MapSettings(m) if m.margins == [10, 2, 2, 2])));
    assert!(msgs.iter().any(|m| matches!(m, ServerMsg::Light(l) if l.shadows.width == 160 && l.shadows.data[0] == 0 && l.shadows.data[40] == 255)));

    // Grass added, dirt's points to it, then undone and done again.
    s.set_tiles(&["Lgrs".to_string(), "Ldrt".to_string()], &[0], b'L');
    assert_eq!(s.terrain.ground.len(), 2);
    assert_eq!(s.terrain.cliffs.len(), 2);
    s.undo(false);
    assert_eq!(s.terrain.ground.len(), 1);
    s.undo(true);
    let saved = s.save();
    assert!(matches!(saved.last(), Some(ServerMsg::State(st)) if !st.dirty && st.message.contains("war3map.w3i") && st.message.contains("war3map.shd")), "{saved:?}");
    let t = w3e_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3e")).unwrap()).unwrap();
    assert_eq!((t.width, t.offset_x, t.ground.len()), (41, -128.0 - 8.0 * 128.0, 2));
    let i = w3i_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3i")).unwrap()).unwrap();
    assert_eq!(i.camera_complements, [10, 2, 2, 2]);
    assert_eq!(std::fs::read(dir.path().join("map/war3map.shd")).unwrap().len(), 160 * 128);
    assert_eq!(wpm_rs::parse(&std::fs::read(dir.path().join("map/war3map.wpm")).unwrap()).unwrap().width, 160);
    // The tile pather: grass (where it lies: everywhere) unwalkable,
    // undone.
    let msgs = s.tile_pathing(0, wpm_rs::NO_WALK);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.undo && st.message.contains("Lgrs")), "{msgs:?}");
    let p = s.placed.pathing.as_ref().unwrap();
    assert!(p.cells.iter().all(|c| c & wpm_rs::NO_WALK != 0 || c & wpm_rs::NO_FLY != 0));
    s.undo(false);
    assert!(s.placed.pathing.as_ref().unwrap().cells.iter().any(|c| c & wpm_rs::NO_WALK == 0));
    // Cut on the right (to x 2944): the camera looking past it removed,
    // the other kept; saved.
    let msgs = s.resize(crate::resize::Sides { right: -8, ..Default::default() }, None);
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("1 cameras")), "{msgs:?}");
    assert_eq!(s.overlays().cameras.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Kept"]);
    let saved = s.save();
    assert!(matches!(saved.last(), Some(ServerMsg::State(st)) if st.message.contains("war3map.w3c")), "{saved:?}");
    let c = w3c_rs::parse_any(&std::fs::read(dir.path().join("map/war3map.w3c")).unwrap()).unwrap();
    assert_eq!(c.cameras.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Kept"]);
    // Too small: refused, nothing changed.
    let msgs = s.resize(crate::resize::Sides { right: -20, ..Default::default() }, None);
    assert!(matches!(&msgs[..], [ServerMsg::State(st)] if st.message.contains("32 to 480")));
}

#[test]
fn the_terrains_text_form_its_header_in_step_its_points_saved_by_the_editor() {
    let dir = TestDir::new("session-w3e-text");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/TerrainArt/Terrain.slk", &testdir::slk(&[&["tileID", "dir", "file"], &["Ldrt", "T", "Dirt"], &["Lgrs", "T", "Grass"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    assert!(s.follow().iter().any(|(n, _)| n == "war3map.w3e"));
    let path = dir.path().join("map/war3map.w3e").display().to_string();
    let text = w3e_rs::text::to_text(&s.terrain);
    // Opened as it is: nothing to take in.
    assert!(s.text_changed(&path, &text).is_empty());
    // A tile added and the corner moved, typed: taken in, drawn again,
    // undone as one.
    let typed = text.replace("ground=Ldrt", "ground=Ldrt,Lgrs").replace("offset_x=-128", "offset_x=-256");
    assert_ne!(typed, text);
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(&msgs[0], ServerMsg::Map(_)), "{msgs:?}");
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.undo && st.dirty), "{msgs:?}");
    assert_eq!((s.terrain.ground.len(), s.terrain.offset_x), (2, -256.0));
    // Another size: the points don't allow it; said, nothing taken.
    let msgs = s.text_changed(&path, &typed.replace("width=3", "width=9"));
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.contains("width")), "{msgs:?}");
    assert_eq!(s.terrain.width, 3);
    s.text_changed(&path, &typed);
    // Undone in the editor: given to the document; the points still the
    // editor's to save.
    s.undo(false);
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { text, .. }] if text.contains("offset_x=-128")), "{out:?}");
    assert!(s.edits.dirty.terrain);
    let saved = s.save();
    assert!(matches!(saved.last(), Some(ServerMsg::State(st)) if st.message.contains("war3map.w3e")), "{saved:?}");
    assert!(saved.iter().any(|m| matches!(m, ServerMsg::SaveDocs(p) if p == &vec![path.clone()])), "{saved:?}");
}

#[test]
fn a_things_fields_said_set_in_their_ranges_undone_and_saved() {
    let dir = TestDir::new("session-thing-fields");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "file", "numVar"], &["LTrc", "Doodads\\Rock", "3"]]));
    dir.write("map/Units/DestructableData.slk", &testdir::slk(&[&["DestructableID", "file", "numVar"], &["LTbr", "Doodads\\Barrel", "1"]]));
    dir.write("map/Units/UnitData.slk", &testdir::slk(&[&["unitID", "file"], &["hfoo", "Units\\Footman"], &["Hpal", "Units\\Paladin"], &["ngol", "Buildings\\GoldMine"]]));
    dir.write("map/Units/UnitAbilities.slk", &testdir::slk(&[&["unitAbilID", "abilList"], &["ngol", "Agld,Abgm"]]));
    dir.write("map/Units/ItemData.slk", &testdir::slk(&[&["itemID", "file"], &["ratc", "Objects\\Claws"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    s.place(0, "LTrc", 0, [1.0, 0.5, 10.0], down);
    s.place(1, "LTbr", 0, [2.0, 0.5, 10.0], down);
    for id in ["hfoo", "Hpal", "ngol"] {
        s.place(2, id, 3, [1.0, 0.5, 10.0], down);
    }
    s.place(3, "ratc", 0, [1.0, 0.5, 10.0], down);
    let names = |s: &Session, file, index| s.thing(file, index).fields.into_iter().map(|(n, _)| n).collect::<Vec<_>>().join(" ");
    let value = |s: &Session, file, index, name: &str| s.thing(file, index).fields.into_iter().find(|(n, _)| n == name).map(|(_, v)| v);
    let common = "x y facing scale_x scale_y scale_z";
    assert_eq!(names(&s, 0, 0), "x y z facing scale_x scale_y scale_z variation state");
    assert_eq!(names(&s, 0, 1), "x y z facing scale_x scale_y scale_z variation state life item_table");
    assert_eq!(names(&s, 1, 0), format!("{common} player hp mana target_acquisition color item_table"));
    assert_eq!(names(&s, 1, 1), format!("{common} player hp mana target_acquisition color item_table level strength agility intelligence"));
    assert_eq!(names(&s, 1, 2), format!("{common} player hp mana target_acquisition color item_table gold"));
    assert_eq!(names(&s, 1, 3), common);
    assert!(s.thing(1, 9).fields.is_empty());
    assert_eq!((value(&s, 1, 0, "facing"), value(&s, 1, 0, "player"), value(&s, 1, 0, "hp")), (Some(270.0), Some(3.0), Some(-1.0)));
    assert_eq!(value(&s, 1, 2, "gold"), Some(12500.0));
    // Set, an undo each; out of its range, or one it hasn't, said and not set.
    let said = |msgs: Vec<ServerMsg>| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    assert_eq!(said(s.thing_set(0, 0, "variation", 2.0)), "");
    assert!(said(s.thing_set(0, 0, "variation", 3.0)).contains("variation can't be 3"));
    assert!(said(s.thing_set(0, 0, "life", 50.0)).contains("has no life"));
    assert_eq!(said(s.thing_set(0, 1, "life", 40.0)), "");
    assert!(said(s.thing_set(0, 1, "life", 140.0)).contains("can't be 140"));
    assert_eq!(said(s.thing_set(0, 0, "scale_z", 2.5)), "");
    assert!(said(s.thing_set(0, 0, "scale_x", 0.0)).contains("can't be 0"));
    assert_eq!(said(s.thing_set(1, 1, "facing", 90.0)), "");
    assert_eq!(said(s.thing_set(1, 1, "level", 5.0)), "");
    assert_eq!(said(s.thing_set(1, 1, "strength", 30.0)), "");
    assert_eq!(said(s.thing_set(1, 0, "hp", 50.0)), "");
    assert!(said(s.thing_set(1, 0, "hp", 0.0)).contains("can't be 0"));
    assert_eq!(said(s.thing_set(1, 0, "player", 12.0)), "");
    assert_eq!(said(s.thing_set(1, 2, "gold", 30000.0)), "");
    assert!(said(s.thing_set(1, 3, "player", 1.0)).contains("has no player"));
    let u = &s.placed.units.as_ref().unwrap().units;
    assert!((u[1].placement.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert_eq!((u[1].level, u[1].stats, u[0].hp, u[0].player, u[2].gold), (5, [30, 0, 0], 50, 12, 30000));
    let d = &s.placed.doodads.as_ref().unwrap().doodads;
    assert_eq!((d[0].placement.variation, d[0].placement.scale, d[1].life), (2, [1.0, 1.0, 2.5], 40));
    // The gold mine's, then the owner's undone.
    s.undo(false);
    s.undo(false);
    let u = &s.placed.units.as_ref().unwrap().units;
    assert_eq!((u[0].player, u[2].gold), (3, 12500));
    s.save();
    let back = doo_rs::parse_units_any(&std::fs::read(dir.path().join("map/war3mapUnits.doo")).unwrap()).unwrap();
    assert_eq!((back.units[1].level, back.units[0].hp, back.units[0].player), (5, 50, 3));
    // Its lists as text: a hero's inventory, abilities, drops; a
    // destructable's drops; read back, or said.
    let texts = |s: &Session, file, index| s.thing(file, index).texts.into_iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join(" ");
    assert_eq!(texts(&s, 1, 1), "items=,,,,, abilities= drops= script_name=gg_unit_Hpal_0001");
    assert_eq!(texts(&s, 1, 0), "abilities= drops= script_name=gg_unit_hfoo_0000");
    assert_eq!(texts(&s, 0, 1), "drops= script_name=gg_dest_LTbr_0001");
    assert_eq!(texts(&s, 0, 0), "", "a doodad has none");
    assert_eq!(texts(&s, 1, 3), "script_name=gg_item_ratc_0003", "an item: its name alone");
    assert_eq!(said(s.thing_set_text(1, 1, "items", "ratc,,ckng")), "");
    assert_eq!(said(s.thing_set_text(1, 1, "abilities", "AHhb:2:0,AHds:1:1")), "");
    assert_eq!(said(s.thing_set_text(1, 0, "drops", "ratc:50,ckng:50;sreg:100")), "");
    assert_eq!(said(s.thing_set_text(0, 1, "drops", "ratc:25")), "");
    assert!(said(s.thing_set_text(1, 0, "drops", "ratc:60,ckng:50")).contains("110%, over 100"));
    assert!(said(s.thing_set_text(1, 1, "abilities", "AHhb:0")).contains("a level from 1"));
    assert!(said(s.thing_set_text(1, 1, "items", "toolong")).contains("isn't an id"));
    assert!(said(s.thing_set_text(1, 0, "items", "ratc")).contains("has no items"));
    assert_eq!(texts(&s, 1, 1), "items=ratc,,ckng,,, abilities=AHhb:2:0,AHds:1:1 drops= script_name=gg_unit_Hpal_0001");
    assert_eq!(texts(&s, 1, 0), "abilities= drops=ratc:50,ckng:50;sreg:100 script_name=gg_unit_hfoo_0000");
    assert_eq!(texts(&s, 0, 1), "drops=ratc:25 script_name=gg_dest_LTbr_0001");
    s.undo(false);
    assert_eq!(texts(&s, 0, 1), "drops= script_name=gg_dest_LTbr_0001");
    // A random unit and a random item: how they're random.
    s.place(2, "uDNR", 0, [1.0, 0.5, 10.0], down);
    s.place(3, "iDNR", 0, [1.0, 0.5, 10.0], down);
    assert_eq!(texts(&s, 1, 4), "random=any:0 random_groups= drops= script_name=gg_unit_uDNR_0004");
    assert_eq!(texts(&s, 1, 5), "random=any:0:0 script_name=gg_item_iDNR_0005");
    assert_eq!(said(s.thing_set_text(1, 4, "random", "table:hfoo:50,hkni:50")), "");
    assert!(said(s.thing_set_text(1, 4, "random", "group:0:0")).contains("no random group 0"));
    assert!(said(s.thing_set_text(1, 4, "random", "table:hfoo:60,hkni:50")).contains("over 100"));
    assert!(said(s.thing_set_text(1, 4, "random", "some")).contains("any, group or table"));
    assert_eq!(said(s.thing_set_text(1, 5, "random", "any:3:2")), "");
    assert!(said(s.thing_set_text(1, 5, "random", "any:3:9")).contains("class 0 to 7"));
    assert_eq!(texts(&s, 1, 4), "random=table:hfoo:50,hkni:50 random_groups= drops= script_name=gg_unit_uDNR_0004");
    assert_eq!(texts(&s, 1, 5), "random=any:3:2 script_name=gg_item_iDNR_0005");
    s.save();
    let back = doo_rs::parse_units_any(&std::fs::read(dir.path().join("map/war3mapUnits.doo")).unwrap()).unwrap();
    assert_eq!(back.units[5].random, doo_rs::Random::Any { level: 3, class: 2 });
}

#[test]
fn sounds_made_set_in_their_ranges_deleted_undone_saved_played_and_in_step_with_their_text() {
    let dir = TestDir::new("session-sounds");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Sound/Rain.wav", b"RIFF....WAVE");
    let mut s = Session::open(dir.path()).unwrap();
    let drawn = s.draw();
    assert!(drawn.iter().any(|m| matches!(m, ServerMsg::Sounds(l) if l.is_empty())), "the page told: none");
    let said = |msgs: &[ServerMsg]| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    let sounds = |s: &Session| s.placed.sounds.as_ref().map(|n| n.sounds.clone()).unwrap_or_default();
    // Made: named after the file, a second one numbered.
    let msgs = s.sound_new("Sound/Rain.wav");
    assert!(matches!(&msgs[0], ServerMsg::Sounds(l) if l.len() == 1) && said(&msgs).is_empty(), "{msgs:?}");
    s.sound_new("Sound\\Rain.wav");
    assert!(said(&s.sound_new("Sound\\Thunder.wav")).contains("isn't in the game's files"));
    let names: Vec<String> = sounds(&s).iter().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["gg_snd_Rain", "gg_snd_Rain_2", "gg_snd_Thunder"]);
    assert_eq!(sounds(&s)[0].file, "Sound\\Rain.wav");
    // Set: in range, or said and not set.
    assert_eq!(said(&s.sound_set(0, "volume", "100")), "");
    assert!(said(&s.sound_set(0, "volume", "128")).contains("volume can't be"));
    assert!(said(&s.sound_set(0, "volume", "1.5")).contains("volume can't be"));
    assert_eq!(said(&s.sound_set(0, "pitch", "1.25")), "");
    assert_eq!(said(&s.sound_set(0, "channel", "10")), "");
    assert!(said(&s.sound_set(0, "channel", "15")).contains("channel can't be"));
    assert_eq!(said(&s.sound_set(0, "flags", &(w3s_rs::LOOPING | w3s_rs::SOUND_3D).to_string())), "");
    assert_eq!(said(&s.sound_set(0, "min_distance", "600")), "");
    assert_eq!(said(&s.sound_set(0, "min_distance", "")), "");
    assert!(said(&s.sound_set(0, "name", "gg_snd_Rain_2")).contains("Another sound"));
    assert!(said(&s.sound_set(0, "name", "Rain")).contains("gg_snd_"));
    assert_eq!(said(&s.sound_set(0, "name", "gg_snd_Storm")), "");
    let rain = &sounds(&s)[0];
    assert_eq!((rain.name.as_str(), rain.volume, rain.pitch, rain.channel, rain.flags), ("gg_snd_Storm", 100, 1.25, 10, w3s_rs::LOOPING | w3s_rs::SOUND_3D));
    assert_eq!(rain.min_distance, w3s_rs::DEFAULT_REAL);
    // Deleted, undone; the name's set undone.
    s.sound_delete(1);
    assert_eq!(sounds(&s).len(), 2);
    let undone = s.undo(false);
    assert!(matches!(&undone[0], ServerMsg::Sounds(l) if l.len() == 3));
    s.undo(false);
    assert_eq!(sounds(&s)[0].name, "gg_snd_Rain");
    s.undo(true);
    // The sound files to pick from: the map's (a root folder's) and the ones to be imported.
    assert_eq!(s.sound_files(), ["Sound\\Rain.wav"]);
    // Played: the file's bytes, or why not.
    assert!(matches!(s.sound_data("Sound\\Rain.wav"), ServerMsg::SoundData { bytes, error, .. } if bytes == b"RIFF....WAVE" && error.is_empty()));
    assert!(matches!(s.sound_data("Sound\\Thunder.wav"), ServerMsg::SoundData { bytes, error, .. } if bytes.is_empty() && error.contains("isn't")));
    // Saved into the root.
    let saved = s.save();
    assert!(said(&saved).contains("war3map.w3s"), "{saved:?}");
    let back = w3s_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3s")).unwrap()).unwrap();
    assert_eq!(back.sounds, sounds(&s));
    // Its text form: the editor's change goes to it, typed in it the editor follows.
    let path = dir.path().join("map/war3map.w3s").display().to_string();
    let text = s.piece_text("war3map.w3s", &path).unwrap();
    assert!(text.contains("[gg_snd_Storm]"), "{text}");
    s.text_changed(&path, &text);
    s.sound_set(0, "volume", "50");
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { text, .. }] if text.contains("volume=50")), "{out:?}");
    let typed = s.piece_text("war3map.w3s", &path).unwrap().replace("[gg_snd_Storm]", "[gg_snd_Typed]");
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(&msgs[0], ServerMsg::Sounds(l) if l[0].name == "gg_snd_Typed"), "{msgs:?}");
}

#[test]
fn objects_listed_their_fields_set_reset_made_deleted_undone_saved_and_in_step_with_their_text() {
    let dir = TestDir::new("session-objects");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write(
        "map/Units/UnitMetaData.slk",
        &testdir::slk(&[
            &["ID", "field", "slk", "type", "minVal", "maxVal", "category", "useItem", "useUnit", "useHero", "useBuilding"],
            &["uhpm", "HP", "UnitBalance", "int", "1", "100000", "stats", "0", "1", "1", "1"],
            &["unam", "Name", "Profile", "string", "", "", "text", "1", "1", "1", "1"],
            &["umvs", "spd", "UnitBalance", "unreal", "0", "522", "move", "0", "1", "1", "0"],
            &["ustr", "STR", "UnitBalance", "int", "0", "100", "stats", "0", "0", "1", "0"],
            &["ubdg", "isbldg", "UnitData", "bool", "", "", "stats", "0", "1", "1", "1"],
            &["uabi", "abilList", "UnitAbilities", "abilityList", "", "", "abil", "0", "1", "1", "1"],
        ]),
    );
    dir.write("map/Units/UnitData.slk", &testdir::slk(&[&["unitID", "race", "isbldg"], &["hfoo", "human", "0"], &["ogru", "orc", "0"], &["Hpal", "human", "0"], &["htow", "human", "1"]]));
    dir.write("map/Units/UnitBalance.slk", &testdir::slk(&[&["unitBalanceID", "HP", "spd"], &["hfoo", "420", "270"], &["ogru", "700", "270"]]));
    dir.write("map/Units/HumanUnitStrings.txt", b"[hfoo]\nName=Footman\n");
    dir.write("map/Units/UnitAbilities.slk", &testdir::slk(&[&["unitAbilID", "abilList"], &["hfoo", "AHhb,Adef"]]));
    dir.write("map/Units/HumanAbilityStrings.txt", b"[AHhb]\nName=Holy Light\n");
    dir.write("map/Units/AbilityData.slk", &testdir::slk(&[&["alias", "code"], &["AHhb", "AHhb"]]));
    dir.write("map/UI/UnitEditorData.txt", b"[attackType]\n00=normal,WESTRING_A\n01=pierce,Pierce\nNumValues=2\n\n[targetList]\n00=air,Air\n01_Alt=aliv\n01=alive,Alive\n");
    dir.write("map/UI/WorldEditStrings.txt", b"[WorldEditStrings]\nWESTRING_A=Normal\n");
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let said = |msgs: &[ServerMsg]| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    let list = |s: &mut Session| match s.object_list(0) {
        ServerMsg::ObjectList(l) => l.objects,
        m => panic!("{m:?}"),
    };
    let fields = |s: &mut Session, id: &str| match s.object_fields(0, id) {
        ServerMsg::ObjectFields(f) => f,
        m => panic!("{m:?}"),
    };
    let groups: Vec<(String, String)> = list(&mut s).into_iter().map(|o| (o.0, o.2)).collect();
    assert_eq!(groups, [
        ("htow".to_string(), "Melee · human · Buildings".to_string()),
        ("Hpal".into(), "Melee · human · Heroes".into()),
        ("hfoo".into(), "Melee · human · Units".into()),
        ("ogru".into(), "Melee · orc · Units".into()),
    ]);
    // The field types' values first, their names World Editor's.
    let first = s.object_list_first(0);
    let ServerMsg::ObjectEnums(enums) = &first[0] else { panic!("{first:?}") };
    assert_eq!(enums[0], ("attackType".into(), false, vec![("normal".into(), "Normal".into()), ("pierce".into(), "Pierce".into())]));
    assert_eq!((enums[1].0.as_str(), enums[1].1, enums[1].2.len()), ("targetList", true, 2));
    assert!(matches!(&s.object_list_first(0)[..], [ServerMsg::ObjectList(_)]), "once");
    let f = fields(&mut s, "hfoo");
    let row = |f: &crate::protocol::ObjectFieldsMsg, id: &str| f.fields.iter().find(|r| r.field == id).cloned().unwrap();
    assert_eq!((row(&f, "uabi").value.as_str(), row(&f, "uabi").hint.as_str()), ("AHhb,Adef", "Holy Light, Adef"), "an id list's names");
    let ids = |f: &crate::protocol::ObjectFieldsMsg| f.fields.iter().map(|r| r.field.as_str()).collect::<Vec<_>>().join(" ");
    assert_eq!(ids(&f), "uabi umvs uhpm ubdg unam", "by category: abil, move, stats, text; a unit's");
    assert_eq!(ids(&fields(&mut s, "Hpal")), "uabi umvs uhpm ustr ubdg unam", "a hero's");
    assert_eq!(ids(&fields(&mut s, "htow")), "uabi uhpm ubdg unam", "a building's");
    assert_eq!((row(&f, "uhpm").value.as_str(), row(&f, "uhpm").changed), ("420", false));
    // Set in its limits, an undo each; empty: the game's again.
    assert_eq!(said(&s.object_set(0, "hfoo", "uhpm", 0, 0, "500")), "");
    assert!(said(&s.object_set(0, "hfoo", "uhpm", 0, 0, "0")).contains("1 to 100000"));
    assert_eq!(said(&s.object_set(0, "hfoo", "unam", 0, 0, "Swordsman")), "");
    let f = fields(&mut s, "hfoo");
    assert_eq!((row(&f, "uhpm").value.as_str(), row(&f, "uhpm").default.as_str(), row(&f, "uhpm").changed), ("500", "420", true));
    assert_eq!(s.types.as_ref().unwrap().get(w3abdhqtu_rs::Kind::Unit, "hfoo", "HP"), Some("500"), "the types follow");
    assert_eq!(list(&mut s)[2], ("hfoo".into(), "Swordsman".into(), "Melee · human · Units".into(), 1));
    s.object_set(0, "hfoo", "uhpm", 0, 0, "");
    assert!(!row(&fields(&mut s, "hfoo"), "uhpm").changed);
    // A custom one: its id World Editor's, its base's changes copied.
    let made = s.object_new(0, "hfoo");
    assert!(made.iter().any(|m| matches!(m, ServerMsg::ObjectPalette(p) if p.iter().any(|e| e.id == "h000" && e.custom))), "the palette has it");
    assert!(matches!(&made[1], ServerMsg::ObjectFields(f) if f.id == "h000" && f.base == "hfoo"), "{made:?}");
    assert!(said(&s.object_new(0, "zzzz")).contains("isn't an object"));
    s.object_set(0, "h000", "uhpm", 0, 0, "900");
    assert_eq!(list(&mut s)[4], ("h000".into(), "Swordsman".into(), "Custom · Melee · human · Units".into(), 2), "its base's name change copied");
    assert_eq!(row(&fields(&mut s, "h000"), "uhpm").value, "900");
    // Copied and pasted: its base, its changes.
    let pasted = s.object_new(0, "h000");
    assert!(matches!(&pasted[1], ServerMsg::ObjectFields(f) if f.id == "h001" && f.base == "hfoo"), "{pasted:?}");
    assert_eq!(row(&fields(&mut s, "h001"), "uhpm").value, "900");
    s.undo(false);
    // Deleted, undone.
    s.object_delete(0, "h000");
    assert_eq!(list(&mut s).len(), 4);
    let undone = s.undo(false);
    assert!(matches!(undone.iter().find(|m| matches!(m, ServerMsg::ObjectList(_))), Some(ServerMsg::ObjectList(l)) if l.objects.len() == 5));
    // Saved into the root.
    assert!(said(&s.save()).contains("war3map.w3u"));
    let back = w3abdhqtu_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3u")).unwrap(), w3abdhqtu_rs::Kind::Unit).unwrap();
    assert_eq!((back.originals.len(), back.customs.len()), (1, 1));
    // Its text form: the editor's change goes to it, typed in it the editor follows.
    let path = dir.path().join("map/war3map.w3u").display().to_string();
    let text = s.piece_text("war3map.w3u", &path).unwrap();
    assert!(text.contains("[h000]") && text.contains("HP=900"), "{text}");
    s.text_changed(&path, &text);
    s.object_set(0, "h000", "uhpm", 0, 0, "950");
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { text, .. }] if text.contains("HP=950")), "{out:?}");
    let typed = s.piece_text("war3map.w3u", &path).unwrap().replace("HP=950", "HP=990");
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(&msgs[0], ServerMsg::ObjectList(_)), "{msgs:?}");
    assert_eq!(row(&fields(&mut s, "h000"), "uhpm").value, "990");
}

#[test]
fn an_objects_string_of_the_map_shown_as_its_text_and_set_in_war3map_wts() {
    let dir = TestDir::new("session-object-strings");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Units/UnitMetaData.slk", &testdir::slk(&[&["ID", "field", "slk", "type", "category"], &["unam", "Name", "Profile", "string", "text"]]));
    dir.write("map/Units/UnitData.slk", &testdir::slk(&[&["unitID", "race"], &["hfoo", "human"]]));
    dir.write("map/war3map.wts", "\u{FEFF}STRING 7\n{\nKnight of the Map\n}\n".as_bytes());
    let mut data = w3abdhqtu_rs::ObjectData::new(w3abdhqtu_rs::Kind::Unit);
    data.version = 2;
    crate::objdata::set(&mut data, parser_rs::bin::Rawcode(*b"hfoo"), parser_rs::bin::Rawcode(*b"unam"), 0, 0, Some(w3abdhqtu_rs::Value::Str("TRIGSTR_007".into())));
    dir.write("map/war3map.w3u", &data.to_bytes().unwrap());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let name = |s: &mut Session| match s.object_fields(0, "hfoo") {
        ServerMsg::ObjectFields(f) => (f.name.clone(), f.fields[0].value.clone()),
        m => panic!("{m:?}"),
    };
    assert_eq!(name(&mut s), ("Knight of the Map".into(), "Knight of the Map".into()));
    // Typed: the string set, the field still naming it.
    let msgs = s.object_set(0, "hfoo", "unam", 0, 0, "Knight Renamed");
    assert!(matches!(msgs.last(), Some(ServerMsg::State(st)) if st.message.is_empty() && st.dirty), "{msgs:?}");
    assert_eq!(name(&mut s), ("Knight Renamed".into(), "Knight Renamed".into()));
    let mods: Vec<_> = s.placed.objects[&w3abdhqtu_rs::Kind::Unit].originals[0].mods().map(|m| m.value.clone()).collect();
    assert_eq!(mods, [w3abdhqtu_rs::Value::Str("TRIGSTR_007".into())]);
    s.undo(false);
    assert_eq!(name(&mut s).0, "Knight of the Map");
    s.undo(true);
    // A copy names a string of its own, the same text; undone as one.
    let copy = s.object_new(0, "hfoo");
    assert!(matches!(&copy[1], ServerMsg::ObjectFields(f) if f.name == "Knight Renamed"), "{copy:?}");
    let mods: Vec<_> = s.placed.objects[&w3abdhqtu_rs::Kind::Unit].customs[0].mods().map(|m| m.value.clone()).collect();
    assert_eq!(mods, [w3abdhqtu_rs::Value::Str("TRIGSTR_008".into())]);
    s.object_set(0, "h000", "unam", 0, 0, "The Copy");
    assert_eq!(name(&mut s).0, "Knight Renamed", "the original's string untouched");
    s.undo(false);
    s.undo(false);
    assert!(s.placed.objects[&w3abdhqtu_rs::Kind::Unit].customs.is_empty() && !s.placed.strings.as_ref().unwrap().0.contains("STRING 8"));
    let saved = s.save();
    assert!(matches!(saved.last(), Some(ServerMsg::State(st)) if st.message.contains("war3map.wts")), "{saved:?}");
    assert_eq!(std::fs::read_to_string(dir.path().join("map/war3map.wts")).unwrap(), "\u{FEFF}STRING 7\n{\nKnight Renamed\n}\n");
}

#[test]
fn files_imported_moved_deleted_undone_and_saved_with_their_list() {
    let dir = TestDir::new("session-imports");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/war3mapImported/old.txt", b"old");
    dir.write("map/war3map.imp", &imp_rs::Imports { version: 1, files: vec![imp_rs::Import { flags: 0, path: "old.txt".into() }] }.to_bytes().unwrap());
    dir.write("disk/Rain.wav", b"RIFF....WAVE");
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let list = |s: &Session| match s.imports_msg() {
        ServerMsg::Imports(l) => l.into_iter().map(|(p, _, f, found, pending)| format!("{p} {f} {found} {pending}")).collect::<Vec<_>>(),
        m => panic!("{m:?}"),
    };
    let said = |msgs: &[ServerMsg]| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    assert_eq!(list(&s), ["war3mapImported\\old.txt 0 true false"]);
    let source = dir.path().join("disk/Rain.wav").display().to_string();
    assert_eq!(said(&s.import_add(&source, "")), "");
    assert!(said(&s.import_add(&source, "war3mapImported/Rain.wav")).contains("imported already"));
    assert_eq!(said(&s.import_add(&source, "Sound\\Custom\\Rain.wav")), "");
    assert_eq!(list(&s)[1..], ["war3mapImported\\Rain.wav 0 true true".to_string(), "Sound\\Custom\\Rain.wav 1 true true".into()]);
    assert!(matches!(s.import_preview(2), ServerMsg::ImportPreview { what: 2, size: 12, .. }));
    // Moved: its file with it; deleted: out on save.
    assert_eq!(said(&s.import_set(0, "Docs\\old.txt")), "");
    assert_eq!(said(&s.import_delete(1)), "");
    assert_eq!(list(&s), ["Docs\\old.txt 1 true true", "Sound\\Custom\\Rain.wav 1 true true"]);
    s.undo(false);
    assert_eq!(list(&s).len(), 3, "the delete undone");
    s.undo(true);
    let saved = s.save();
    assert!(said(&saved).contains("war3map.imp"), "{saved:?}");
    assert_eq!(std::fs::read(dir.path().join("map/Docs/old.txt")).unwrap(), b"old");
    assert!(!dir.path().join("map/war3mapImported/old.txt").exists(), "moved");
    assert!(!dir.path().join("map/war3mapImported/Rain.wav").exists(), "deleted before it was written");
    assert_eq!(std::fs::read(dir.path().join("map/Sound/Custom/Rain.wav")).unwrap(), b"RIFF....WAVE");
    let back = imp_rs::parse(&std::fs::read(dir.path().join("map/war3map.imp")).unwrap()).unwrap();
    assert_eq!(back.files.iter().map(|f| f.path_in_map()).collect::<Vec<_>>(), ["Docs\\old.txt", "Sound\\Custom\\Rain.wav"]);
    assert_eq!(list(&s), ["Docs\\old.txt 1 true false", "Sound\\Custom\\Rain.wav 1 true false"]);
}

#[test]
fn the_maps_info_shown_set_its_strings_in_war3map_wts_undone_and_saved() {
    let dir = TestDir::new("session-info");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut info = w3i_rs::parse(&testdir::info()).unwrap();
    info.name = "TRIGSTR_001".into();
    info.players = vec![w3i_rs::Player { number: 0, kind: 1, race: 1, name: "Red".into(), ..Default::default() }];
    info.forces = vec![w3i_rs::Force { flags: 0, players: 1, name: "Force 1".into() }];
    dir.write("map/war3map.w3i", &info.to_bytes().unwrap());
    dir.write("map/war3map.wts", b"STRING 1\n{\nThe Map\n}\n");
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let get = |s: &Session, k: &str| s.info_fields().into_iter().find(|(f, _)| f == k).map(|(_, v)| v);
    let said = |msgs: &[ServerMsg]| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    assert_eq!(get(&s, "name").as_deref(), Some("The Map"), "a string of the map's as its text");
    assert_eq!(get(&s, "tft").as_deref(), Some("1"));
    assert_eq!(get(&s, "player.0.name").as_deref(), Some("Red"));
    assert_eq!(get(&s, "force.0.players").as_deref(), Some("1"));
    // Its name a string: the string set; the author a text: set.
    let msgs = s.info_set("name", "The Renamed Map");
    assert!(matches!(&msgs[1], ServerMsg::Map(m) if m.name == "The Renamed Map"), "{msgs:?}");
    assert_eq!(s.info.as_ref().unwrap().name, "TRIGSTR_001");
    assert_eq!(said(&s.info_set("author", "Someone")), "");
    assert_eq!(said(&s.info_set("fog_style", "2")), "");
    assert_eq!(said(&s.info_set("fog_color", "255,0,0,255")), "");
    assert_eq!(said(&s.info_set("player.0.race", "2")), "");
    assert_eq!(said(&s.info_set("force.0.flags", "3")), "");
    assert_eq!(said(&s.info_set("weather", "RAhr")), "");
    assert!(said(&s.info_set("fog_style", "7")).contains("0 to 3"));
    assert!(said(&s.info_set("fog_color", "300,0,0,0")).contains("r,g,b,a"));
    assert!(said(&s.info_set("player.3.name", "X")).contains("no player 3"));
    assert!(said(&s.info_set("nothing", "X")).contains("no field nothing"));
    let i = s.info.as_ref().unwrap();
    assert_eq!((i.author.as_str(), i.fog.style, i.fog.color, i.players[0].race, i.forces[0].flags, i.weather.to_string().as_str()), ("Someone", 2, [0, 0, 255, 255], 2, 3, "RAhr"));
    assert_eq!(get(&s, "fog_color").as_deref(), Some("255,0,0,255"));
    s.undo(false);
    assert_eq!(get(&s, "weather").as_deref(), Some(""));
    s.undo(true);
    let saved = s.save();
    assert!(said(&saved).contains("war3map.w3i") && said(&saved).contains("war3map.wts"), "{saved:?}");
    let back = w3i_rs::parse(&std::fs::read(dir.path().join("map/war3map.w3i")).unwrap()).unwrap();
    assert_eq!((back.author.as_str(), back.players[0].race), ("Someone", 2));
    // Players and forces added and taken out (a player out of its forces too).
    assert_eq!(said(&s.info_set("player.add", "3")), "");
    assert!(said(&s.info_set("player.add", "3")).contains("there already"));
    assert!(said(&s.info_set("player.add", "30")).contains("1 to 12"));
    assert_eq!(get(&s, "player.1.name").as_deref(), Some("Player 4"));
    assert_eq!(said(&s.info_set("force.add", "")), "");
    assert_eq!(said(&s.info_set("force.1.players", "9")), "");
    assert_eq!(said(&s.info_set("player.1.remove", "")), "");
    assert_eq!((get(&s, "player.1.name"), get(&s, "force.1.players").as_deref()), (None, Some("1")));
    assert_eq!(said(&s.info_set("force.0.remove", "")), "");
    assert_eq!(get(&s, "force.0.name").as_deref(), Some("Force 2"));
    // The tech tree and the upgrades.
    assert_eq!(said(&s.info_set("techs", "AHbz:3;hfoo:1")), "");
    assert_eq!(said(&s.info_set("upgrades", "Rhme:0:2:1;Rhar:1:0:3")), "");
    assert!(said(&s.info_set("upgrades", "Rhme:0:5:1")).contains("availability 0 to 2"));
    assert!(said(&s.info_set("techs", "toolong:1")).contains("four characters"));
    assert_eq!((get(&s, "techs").as_deref(), get(&s, "upgrades").as_deref()), (Some("AHbz:3;hfoo:1"), Some("Rhme:0:2:1;Rhar:1:0:3")));
    assert_eq!(s.info.as_ref().unwrap().upgrades[1], w3i_rs::Upgrade { players: 3, id: parser_rs::bin::Rawcode(*b"Rhar"), level: 1, availability: 0 });
    // Random groups and item tables.
    assert_eq!(said(&s.info_set("unit_table.add", "")), "");
    assert_eq!(said(&s.info_set("unit_table.0.columns", "0,2")), "");
    assert_eq!(said(&s.info_set("unit_table.0.rows", "60:hfoo,ratc;40:hkni,")), "");
    assert!(said(&s.info_set("unit_table.0.rows", "60:hfoo")).contains("1 ids for 2 columns"));
    assert!(said(&s.info_set("unit_table.0.rows", "60:hfoo,ratc;50:hkni,ratc")).contains("110%"));
    assert!(said(&s.info_set("unit_table.0.columns", "0,5")).contains("a column is"));
    assert_eq!(get(&s, "unit_table.0.name").as_deref(), Some("Random Group 1"));
    assert_eq!(get(&s, "unit_table.0.rows").as_deref(), Some("60:hfoo,ratc;40:hkni,"));
    assert_eq!(said(&s.info_set("unit_table.0.columns", "0")), "");
    assert_eq!(get(&s, "unit_table.0.rows").as_deref(), Some("60:hfoo;40:hkni"), "its rows cut to its columns");
    assert_eq!(said(&s.info_set("item_table.add", "Loot")), "");
    assert_eq!(said(&s.info_set("item_table.0.sets", "ratc:50,ckng:50;sreg:100")), "");
    assert_eq!(get(&s, "item_table.0.sets").as_deref(), Some("ratc:50,ckng:50;sreg:100"));
    assert_eq!(s.info.as_ref().unwrap().item_tables[0].sets[0][0], (50, parser_rs::bin::Rawcode(*b"ratc")));
    assert_eq!(said(&s.info_set("item_table.0.remove", "")), "");
    assert_eq!(get(&s, "item_table.0.name"), None);
    // Its text form: the editor's change goes to it, typed in it the editor follows.
    let path = dir.path().join("map/war3map.w3i").display().to_string();
    let text = s.piece_text("war3map.w3i", &path).unwrap();
    s.text_changed(&path, &text);
    s.info_set("author", "Another");
    let out = s.sync_docs();
    assert!(matches!(&out[..], [ServerMsg::TextOut { text, .. }] if text.contains("Another")), "{out:?}");
    let typed = s.piece_text("war3map.w3i", &path).unwrap().replace("Another", "Typed In Text");
    let msgs = s.text_changed(&path, &typed);
    assert!(matches!(&msgs[1], ServerMsg::Map(m) if m.author == "Typed In Text"), "{msgs:?}");
}

#[test]
fn the_scripts_names_of_what_a_click_on_the_map_picks_for_a_triggers_parameter() {
    let dir = TestDir::new("session-map-names");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut regions = w3r_rs::Regions::new();
    regions.regions.push(w3r_rs::Region::new("My Spawn 1", -64.0, -64.0, 64.0, 64.0, 0));
    dir.write("map/war3map.w3r", &regions.to_bytes().unwrap());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    s.camera_view(-1, [1.0, 0.5, 0.0, 10.0, 0.0, -1.0, 70.0, 0.0]);
    s.camera_rename(0, "Intro Cam!");
    let down = [0.0, 0.0, -1.0];
    let name = |m: ServerMsg| match m {
        ServerMsg::MapName { name, said, .. } => (name, said),
        m => panic!("{m:?}"),
    };
    // (1, 0.5) cells: (0, 0), inside the region.
    assert_eq!(name(s.map_name("rect", -1, [1.0, 0.5, 10.0], down)), ("gg_rct_My_Spawn_1".into(), "My Spawn 1".into()));
    assert!(name(s.map_name("rect", -1, [2.0, 0.5, 10.0], down)).1.contains("No region there"));
    assert_eq!(name(s.map_name("camerasetup", 0, [0.0; 3], down)).0, "gg_cam_Intro_Cam_u", "World Editor's: a name ending in _ gets a u");
    assert!(name(s.map_name("camerasetup", 3, [0.0; 3], down)).1.contains("No camera"));
    assert!(name(s.map_name("unit", -1, [1.0, 0.5, 10.0], down)).1.contains("No unit there"));
    assert!(name(s.map_name("integer", -1, [1.0, 0.5, 10.0], down)).1.contains("Nothing of type integer"));
    // And back: what a name is, and where.
    let find = |s: &Session, n: &str| match s.map_find(n) {
        ServerMsg::MapFound { what, index, at, said, .. } => (what, index, at, said),
        m => panic!("{m:?}"),
    };
    assert_eq!(find(&s, "gg_rct_My_Spawn_1"), (2, 0, [0.0, 0.0, 0.0], "My Spawn 1".to_string()));
    assert_eq!(find(&s, "gg_cam_Intro_Cam_u").0, 3);
    s.place(2, "hfoo", 0, [1.0, 0.5, 10.0], down);
    let n = s.placed.units.as_ref().unwrap().units[0].number;
    assert_eq!(find(&s, &format!("gg_unit_hfoo_{n:04}")).0, 1);
    assert!(find(&s, "gg_unit_hfoo_0999").3.contains("no gg_unit_hfoo_0999"));
    assert!(find(&s, "udg_Hero").3.contains("no name of what's on the map"));
}

#[test]
fn the_camera_bounds_set_by_their_sides_within_the_map_and_from_the_playable_area() {
    let dir = TestDir::new("session-camera-bounds");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &terrain_32());
    let mut info = w3i_rs::parse(&testdir::info()).unwrap();
    info.camera_complements = [2, 2, 2, 2];
    dir.write("map/war3map.w3i", &info.to_bytes().unwrap());
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let get = |s: &Session, k: &str| s.info_fields().into_iter().find(|(f, _)| f == k).map(|(_, v)| v).unwrap();
    let said = |msgs: &[ServerMsg]| match msgs.last() {
        Some(ServerMsg::State(st)) => st.message.clone(),
        m => panic!("{m:?}"),
    };
    let (x0, y0) = (s.terrain.offset_x, s.terrain.offset_y);
    assert_eq!(get(&s, "camera.extent"), format!("{},{},{},{}", x0, y0, x0 + 32.0 * 128.0, y0 + 32.0 * 128.0));
    // From the playable area: four cells in on the sides, two at the bottom and top.
    let msgs = s.info_set("camera.reset", "");
    assert!(msgs.iter().any(|m| matches!(m, ServerMsg::Overlays(_))), "the bounds' lines again");
    assert_eq!(get(&s, "camera.left"), format!("{}", x0 + 6.0 * 128.0));
    assert_eq!(get(&s, "camera.top"), format!("{}", y0 + 28.0 * 128.0));
    // A side: the corners a rectangle still.
    assert_eq!(said(&s.info_set("camera.left", &format!("{}", x0 + 512.0))), "");
    let b = s.info.as_ref().unwrap().camera_bounds;
    assert_eq!((b[0], b[4]), (x0 + 512.0, x0 + 512.0));
    // Past the map, or past the other side: said, kept.
    assert!(said(&s.info_set("camera.right", &format!("{}", x0 + 40.0 * 128.0))).contains("within the map"));
    assert!(said(&s.info_set("camera.left", &format!("{}", x0 + 30.0 * 128.0))).contains("within the map"));
    assert_eq!(s.info.as_ref().unwrap().camera_bounds[0], x0 + 512.0);
    s.undo(false);
    assert_eq!(get(&s, "camera.left"), format!("{}", x0 + 6.0 * 128.0));
}

#[test]
fn a_thing_copied_pasted_anew_where_the_ray_meets_the_ground_and_cut() {
    let dir = TestDir::new("session-paste");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "file", "numVar"], &["LTrc", "Doodads\\Rock", "3"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    let said = |msgs: &[ServerMsg]| msgs.iter().rev().find_map(|m| if let ServerMsg::State(st) = m { Some(st.message.clone()) } else { None }).unwrap();
    assert_eq!(said(&s.paste([1.0, 0.5, 10.0], down)), "Nothing copied to paste.");
    s.place(0, "LTrc", 0, [1.0, 0.5, 10.0], down);
    s.thing_set(0, 0, "variation", 2.0);
    s.thing_set(0, 0, "scale_z", 2.5);
    s.place(2, "hfoo", 3, [1.0, 0.5, 10.0], down);
    s.thing_set_text(1, 0, "abilities", "AHhb:2:0");
    // A doodad: its variation, scale kept; where the ray meets, a number of its own.
    assert_eq!(said(&s.copy(&[(0, 0)])), "");
    let msgs = s.paste([2.0, 0.5, 10.0], down);
    assert!(matches!(msgs.last(), Some(ServerMsg::Pasted(t)) if t == &[(0, 1)]), "{msgs:?}");
    let d = &s.placed.doodads.as_ref().unwrap().doodads;
    assert_eq!((d.len(), d[1].placement.x, d[1].placement.variation, d[1].placement.scale[2], d[1].number), (2, 128.0, 2, 2.5, 1));
    // A unit cut (copied, deleted): pasted after, its owner and abilities kept.
    s.copy(&[(1, 0)]);
    s.delete(1, 0);
    assert!(s.placed.units.as_ref().unwrap().units.is_empty());
    s.paste([2.0, 0.5, 10.0], down);
    let u = &s.placed.units.as_ref().unwrap().units;
    assert_eq!((u.len(), u[0].player, u[0].placement.x, u[0].abilities.len()), (1, 3, 128.0, 1));
    // An undo each; a start location not copied.
    s.undo(false);
    assert!(s.placed.units.as_ref().unwrap().units.is_empty());
    s.place(2, "sloc", 0, [1.0, 0.5, 10.0], down);
    assert!(said(&s.copy(&[(1, 0)])).contains("start location"));
    assert!(said(&s.copy(&[(0, 9)])).contains("no thing 9"));
}

#[test]
fn things_together_moved_turned_copied_pasted_around_the_mouse_and_deleted_one_undo_each() {
    let dir = TestDir::new("session-group");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write("map/Doodads/Doodads.slk", &testdir::slk(&[&["doodID", "file", "numVar"], &["LTrc", "Doodads\\Rock", "1"]]));
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let down = [0.0, 0.0, -1.0];
    // A doodad at (0, 0), a footman at (128, 0): the game's units.
    s.place(0, "LTrc", 0, [1.0, 0.5, 10.0], down);
    s.place(2, "hfoo", 0, [2.0, 0.5, 10.0], down);
    let both = [(0, 0), (1, 0)];
    let at = |s: &Session| {
        let d = &s.placed.doodads.as_ref().unwrap().doodads;
        let u = &s.placed.units.as_ref().unwrap().units;
        (d.iter().map(|d| (d.placement.x, d.placement.angle)).collect::<Vec<_>>(), u.iter().map(|u| (u.placement.x, u.placement.angle)).collect::<Vec<_>>())
    };
    // Dragged half a cell, then a cell: both a cell right; one undo.
    s.group_move(true, &both, [1.0, 0.5, 10.0], down);
    s.group_move(false, &both, [1.5, 0.5, 10.0], down);
    s.group_move(false, &both, [2.0, 0.5, 10.0], down);
    s.stroke_end();
    assert_eq!(at(&s), (vec![(128.0, 270f32.to_radians())], vec![(256.0, 270f32.to_radians())]));
    s.undo(false);
    assert_eq!((at(&s).0[0].0, at(&s).1[0].0), (0.0, 128.0));
    s.undo(true);
    s.group_rotate(&both, std::f32::consts::FRAC_PI_2);
    assert!(at(&s).0[0].1.abs() < 1e-5 && at(&s).1[0].1.abs() < 1e-5);
    // Copied, pasted with their middle (192) at 64: each as far from it.
    s.copy(&both);
    let msgs = s.paste([1.5, 0.5, 10.0], down);
    assert!(matches!(msgs.last(), Some(ServerMsg::Pasted(t)) if t == &[(0, 1), (1, 1)]), "{msgs:?}");
    assert_eq!((at(&s).0[1].0, at(&s).1[1].0), (0.0, 128.0));
    // All four taken away at once, one undo brings them back.
    s.group_delete(&[(0, 0), (1, 0), (0, 1), (1, 1)]);
    assert_eq!((at(&s).0.len(), at(&s).1.len()), (0, 0));
    s.undo(false);
    assert_eq!((at(&s).0.len(), at(&s).1.len()), (2, 2));
}

#[test]
fn gameplay_constants_the_games_or_the_maps_own_set_checked_undone_saved_and_off_again() {
    let dir = TestDir::new("session-constants");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write(
        "map/Units/MiscMetaData.slk",
        &testdir::slk(&[
            &["ID", "field", "section", "displayName", "type", "minVal", "maxVal"],
            &["mhel", "MaxHeroLevel", "Misc", "WESTRING_MHEL", "int", "1", "10000"],
            &["gexp", "GlobalExperience", "Misc", "WESTRING_GEXP", "bool", "", ""],
            &["twn1", "DependencyOr", "TWN1", "WESTRING_TWN1", "unitList", "", ""],
        ]),
    );
    dir.write("map/Units/MiscData.txt", b"[Misc]\nMaxHeroLevel=5\nGlobalExperience=1\n");
    dir.write("map/Units/MiscGame.txt", b"[Misc]\nMaxHeroLevel=10\n");
    dir.write("map/Units/ItemFunc.txt", b"[TWN1]\nDependencyOr=htow,ogre\n");
    dir.write("map/UI/WorldEditStrings.txt", b"[WorldEditStrings]\nWESTRING_MHEL=Hero Maximum Level\n");
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let fields = |s: &mut Session| match s.constants_msg(crate::constants::Book::Misc) {
        ServerMsg::Constants { custom, fields, .. } => (custom, fields),
        m => panic!("{m:?}"),
    };
    let said = |msgs: &[ServerMsg]| msgs.iter().rev().find_map(|m| if let ServerMsg::State(st) = m { Some(st.message.clone()) } else { None }).unwrap();
    // The game's: MiscGame.txt over MiscData.txt; named as World Editor names them.
    let (custom, f) = fields(&mut s);
    assert!(!custom);
    assert_eq!((f[0].name.as_str(), f[0].value.as_str(), f[0].game.as_str(), f[1].value.as_str(), f[2].value.as_str()), ("Hero Maximum Level", "10", "10", "1", "htow,ogre"));
    // Set: the map's own, the key changed alone; out of its limits said.
    assert_eq!(said(&s.constant_set(crate::constants::Book::Misc, "mhel", "25")), "");
    assert!(said(&s.constant_set(crate::constants::Book::Misc, "mhel", "0")).contains("1 at least"));
    assert_eq!(said(&s.constant_set(crate::constants::Book::Misc, "twn1", "htow, ogre, etol")), "");
    assert_eq!(s.placed.books[0].as_deref(), Some("[Misc]\r\nMaxHeroLevel=25\r\n\r\n[TWN1]\r\nDependencyOr=htow,ogre,etol\r\n"));
    let (custom, f) = fields(&mut s);
    assert!(custom && f[0].value == "25" && f[0].game == "10");
    // The game's value again: its line out.
    s.constant_set(crate::constants::Book::Misc, "twn1", "htow,ogre");
    assert_eq!(s.placed.books[0].as_deref(), Some("[Misc]\r\nMaxHeroLevel=25\r\n\r\n[TWN1]\r\n"));
    s.undo(false);
    assert!(s.placed.books[0].as_deref().unwrap().contains("etol"));
    // Saved into the map's root; off again: the file gone on the next save.
    let saved = s.save();
    assert!(said(&saved).contains("war3mapMisc.txt"), "{saved:?}");
    assert!(std::fs::read_to_string(dir.path().join("map/war3mapMisc.txt")).unwrap().contains("MaxHeroLevel=25"));
    s.constant_set(crate::constants::Book::Misc, "custom", "0");
    assert!(!fields(&mut s).0 && fields(&mut s).1[0].value == "10");
    let saved = s.save();
    assert!(said(&saved).contains("removed"), "{saved:?}");
    assert!(!dir.path().join("map/war3mapMisc.txt").exists());
}

#[test]
fn the_game_interface_its_strings_sections_and_the_maps_strings() {
    let dir = TestDir::new("session-skin");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    dir.write(
        "map/UI/SkinMetaData.slk",
        &testdir::slk(&[
            &["ID", "field", "section", "displayName", "type"],
            &["c003", "LumberIcon", "CustomSkin", "WESTRING_C003", "icon"],
            &["f001", "GOLD", "FrameDef", "WESTRING_F001", "string"],
            &["e001", "Emptycontrolgroup", "Errors", "WESTRING_E001", "string"],
        ]),
    );
    dir.write("map/UI/war3skins.txt", b"[Human]\nLumberIcon=Human.blp\n[Default]\nLumberIcon=Default.blp\n");
    dir.write("map/UI/FrameDef/FrameDef.toc", b"UI\\FrameDef\\GlobalStrings.fdf\r\n");
    dir.write("map/UI/FrameDef/GlobalStrings.fdf", b"StringList {\n    GOLD \"Gold\",\n}\n");
    dir.write("map/Units/CommandStrings.txt", b"[Errors]\nEmptycontrolgroup=Unable to form an empty control group.\n");
    // The map's: its gold a string of its own.
    dir.write("map/war3mapSkin.txt", b"[FrameDef]\r\nGOLD=TRIGSTR_001\r\n");
    dir.write("map/war3map.wts", b"STRING 1\r\n{\r\nTreasure\r\n}\r\n");
    let mut s = Session::open(dir.path()).unwrap();
    s.draw();
    let skin = crate::constants::Book::Skin;
    let fields = |s: &mut Session| match s.constants_msg(skin) {
        ServerMsg::Constants { book: 1, custom, fields } => (custom, fields.into_iter().map(|f| (f.value, f.game)).collect::<Vec<_>>()),
        m => panic!("{m:?}"),
    };
    // The race skins' Human over Default; the frames' strings; the errors.
    assert_eq!(
        fields(&mut s),
        (true, vec![("Human.blp".into(), "Human.blp".into()), ("Treasure".into(), "Gold".into()), ("Unable to form an empty control group.".into(), "Unable to form an empty control group.".into())])
    );
    // A string of the map's: the string set, the file as it was.
    s.constant_set(skin, "f001", "Riches");
    assert!(s.placed.strings.as_ref().unwrap().0.contains("Riches"));
    assert_eq!(s.placed.books[1].as_deref(), Some("[FrameDef]\r\nGOLD=TRIGSTR_001\r\n"));
    assert_eq!(fields(&mut s).1[1].0, "Riches");
    // Another: on its line, in its section.
    s.constant_set(skin, "c003", "ReplaceableTextures\\CommandButtons\\BTNAnkh.blp");
    assert_eq!(s.placed.books[1].as_deref(), Some("[FrameDef]\r\nGOLD=TRIGSTR_001\r\n\r\n[CustomSkin]\r\nLumberIcon=ReplaceableTextures\\CommandButtons\\BTNAnkh.blp\r\n"));
    let saved = s.save();
    assert!(saved.iter().any(|m| matches!(m, ServerMsg::State(st) if st.message.contains("war3mapSkin.txt") && st.message.contains("war3map.wts"))), "{saved:?}");
}
