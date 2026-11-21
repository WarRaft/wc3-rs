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
        ClientMsg::Fs {
            request: 3,
            op: Some(FsOp::Rename),
            path: "/m/Map.w3x/a.j".into(),
            to: "/m/Map.w3x/b.j".into(),
            flags: fs_flags::OVERWRITE,
            data: vec![0, 1, 255],
        },
        ClientMsg::Texture { request: 4, name: "x/war3map.shd".into(), data: b"BLP1".to_vec() },
        ClientMsg::Objects { request: 5, op: 1, name: "/m/war3map.w3u".into(), data: b"[w3u]".to_vec() },
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
            Diagnostic { range: range(0, 1, 0, 2), level: Level::Error, message: "bad".into(), rule: "", fix: None },
            Diagnostic {
                range: range(5, 0, 5, 9),
                level: Level::Warning,
                message: "meh".into(),
                rule: "empty-branch",
                fix: Some(crate::analysis::Fix { title: "Remove".into(), edits: vec![(range(5, 0, 7, 0), String::new())] }),
            },
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

#[test]
fn fs_answers_carry_errors_stats_entries_and_bytes() {
    use crate::mapfs::{FsError, Kind, Stat};
    let msg = ServerMsg::Fs {
        request: 5,
        error: Some(FsError::NoPermissions("no".into())),
        stat: Some(Stat { kind: Kind::Dir, size: 1 << 40, mtime: 7, readonly: true, note: "n".into() }),
        entries: vec![("a.j".into(), Kind::File)],
        data: vec![9, 8],
    };
    let frame = msg.encode();
    let body = &frame[HEADER..];
    assert_eq!(frame[4], 131);
    // request, error 5 + "no", stat: folder, size, time, read-only, "n".
    assert_eq!(&body[..4], &5u32.to_le_bytes());
    assert_eq!(body[4], 5);
    assert_eq!(&body[5..11], &[2, 0, 0, 0, b'n', b'o']);
    assert_eq!(body[11], 2);
    assert_eq!(&body[12..20], &(1u64 << 40).to_le_bytes());
    assert!(body.ends_with(&[2, 0, 0, 0, 9, 8]));
}

#[test]
fn texture_answers_round_trip() {
    use crate::texture::{Mip, Texture};
    let msg = ServerMsg::Texture {
        request: 8,
        texture: Texture {
            error: String::new(),
            version: "BLP1".into(),
            encoding: "JPEG".into(),
            alpha_bits: 8,
            width: 2,
            height: 1,
            mips: vec![
                Mip { width: 2, height: 1, error: String::new(), rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] },
                Mip { width: 1, height: 1, error: "not in the file".into(), rgba: Vec::new() },
            ],
            cells: vec![0x40, 0xCA],
            flags: vec![(0x02, "no walking".into())],
        },
    };
    let frame = msg.encode();
    assert_eq!(frame[4], 132);
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    assert_eq!(ServerMsg::decode(kind, &body).unwrap(), Some(msg));
}

#[test]
fn objects_answers_round_trip() {
    let msg = ServerMsg::Objects { request: 9, error: "line 2: bad".into(), data: vec![1, 2] };
    let frame = msg.encode();
    assert_eq!(frame[4], 133);
    let (kind, body) = read_frame(&mut &frame[..]).unwrap().unwrap();
    assert_eq!(ServerMsg::decode(kind, &body).unwrap(), Some(msg));
}
