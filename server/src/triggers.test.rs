use super::*;

const DATA: &str = "[TriggerEvents]\nMapInitializationEvent=0,nothing\n[TriggerActions]\nDoNothing=0,nothing\n";
const TEXT: &str = "[wtg]\nversion=7\ngame=2\n\n[trigger]\nname=\"Init\"\ndescription=\"\"\ncategory=0\nevent=MapInitializationEvent()\naction=DoNothing()\n";

#[test]
fn text_and_back_with_the_editors_data() {
    let data = TriggerData::parse(DATA);
    let bytes = convert(TO_BYTES, "war3map.wtg", TEXT.as_bytes(), &data).unwrap();
    assert_eq!(convert(TO_TEXT, "war3map.wtg", &bytes, &data).unwrap(), TEXT.as_bytes());
    let err = convert(TO_TEXT, "m/war3map.wtg", &bytes, &TriggerData::default()).unwrap_err();
    assert!(err.starts_with("Can't read war3map.wtg: no UI\\TriggerData.txt"), "{err}");
    let err = convert(TO_BYTES, "war3map.wtg", TEXT.replace("DoNothing()", "DoNothing(x)").as_bytes(), &data).unwrap_err();
    assert_eq!(err, "Can't save war3map.wtg: line 10: `action`: `DoNothing` takes 0 parameters (UI\\TriggerData.txt), not 1");
}

#[test]
fn diagnostics_only_for_trigger_text() {
    let text = TEXT.replace("action=", "acton=");
    let doc = bni_rs::parse(&text);
    let errors = diagnostics(Some(Path::new("war3map.wtg")), &doc, &text);
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [10]);
    assert!(diagnostics(Some(Path::new("war3map.ini")), &doc, &text).is_empty());
}

#[test]
fn values_are_raw_and_highlighted_as_calls() {
    let text = "[wtg]\n[trigger]\nname=\"a, \\\"b\\\"\"\naction=CustomScriptCode(\"call X(\\\"y\\\")\")\n";
    let doc = crate::document::Document {
        language: crate::document::Language::Bni,
        version: 0,
        text: text.into(),
        path: Some(Path::new("/m/war3map.wtg").into()),
    };
    let d = crate::analysis::diagnostics(&doc, None);
    assert!(d.is_empty(), "{d:?}");
    let tokens = crate::analysis::semantic_tokens(&doc, None);
    // [wtg] [trigger] name "…" action CustomScriptCode "…": seven tokens.
    assert_eq!(tokens.len(), 7 * 5, "{tokens:?}");
}

// ---------------------------------------------------- a trigger of its own

const TRG: &str = concat!(
    "[trigger]\n",                                        // 1
    "name=\"Spawn\"\n",                                   // 2
    "description=\"\"\n",                                 // 3
    "event=MapInitializationEvent()\n",                   // 4
    "action=IfThenElseMultiple()\n",                      // 5
    ">condition:0=OperatorEqual()\n",                     // 6
    ">action:1=DoNothing()\n",                            // 7
    ">action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n", // 8
    ">>action:0=DoNothing()\n",                           // 9
    ">action:2=DoNothing()\n",                            // 10
    "action=DoNothing()\n",                               // 11
);

/// The text after `edit`'s answer is applied.
fn applied(text: &str, op: u32, line: usize, arg: &str) -> Result<String, String> {
    let answer = edit(text, op, line, arg);
    if let Some(e) = answer.strip_prefix("{\"error\":") {
        return Err(e.trim_end_matches('}').to_string());
    }
    let mut doc = crate::document::Document { language: crate::document::Language::Bni, version: 0, text: text.into(), path: None };
    // [[l, c, el, ec, "text"]]: read by hand, the text a JSON string.
    let (edits, _) = answer.rsplit_once(",\"line\":").expect(&answer);
    let inner = edits.strip_prefix("{\"edits\":[").and_then(|a| a.strip_suffix(']')).expect(&answer);
    if inner.is_empty() {
        return Ok(text.into());
    }
    let inner = inner.strip_prefix('[').and_then(|a| a.strip_suffix(']')).unwrap();
    let mut parts = inner.splitn(5, ',');
    let mut n = || parts.next().unwrap().parse::<u32>().unwrap();
    let (l, c, el, ec) = (n(), n(), n(), n());
    let json = parts.next().unwrap();
    let new: String = json[1..json.len() - 1].replace("\\u000a", "\n").replace("\\u000d", "\r").replace("\\n", "\n").replace("\\r", "\r").replace("\\\"", "\"").replace("\\\\", "\\");
    use crate::document::{Position, Range};
    doc.apply(Range::new(Position::new(l, c), Position::new(el, ec)), &new);
    Ok(doc.text)
}

#[test]
fn trg_diagnostics_and_tokens() {
    let text = "[trigger]\ncategory=1\naction=DoNothing()\n";
    let doc = crate::document::Document {
        language: crate::document::Language::Bni,
        version: 0,
        text: text.into(),
        path: Some(Path::new("/m/Spawn.trg").into()),
    };
    let d = crate::analysis::diagnostics(&doc, None);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].message.contains("its folder is its category"), "{d:?}");
    // [trigger] category action DoNothing: four tokens (a field that
    // isn't there has its value left alone).
    assert_eq!(crate::analysis::semantic_tokens(&doc, None).len(), 4 * 5);
}

#[test]
fn trg_view() {
    let names = crate::names::Names::default();
    let v = view(TRG, &names);
    assert!(v.starts_with("{\"name\":\"Spawn\",\"description\":\"\",\"enabled\":1,"), "{v}");
    assert!(v.contains("\"data\":false"), "{v}");
    assert!(v.contains("\"labels\":[\"Events\",\"Conditions\",\"Actions\"]"), "{v}");
    assert!(v.contains("{\"line\":5,\"kind\":2,\"enabled\":true,\"name\":\"IfThenElseMultiple\",\"known\":false,"), "{v}");
    assert!(v.contains("{\"label\":\"Then - Actions\",\"group\":1,\"kind\":2,\"nodes\":[{\"line\":7,"), "{v}");
    assert!(v.contains("{\"label\":\"Loop - Actions\",\"group\":0,\"kind\":2,\"nodes\":[{\"line\":9,"), "{v}");
    // A call the files don't have isn't in parentheses: it has its own.
    assert!(v.contains("{\"r\":\"c\",\"s\":[{\"r\":\"u\",\"s\":[\"GetUnitsInRectAll(\"]}"), "{v}");
}

#[test]
fn trg_toggle_delete_copy() {
    let t = applied(TRG, op::TOGGLE, 9, "").unwrap();
    assert!(t.contains("\n>>!action:0=DoNothing()\n"), "{t}");
    assert_eq!(applied(&t, op::TOGGLE, 9, "").unwrap(), TRG);
    // What's inside goes too.
    let t = applied(TRG, op::DELETE, 8, "").unwrap();
    assert_eq!(t, TRG.replace(">action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n", ""));
    assert!(applied(TRG, op::DELETE, 2, "").unwrap_err().contains("line 2 isn't an event"));
    assert_eq!(
        edit(TRG, op::COPY, 8, ""),
        "{\"text\":\"action=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\\u000a>action:0=DoNothing()\\u000a\"}"
    );
}

#[test]
fn trg_move() {
    // Down within its block: before what's at index 2 (past the end).
    let t = applied(TRG, op::MOVE, 7, "5 1 2").unwrap();
    assert!(t.contains(">action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n>action:1=DoNothing()\n>action:2"), "{t}");
    // Into another block, deeper, with what's inside it.
    let t = applied(TRG, op::MOVE, 8, "5 2 0").unwrap();
    assert!(t.ends_with(">action:1=DoNothing()\n>action:2=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n>action:2=DoNothing()\naction=DoNothing()\n"), "{t}");
    // Out to the top.
    let t = applied(TRG, op::MOVE, 9, "0 2 0").unwrap();
    assert!(t.contains("event=MapInitializationEvent()\naction=DoNothing()\naction=IfThenElseMultiple()\n"), "{t}");
    // Into an empty block: a loop's, after the blocks before it.
    let t = applied(TRG, op::DELETE, 9, "").unwrap();
    let t = applied(&t, op::MOVE, 11 - 1, "8 0 0").unwrap();
    assert!(t.ends_with(">action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n>action:2=DoNothing()\n"), "{t}");
    // An empty kind at the top: conditions go between events and actions.
    let t = applied(TRG, op::MOVE, 6, "0 1 0").unwrap();
    assert!(t.contains("event=MapInitializationEvent()\ncondition=OperatorEqual()\naction=IfThenElseMultiple()\n>action:1"), "{t}");
    // What can't be.
    assert!(applied(TRG, op::MOVE, 7, "5 0 0").unwrap_err().contains("an action can't go where a condition goes"));
    assert!(applied(TRG, op::MOVE, 5, "8 0 0").unwrap_err().contains("into itself"));
    assert!(applied(TRG, op::MOVE, 7, "7 0 0").unwrap_err().contains("into itself"));
    assert!(applied(TRG, op::MOVE, 7, "11 0 0").unwrap_err().contains("`DoNothing` has no block 0"));
    assert!(applied(TRG, op::MOVE, 4, "0 2 0").unwrap_err().contains("an event can't go"));
    // In place: nothing changes.
    assert_eq!(edit(TRG, op::MOVE, 7, "5 1 0"), "{\"edits\":[],\"line\":7}");
    // Where it went.
    assert!(edit(TRG, op::MOVE, 9, "0 2 0").ends_with(",\"line\":5}"));
}

#[test]
fn trg_paste_and_fields() {
    let copied = "action=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>action:0=DoNothing()\n";
    let t = applied(TRG, op::PASTE, 0, &format!("5 2 1\n{copied}")).unwrap();
    assert!(t.contains(">action:2=DoNothing()\n>action:2=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\naction=DoNothing()\n"), "{t}");
    assert!(applied(TRG, op::PASTE, 0, "0 0 0\naction=DoNothing()\n").unwrap_err().contains("an action can't go where an event goes"));
    assert!(applied(TRG, op::PASTE, 0, "0 2 0\nhello\n").unwrap_err().contains("not"));

    let t = applied(TRG, op::FIELD, 0, "description\nSays \"hi\"").unwrap();
    assert!(t.contains("description=\"Says \\\"hi\\\"\"\n"), "{t}");
    let t = applied(&t, op::FIELD, 0, "initially_off\n1").unwrap();
    assert!(t.contains("description=\"Says \\\"hi\\\"\"\ninitially_off=1\nevent="), "{t}");
    let t = applied(&t, op::FIELD, 0, "initially_off\n0").unwrap();
    assert!(!t.contains("initially_off"), "{t}");
    assert!(applied(TRG, op::FIELD, 0, "category\n1").unwrap_err().contains("no field"));
}

#[test]
fn trg_line_breaks_kept() {
    // CRLF text gets CRLF lines; a last line without a break gets one
    // before what goes after it.
    let crlf = "[trigger]\r\nname=\"a\"\r\naction=DoNothing()\r\n";
    let t = applied(crlf, op::PASTE, 0, "0 2 1\naction=Two()\n").unwrap();
    assert_eq!(t, "[trigger]\r\nname=\"a\"\r\naction=DoNothing()\r\naction=Two()\r\n");
    let bare = "[trigger]\naction=DoNothing()";
    assert_eq!(applied(bare, op::PASTE, 0, "0 2 1\naction=Two()\n").unwrap(), "[trigger]\naction=DoNothing()\naction=Two()\n");
    assert_eq!(applied(bare, op::TOGGLE, 2, "").unwrap(), "[trigger]\n!action=DoNothing()");
}
