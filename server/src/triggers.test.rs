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
    let answer = edit(text, op, line, arg, &wtg_rs::ui::Ui::default());
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
    assert!(v.starts_with("{\"trigger\":true,\"variables\":[],\"name\":\"Spawn\",\"description\":\"\",\"enabled\":1,"), "{v}");
    assert!(v.contains("\"data\":false"), "{v}");
    assert!(v.contains("\"labels\":[\"Events\",\"Conditions\",\"Actions\",\"Variables\"]"), "{v}");
    assert!(v.contains("{\"line\":5,\"kind\":2,\"enabled\":true,\"name\":\"IfThenElseMultiple\",\"known\":false,"), "{v}");
    assert!(v.contains("{\"label\":\"Then - Actions\",\"group\":1,\"kind\":2,\"nodes\":[{\"line\":7,"), "{v}");
    assert!(v.contains("{\"label\":\"Loop - Actions\",\"group\":0,\"kind\":2,\"nodes\":[{\"line\":9,"), "{v}");
    // A call the files don't have isn't in parentheses: it has its own.
    assert!(v.contains("{\"r\":\"c\",\"p\":\"0\",\"s\":[{\"r\":\"u\",\"p\":\"\",\"s\":[\"GetUnitsInRectAll(\"]}"), "{v}");
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
        edit_(TRG, op::COPY, 8, ""),
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
    assert_eq!(edit_(TRG, op::MOVE, 7, "5 1 0"), "{\"edits\":[],\"line\":7}");
    // Where it went.
    assert!(edit_(TRG, op::MOVE, 9, "0 2 0").ends_with(",\"line\":5}"));
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

#[test]
fn extract_as_files() {
    let answer = extract(TEXT);
    assert_eq!(answer, "{\"folders\":[],\"files\":[[\"000_Init.trg\",\"[trigger]\\u000aname=\\\"Init\\\"\\u000adescription=\\\"\\\"\\u000aevent=MapInitializationEvent()\\u000aaction=DoNothing()\\u000a\"]]}");
    assert!(extract("[wtg]\nversion=9\n").starts_with("{\"error\":\"Can't extract the triggers: line 2:"));
}

#[test]
fn trg_view_of_variables() {
    let v = view("[variable]\nname=\"hero\"\ntype=\"unit\"\narray=1\nsize=12\n", &crate::names::Names::default());
    assert!(v.starts_with("{\"trigger\":false,\"variables\":[{\"line\":1,\"name\":\"hero\",\"type\":\"unit\",\"type_name\":\"unit\",\"array\":true,\"size\":12,\"initial\":null}],"), "{v}");
}

fn edit_(text: &str, op: u32, line: usize, arg: &str) -> String {
    edit(text, op, line, arg, &wtg_rs::ui::Ui::default())
}

fn ui() -> wtg_rs::ui::Ui {
    let data = concat!(
        "[TriggerCategories]\nTC_UNIT=WESTRING_TRIGCAT_UNIT,x\n",
        "[TriggerTypes]\nplayer=0,1,1,WESTRING_TRIGTYPE_player\nunitcode=0,1,1,WESTRING_TRIGTYPE_unitcode,integer\n",
        "[TriggerParams]\nPlayer00=0,player,Player(0),WESTRING_PLAYER_00\nPlayer01=0,player,Player(1),WESTRING_PLAYER_01\n",
        "[TriggerActions]\nCreateNUnitsAtLoc=0,integer,unitcode,player\n_CreateNUnitsAtLoc_Defaults=1,hfoo,Player00\n_CreateNUnitsAtLoc_Category=TC_UNIT\n",
        "SetVariable=0,AnyGlobal,Null\n",
        "[TriggerCalls]\nGetOwningPlayer=0,0,player,unit\nGetTriggerUnit=0,0,unit\n_GetOwningPlayer_Defaults=GetTriggerUnit\n",
    );
    let strings = "[TriggerActionStrings]\nCreateNUnitsAtLoc=\"Create Units\"\nCreateNUnitsAtLoc=\"Create \",~Number\n";
    wtg_rs::ui::Ui::parse(data, strings, &|k| k.strip_prefix("WESTRING_TRIGCAT_").map_or(k.to_string(), |r| r.to_string()))
}

#[test]
fn trg_set_param_and_new() {
    let ui = ui();
    let text = "[trigger]\naction=CreateNUnitsAtLoc(\"1\", \"hfoo\", Player00)\n";
    let run = |t: &str, op: u32, line: usize, arg: &str| -> String {
        let a = edit(t, op, line, arg, &ui);
        assert!(!a.contains("error"), "{a}");
        a
    };
    assert!(run(text, op::SET_PARAM, 2, "2\npreset\nPlayer01").contains("CreateNUnitsAtLoc(\\\"1\\\", \\\"hfoo\\\", Player01)"));
    assert!(run(text, op::SET_PARAM, 2, "2\ncall\nGetOwningPlayer").contains("Player01".replace("Player01", "GetOwningPlayer(GetTriggerUnit())").as_str()));
    assert!(run(text, op::SET_PARAM, 2, "0\nvalue\n5 \"x\"").contains("\\\"5 \\\\\\\"x\\\\\\\"\\\""));
    assert!(run(text, op::SET_PARAM, 2, "2\narray\nplayers").contains("$players[\\\"0\\\"]"));
    let t = "[trigger]\naction=CreateNUnitsAtLoc(\"1\", \"hfoo\", GetOwningPlayer(GetTriggerUnit()))\n";
    assert!(run(t, op::SET_PARAM, 2, "2.0\nvariable\nu").contains("GetOwningPlayer($u)"));
    assert!(edit(t, op::SET_PARAM, 2, "2.5\nvariable\nu", &ui).contains("no parameter 2.5"));
    // A new line, with its defaults, at its place.
    let a = run(text, op::NEW, 0, "0 2 0\n2 CreateNUnitsAtLoc");
    assert_eq!(a, "{\"edits\":[[2,0,2,0,\"action=CreateNUnitsAtLoc(\\\"1\\\", \\\"hfoo\\\", Player00)\\u000a\"]],\"line\":2}");
    assert!(edit(text, op::NEW, 0, "0 0 0\n2 CreateNUnitsAtLoc", &ui).contains("an action can't go where an event goes"));
}

#[test]
fn trg_choices_and_catalog() {
    let ui = ui();
    let var = |name: &str, kind: &str, array: u32| wtg_rs::Variable {
        name: name.into(), kind: kind.into(), unknown: 1, is_array: array, array_size: 0, has_initial: 0, initial: String::new(),
    };
    let vars = [var("p", "player", 0), var("ps", "player", 1), var("n", "integer", 0)];
    let text = "[trigger]\naction=CreateNUnitsAtLoc(\"1\", \"hfoo\", Player00)\naction=SetVariable($n, \"3\")\n";
    let c = choices(text, 2, "2", &ui, &vars);
    assert_eq!(c, concat!(
        "{\"type\":\"player\",\"type_name\":\"WESTRING_TRIGTYPE_player\",\"current\":{\"kind\":\"preset\",\"value\":\"Player00\"},",
        "\"presets\":[[\"Player00\",\"WESTRING_PLAYER_00\"],[\"Player01\",\"WESTRING_PLAYER_01\"]],",
        "\"variables\":[[\"p\",\"WESTRING_TRIGTYPE_player\",false],[\"ps\",\"WESTRING_TRIGTYPE_player\",true]],",
        "\"calls\":[[\"GetOwningPlayer\",\"GetOwningPlayer\",\"\"]],\"value\":false}"
    ));
    // A type of the editor's own over integer takes a value.
    assert!(choices(text, 2, "1", &ui, &vars).ends_with("\"value\":true}"));
    // SetVariable: any variable, then the variable's type.
    assert!(choices(text, 3, "0", &ui, &vars).contains("\"type_name\":\"Any variable\""));
    assert!(choices(text, 3, "1", &ui, &vars).starts_with("{\"type\":\"integer\""));
    assert!(choices(text, 3, "7", &ui, &vars).starts_with("{\"error\""));
    assert_eq!(catalog(2, &ui), "[[\"SetVariable\",\"SetVariable\",\"\",\"\",false],[\"CreateNUnitsAtLoc\",\"Create Units\",\"UNIT\",\"\",true]]");
    assert_eq!(catalog(5, &ui), "[]");
}

#[test]
fn trg_without_a_trigger_and_new_variables() {
    let ui = ui();
    let ok = |t: &str, op: u32, line: usize, arg: &str| applied(t, op, line, arg).unwrap();
    // A new file: a field, or a new line, brings its [trigger].
    assert_eq!(ok("", op::FIELD, 0, "description\nx"), "[trigger]\ndescription=\"x\"\n");
    let vars = "[variable]\nname=\"a\"\ntype=\"unit\"\nsize=1\n";
    assert_eq!(ok(vars, op::PASTE, 0, "0 2 0\naction=DoNothing()\n"), format!("{vars}\n[trigger]\naction=DoNothing()\n"));
    // New variables: after the last, before the trigger, or at the end.
    let t = ok(vars, op::NEW_VARIABLE, 0, "b\nplayer\n1\n12\n");
    assert_eq!(t, format!("{vars}\n[variable]\nname=\"b\"\ntype=\"player\"\narray=1\nsize=12\n"));
    let t = ok("[trigger]\naction=DoNothing()\n", op::NEW_VARIABLE, 0, "n\ninteger\n0\n\n5");
    assert_eq!(t, "[variable]\nname=\"n\"\ntype=\"integer\"\nsize=1\ninitial=\"5\"\n\n[trigger]\naction=DoNothing()\n");
    assert_eq!(ok("", op::NEW_VARIABLE, 0, "n\ninteger\n0\n\n"), "[variable]\nname=\"n\"\ntype=\"integer\"\nsize=1\n");
    assert!(edit(vars, op::NEW_VARIABLE, 0, "A\nunit\n0\n\n", &ui).contains("has a variable A already"));
    assert!(edit(vars, op::NEW_VARIABLE, 0, "1x\nunit\n0\n\n", &ui).contains("isn't a variable's name"));
    // Deleting a variable: its section.
    let two = format!("{vars}\n[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n");
    assert_eq!(ok(&two, op::DELETE, 1, ""), "[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n");
    assert!(catalog(3, &ui).starts_with("[[\"player\",\"WESTRING_TRIGTYPE_player\"]"));
}
