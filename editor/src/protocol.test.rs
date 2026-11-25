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
