use super::*;
use crate::testdir::{self, TestDir};

/// Every frame `serve` writes for `input`.
fn answers(root: &Path, input: &[u8]) -> Vec<ServerMsg> {
    let mut out = Vec::new();
    serve(&mut &input[..], &mut out, root).unwrap();
    let mut r = &out[..];
    let mut msgs = Vec::new();
    while let Some((kind, body)) = protocol::read_frame(&mut r).unwrap() {
        msgs.push(ServerMsg::decode(kind, &body).unwrap().unwrap());
    }
    msgs
}

#[test]
fn hello_is_answered_with_the_map_each_time() {
    let dir = TestDir::new("serve");
    dir.write("wc3.bni", b"[map]\nroot=map\n");
    dir.write("map/war3map.w3e", &testdir::terrain());
    let mut input = ClientMsg::Hello.encode();
    // A kind it doesn't know, skipped.
    input.extend_from_slice(&[1, 0, 0, 0, 99, 7]);
    input.extend(ClientMsg::Hello.encode());
    let msgs = answers(dir.path(), &input);
    // Each: the map, its ground, its water, its terrain.
    let kinds: Vec<&str> = msgs
        .iter()
        .map(|m| match m {
            ServerMsg::Map(m) if m.width == 3 => "map",
            ServerMsg::Ground(_) => "ground",
            ServerMsg::Water(_) => "water",
            ServerMsg::Cliffs(_) => "cliffs",
            ServerMsg::Objects(_) => "objects",
            ServerMsg::Overlays(_) => "overlays",
            ServerMsg::Palette(_) => "palette",
            ServerMsg::State(_) => "state",
            ServerMsg::Instances(_) => "instances",
            ServerMsg::ObjectPalette(_) => "objects palette",
            ServerMsg::Sounds(_) => "sounds",
            ServerMsg::Follow(_) => "follow",
            ServerMsg::Light(_) => "light",
            ServerMsg::Terrain(t) if t.width == 3 => "terrain",
            _ => "?",
        })
        .collect();
    assert_eq!(kinds, [
        "map", "ground", "water", "cliffs", "terrain", "objects", "instances", "overlays", "palette", "objects palette", "sounds", "follow", "light", "state",
        "map", "ground", "water", "cliffs", "terrain", "objects", "instances", "overlays", "palette", "objects palette", "sounds", "follow", "light", "state",
    ]);
}

#[test]
fn a_map_that_cant_open_is_an_error_message() {
    let dir = TestDir::new("serve-error");
    let msgs = answers(dir.path(), &ClientMsg::Hello.encode());
    assert!(matches!(&msgs[..], [ServerMsg::Error { message }] if message.contains("wc3.bni")));
}

#[test]
fn a_pick_before_the_map_is_open_meets_nothing() {
    let dir = TestDir::new("serve-pick");
    let msgs = answers(dir.path(), &ClientMsg::Pick { origin: [0.0; 3], dir: [0.0, 0.0, -1.0] }.encode());
    assert_eq!(msgs, [ServerMsg::Picked(protocol::PickedMsg::default())]);
}
