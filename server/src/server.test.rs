//! The loop end to end: encoded editor messages in, decoded answers out.

use super::*;
use crate::document::{Position, Range};
use crate::project::tests::TempDir;
use crate::protocol::ServerMsg;
use parser_rs::Level;
use std::sync::mpsc;

/// What the server sent back, decoded.
#[derive(Debug)]
struct Answer {
    id: String,
    version: i32,
    diagnostics: Vec<crate::analysis::Diagnostic>,
    tokens: Vec<u32>,
}

/// Feeds `events` to a fresh server, as one burst, and decodes its output.
fn session(events: Vec<Event>) -> Vec<Answer> {
    let (tx, rx) = mpsc::channel();
    for e in events {
        tx.send(e).unwrap();
    }
    tx.send(Event::Exit).unwrap();
    let mut out = Vec::new();
    Server::new(&mut out, None).run(rx).unwrap();

    let mut input = &out[..];
    let mut answers = Vec::new();
    while let Some((kind, body)) = read_frame(&mut input).unwrap() {
        match ServerMsg::decode(kind, &body).unwrap() {
            Some(ServerMsg::File { id, version, diagnostics, tokens }) => {
                answers.push(Answer { id, version, diagnostics, tokens })
            }
            Some(ServerMsg::Legend { .. }) => assert!(answers.is_empty(), "the legend comes first"),
            Some(ServerMsg::Answer { .. } | ServerMsg::Fs { .. }) => {}
            None => panic!("unknown frame {kind}"),
        }
    }
    answers
}

fn open(id: &str, lang: &str, text: &str) -> Event {
    Event::Client(ClientMsg::Open {
        id: id.into(),
        path: String::new(),
        lang: lang.into(),
        version: 1,
        text: text.into(),
    })
}

fn insert(id: &str, version: i32, line: u32, text: &str) -> Event {
    let at = Position::new(line, 0);
    Event::Client(ClientMsg::Edit {
        id: id.into(),
        version,
        changes: vec![(Range::new(at, at), text.into())],
    })
}

const FUNC: &str = "function F takes nothing returns nothing\nendfunction";

#[test]
fn the_legend_is_sent_first() {
    let (tx, rx) = mpsc::channel();
    tx.send(Event::Exit).unwrap();
    let mut out = Vec::new();
    Server::new(&mut out, None).run(rx).unwrap();
    let (kind, body) = read_frame(&mut &out[..]).unwrap().unwrap();
    let Some(ServerMsg::Legend { types, modifiers }) = ServerMsg::decode(kind, &body).unwrap() else {
        panic!("expected the legend");
    };
    assert_eq!((types.len(), modifiers.len()), (23, 10));
    assert_eq!(types[0], "namespace");
}

#[test]
fn an_open_document_gets_diagnostics_and_tokens_in_one_message() {
    let answers = session(vec![open("a.j", "jass", FUNC)]);
    assert_eq!(answers.len(), 1);
    let a = &answers[0];
    assert_eq!((a.id.as_str(), a.version), ("a.j", 1));
    assert!(a.diagnostics.is_empty());
    assert!(!a.tokens.is_empty() && a.tokens.len().is_multiple_of(5));
}

#[test]
fn a_burst_of_edits_is_analyzed_once_at_the_last_version() {
    let answers = session(vec![
        open("a.j", "jass", FUNC),
        insert("a.j", 2, 1, "    local integer x = 1\n"),
        insert("a.j", 3, 1, "    local integer y = 2\n"),
    ]);
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].version, 3);
    let lines: Vec<_> = answers[0].diagnostics.iter().map(|d| d.range.start.line).collect();
    assert_eq!(lines, vec![1, 2]); // both unused locals
}

#[test]
fn a_language_change_re_analyzes_the_same_text() {
    let answers = session(vec![
        open("a.txt", "bni", "x"),
        Event::Client(ClientMsg::Lang { id: "a.txt".into(), lang: "jass".into() }),
    ]);
    assert_eq!(answers.len(), 1);
    // `x` alone is not JASS.
    assert_eq!(answers[0].diagnostics[0].level, Level::Error);
}

#[test]
fn closing_or_leaving_known_languages_clears_the_document() {
    let answers = session(vec![
        open("a.j", "jass", "nonsense here"),
        Event::Client(ClientMsg::Close { id: "a.j".into() }),
        open("b.j", "jass", "nonsense here"),
        Event::Client(ClientMsg::Lang { id: "b.j".into(), lang: "plaintext".into() }),
    ]);
    assert_eq!(answers.len(), 2);
    for a in &answers {
        assert!(a.diagnostics.is_empty() && a.tokens.is_empty());
    }
}

#[test]
fn unknown_languages_and_documents_are_ignored() {
    let answers = session(vec![
        open("a.cob", "cobol", "garbage"),
        insert("nope.j", 2, 0, "x"),
    ]);
    assert!(answers.is_empty());
}

#[test]
fn init_reports_every_project_file_without_opening_it() {
    let tmp = TempDir::new("server-init");
    std::fs::write(tmp.0.join(project::MANIFEST), "[project]\nsources=.\n[jass]\ndialect=vjass\n").unwrap();
    std::fs::write(tmp.0.join("bad.j"), "globals\n    integer library = 1\nendglobals").unwrap();
    let bad = tmp.0.join("bad.j").to_str().unwrap().to_string();

    let answers = session(vec![Event::Client(ClientMsg::Init { roots: vec![tmp.0.to_str().unwrap().into()] })]);
    let report = answers.iter().find(|a| a.id == bad).expect("bad.j is reported");
    // vJASS from the project: `library` is a keyword, so this is an error.
    assert_eq!(report.version, 0);
    assert_eq!(report.diagnostics[0].level, Level::Error);
    // Not open: no highlighting.
    assert!(report.tokens.is_empty());
}

#[test]
fn read_events_decodes_frames_until_input_ends() {
    let mut input = ClientMsg::Close { id: "a".into() }.encode();
    input.extend_from_slice(&[1, 0, 0, 0, 200, 0]); // unknown kind: skipped
    input.extend(ClientMsg::Close { id: "b".into() }.encode());
    let (tx, rx) = mpsc::channel();
    read_events(&input[..], &tx);
    let got: Vec<_> = rx.try_iter().collect();
    assert!(matches!(&got[..], [
        Event::Client(ClientMsg::Close { id: a }),
        Event::Client(ClientMsg::Close { id: b }),
        Event::Exit,
    ] if a == "a" && b == "b"));
}

#[test]
fn queries_are_answered_after_the_edits_before_them() {
    let (tx, rx) = mpsc::channel();
    let send = |e| tx.send(e).unwrap();
    send(open("a.j", "jass", "function F takes nothing returns nothing\nendfunction"));
    // Rename F to G, then ask where `G` (line 2, after the call) is declared.
    send(Event::Client(ClientMsg::Edit {
        id: "a.j".into(),
        version: 2,
        changes: vec![
            (Range::new(Position::new(0, 9), Position::new(0, 10)), "G".into()),
            (Range::new(Position::new(1, 0), Position::new(1, 0)), "function H takes nothing returns nothing\n    call G()\n".into()),
        ],
    }));
    send(Event::Client(ClientMsg::Query {
        request: 7,
        kind: Some(crate::index::QueryKind::Definition),
        id: "a.j".into(),
        pos: Position::new(2, 10),
        text: String::new(),
    }));
    send(Event::Exit);
    let mut out = Vec::new();
    Server::new(&mut out, None).run(rx).unwrap();

    let mut input = &out[..];
    let mut answer = None;
    while let Some((kind, body)) = read_frame(&mut input).unwrap() {
        if let Some(ServerMsg::Answer { request, locations, .. }) = ServerMsg::decode(kind, &body).unwrap() {
            answer = Some((request, locations));
        }
    }
    let (request, locations) = answer.expect("the query is answered");
    assert_eq!(request, 7);
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].name, "G");
    assert_eq!(locations[0].range, Range::new(Position::new(0, 9), Position::new(0, 10)));
}

#[test]
fn fs_messages_change_maps_and_the_model_follows() {
    use crate::protocol::{fs_flags, FsOp};
    let tmp = TempDir::new("server-fs");
    std::fs::write(tmp.0.join(project::MANIFEST), "[project]\nsources=.\n").unwrap();
    let map = tmp.0.join("Map.w3x");
    crate::map::tests::make_map(&map, &[("war3map.j", "function main takes nothing returns nothing\nendfunction\n")]);
    let script = map.join("war3map.j").to_string_lossy().into_owned();

    let fs = |request, op, path: &str, flags, data: &[u8]| {
        Event::Client(ClientMsg::Fs { request, op: Some(op), path: path.into(), to: String::new(), flags, data: data.to_vec() })
    };
    let answers = session(vec![
        Event::Client(ClientMsg::Init { roots: vec![tmp.0.to_string_lossy().into_owned()] }),
        fs(1, FsOp::Write, &script, fs_flags::OVERWRITE, b"function main takes nothing returns nothing\n"),
    ]);
    // Saved with an error in it: the model read the map again.
    let last = answers.iter().rev().find(|a| a.id == script).expect("the script is reported");
    assert_eq!(last.diagnostics.len(), 1, "{:?}", last.diagnostics);
    assert_eq!(
        crate::map::load(&map.join("war3map.j")).unwrap().text,
        "function main takes nothing returns nothing\n"
    );
}
