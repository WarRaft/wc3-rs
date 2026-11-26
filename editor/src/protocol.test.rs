use super::*;

fn round_trip(msg: ServerMsg) {
    let frame = msg.encode();
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    assert_eq!(ServerMsg::decode(kind, &body).unwrap(), Some(msg));
}

#[test]
fn map_and_error_round_trip() {
    round_trip(ServerMsg::Map(MapInfo {
        project: "/p".into(),
        name: "Карта".into(),
        author: "a".into(),
        width: 65,
        height: 33,
        tileset: "L".into(),
        tileset_name: "Lordaeron Summer".into(),
        notes: vec!["one".into(), "two".into()],
    }));
    round_trip(ServerMsg::Error { message: "no".into() });
}

#[test]
fn the_frame_is_length_kind_body() {
    let frame = ServerMsg::Error { message: "ab".into() }.encode();
    assert_eq!(frame, [6, 0, 0, 0, 129, 2, 0, 0, 0, b'a', b'b']);
    assert_eq!(ClientMsg::Hello.encode(), [0, 0, 0, 0, 1]);
}

#[test]
fn unknown_kinds_are_skipped_and_short_bodies_are_errors() {
    assert_eq!(ClientMsg::decode(77, &[1, 2]).unwrap(), None);
    assert_eq!(ClientMsg::decode(1, &[]).unwrap(), Some(ClientMsg::Hello));
    assert!(ServerMsg::decode(129, &[9, 0, 0, 0, b'a']).is_err());
}

#[test]
fn edits_round_trip() {
    let brush = crate::edit::Brush { tool: crate::edit::Tool::Texture, size: 4.0, strength: 0.5, texture: parser_rs::bin::Rawcode::parse("Lgrs"), cliff: -1, flags: 0 };
    for msg in [
        ClientMsg::Brush { start: true, origin: [1.0, 2.0, 3.0], dir: [0.0, 0.0, -1.0], brush },
        ClientMsg::StrokeEnd,
        ClientMsg::Undo,
        ClientMsg::Redo,
        ClientMsg::Save,
        ClientMsg::ThingAsk { file: 1, index: 7 },
        ClientMsg::ObjectList { kind: 4 },
        ClientMsg::ObjectAsk { kind: 0, id: "hfoo".into() },
        ClientMsg::ObjectSet { kind: 4, id: "A000".into(), field: "Hbz1".into(), level: 2, column: 1, value: "12".into() },
        ClientMsg::ObjectNew { kind: 0, base: "hfoo".into() },
        ClientMsg::ObjectDelete { kind: 0, id: "h000".into() },
        ClientMsg::SoundNew { file: "Sound\\Ambient\\Rain.wav".into() },
        ClientMsg::SoundSet { index: 1, field: "volume".into(), value: "".into() },
        ClientMsg::SoundDelete { index: 2 },
        ClientMsg::SoundPlay { file: "a.wav".into() },
        ClientMsg::ThingSet { file: 0, index: 2, field: "scale_x".into(), value: 1.5 },
    ] {
        let frame = msg.encode();
        let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
        assert_eq!(ClientMsg::decode(kind, &body).unwrap(), Some(msg));
    }
    // What the page sends for that brush (`encode.brush`): 52 bytes.
    let page = ClientMsg::Brush {
        start: true,
        origin: [1.0, 2.0, 3.0],
        dir: [0.0, 0.0, -1.0],
        brush: crate::edit::Brush { tool: crate::edit::Tool::Texture, size: 4.0, strength: 0.5, texture: parser_rs::bin::Rawcode::parse("Lgrs"), cliff: -1, flags: 0 },
    };
    assert_eq!(page.encode().len(), 52);
    let mut rain = w3s_rs::Sound::new("gg_snd_Rain", "Sound\\Ambient\\Rain.wav");
    rain.volume = 100;
    round_trip(ServerMsg::Sounds(vec![rain, w3s_rs::Sound::new("gg_snd_B", "b.mp3")]));
    round_trip(ServerMsg::SoundData { file: "a.wav".into(), bytes: vec![1, 2, 3], error: String::new() });
    round_trip(ServerMsg::ObjectList(ObjectListMsg { kind: 0, objects: vec![("hfoo".into(), "Footman".into(), "human".into(), 1)] }));
    round_trip(ServerMsg::ObjectFields(ObjectFieldsMsg {
        kind: 4,
        id: "A000".into(),
        base: "AHbz".into(),
        name: "Blizzard".into(),
        levels: 3,
        fields: vec![FieldRow { field: "Hbz1".into(), name: "Number of Waves".into(), category: "data".into(), ty: "int".into(), level: 2, column: 1, value: "8".into(), default: "6".into(), changed: true }],
    }));
    round_trip(ServerMsg::Thing(ThingMsg { file: 1, index: 7, fields: vec![("x".into(), 64.0), ("hp".into(), -1.0)] }));
    round_trip(ServerMsg::State(StateMsg { dirty: true, undo: true, redo: false, message: "Saved".into() }));
    round_trip(ServerMsg::Palette(PaletteMsg {
        grounds: vec![PaletteEntry { id: "Ldrt".into(), name: "Dirt".into(), icon: crate::textures::Icon { width: 1, height: 1, rgba: vec![1, 2, 3, 4] } }],
        cliffs: vec![PaletteEntry { id: "CLdi".into(), name: "Dirt Cliff".into(), icon: Default::default() }],
    }));
}
