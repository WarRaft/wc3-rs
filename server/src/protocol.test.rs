use super::*;

fn decode_client(frame: &[u8]) -> ClientMsg {
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    ClientMsg::decode(kind, &body).unwrap().unwrap()
}

fn range(a: u32, b: u32, c: u32, d: u32) -> Range {
    Range::new(Position::new(a, b), Position::new(c, d))
}

#[test]
fn client_messages_round_trip() {
    let messages = [
        ClientMsg::Init { roots: vec!["/a".into(), "/b c".into()] },
        ClientMsg::Open {
            id: "file:///a/x.j".into(),
            path: "/a/x.j".into(),
            lang: "jass".into(),
            version: 7,
            text: "юникод 😀\n".into(),
        },
        ClientMsg::Edit {
            id: "u".into(),
            version: -1,
            changes: vec![(range(1, 2, 3, 4), "x".into()), (range(0, 0, 0, 0), String::new())],
        },
        ClientMsg::Lang { id: "u".into(), lang: "bni".into() },
        ClientMsg::Close { id: "u".into() },
        ClientMsg::Query {
            request: 9,
            kind: Some(QueryKind::References),
            id: "u".into(),
            pos: Position::new(3, 4),
            text: "q".into(),
        },
    ];
    for msg in messages {
        assert_eq!(decode_client(&msg.encode()), msg);
    }
}

#[test]
fn file_messages_round_trip() {
    let msg = ServerMsg::File {
        id: "u".into(),
        version: 3,
        diagnostics: vec![
            Diagnostic { range: range(0, 1, 0, 2), level: Level::Error, message: "bad".into(), rule: "" },
            Diagnostic { range: range(5, 0, 5, 9), level: Level::Warning, message: "meh".into(), rule: "unused-local" },
        ],
        tokens: vec![0, 1, 2, 3, 4],
    };
    let frame = msg.encode();
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    assert_eq!(ServerMsg::decode(kind, &body).unwrap(), Some(msg));
}

#[test]
fn frames_have_a_five_byte_header() {
    let frame = ClientMsg::Close { id: "ab".into() }.encode();
    // body: u32 length of "ab" + its two bytes
    assert_eq!(frame, [6, 0, 0, 0, 5, 2, 0, 0, 0, b'a', b'b']);
}

#[test]
fn input_ends_cleanly_only_between_frames() {
    assert_eq!(read_frame(&mut &[][..]).unwrap(), None);
    assert!(read_frame(&mut &[1, 0][..]).is_err());
    assert!(read_frame(&mut &[3, 0, 0, 0, 5, 1][..]).is_err());
}

#[test]
fn absurd_lengths_are_rejected() {
    let header = [0xff, 0xff, 0xff, 0xff, 1];
    assert!(read_frame(&mut &header[..]).is_err());
}

#[test]
fn unknown_kinds_are_skipped_and_short_bodies_are_errors() {
    assert_eq!(ClientMsg::decode(99, &[1, 2, 3]), Ok(None));
    assert!(ClientMsg::decode(5, &[9, 0, 0, 0, b'a']).is_err());
    assert!(ClientMsg::decode(5, &[2, 0, 0, 0, 0xff, 0xfe]).is_err());
}

#[test]
fn answers_round_trip() {
    let msg = ServerMsg::Answer {
        request: 42,
        text: "**hi**".into(),
        locations: vec![
            Location { id: "/a.j".into(), range: range(1, 2, 1, 5), name: "Foo".into(), kind: SymbolKind::Native, container: String::new() },
            Location { id: "/b.as".into(), range: range(0, 0, 0, 3), name: "Red".into(), kind: SymbolKind::EnumMember, container: "Game::Ui".into() },
        ],
    };
    let frame = msg.encode();
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    assert_eq!(ServerMsg::decode(kind, &body).unwrap(), Some(msg));
}

#[test]
fn unknown_query_kinds_decode_to_none() {
    let mut body = Vec::new();
    body.extend_from_slice(&1u32.to_le_bytes());
    body.push(200);
    body.extend_from_slice(&0u32.to_le_bytes()); // id ""
    body.extend_from_slice(&[0; 8]); // position
    body.extend_from_slice(&0u32.to_le_bytes()); // text ""
    let Some(ClientMsg::Query { kind, .. }) = ClientMsg::decode(6, &body).unwrap() else { panic!() };
    assert_eq!(kind, None);
}
