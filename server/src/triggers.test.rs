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
    let answer = extract(TEXT);
    assert_eq!(answer, "{\"folders\":[],\"files\":[[\"000_Init.trg\",\"[trigger]\\u000aname=\\\"Init\\\"\\u000adescription=\\\"\\\"\\u000aevent=MapInitializationEvent()\\u000aaction=DoNothing()\\u000a\"]]}");
    assert!(extract("[wtg]\nversion=9\n").starts_with("{\"error\":\"Can't extract the triggers: line 2:"));
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
        "\"calls\":[[\"GetOwningPlayer\",\"GetOwningPlayer\",\"\"]],\"value\":false}"
    ));
    // A type of the editor's own over integer takes a value.
    assert!(choices(text, 2, "1", &ui, &vars).ends_with("\"value\":true}"));
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
    assert!(j.contains("function Trig_T_Actions takes nothing returns nothing\n    call DoNothing()\nendfunction"), "{j}");
    let j = jass("[trigger]\nwhat=1\naction=DoNothing()\n", "/m/b.trg", &names, &[]);
    assert!(j.starts_with("// The text has 1 problem(s); what reads is built:\n//   line 2:"), "{j}");
    // As World Editor names it: a byte each, a `u` at the end; a trigger
    // without a name, as its file.
    let j = jass("[trigger]\nname=\"Шляпа непонятная\"\n", "/m/b.trg", &names, &[]);
    assert!(j.contains(&format!("function InitTrig_{}u takes", "_".repeat(31))), "{j}");
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
