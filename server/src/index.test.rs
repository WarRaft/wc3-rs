use super::*;
use crate::document::Language;
use crate::project::tests::TempDir;

/// A workspace with `files` (name, text) in one project, all on disk.
fn workspace(name: &str, files: &[(&str, &str)]) -> (TempDir, Workspace) {
    let tmp = TempDir::new(name);
    std::fs::write(tmp.0.join("wc3.bni"), "[code]\nsources=.\n").unwrap();
    for (file, text) in files {
        std::fs::write(tmp.0.join(file), text).unwrap();
    }
    let mut ws = Workspace::default();
    ws.set_roots(vec![tmp.0.clone()]);
    ws.take_reports();
    (tmp, ws)
}

fn id(tmp: &TempDir, file: &str) -> String {
    tmp.0.join(file).to_string_lossy().into_owned()
}

/// (file name, line, character) of each location.
fn places(answer: &Answer) -> Vec<(String, u32, u32)> {
    answer
        .locations
        .iter()
        .map(|l| {
            let file = std::path::Path::new(&l.id).file_name().unwrap().to_string_lossy().into_owned();
            (file, l.range.start.line, l.range.start.character)
        })
        .collect()
}

const COMMON: &str = "\
type unit extends handle
native GetUnitX takes unit whichUnit returns real
";

const MAIN: &str = "\
globals
    unit hero = null
endglobals
function Helper takes unit u returns real
    local real x = GetUnitX(u)
    return x
endfunction
function Main takes nothing returns nothing
    local real x = Helper(hero)
endfunction
";

fn ask(ws: &Workspace, tmp: &TempDir, kind: QueryKind, line: u32, character: u32) -> Answer {
    answer(ws, kind, &id(tmp, "main.j"), Position::new(line, character), "")
}

#[test]
fn definition_crosses_files() {
    let (tmp, ws) = workspace("index-def", &[("common.j", COMMON), ("main.j", MAIN)]);
    // `GetUnitX` in Helper's body → its native in common.j.
    let a = ask(&ws, &tmp, QueryKind::Definition, 4, 22);
    assert_eq!(places(&a), vec![("common.j".into(), 1, 7)]);
    assert_eq!(a.locations[0].kind, SymbolKind::Native);
    // `unit` (a type) in main.j → its `type` in common.j.
    let a = ask(&ws, &tmp, QueryKind::Definition, 1, 5);
    assert_eq!(places(&a), vec![("common.j".into(), 0, 5)]);
}

#[test]
fn locals_resolve_within_their_own_function() {
    let (tmp, ws) = workspace("index-locals", &[("main.j", MAIN)]);
    // `x` in Helper's `return x` → Helper's local, not Main's.
    let a = ask(&ws, &tmp, QueryKind::Definition, 5, 11);
    assert_eq!(places(&a), vec![("main.j".into(), 4, 15)]);
    let a = ask(&ws, &tmp, QueryKind::References, 5, 11);
    assert_eq!(places(&a), vec![("main.j".into(), 4, 15), ("main.j".into(), 5, 11)]);
}

#[test]
fn references_include_every_file_and_the_declaration() {
    let (tmp, ws) = workspace("index-refs", &[
        ("common.j", COMMON),
        ("main.j", MAIN),
        ("other.j", "function O takes nothing returns real\n    return GetUnitX(null)\nendfunction\n"),
    ]);
    let a = ask(&ws, &tmp, QueryKind::References, 4, 22);
    assert_eq!(
        places(&a),
        vec![("common.j".into(), 1, 7), ("main.j".into(), 4, 19), ("other.j".into(), 1, 11)]
    );
}

#[test]
fn hover_shows_the_declaration_line() {
    let (tmp, ws) = workspace("index-hover", &[("common.j", COMMON), ("main.j", MAIN)]);
    let a = ask(&ws, &tmp, QueryKind::Hover, 4, 22);
    assert_eq!(a.text, "```jass\nnative GetUnitX takes unit whichUnit returns real\n```\n*common.j:2*");

    // Undeclared: said so.
    let (tmp, ws) = workspace("index-hover-none", &[("main.j", MAIN)]);
    let a = ask(&ws, &tmp, QueryKind::Hover, 4, 22);
    assert!(a.text.contains("`GetUnitX`") && a.text.contains("not declared"));
}

#[test]
fn nothing_under_the_cursor_is_an_empty_answer() {
    let (tmp, ws) = workspace("index-empty", &[("main.j", MAIN)]);
    assert_eq!(ask(&ws, &tmp, QueryKind::Definition, 3, 0), Answer::default()); // `function`
    assert_eq!(answer(&ws, QueryKind::Definition, "nope", Position::new(0, 0), ""), Answer::default());
}

#[test]
fn document_symbols_are_the_files_project_wide_declarations() {
    let (tmp, ws) = workspace("index-doc", &[("main.j", MAIN)]);
    let a = ask(&ws, &tmp, QueryKind::DocumentSymbols, 0, 0);
    let names: Vec<_> = a.locations.iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        vec![("hero", SymbolKind::Global), ("Helper", SymbolKind::Function), ("Main", SymbolKind::Function)]
    );
}

#[test]
fn workspace_symbols_match_case_insensitively_across_files() {
    let (tmp, ws) = workspace("index-ws", &[("common.j", COMMON), ("main.j", MAIN)]);
    let a = answer(&ws, QueryKind::WorkspaceSymbols, "", Position::default(), "unit");
    let names: Vec<_> = a.locations.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["GetUnitX", "unit"]);
    let _ = tmp;
}

#[test]
fn open_documents_answer_from_the_editor_text() {
    let (tmp, mut ws) = workspace("index-open", &[("common.j", COMMON), ("main.j", MAIN)]);
    // In the editor, main.j now declares its own GetUnitX.
    let main = tmp.0.join("main.j");
    let text = format!("{MAIN}function GetUnitX takes unit u returns real\n    return 0.\nendfunction\n");
    ws.open(id(&tmp, "main.j"), Some(main), Language::Jass, 2, text);
    ws.take_reports();
    let a = ask(&ws, &tmp, QueryKind::Definition, 4, 22);
    assert_eq!(places(&a), vec![("common.j".into(), 1, 7), ("main.j".into(), 10, 9)]);
}

const ZINC: &str = "\
library Timers {
    public function StartTimer(real t) { Tick(); }
    function Tick() { }
    public struct Clock {
        real elapsed;
        method reset() { this.elapsed = 0.; }
    }
}
";

const USES_ZINC: &str = "\
function Go takes nothing returns nothing
    call StartTimer(1.)
    call Tick()
endfunction
";

fn ask_in(ws: &Workspace, tmp: &TempDir, file: &str, kind: QueryKind, line: u32, character: u32) -> Answer {
    answer(ws, kind, &id(tmp, file), Position::new(line, character), "")
}

#[test]
fn jass_reaches_public_zinc_but_not_private() {
    let (tmp, ws) = workspace("index-zinc-vis", &[("timers.zn", ZINC), ("go.j", USES_ZINC)]);
    // `StartTimer` in go.j → the public function in timers.zn.
    let a = ask_in(&ws, &tmp, "go.j", QueryKind::Definition, 1, 10);
    assert_eq!(places(&a), vec![("timers.zn".into(), 1, 20)]);
    // `Tick` is private to the library: go.j's `Tick` finds nothing...
    assert!(ask_in(&ws, &tmp, "go.j", QueryKind::Definition, 2, 10).locations.is_empty());
    // ...and the library's own call finds its declaration only.
    let a = ask_in(&ws, &tmp, "timers.zn", QueryKind::References, 2, 13);
    assert_eq!(places(&a), vec![("timers.zn".into(), 1, 41), ("timers.zn".into(), 2, 13)]);
}

#[test]
fn struct_members_resolve_by_name() {
    let (tmp, ws) = workspace("index-zinc-members", &[("timers.zn", ZINC)]);
    // `.elapsed` in reset() → the field.
    let a = ask_in(&ws, &tmp, "timers.zn", QueryKind::Definition, 5, 30);
    assert_eq!(places(&a), vec![("timers.zn".into(), 4, 13)]);
    assert_eq!(a.locations[0].kind, SymbolKind::Field);
}

#[test]
fn zinc_outline_lists_libraries_types_and_members() {
    let (tmp, ws) = workspace("index-zinc-outline", &[("timers.zn", ZINC)]);
    let a = ask_in(&ws, &tmp, "timers.zn", QueryKind::DocumentSymbols, 0, 0);
    let names: Vec<_> = a.locations.iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        vec![
            ("Timers", SymbolKind::Library),
            ("StartTimer", SymbolKind::Function),
            ("Tick", SymbolKind::Function),
            ("Clock", SymbolKind::Struct),
            ("elapsed", SymbolKind::Field),
            ("reset", SymbolKind::Method),
        ]
    );
}

#[test]
fn zinc_inside_vjass_is_indexed() {
    let vjass = format!("// @dialect vjass\n//! zinc\n{ZINC}//! endzinc\n");
    let (tmp, ws) = workspace("index-zinc-embedded", &[("map.j", &vjass), ("go.j", USES_ZINC)]);
    let a = ask_in(&ws, &tmp, "go.j", QueryKind::Definition, 1, 10);
    assert_eq!(places(&a), vec![("map.j".into(), 3, 20)]);
}

const LUA_UTIL: &str = "\
Util = {}
function Util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function StartGame()
  local t = CreateTimer()
  return Util.clamp(1, 0, 2)
end
";

const LUA_MAIN: &str = "\
function main()
  StartGame()
  local t = 0
end
";

#[test]
fn lua_globals_cross_files_and_reach_jass_natives() {
    let (tmp, ws) = workspace("index-lua", &[("common.j", COMMON_TIMER), ("util.lua", LUA_UTIL), ("main.lua", LUA_MAIN)]);
    // `StartGame` in main.lua → util.lua.
    let a = ask_in(&ws, &tmp, "main.lua", QueryKind::Definition, 1, 4);
    assert_eq!(places(&a), vec![("util.lua".into(), 2, 9)]);
    // `CreateTimer` in Lua → the JASS native.
    let a = ask_in(&ws, &tmp, "util.lua", QueryKind::Definition, 3, 14);
    assert_eq!(places(&a), vec![("common.j".into(), 0, 7)]);
    // Field `clamp`: defined by `function Util.clamp`.
    let a = ask_in(&ws, &tmp, "util.lua", QueryKind::Definition, 4, 16);
    assert_eq!(places(&a), vec![("util.lua".into(), 1, 14)]);
}

#[test]
fn lua_locals_stay_in_their_file_and_binding() {
    let (tmp, ws) = workspace("index-lua-locals", &[("util.lua", LUA_UTIL), ("main.lua", LUA_MAIN)]);
    // main.lua's `t` has nothing to do with util.lua's.
    let a = ask_in(&ws, &tmp, "main.lua", QueryKind::References, 2, 8);
    assert_eq!(places(&a), vec![("main.lua".into(), 2, 8)]);
    // `v` in clamp's body → its parameter.
    let a = ask_in(&ws, &tmp, "util.lua", QueryKind::Definition, 1, 65);
    assert_eq!(places(&a), vec![("util.lua".into(), 1, 20)]);
}

#[test]
fn lua_outline_lists_globals_functions_and_fields() {
    let (tmp, ws) = workspace("index-lua-outline", &[("util.lua", LUA_UTIL)]);
    let a = ask_in(&ws, &tmp, "util.lua", QueryKind::DocumentSymbols, 0, 0);
    let names: Vec<_> = a.locations.iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        vec![("Util", SymbolKind::Global), ("clamp", SymbolKind::Function), ("StartGame", SymbolKind::Function)]
    );
}

const COMMON_TIMER: &str = "native CreateTimer takes nothing returns timer\n";

const AS_UNIT: &str = "\
class Unit {
    int hp;
    void damage(int amount) { hp -= amount; }
}
enum Color { Red, Green }
";

const AS_MAIN: &str = "\
void main() {
    Unit u;
    u.damage(2);
    Color c = Red;
}
";

#[test]
fn angelscript_classes_members_and_enums_cross_files() {
    let (tmp, ws) = workspace("index-as", &[("unit.as", AS_UNIT), ("main.as", AS_MAIN)]);
    // `Unit` as a type in main.as → the class.
    let a = ask_in(&ws, &tmp, "main.as", QueryKind::Definition, 1, 5);
    assert_eq!(places(&a), vec![("unit.as".into(), 0, 6)]);
    assert_eq!(a.locations[0].kind, SymbolKind::Class);
    // `.damage` → the method.
    let a = ask_in(&ws, &tmp, "main.as", QueryKind::Definition, 2, 8);
    assert_eq!(places(&a), vec![("unit.as".into(), 2, 9)]);
    // `Red` → the enum member.
    let a = ask_in(&ws, &tmp, "main.as", QueryKind::Definition, 3, 15);
    assert_eq!(places(&a), vec![("unit.as".into(), 4, 13)]);
    assert_eq!(a.locations[0].kind, SymbolKind::EnumMember);
}

#[test]
fn angelscript_implicit_this_reaches_the_field() {
    let (tmp, ws) = workspace("index-as-this", &[("unit.as", AS_UNIT)]);
    // `hp` in damage's body (no `this.`) → the field.
    let a = ask_in(&ws, &tmp, "unit.as", QueryKind::Definition, 2, 30);
    assert_eq!(places(&a), vec![("unit.as".into(), 1, 8)]);
}

#[test]
fn angelscript_outline() {
    let (tmp, ws) = workspace("index-as-outline", &[("unit.as", AS_UNIT)]);
    let a = ask_in(&ws, &tmp, "unit.as", QueryKind::DocumentSymbols, 0, 0);
    let names: Vec<_> = a.locations.iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        vec![
            ("Unit", SymbolKind::Class),
            ("hp", SymbolKind::Field),
            ("damage", SymbolKind::Method),
            ("Color", SymbolKind::Enum),
            ("Red", SymbolKind::EnumMember),
            ("Green", SymbolKind::EnumMember),
        ]
    );
}

const AS_NS: &str = "\
namespace Game {
    class Unit { }
    namespace AI {
        class Unit { }
        void think() { Unit@ mine; Game::Unit@ outer; ::Spawn(); }
    }
    void play() { Unit@ u; AI::Unit@ bot; }
}
void Spawn() { Game::Unit@ a; Game::AI::Unit@ b; }
";

/// (line, character) of each location in `file`.
fn lines(answer: &Answer) -> Vec<(u32, u32)> {
    answer.locations.iter().map(|l| (l.range.start.line, l.range.start.character)).collect()
}

#[test]
fn angelscript_names_resolve_in_their_namespace() {
    let (tmp, ws) = workspace("index-as-ns", &[("ns.as", AS_NS)]);
    // `Unit` inside AI → AI's own class, not Game's.
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::Definition, 4, 24);
    assert_eq!(lines(&a), vec![(3, 14)]);
    assert_eq!(a.locations[0].container, "Game::AI");
    // `Game::Unit` inside AI → the outer class.
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::Definition, 4, 43);
    assert_eq!(lines(&a), vec![(1, 10)]);
    // References of Game's `Unit`: its declaration, `Game::Unit` in AI,
    // plain `Unit` in Game::play, `Game::Unit` at the top level.
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::References, 1, 11);
    assert_eq!(lines(&a), vec![(1, 10), (4, 41), (6, 18), (8, 21)]);
    // ... and of AI's: `Unit` in AI, `AI::Unit` in Game, the full path.
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::References, 3, 15);
    assert_eq!(lines(&a), vec![(3, 14), (4, 23), (6, 31), (8, 40)]);
    // `::Spawn` → the global function.
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::Definition, 4, 60);
    assert_eq!(lines(&a), vec![(8, 5)]);
}

#[test]
fn angelscript_hover_and_outline_show_the_namespace() {
    let (tmp, ws) = workspace("index-as-ns-hover", &[("ns.as", AS_NS)]);
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::Hover, 4, 24);
    assert!(a.text.ends_with("*ns.as:4, in `Game::AI`*"), "{}", a.text);
    let a = ask_in(&ws, &tmp, "ns.as", QueryKind::DocumentSymbols, 0, 0);
    let outline: Vec<_> = a.locations.iter().map(|l| (l.name.as_str(), l.container.as_str())).collect();
    assert_eq!(
        outline,
        vec![
            ("Game", ""),
            ("Unit", "Game"),
            ("AI", "Game"),
            ("Unit", "Game::AI"),
            ("think", "Game::AI"),
            ("play", "Game"),
            ("Spawn", ""),
        ]
    );
}

#[test]
fn angelscript_namespaces_split_across_files_are_one() {
    let a = "namespace Game { void Start() { } }\n";
    let b = "namespace Game { void Run() { Start(); } }\nvoid main() { Game::Start(); }\n";
    let (tmp, ws) = workspace("index-as-ns-files", &[("a.as", a), ("b.as", b)]);
    let r = ask_in(&ws, &tmp, "a.as", QueryKind::References, 0, 23);
    assert_eq!(places(&r), vec![("a.as".into(), 0, 22), ("b.as".into(), 0, 30), ("b.as".into(), 1, 20)]);
    // `Game` itself: declared in both files.
    let r = ask_in(&ws, &tmp, "b.as", QueryKind::Definition, 1, 15);
    assert_eq!(places(&r), vec![("a.as".into(), 0, 10), ("b.as".into(), 0, 10)]);
}
