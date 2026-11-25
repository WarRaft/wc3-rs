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
    assert_eq!(msgs.len(), 2);
    assert!(msgs.iter().all(|m| matches!(m, ServerMsg::Map(m) if m.width == 3)));
}

#[test]
fn a_map_that_cant_open_is_an_error_message() {
    let dir = TestDir::new("serve-error");
    let msgs = answers(dir.path(), &ClientMsg::Hello.encode());
    assert!(matches!(&msgs[..], [ServerMsg::Error { message }] if message.contains("wc3.bni")));
}
