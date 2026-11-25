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

/// A JSON string's text, its quotes off.
fn json_text(s: &str) -> String {
    let mut out = String::new();
    let mut c = s.chars();
    while let Some(ch) = c.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match c.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = c.by_ref().take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
            }
            Some(x) => out.push(x),
            None => {}
        }
    }
    out
}

/// The text after `edit`'s answer is applied.
fn applied(text: &str, op: u32, line: usize, arg: &str) -> Result<String, String> {
    answer_applied(text, &edit(text, op, line, arg, &wtg_rs::ui::Ui::default()))
}

/// The text after the edit of the answer `answer` is applied.
fn answer_applied(text: &str, answer: &str) -> Result<String, String> {
    let answer = answer.to_string();
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
    let new = json_text(&json[1..json.len() - 1]);
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
    let v = view(TRG, &names, &[], &Triggers::of("", TRG, &[]));
    assert!(v.starts_with("{\"trigger\":true,\"script\":\"gg_trg_Spawn\",\"variables\":[],\"name\":\"Spawn\",\"description\":\"\",\"enabled\":1,"), "{v}");
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
    let answer = extract(TEXT, None);
    assert_eq!(answer, "{\"folders\":[],\"files\":[[\"000_Init.trg\",\"[trigger]\\u000aname=\\\"Init\\\"\\u000adescription=\\\"\\\"\\u000aevent=MapInitializationEvent()\\u000aaction=DoNothing()\\u000a\"]]}");
    assert!(extract("[wtg]\nversion=9\n", None).starts_with("{\"error\":\"Can't extract the triggers: line 2:"));
}

#[test]
fn trg_view_of_variables() {
    let v = view("[variable]\nname=\"hero\"\ntype=\"unit\"\narray=1\nsize=12\n", &crate::names::Names::default(), &[], &Triggers::of("", "", &[]));
    assert!(v.starts_with("{\"trigger\":false,\"script\":\"gg_trg_u\",\"variables\":[{\"line\":1,\"scope\":\"global\",\"name\":\"hero\",\"type\":\"unit\",\"type_name\":\"unit\",\"array\":true,\"size\":12,\"initial\":null}],"), "{v}");
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
        "SetVariable=0,AnyGlobal,Null\nCustomScriptCode=0,scriptcode\n",
        "[TriggerConditions]\nOperatorComparePlayer=0,player,EqualNotEqualOperator,player\n",
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
    let var = |name: &str, kind: &str, array: u32| Declared {
        variable: wtg_rs::Variable {
            name: name.into(), kind: kind.into(), unknown: 1, is_array: array, array_size: 0, has_initial: 0, initial: String::new(),
        },
        scope: Scope::Global,
    };
    let vars = [var("p", "player", 0), var("ps", "player", 1), var("n", "integer", 0)];
    let text = "[trigger]\naction=CreateNUnitsAtLoc(\"1\", \"hfoo\", Player00)\naction=SetVariable($n, \"3\")\n";
    let c = choices(text, 2, "2", &ui, &vars);
    assert_eq!(c, concat!(
        "{\"type\":\"player\",\"type_name\":\"WESTRING_TRIGTYPE_player\",\"current\":{\"kind\":\"preset\",\"value\":\"Player00\"},",
        "\"presets\":[[\"Player00\",\"WESTRING_PLAYER_00\"],[\"Player01\",\"WESTRING_PLAYER_01\"]],",
        "\"variables\":[[\"p\",\"WESTRING_TRIGTYPE_player\",false,\"\"],[\"ps\",\"WESTRING_TRIGTYPE_player\",true,\"\"]],",
        "\"calls\":[[\"GetOwningPlayer\",\"GetOwningPlayer\",\"\"]],\"value\":false,\"objects\":[]}"
    ));
    // A type of the editor's own over integer takes a value.
    assert!(choices(text, 2, "1", &ui, &vars).ends_with("\"value\":true,\"objects\":[]}"));
    // One of objects: they're offered, by name.
    let mut with = ui.clone();
    with.objects.insert("unitcode".into(), [("hfoo".to_string(), "Footman".to_string()), ("hkni".to_string(), "Knight".to_string()), ("Hamg".to_string(), "Archmage".to_string())].into());
    assert!(choices(text, 2, "1", &with, &vars).ends_with("\"value\":true,\"objects\":[[\"Hamg\",\"Archmage\"],[\"hfoo\",\"Footman\"],[\"hkni\",\"Knight\"]]}"));
    // SetVariable: any variable, then the variable's type.
    assert!(choices(text, 3, "0", &ui, &vars).contains("\"type_name\":\"Any variable\""));
    assert!(choices(text, 3, "1", &ui, &vars).starts_with("{\"type\":\"integer\""));
    assert!(choices(text, 3, "7", &ui, &vars).starts_with("{\"error\""));
    assert_eq!(catalog(2, &ui), "[[\"CustomScriptCode\",\"CustomScriptCode\",\"\",\"\",false],[\"SetVariable\",\"SetVariable\",\"\",\"\",false],[\"CreateNUnitsAtLoc\",\"Create Units\",\"UNIT\",\"\",true]]");
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
    let t = ok(vars, op::NEW_VARIABLE, 0, "b\nplayer\n1\n12\n\n");
    assert_eq!(t, format!("{vars}\n[variable]\nname=\"b\"\ntype=\"player\"\narray=1\nsize=12\n"));
    let t = ok("[trigger]\naction=DoNothing()\n", op::NEW_VARIABLE, 0, "n\ninteger\n0\n\nglobal\n5");
    assert_eq!(t, "[variable]\nname=\"n\"\ntype=\"integer\"\nsize=1\ninitial=\"5\"\n\n[trigger]\naction=DoNothing()\n");
    assert_eq!(ok("", op::NEW_VARIABLE, 0, "n\ninteger\n0\n\n\n"), "[variable]\nname=\"n\"\ntype=\"integer\"\nsize=1\n");
    assert!(edit(vars, op::NEW_VARIABLE, 0, "A\nunit\n0\n\n\n", &ui).contains("has a variable A already"));
    assert!(edit(vars, op::NEW_VARIABLE, 0, "1x\nunit\n0\n\n\n", &ui).contains("isn't a variable's name"));
    // Deleting a variable: its section.
    let two = format!("{vars}\n[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n");
    assert_eq!(ok(&two, op::DELETE, 1, ""), "[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n");
    assert!(catalog(3, &ui).starts_with("[[\"player\",\"WESTRING_TRIGTYPE_player\"]"));
}

#[test]
fn trg_view_marks_undeclared_variables() {
    let names = crate::names::Names::default();
    let text = "[trigger]\naction=SetVariable($a, $b)\naction=SetVariable($gg_trg_X, $a)\n";
    let a = wtg_rs::Variable { name: "a".into(), kind: "integer".into(), unknown: 1, is_array: 0, array_size: 1, has_initial: 0, initial: String::new() };
    let v = view(text, &names, &[Declared { variable: a, scope: Scope::Global }], &Triggers::of("/m/X.trg", text, &[]));
    assert!(v.contains("{\"r\":\"v\",\"p\":\"0\",\"s\":[\"a\"]}"), "{v}");
    assert!(v.contains("{\"r\":\"u\",\"p\":\"1\",\"s\":[\"b\"]}"), "{v}");
    assert!(v.contains("\"errors\":[[2,\"the variable `b` isn't declared in the project's trigger files ([variable])\"]]"), "{v}");
}

#[test]
fn private_and_local_variables_seen_where_they_are() {
    let ui = ui();
    let declared = |name: &str, scope: Scope| Declared {
        variable: wtg_rs::Variable {
            name: name.into(), kind: "player".into(), unknown: 1, is_array: 0, array_size: 1, has_initial: 0, initial: String::new(),
        },
        scope,
    };
    let seen = [declared("g", Scope::Global), declared("p", Scope::Private), declared("l", Scope::Local)];
    let text = concat!(
        "[trigger]\n",                                                 // 1
        "condition=OperatorComparePlayer(\"1\", OperatorEqualENE, $l)\n", // 2
        "action=CreateNUnitsAtLoc(\"1\", \"hfoo\", $l)\n",            // 3
        "action=ForGroupMultiple()\n",                                 // 4
        ">action:0=CreateNUnitsAtLoc(\"1\", \"hfoo\", $l)\n",         // 5
        "action=ForLoopAMultiple()\n",                                 // 6
        ">action:0=CreateNUnitsAtLoc(\"1\", \"hfoo\", $p)\n",         // 7
    );
    let names = |line: usize| -> Vec<String> {
        let c = choices(text, line, "2", &ui, &seen);
        ["g", "p", "l"].iter().filter(|n| c.contains(&format!("[\"{n}\","))).map(|n| n.to_string()).collect()
    };
    // In the actions, inside any block: all (a build makes a
    // ForGroupMultiple's a loop of the trigger's function); in a
    // condition: no local.
    assert_eq!(names(3), ["g", "p", "l"]);
    assert_eq!(names(5), ["g", "p", "l"]);
    assert_eq!(names(7), ["g", "p", "l"]);
    assert_eq!(names(2), ["g", "p"]);
    assert!(choices(text, 3, "2", &ui, &seen).contains("[\"l\",\"WESTRING_TRIGTYPE_player\",false,\"local\"]"));
    // The view: the same.
    let v = view(text, &crate::names::Names::default(), &seen, &Triggers::of("", text, &[]));
    assert!(v.contains("[2,\"the local `l` isn't seen here"), "{v}");
    assert!(!v.contains("[3,\"the local") && !v.contains("[5,\"") && !v.contains("[7,\""), "{v}");
    // A new local brings its trigger; a private one is written with its scope.
    let t = applied("", op::NEW_VARIABLE, 0, "l\ninteger\n0\n\nlocal\n").unwrap();
    assert_eq!(t, "[variable]\nname=\"l\"\ntype=\"integer\"\nscope=local\nsize=1\n\n[trigger]\n");
    assert!(applied("", op::NEW_VARIABLE, 0, "p\ninteger\n0\n\nprivate\n").unwrap().contains("scope=private\n"));
}

#[test]
fn trg_jass() {
    let names = crate::names::Names::default();
    let j = jass("[trigger]\nname=\"T\"\naction=DoNothing()\n", "/m/b.trg", &names, &[]);
    assert!(j.contains("function Trig_T_Actions\n    DoNothing()\nendfunction"), "{j}");
    let j = jass("[trigger]\nwhat=1\naction=DoNothing()\n", "/m/b.trg", &names, &[]);
    assert!(j.starts_with("// The text has 1 problem(s); what reads is built:\n//   line 2:"), "{j}");
    // As World Editor names it: a byte each, a `u` at the end; a trigger
    // without a name, as its file.
    let j = jass("[trigger]\nname=\"Шляпа непонятная\"\n", "/m/b.trg", &names, &[]);
    assert!(j.contains(&format!("trigger gg_trg_{}u = CreateTrigger()", "_".repeat(31))), "{j}");
    let j = jass("[trigger]\naction=DoNothing()\n", "/m/000_Spawn.trg", &names, &[]);
    assert!(j.contains("function Trig_000_Spawn_Actions"), "{j}");
}

#[test]
fn replacing_a_line_and_setting_a_variable() {
    // An if/then/else becomes a loop: a loop's one block is 0, of
    // actions, and the if's 0 is conditions: all that's inside goes.
    let t = applied(TRG, op::REPLACE, 5, "ForLoopAMultiple").unwrap();
    assert_eq!(t, TRG.replace(
        "action=IfThenElseMultiple()\n>condition:0=OperatorEqual()\n>action:1=DoNothing()\n>action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n>action:2=DoNothing()\n",
        "action=ForLoopAMultiple()\n",
    ));
    // Off stays off; in a block stays there.
    let t = applied(&TRG.replace(">action:1=DoNothing", ">!action:1=DoNothing"), op::REPLACE, 7, "PolledWait").unwrap();
    assert!(t.contains("\n>!action:1=PolledWait()\n"), "{t}");
    // The same function: nothing changes.
    assert_eq!(applied(TRG, op::REPLACE, 8, "ForGroupMultiple").unwrap(), TRG);
    assert!(applied(TRG, op::REPLACE, 2, "DoNothing").unwrap_err().contains("isn't an event"));

    let two = "[variable]\nname=\"a\"\ntype=\"unit\"\nsize=1\n\n[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n";
    let t = applied(two, op::SET_VARIABLE, 1, "A2\ninteger\n1\n8\nprivate\n3").unwrap();
    assert_eq!(t, "[variable]\nname=\"A2\"\ntype=\"integer\"\nscope=private\narray=1\nsize=8\ninitial=\"3\"\n\n[variable]\nname=\"b\"\ntype=\"unit\"\nsize=1\n");
    // Its own name again is fine; another's isn't.
    assert_eq!(applied(two, op::SET_VARIABLE, 6, "B\nunit\n0\n\n\n").unwrap(), two.replace("\"b\"", "\"B\""));
    assert!(applied(two, op::SET_VARIABLE, 6, "a\nunit\n0\n\n\n").unwrap_err().contains("has a variable a already"));
    assert!(applied(two, op::SET_VARIABLE, 2, "a\nunit\n0\n\n\n").unwrap_err().contains("isn't a variable's"));
}

#[test]
fn changing_a_variable_in_its_project() {
    let ui = ui();
    let me = concat!(
        "[variable]\nname=\"p\"\ntype=\"player\"\nsize=1\n\n", // 1-4
        "[trigger]\n",                                         // 6
        "condition=OperatorComparePlayer($p, OperatorEqualENE, Player00) // $p\n", // 7
        "action=CreateNUnitsAtLoc(\"1\", \"hfoo\", $p)\n",    // 8
        "action=CustomScriptCode(\"call X(udg_p, \\\"$p\\\")\")\n", // 9
        "action=CustomScriptCode(\"// udg_p\\ncall X(\\\"udg_p\\\")\")\n", // 10: not a name
    );
    let other = "[trigger]\naction=SetVariable($p, Player01)\naction=SetVariable($px, $p)\n";
    let own = "[variable]\nname=\"p\"\ntype=\"unit\"\nscope=private\nsize=1\n\n[trigger]\naction=SetVariable($p, $p)\n";
    // As the index reads it: a name in a comment or a string isn't one.
    let jass = "function F takes nothing returns nothing\n    call X(udg_p) // udg_p\n    call X(\"udg_p\")\nendfunction\n";
    let names = jass_rs::semantic::symbols(jass, jass_rs::Dialect::VJass);
    let q = "[variable]\nname=\"q\"\ntype=\"unit\"\nsize=1\n";
    let project = Project { me: "/m/a.trg", others: vec![("/m/b.trg", other, &[]), ("/m/c.trg", own, &[]), ("/m/s.j", jass, &names), ("/m/q.trg", q, &[])] };
    let check = |arg: &str| edit_in(me, op::CHECK_VARIABLE, 1, arg, &ui, &project);

    // A rename: the uses here and in b.trg (not c.trg's own p, not in a
    // string or a comment); the JASS name changes, so the custom script
    // and s.j are places to fix.
    let c = check("hero\nplayer\n0\n\n\n");
    assert_eq!(c, concat!(
        "{\"renamed\":4,\"places\":[",
        "[\"/m/a.trg\",9,\"custom script uses udg_p, which is udg_hero now\"],",
        "[\"/m/s.j\",2,\"custom script uses udg_p, which is udg_hero now\"]]}",
    ));
    let a = edit_in(me, op::SET_VARIABLE, 1, "hero\nplayer\n0\n\n\n", &ui, &project);
    assert!(a.contains("condition=OperatorComparePlayer($hero, OperatorEqualENE, Player00) // $p"), "{a}");
    assert!(a.contains("CreateNUnitsAtLoc(\\\"1\\\", \\\"hfoo\\\", $hero)"), "{a}");
    assert!(!a.contains("CustomScriptCode"), "{a}");
    assert!(a.contains("\"files\":[[\"/m/b.trg\","), "{a}");
    assert!(a.contains("SetVariable($hero, Player01)\\u000aaction=SetVariable($px, $hero)"), "{a}");
    assert!(!a.contains("c.trg"), "{a}");

    // A deletion: every use, and the custom script.
    let c = check("");
    for line in ["\"/m/a.trg\",7,\"uses p", "\"/m/a.trg\",8,", "\"/m/b.trg\",2,", "\"/m/b.trg\",3,", "\"/m/s.j\",2,"] {
        assert!(c.contains(line), "{line}: {c}");
    }
    // Private: b.trg loses it, and its JASS name changes.
    let c = check("p\nplayer\n0\n\nprivate\n");
    assert!(c.contains("[\"/m/b.trg\",2,\"p is private to a.trg now\"]"), "{c}");
    assert!(c.contains("which is udg_a__p now"), "{c}");
    // Local: the condition too.
    let c = check("p\nplayer\n0\n\nlocal\n");
    assert!(c.contains("[\"/m/a.trg\",7,\"p is a local now: a trigger's locals are its actions'\"]"), "{c}");
    // A type: the parameters that take a player, SetVariable's value.
    let c = check("p\nunit\n0\n\n\n");
    assert!(c.contains("[\"/m/a.trg\",8,\"takes WESTRING_TRIGTYPE_player, and p is unit now\"]"), "{c}");
    assert!(c.contains("[\"/m/b.trg\",2,\"sets p to WESTRING_TRIGTYPE_player (its old type), and it's unit now\"]"), "{c}");
    // An array: every use needs an index; nothing renamed; size alone is safe.
    let c = check("p\nplayer\n1\n4\n\n");
    assert!(c.contains("[\"/m/a.trg\",8,\"p is an array now: it needs an index\"]"), "{c}");
    assert_eq!(check("p\nplayer\n0\n\n\n"), "{\"renamed\":0,\"places\":[]}");
    // Clashes: another file's global, or hiding one.
    assert!(check("q\nplayer\n0\n\n\n").contains("q.trg declares a variable q already"));
    let alone = Project { me: "/m/c.trg", others: vec![("/m/q.trg", q, &[])] };
    assert!(edit_in(own, op::SET_VARIABLE, 1, "q\nunit\n0\n\nprivate\n", &ui, &alone).contains("q is a global of q.trg: this file's would hide it"));
}

#[test]
fn custom_script_of_lines() {
    let text = "[trigger]\naction=CustomScriptCode(\"\")\n";
    let t = applied(text, op::SET_PARAM, 2, "0\nvalue\nlocal integer n\n\tset n = \"1\"").unwrap();
    // One line of the text: the lines are `\n` in the string.
    assert_eq!(t, "[trigger]\naction=CustomScriptCode(\"local integer n\\n\\tset n = \\\"1\\\"\")\n");
    let p = wtg_rs::text::trigger_from_text(&t, None);
    assert_eq!(p.trigger.unwrap().ecas[0].params[0].value, "local integer n\n\tset n = \"1\"");
}

#[test]
fn several_lines_at_once() {
    // Then's two actions (7, 8 with 9 inside it) and the last one.
    let t = applied(TRG, op::DELETE, 8, "7 9").unwrap();
    assert_eq!(t, TRG.replace(">action:1=DoNothing()\n>action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n>>action:0=DoNothing()\n", ""));
    let copied = edit(TRG, op::COPY, 8, "7", &wtg_rs::ui::Ui::default());
    assert_eq!(copied, "{\"text\":\"action=DoNothing()\\u000aaction=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\\u000a>action:0=DoNothing()\\u000a\"}");
    // All turned as the first: off.
    let t = applied(TRG, op::TOGGLE, 7, "11 10").unwrap();
    assert!(t.contains(">!action:1=DoNothing()\n") && t.contains(">!action:2=DoNothing()\n") && t.ends_with("!action=DoNothing()\n"), "{t}");
    assert!(applied(TRG, op::DELETE, 7, "2").unwrap_err().contains("line 2 isn't"));
}

#[test]
fn moving_several_lines() {
    // Then's DoNothing (7) and the Else's (10), into the ForGroup's loop
    // (8, block 0), before what's there: in the text's order.
    let t = applied(TRG, op::MOVE, 10, "8 0 0\n7").unwrap();
    assert_eq!(t, concat!(
        "[trigger]\n", "name=\"Spawn\"\n", "description=\"\"\n", "event=MapInitializationEvent()\n",
        "action=IfThenElseMultiple()\n", ">condition:0=OperatorEqual()\n",
        ">action:1=ForGroupMultiple(GetUnitsInRectAll(GetPlayableMapRect()))\n",
        ">>action:0=DoNothing()\n", ">>action:0=DoNothing()\n", ">>action:0=DoNothing()\n",
        "action=DoNothing()\n",
    ));
    // The if (with what's inside) and the last action: to the top of the actions.
    let t = applied(TRG, op::MOVE, 11, "0 2 0\n5").unwrap();
    assert!(t.starts_with("[trigger]\nname=\"Spawn\"\ndescription=\"\"\nevent=MapInitializationEvent()\naction=IfThenElseMultiple()\n"), "{t}");
    assert!(t.ends_with(">action:2=DoNothing()\naction=DoNothing()\n"), "{t}");
    // Not into one of them; not an event among actions.
    assert!(applied(TRG, op::MOVE, 11, "8 0 0\n5").unwrap_err().contains("into itself"));
    assert!(applied(TRG, op::MOVE, 11, "0 2 0\n4").unwrap_err().contains("can't go where"));
}

#[test]
fn an_empty_name_is_the_files() {
    let ui = wtg_rs::ui::Ui::default();
    let project = Project { me: "/m/Spawn.trg", others: vec![] };
    let t = edit_in(TRG, op::FIELD, 0, "name\n", &ui, &project);
    assert_eq!(t, "{\"edits\":[[1,0,2,0,\"\"]],\"line\":0}");
    let t = TRG.replace("name=\"Spawn\"\n", "");
    assert!(edit_in(&t, op::FIELD, 0, "name\nSpawn", &ui, &project).contains("\"name=\\\"Spawn\\\"\\u000a\""));
    assert_eq!(applied(TRG, op::FIELD, 0, "description\n").unwrap(), TRG.replace("description=\"\"\n", ""));
}

#[test]
fn references_to_triggers_checked() {
    let names = crate::names::Names::default();
    let others = [
        ("/m/1.trg", "[trigger]\nname=\"Шляпа\"\n"),
        ("/m/2.trg", "[trigger]\nname=\"Кепка\"\n"),
        ("/m/3.trg", "[trigger]\nname=\"Spawn: 1\"\n"),
        ("/m/4.trg", "[trigger]\nname=\"Init\"\n"),
    ];
    // Шляпа and Кепка: gg_trg_ and ten _ and u, both.
    let hat = format!("gg_trg_{}u", "_".repeat(10));
    let text = format!(concat!(
        "[trigger]\n",
        "name=\"Init\"\n",                                      // 2: as 4.trg
        "action=SetVariable(${0}, $gg_trg_Spawn__1)\n",          // 3: both hats, then 3.trg
        "action=SetVariable($gg_trg_Spawn_1, $gg_rct_Area)\n",   // 4: none; a region's isn't checked
        "action=SetVariable($gg_trg_Init, $gg_trg_Init)\n",      // 5: both Inits
    ), hat);
    let v = view(&text, &names, &[], &Triggers::of("/m/9.trg", &text, &others));
    assert!(v.contains("[2,\"the map's script would name 4.trg gg_trg_Init too: rename one\"]"), "{v}");
    assert!(v.contains(&format!("[3,\"`{hat}` may mean 2 triggers (1.trg, 2.trg): rename one\"]")), "{v}");
    assert!(!v.contains("`gg_trg_Spawn__1`"), "{v}");
    assert!(v.contains("[4,\"no trigger of the project is named `gg_trg_Spawn_1`\"]"), "{v}");
    assert!(v.contains("[5,\"`gg_trg_Init` may mean 2 triggers (4.trg, 9.trg): rename one\"]"), "{v}");
}

#[test]
fn renaming_a_trigger_retargets_references() {
    let ui = wtg_rs::ui::Ui::default();
    let me = "[trigger]\nname=\"Шляпа\"\n";
    let hat = format!("gg_trg_{}u", "_".repeat(10));
    let cap = format!("gg_trg_{}u", "_".repeat(10));
    let b = format!(concat!(
        "[trigger]\n",
        "action=SetVariable(${0}, $gg_trg_Other) // ${0}\n",
        "action=CustomScriptCode(\"call TriggerExecute({0}) // {0}\\ncall X(\\\"{0}\\\")\")\n",
        "action=DisplayText(\"{0}\")\n",
    ), hat);
    let jass = format!("function F takes nothing returns nothing\n    call TriggerExecute({hat}) // {hat}\nendfunction\n");
    let names = jass_rs::semantic::symbols(&jass, jass_rs::Dialect::VJass);
    let other = "[trigger]\nname=\"Other\"\n";
    let project = Project { me: "/m/a.trg", others: vec![("/m/b.trg", &b, &[]), ("/m/c.trg", other, &[]), ("/m/s.j", &jass, &names)] };

    // The name goes into the text as it's typed; the references follow
    // once it's done (RENAMED, from the name before).
    let a = edit_in(me, op::FIELD, 0, "name\nHat", &ui, &project);
    assert!(a.contains("name=\\\"Hat\\\"") && !a.contains("files"), "{a}");
    let hat_text = "[trigger]\nname=\"Hat\"\n";
    let a = edit_in(hat_text, op::RENAMED, 0, "Шляпа", &ui, &project);
    // Not in comments or strings, not in another action's text.
    assert!(a.contains(&format!("SetVariable($gg_trg_Hat, $gg_trg_Other) // ${hat}")), "{a}");
    assert!(a.contains(&format!("TriggerExecute(gg_trg_Hat) // {hat}\\\\ncall X(\\\\\\\"{hat}\\\\\\\")")), "{a}");
    assert!(!a.contains("DisplayText"), "{a}");
    assert!(a.contains(&format!("[\"/m/s.j\",1,0,2,0,\"    call TriggerExecute(gg_trg_Hat) // {hat}\\u000a\"]")), "{a}");
    // Кепка is named alike in the script: nothing to change.
    assert_eq!(hat, cap);
    assert!(!edit_in("[trigger]\nname=\"Кепка\"\n", op::RENAMED, 0, "Шляпа", &ui, &project).contains("files"));
    // Named as another: refused, the references left as they are.
    let a = edit_in("[trigger]\nname=\"Other\"\n", op::RENAMED, 0, "Шляпа", &ui, &project);
    assert!(a.contains(&format!("the map's script names c.trg gg_trg_Other too: references to {hat} were left as they are")), "{a}");
    let quiet = Project { me: "/m/a.trg", others: vec![("/m/b.trg", "[trigger]\naction=SetVariable($gg_trg_X, $gg_trg_X)\n", &[])] };
    assert!(!edit_in(hat_text, op::RENAMED, 0, "Шляпа", &ui, &quiet).contains("files"));
}

#[test]
fn comments_are_no_triggers() {
    // Two comments named alike in the script: no trigger, no clash.
    let names = crate::names::Names::default();
    let others = [("/m/1.trg", "[trigger]\nname=\"Шляпа\"\ncomment=1\n")];
    let text = "[trigger]\nname=\"Кепка\"\ncomment=1\naction=TriggerExecute($gg_trg___________u)\n";
    let v = view(text, &names, &[], &Triggers::of("/m/2.trg", text, &others));
    assert!(v.contains("\"errors\":[[4,\"no trigger of the project is named `gg_trg___________u`\"]]"), "{v}");
}

const MAP: &str = concat!(
    "[wtg]\nversion=7\ngame=2\n\n",
    "[category]\nindex=0\nname=\"Init\"\n\n",
    "[variable]\nname=\"hero\"\ntype=\"unit\"\n\n",
    "[trigger]\nname=\"A\"\ndescription=\"\"\ncategory=0\nevent=MapInitializationEvent()\naction=DoNothing()\n\n",
    "[trigger]\nname=\"B\"\ncategory=0\nenabled=0\naction=DoNothing()\naction=TriggerExecute($gg_trg_A)\n",
);

#[test]
fn wtg_view_has_the_tree_and_the_picked_trigger() {
    let names = crate::names::Names::default();
    let v = wtg_view(MAP, 0, &names, None);
    assert!(v.starts_with("{\"categories\":[{\"line\":5,\"index\":0,\"name\":\"Init\",\"comment\":0}],\"triggers\":[{\"line\":13,\"name\":\"A\",\"category\":0,\"enabled\":1,\"comment\":0,\"custom\":0},{\"line\":20,\"name\":\"B\",\"category\":0,\"enabled\":0,\"comment\":0,\"custom\":0}],\"variables\":[{\"line\":9,\"scope\":\"global\",\"name\":\"hero\",\"type\":\"unit\",\"type_name\":\"unit\",\"array\":false,\"size\":0,\"initial\":null}],"), "{v}");
    // The first, without its category= (line 16): its lines are the text's.
    assert!(v.contains("\"picked\":1,\"at\":[13,14,15,17,18,19],\"view\":{\"trigger\":true,\"script\":\"gg_trg_A\""), "{v}");
    // B, the second; its reference to A is one of the map's triggers.
    let v = wtg_view(MAP, 2, &names, None);
    assert!(v.contains("\"picked\":2,\"at\":[20,21,23,24,25],"), "{v}");
    assert!(v.contains("\"errors\":[]"), "{v}");
    assert!(!v.contains("\"category\" ") && !v.contains("no field `category`"), "{v}");
    // No trigger: no view.
    assert!(wtg_view("[wtg]\nversion=7\n", 0, &names, None).ends_with("\"picked\":0,\"at\":[],\"view\":null,\"wct\":null}"));
}

#[test]
fn wtg_edits_are_of_the_whole_text() {
    let ui = wtg_rs::ui::Ui::default();
    // B's first action (its section's line 4, the text's 24) turned off.
    let a = wtg_edit(MAP, op::TOGGLE, 4, "2\n", &ui, None);
    assert_eq!(a, "{\"edits\":[[23,0,24,0,\"!action=DoNothing()\\u000a\"]],\"line\":4}");
    // Its name: the category= stays.
    let a = wtg_edit(MAP, op::FIELD, 0, "2\nname\nC", &ui, None);
    assert_eq!(a, "{\"edits\":[[20,0,21,0,\"name=\\\"C\\\"\\u000a\"]],\"line\":0}");
    // A's actions deleted: the next trigger is untouched.
    let a = wtg_edit(MAP, op::DELETE, 5, "1\n", &ui, None);
    assert_eq!(a, "{\"edits\":[[17,0,18,0,\"\"]],\"line\":0}");
    // B's second action moved first: category= stays where it was.
    let a = wtg_edit(MAP, op::MOVE, 5, "2\n0 2 0", &ui, None);
    assert_eq!(a, "{\"edits\":[[23,0,25,0,\"action=TriggerExecute($gg_trg_A)\\u000aaction=DoNothing()\\u000a\"]],\"line\":4}");
    assert!(wtg_edit("[wtg]\n", op::TOGGLE, 1, "0\n", &ui, None).starts_with("{\"error\":"));
}

/// The text after the edit of a `.wtg`'s answer `a` is applied.
fn wtg_applied(text: &str, a: &str) -> String {
    answer_applied(text, a).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn wtg_tree_operations() {
    let ui = wtg_rs::ui::Ui::default();
    // A new trigger after its category's last, picked.
    let a = wtg_edit(MAP, map_op::NEW_TRIGGER, 0, "1\n0\nC", &ui, None);
    assert!(a.ends_with("\"pick\":3}"), "{a}");
    let t = wtg_applied(MAP, &a);
    assert!(t.ends_with("action=TriggerExecute($gg_trg_A)\n\n[trigger]\nname=\"C\"\ndescription=\"\"\ncategory=0\n"), "{t}");
    // Named as another in the script: refused.
    assert!(wtg_edit(MAP, map_op::NEW_TRIGGER, 0, "1\n0\nA", &ui, None).contains("gg_trg_A too"));
    // A new category after the last, with the next index.
    let t = wtg_applied(MAP, &wtg_edit(MAP, map_op::NEW_CATEGORY, 0, "1\nMore", &ui, None));
    assert!(t.contains("name=\"Init\"\n\n[category]\nindex=1\nname=\"More\"\n\n[variable]"), "{t}");
    // A trigger of it goes after the others.
    let a = wtg_edit(&t, map_op::NEW_TRIGGER, 0, "1\n1\nD", &ui, None);
    assert!(a.ends_with("\"pick\":3}"), "{a}");
    // Renamed.
    let r = wtg_applied(&t, &wtg_edit(&t, map_op::RENAME_CATEGORY, 0, "1\n1\nLess", &ui, None));
    assert!(r.contains("index=1\nname=\"Less\"\n"), "{r}");
    // The picked trigger deleted: the next is picked in its place.
    let a = wtg_edit(MAP, map_op::DELETE_TRIGGER, 0, "1\n", &ui, None);
    assert!(a.ends_with("\"pick\":1}"), "{a}");
    let t = wtg_applied(MAP, &a);
    assert!(!t.contains("name=\"A\"") && t.contains("[variable]\nname=\"hero\"\ntype=\"unit\"\n\n[trigger]\nname=\"B\""), "{t}");
    // The last deleted: no blank lines left at the end.
    let t = wtg_applied(MAP, &wtg_edit(MAP, map_op::DELETE_TRIGGER, 0, "2\n", &ui, None));
    assert!(t.ends_with("action=DoNothing()\n"), "{t:?}");
    // A category with its triggers.
    let t = wtg_applied(MAP, &wtg_edit(MAP, map_op::DELETE_CATEGORY, 0, "2\n0", &ui, None));
    assert_eq!(t, "[wtg]\nversion=7\ngame=2\n\n[variable]\nname=\"hero\"\ntype=\"unit\"\n");
}

#[test]
fn wtg_variables_are_globals() {
    let ui = wtg_rs::ui::Ui::default();
    // New: after the last variable.
    let t = wtg_applied(MAP, &wtg_edit(MAP, op::NEW_VARIABLE, 0, "1\nn\ninteger\n0\n1\nglobal\n", &ui, None));
    assert!(t.contains("type=\"unit\"\n\n[variable]\nname=\"n\"\ntype=\"integer\"\nsize=1\n\n[trigger]"), "{t}");
    // Reign of Chaos's file: no size.
    let roc = MAP.replace("version=7", "version=4");
    let t = wtg_applied(&roc, &wtg_edit(&roc, op::NEW_VARIABLE, 0, "1\nn\ninteger\n0\n1\nglobal\n", &ui, None));
    assert!(t.contains("[variable]\nname=\"n\"\ntype=\"integer\"\n\n[trigger]"), "{t}");
    // Private and local ones are a .trg's; a name taken is refused.
    assert!(wtg_edit(MAP, op::NEW_VARIABLE, 0, "1\nn\ninteger\n0\n1\nlocal\n", &ui, None).contains("a map's variables are globals"));
    assert!(wtg_edit(MAP, op::NEW_VARIABLE, 0, "1\nHERO\ninteger\n0\n1\nglobal\n", &ui, None).contains("has a variable HERO already"));
    // Renamed: its uses in every trigger.
    let text = MAP.replace("action=DoNothing()\n\n", "action=SetVariable($hero, $hero)\n\n");
    let check = wtg_edit(&text, op::CHECK_VARIABLE, 9, "1\nboss\nunit\n0\n1\nglobal\n", &ui, None);
    assert_eq!(check, "{\"renamed\":2,\"places\":[]}");
    let t = wtg_applied(&text, &wtg_edit(&text, op::SET_VARIABLE, 9, "1\nboss\nunit\n0\n1\nglobal\n", &ui, None));
    assert!(t.contains("[variable]\nname=\"boss\"\ntype=\"unit\"\nsize=1\n\n") && t.contains("action=SetVariable($boss, $boss)\n"), "{t}");
    // Deleted: its uses are the places, at their lines in the text.
    let check = wtg_edit(&text, op::CHECK_VARIABLE, 9, "1\n", &ui, None);
    assert_eq!(check, "{\"renamed\":0,\"places\":[[\"\",18,\"uses hero, which is deleted\"]]}");
    let t = wtg_applied(&text, &wtg_edit(&text, map_op::DELETE_VARIABLE, 9, "1\n", &ui, None));
    assert!(!t.contains("[variable]") && t.contains("name=\"Init\"\n\n[trigger]"), "{t}");
}

#[test]
fn wtg_rename_retargets_the_other_triggers() {
    let ui = wtg_rs::ui::Ui::default();
    let text = MAP.replace("name=\"A\"", "name=\"Z\"");
    let t = wtg_applied(&text, &wtg_edit(&text, op::RENAMED, 0, "1\nA", &ui, None));
    assert!(t.contains("action=TriggerExecute($gg_trg_Z)\n"), "{t}");
}

/// Every trigger of a real map's text (`WTG_TEXT`, a file of it): put
/// back unchanged, turned off and on again, the text is the same; its
/// view reads.
#[test]
#[ignore]
fn wtg_slices_of_a_real_map() {
    let text = std::fs::read_to_string(std::env::var("WTG_TEXT").expect("WTG_TEXT")).unwrap();
    let names = crate::names::Names::default();
    let ui = wtg_rs::ui::Ui::default();
    let map = Map::of(&text);
    let count = text.lines().filter(|l| l.trim() == "[trigger]").count();
    assert_eq!(map.triggers.len(), count);
    let mut toggled = 0;
    for (k, slice) in map.triggers.iter().enumerate() {
        let nth = k + 1;
        let mut lines = map.lines.clone();
        slice.put(&mut lines, slice.text.split_inclusive('\n').map(String::from).collect(), None);
        assert_eq!(lines, map.lines, "trigger {nth}");
        if nth % 50 == 1 {
            assert!(!wtg_view(&text, nth, &names, None).contains("no field `category`"), "trigger {nth}");
        }
        if let Some(e) = slice.text.lines().position(|l| eca_line(l).is_some()) {
            let once = wtg_edit(&text, op::TOGGLE, e + 1, &format!("{nth}\n"), &ui, None);
            let once = answer_applied(&text, &once).unwrap();
            let twice = answer_applied(&once, &wtg_edit(&once, op::TOGGLE, e + 1, &format!("{nth}\n"), &ui, None)).unwrap();
            assert!(twice == text, "trigger {nth}");
            toggled += 1;
        }
    }
    println!("{count} triggers, {toggled} toggled back");
    // Each one's JASS reads as JASS (this toolchain's: no set, no call),
    // with the editor's files of the game folders in `WC3_GAME` (a path
    // list, as PATH), when given.
    let names = match std::env::var_os("WC3_GAME") {
        Some(g) => crate::names::names(&crate::game::Lookup { game: std::env::split_paths(&g).collect(), roots: vec![] }),
        None => return,
    };
    let mut bad = Vec::new();
    for nth in 1..=count {
        let j = wtg_jass(&text, nth, &names);
        if let Err(e) = jass_rs::parse(&j) {
            bad.push(format!("trigger {nth}: line {}: {}\n{}", e.line, e.message, j.lines().nth(e.line.saturating_sub(1)).unwrap_or("")));
        }
    }
    assert!(bad.is_empty(), "{} of {count} don't read:\n{}", bad.len(), bad.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
    // A variable's uses, over every trigger.
    if let Some((_, r)) = map.sections.iter().find(|(n, _)| n == wtg_rs::text::VARIABLE) {
        let t = std::time::Instant::now();
        let check = wtg_edit(&text, op::CHECK_VARIABLE, r.start + 1, "1\n", &ui, None);
        println!("checked deleting the first variable in {:?}: {}", t.elapsed(), &check[..check.len().min(120)]);
        assert!(check.starts_with("{\"renamed\":0,\"places\":["), "{check}");
    }
    assert!(count > 0);
}

#[test]
fn wtg_moving_triggers() {
    let ui = wtg_rs::ui::Ui::default();
    let two = applied_map(&applied_map(MAP, map_op::NEW_CATEGORY, "More", &ui), map_op::NEW_TRIGGER, "1\nC", &ui);
    let names = |t: &str| Map::of(t).triggers.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ");
    assert_eq!(names(&two), "A B C");
    // B before A: its category stays, and it's picked where it is now.
    let a = wtg_edit(&two, map_op::MOVE_TRIGGER, 0, "1\n2\n0\n1", &ui, None);
    assert!(a.ends_with("\"pick\":1}"), "{a}");
    let t = wtg_applied(&two, &a);
    assert_eq!(names(&t), "B A C");
    assert!(t.contains("[trigger]\nname=\"B\"\ncategory=0\nenabled=0\n"), "{t}");
    // A to the other category, at its end: after C, its category= that one's.
    let a = wtg_edit(&two, map_op::MOVE_TRIGGER, 0, "1\n1\n1\n0", &ui, None);
    assert!(a.ends_with("\"pick\":3}"), "{a}");
    let t = wtg_applied(&two, &a);
    assert_eq!(names(&t), "B C A");
    assert!(t.ends_with("category=1\n\n[trigger]\nname=\"A\"\ndescription=\"\"\ncategory=1\nevent=MapInitializationEvent()\naction=DoNothing()\n"), "{t}");
    // C before B, into category 0; the text stays one blank line between sections.
    let t = wtg_applied(&two, &wtg_edit(&two, map_op::MOVE_TRIGGER, 0, "1\n3\n0\n2", &ui, None));
    assert_eq!(names(&t), "A C B");
    assert!(!t.contains("\n\n\n") && t.contains("name=\"C\"\ndescription=\"\"\ncategory=0\n\n[trigger]\nname=\"B\""), "{t}");
    // Before itself: nothing.
    assert_eq!(wtg_applied(&two, &wtg_edit(&two, map_op::MOVE_TRIGGER, 0, "1\n2\n0\n2", &ui, None)), two);
}

/// `text` after the tree's operation `op` with `arg`, the first trigger picked.
fn applied_map(text: &str, op: u32, arg: &str, ui: &wtg_rs::ui::Ui) -> String {
    wtg_applied(text, &wtg_edit(text, op, 0, &format!("1\n{arg}"), ui, None))
}

#[test]
fn custom_script_is_one_line_in_a_map() {
    let ui = wtg_rs::ui::Ui::default();
    let many = "action=CustomScriptCode(\"call A()\\ncall B()\")";
    // A .trg has it.
    assert!(diagnostics(Some(Path::new("a.trg")), &bni_rs::parse(&format!("[trigger]\n{many}\n")), &format!("[trigger]\n{many}\n")).is_empty());
    // A map's text: a problem at its line, and it doesn't save.
    let text = MAP.replace("action=DoNothing()\n\n", &format!("{many}\n\n"));
    let errors = diagnostics(Some(Path::new("war3map.wtg")), &bni_rs::parse(&text), &text);
    assert_eq!(errors.iter().map(|e| (e.line, e.message.as_str())).collect::<Vec<_>>(), [(18, ONE_LINE)]);
    let data = TriggerData::parse("[TriggerEvents]\nMapInitializationEvent=0,nothing\n[TriggerActions]\nDoNothing=0,nothing\nCustomScriptCode=0,scriptcode\nTriggerExecute=0,trigger\n");
    let err = convert(TO_BYTES, "war3map.wtg", text.as_bytes(), &data).unwrap_err();
    assert_eq!(err, format!("Can't save war3map.wtg: line 18: {ONE_LINE}"));
    // Pasted or set in the view: refused; one line is fine.
    let a = wtg_edit(MAP, op::PASTE, 0, &format!("1\n0 2 0\n{many}\n"), &ui, None);
    assert_eq!(a, format!("{{\"error\":{}}}", crate::objects::json_string(ONE_LINE)));
    let a = wtg_edit(MAP, op::PASTE, 0, "1\n0 2 0\naction=CustomScriptCode(\"call A()\")\n", &ui, None);
    assert!(a.starts_with("{\"edits\":"), "{a}");
}

#[test]
fn wtg_jass_is_the_picked_triggers() {
    let names = crate::names::Names::default();
    let j = wtg_jass(MAP, 2, &names);
    assert!(j.contains("// Trigger: B\n") && j.contains("trigger gg_trg_B = null"), "{j}");
    let j = wtg_jass(MAP, 1, &names);
    assert!(j.contains("trigger gg_trg_A = CreateTrigger()\n\nfunction main\n") && !j.contains("call ") && !j.contains("takes nothing"), "{j}");
    jass_rs::parse(&j).unwrap_or_else(|e| panic!("line {}: {}\n{j}", e.line, e.message));
    assert_eq!(wtg_jass("[wtg]\nversion=7\n", 0, &names), "// The map has no triggers.\n");
}

/// Every trigger of a real map's text (`WTG_TEXT`) as a JASS file of its
/// own in the folder `WTG_OUT` (`0001.j` ...), and the triggers run at map
/// initialization in `initialization.txt`, with the editor's files of
/// `WC3_GAME`: for a build of them all to be tried (wc3-build's
/// `tests/real_map.rs`, which makes the variables of the map's text).
#[test]
#[ignore]
fn wtg_jass_files() {
    let text = std::fs::read_to_string(std::env::var("WTG_TEXT").expect("WTG_TEXT")).unwrap();
    let out = std::path::PathBuf::from(std::env::var("WTG_OUT").expect("WTG_OUT"));
    let names = crate::names::names(&crate::game::Lookup { game: std::env::split_paths(&std::env::var_os("WC3_GAME").expect("WC3_GAME")).collect(), roots: vec![] });
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let map = Map::of(&text);
    for nth in 1..=map.triggers.len() {
        std::fs::write(out.join(format!("{nth:04}.j")), wtg_jass(&text, nth, &names)).unwrap();
    }
    let vars: Vec<Declared> = wtg_variables(&text, &wtg_rs::text::parse_bni(&text)).into_iter().map(|(_, d)| d).collect();
    // The triggers run at map initialization, a name a line.
    let file = wtg_rs::text::from_text(&text, None).file.expect("the text reads");
    std::fs::write(out.join("initialization.txt"), wtg_rs::jass::initialization_triggers(&file.triggers).join("\n")).unwrap();
    println!("{} triggers, {} variables", map.triggers.len(), vars.len());
}

/// The custom script beside [`MAP`]: A's code, B's none.
const WCT: &str = "// @wct version=1\n// @wct header\n// @wct trigger\nfunction A takes nothing returns nothing\nendfunction\n// @wct trigger none\n";

/// The custom script after the answer `a`'s edits of it.
fn wct_applied(a: &str) -> String {
    wct_applied_to(WCT, a)
}

/// The custom script `wct` after the answer `a`'s edits of it.
fn wct_applied_to(wct: &str, a: &str) -> String {
    let (_, edits) = a.split_once(",\"wct\":").unwrap_or_else(|| panic!("no wct edits: {a}"));
    let edits = edits.strip_suffix('}').unwrap();
    answer_applied(wct, &format!("{{\"edits\":{edits},\"line\":0}}")).unwrap()
}

#[test]
fn the_custom_script_goes_with_the_triggers() {
    let ui = wtg_rs::ui::Ui::default();
    let names = crate::names::Names::default();
    // Where each piece is.
    let v = wtg_view(MAP, 1, &names, Some(WCT));
    assert!(v.ends_with("\"wct\":{\"header\":[1,2],\"code\":[[2,5,false],[5,6,true]],\"problem\":null}}"), "{v}");
    // A new trigger (third): a piece without code at the end.
    let a = wtg_edit(MAP, map_op::NEW_TRIGGER, 0, "1\n0\nC", &ui, Some(WCT));
    assert!(wct_applied(&a).ends_with("// @wct trigger none\n// @wct trigger none\n"), "{a}");
    // A deleted: its code with it.
    let a = wtg_edit(MAP, map_op::DELETE_TRIGGER, 0, "1\n", &ui, Some(WCT));
    assert_eq!(wct_applied(&a), "// @wct version=1\n// @wct header\n// @wct trigger none\n");
    // B before A: the pieces swapped.
    let a = wtg_edit(MAP, map_op::MOVE_TRIGGER, 0, "1\n2\n0\n1", &ui, Some(WCT));
    assert_eq!(wct_applied(&a), "// @wct version=1\n// @wct header\n// @wct trigger none\n// @wct trigger\nfunction A takes nothing returns nothing\nendfunction\n");
    // The category with both: no pieces left.
    let a = wtg_edit(MAP, map_op::DELETE_CATEGORY, 0, "1\n0", &ui, Some(WCT));
    assert_eq!(wct_applied(&a), "// @wct version=1\n// @wct header\n");
    // An edit of a trigger itself doesn't touch it.
    assert!(!wtg_edit(MAP, op::TOGGLE, 4, "2\n", &ui, Some(WCT)).contains("\"wct\""));
    // Pieces that don't go with the triggers: left alone, and said so.
    let one = "// @wct trigger none\n";
    let a = wtg_edit(MAP, map_op::DELETE_TRIGGER, 0, "1\n", &ui, Some(one));
    assert!(a.contains("\"warning\":\"war3map.wct has the code of 1 triggers, war3map.wtg has 2"), "{a}");
    assert!(wtg_view(MAP, 1, &names, Some(one)).contains("\"problem\":\"war3map.wct has the code of 1 triggers"));
}

/// Each trigger's name and its code in the custom script, in order.
fn code_by_trigger(wtg: &str, wct: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = wct.split_inclusive('\n').collect();
    let p = crate::customtext::pieces(wct);
    Map::of(wtg).triggers.iter().zip(&p.triggers).map(|(t, (a, b, _))| (t.name.clone(), lines[a + 1..*b].concat())).collect()
}

/// A real map's triggers (`WTG_TEXT`) and custom script (`WCT_TEXT`), as
/// text: they go together, and moving or deleting triggers keeps each
/// one's code with it.
#[test]
fn a_real_maps_custom_script_goes_with_its_triggers() {
    let (Some(wtg), Some(wct)) = (std::env::var_os("WTG_TEXT"), std::env::var_os("WCT_TEXT")) else { return };
    let (wtg, wct) = (std::fs::read_to_string(wtg).unwrap(), std::fs::read_to_string(wct).unwrap());
    let ui = wtg_rs::ui::Ui::default();
    assert!(wtg_view(&wtg, 1, &crate::names::Names::default(), Some(&wct)).contains("\"problem\":null}}"));
    let before = code_by_trigger(&wtg, &wct);
    let custom = before.iter().filter(|(_, c)| !c.trim().is_empty()).count();
    let n = before.len();
    let both = |op: u32, nth: usize, arg: &str| {
        let a = wtg_edit(&wtg, op, 0, &format!("{nth}\n{arg}"), &ui, Some(&wct));
        let new_wtg = wtg_applied(&wtg, &format!("{}}}", a.split(",\"wct\":").next().unwrap()));
        let (_, edits) = a.split_once(",\"wct\":").unwrap_or_else(|| panic!("no wct edits: {}", &a[..200.min(a.len())]));
        let new_wct = answer_applied(&wct, &format!("{{\"edits\":{},\"line\":0}}", edits.strip_suffix('}').unwrap())).unwrap();
        code_by_trigger(&new_wtg, &new_wct)
    };
    let sorted = |mut v: Vec<(String, String)>| {
        v.sort();
        v
    };
    // The last custom one moved before the first trigger.
    let k = before.iter().rposition(|(_, c)| !c.trim().is_empty()).unwrap() + 1;
    let moved = both(map_op::MOVE_TRIGGER, 1, &format!("{k}\n0\n1"));
    assert_eq!(moved.len(), n);
    assert_eq!(sorted(moved), sorted(before.clone()));
    // The first deleted: the others' code stays theirs.
    let deleted = both(map_op::DELETE_TRIGGER, 1, "");
    assert_eq!(deleted, before[1..].to_vec());
    println!("{n} triggers, {custom} of custom text: their code went with them");
}

#[test]
fn a_renamed_trigger_s_names_in_the_custom_script_follow() {
    let ui = wtg_rs::ui::Ui::default();
    let wct = "// @wct version=1\n// @wct header\n// @wct trigger\nfunction InitTrig_A takes nothing returns nothing\n    set gg_trg_A = CreateTrigger() // gg_trg_AB stays\nendfunction\n// @wct trigger\ncall DisableTrigger(gg_trg_A)\nset InitTrig_AX = 1\n";
    // A's name set to "Spawn Wave", then the renaming that follows it.
    let named = wtg_applied(MAP, &wtg_edit(MAP, op::FIELD, 0, "1\nname\nSpawn Wave", &ui, None));
    let a = wtg_edit(&named, op::RENAMED, 0, "1\nA", &ui, Some(wct));
    let (_, edits) = a.split_once(",\"wct\":").unwrap_or_else(|| panic!("no wct edits: {a}"));
    let after = answer_applied(wct, &format!("{{\"edits\":{},\"line\":0}}", edits.strip_suffix('}').unwrap())).unwrap();
    assert_eq!(
        after,
        "// @wct version=1\n// @wct header\n// @wct trigger\nfunction InitTrig_Spawn_Wave takes nothing returns nothing\n    set gg_trg_Spawn_Wave = CreateTrigger() // gg_trg_AB stays\nendfunction\n// @wct trigger\ncall DisableTrigger(gg_trg_Spawn_Wave)\nset InitTrig_AX = 1\n"
    );
    // The same name in the script (a change of case only to its spaces): nothing.
    assert!(!wtg_edit(MAP, op::RENAMED, 0, "1\nA", &ui, Some(wct)).contains("\"wct\""));
    assert_eq!(rename_trigger_in_code("xgg_trg_A gg_trg_A_ gg_trg_A", "A", "B"), "xgg_trg_A gg_trg_A_ gg_trg_B");
}

#[test]
fn a_triggers_jass_in_lua_and_angelscript() {
    use wc3_build::jass::Script;
    let common = "type agent extends handle\ntype trigger extends agent\ntype triggeraction extends handle\nnative CreateTrigger takes nothing returns trigger\nnative TriggerAddAction takes trigger whichTrigger, code actionFunc returns triggeraction\nnative ExecuteFunc takes string funcName returns nothing\n";
    let jass = "function Trig_A_Actions takes nothing returns nothing\n    call ExecuteFunc(\"Trig_A_Actions\")\n    call ExecuteFunc(udg_name)\nendfunction\nfunction InitTrig_A takes nothing returns nothing\n    set gg_trg_A = CreateTrigger()\n    call TriggerAddAction(gg_trg_A, function Trig_A_Actions)\nendfunction\n";
    let lua = converted(jass, Script::Lua, common, "");
    assert!(lua.contains("function InitTrig_A()\n    gg_trg_A = CreateTrigger()\n    TriggerAddAction(gg_trg_A, Trig_A_Actions)\nend"), "{lua}");
    // Lua has ExecuteFunc: nothing to say.
    assert!(!lua.contains("error(s)"), "{lua}");
    let angel = converted(jass, Script::AngelScript, common, "");
    assert!(angel.starts_with("// 1 error(s):\n//   line 3 of its JASS: ExecuteFunc of a name told only as the game runs"), "{angel}");
    assert!(angel.contains("void Trig_A_Actions() {\n    Trig_A_Actions();\n    Jass::ExecuteFunc(udg_name);\n}"), "{angel}");
    assert!(angel.contains("void InitTrig_A() {\n    gg_trg_A = Jass::CreateTrigger();\n    Jass::TriggerAddAction(gg_trg_A, @Trig_A_Actions);\n}"), "{angel}");
    // Without common.j AngelScript can't be had: said, not code.
    assert!(converted(jass, Script::AngelScript, "", "").starts_with("// It can't be built:"));
}

#[test]
fn convert_to_custom_text() {
    let ui = ui();
    let wct = "// @wct version=1\n// @wct header\n// @wct trigger none\n// @wct trigger none\n";
    let a = convert_wtg(MAP, 1, &ui, Some(wct));
    // The trigger: custom text, run at map initialization by its flag, no ECAs.
    let text = wtg_applied(MAP, &a);
    assert!(text.contains("[trigger]\nname=\"A\"\ndescription=\"\"\ncategory=0\ncustom_text=1\nrun_on_init=1\n\n[trigger]\nname=\"B\""), "{text}");
    // Its code in the custom script: the game's JASS, InitTrig_A making gg_trg_A.
    let code = wct_applied(&a);
    assert!(code.starts_with("// @wct version=1\n// @wct header\n// @wct trigger\n//===========================================================================\n// Trigger: A\n"), "{code}");
    assert!(code.contains("function Trig_A_Actions takes nothing returns nothing\n    call DoNothing()\nendfunction\n"), "{code}");
    assert!(code.contains("function InitTrig_A takes nothing returns nothing\n    set gg_trg_A = CreateTrigger()\n"), "{code}");
    assert!(code.ends_with("endfunction\n// @wct trigger none\n"), "{code}");
    // What can't be: a map without its custom script, a trigger custom already.
    assert!(convert_wtg(MAP, 1, &ui, None).contains("has no custom script"));
    let custom = convert_wtg(&wtg_applied(MAP, &a), 1, &ui, Some(&code));
    assert!(custom.contains("custom text already"), "{custom}");

    // A .trg: its code as the file beside it (`create`).
    let trg = "[variable]\nname=\"n\"\ntype=\"integer\"\nscope=local\n\n[trigger]\nname=\"T\"\nevent=MapInitializationEvent()\naction=SetVariable($n, \"3\")\n";
    let a = convert_trg(trg, "/nowhere/T.trg", &[], &ui);
    assert!(a.contains("\"create\":[\"/nowhere/T.j\",\"// @dialect vjass\\u000a//==="), "{a}");
    assert!(a.contains("function Trig_T_Actions takes nothing returns nothing\\u000a    local integer n\\u000a    set n = 3\\u000aendfunction"), "{a}");
    // The local's declaration gone with the actions: the code has it.
    assert!(a.starts_with("{\"edits\":[[0,0,9,0,\"[trigger]\\u000aname=\\\"T\\\"\\u000adescription=\\\"\\\"\\u000acustom_text=1\\u000arun_on_init=1\\u000a\"]]"), "{a}");
}

/// 1.31's: the tree's order, ids, a category's parent.
const TREE: &str = concat!(
    "[wtg]\nversion=7\ngame=2\ntree=1\nmap=\"map.w3x\"\nmap_element=1,0,0,1,0\n\n",
    "[category]\nindex=33554432\nname=\"Init\"\nopen=1\n\n",
    "[trigger]\nname=\"A\"\ndescription=\"\"\ncategory=33554432\nid=50331648\nevent=MapInitializationEvent()\naction=DoNothing()\n\n",
    "[variable]\nname=\"hero\"\ntype=\"unit\"\nid=201326592\nparent=33554432\n\n",
    "[trigger]\nname=\"Read me\"\ndescription=\"\"\ncategory=0\nid=67108864\ncomment=1\n\n",
    "[trigger]\nname=\"B\"\ncategory=33554432\nid=50331649\naction=TriggerExecute($gg_trg_A)\n",
);

#[test]
fn wtg_view_of_a_131_tree() {
    let names = crate::names::Names::default();
    let v = wtg_view(TREE, 3, &names, None);
    assert!(v.contains("\"errors\":[]"), "{v}");
    assert!(v.contains("\"name\":\"hero\""), "{v}");
    let v = wtg_view(TREE, 1, &names, None);
    assert!(v.contains("\"errors\":[]"), "{v}");
}

/// [`TREE`]'s custom script, 1.31's: A's piece and B's, none for the comment.
const TREE_WCT: &str = "// @wct version=1\n// @wct magic\n// @wct header\n// @wct trigger\n// A\n// @wct trigger\n// B\n";

#[test]
fn a_131_custom_script_goes_with_the_triggers() {
    let ui = wtg_rs::ui::Ui::default();
    let names = crate::names::Names::default();
    // The comment's piece: none.
    let v = wtg_view(TREE, 1, &names, Some(TREE_WCT));
    assert!(v.ends_with("\"wct\":{\"header\":[2,3],\"code\":[[3,5,false],null,[5,7,false]],\"problem\":null}}"), "{v}");
    // A deleted: its piece with it; a new one at its category's end (after
    // B): a piece after B's.
    let a = wtg_edit(TREE, map_op::DELETE_TRIGGER, 0, "1\n", &ui, Some(TREE_WCT));
    assert_eq!(wct_applied_to(TREE_WCT, &a), "// @wct version=1\n// @wct magic\n// @wct header\n// @wct trigger\n// B\n");
    let a = wtg_edit(TREE, map_op::NEW_TRIGGER, 0, "1\n33554432\nC", &ui, Some(TREE_WCT));
    assert_eq!(wct_applied_to(TREE_WCT, &a), "// @wct version=1\n// @wct magic\n// @wct header\n// @wct trigger\n// A\n// @wct trigger\n// B\n// @wct trigger none\n", "{a}");
    // A category new in a tree without one: a category's id.
    let empty = "[wtg]\nversion=7\ngame=2\ntree=1\n";
    assert!(applied_map(empty, map_op::NEW_CATEGORY, "Init", &ui).contains("[category]\nindex=33554432\n"));
    // The classic count would be wrong: it says how many are wanted.
    let v = wtg_view(TREE, 1, &names, Some("// @wct magic\n// @wct trigger\n// A\n"));
    assert!(v.contains("\"problem\":\"war3map.wct has the code of 1 triggers, war3map.wtg has 2 that aren't comments"), "{v}");
}

#[test]
fn a_131_tree_saved_and_read_back() {
    let data = TriggerData::parse("[TriggerEvents]\nMapInitializationEvent=0,nothing\n[TriggerActions]\nDoNothing=0,nothing\nTriggerExecute=0,trigger\n");
    let bytes = convert(TO_BYTES, "war3map.wtg", TREE.as_bytes(), &data).unwrap();
    assert!(matches!(wtg_rs::parse_any(&bytes, &data), Ok(wtg_rs::File::Tree(_))));
    let text = String::from_utf8(convert(TO_TEXT, "war3map.wtg", &bytes, &data).unwrap()).unwrap();
    // As it was (the deleted ids' line the file has).
    assert_eq!(convert(TO_BYTES, "war3map.wtg", text.as_bytes(), &data).unwrap(), bytes);
    assert!(text.contains("[variable]\nname=\"hero\"") && text.contains("id=50331649\n"), "{text}");
}

#[test]
fn a_131_tree_extracted() {
    let e = extract(TREE, Some(TREE_WCT));
    assert!(!e.contains("error"), "{e}");
    // The category's triggers in its folder; the comment at the top (its
    // parent the map) at the root.
    assert!(e.contains("[\"000_Init/000_A.trg\",") && e.contains("[\"000_Init/001_B.trg\",") && e.contains("[\"000_Read me.trg\","), "{e}");
}
