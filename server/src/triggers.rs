//! Triggers (`war3map.wtg`) for the editor: shown as their BNI text form
//! (`wtg-rs`: categories, variables, and each trigger's events,
//! conditions and actions as lines of calls) and saved back to bytes.
//!
//! How many parameters each function takes isn't in the file: it's World
//! Editor's `UI\TriggerData.txt`, read through the project's lookup (its
//! game folders, then its map's roots, the last found winning: a modded
//! editor's data in a root replaces the game's). Opening needs it; saving
//! checks every function against it. While typing, the text is checked
//! without it (its syntax and fields). A value is the rest of its line as
//! it is (strings as JASS writes them), highlighted by what it is
//! (`wtg_rs::text::semantic_tokens`).

use std::collections::HashMap;
use std::path::Path;

use wtg_rs::TriggerData;
use wtg_rs::text::{Declared, Scope};

use crate::objects::{TO_BYTES, TO_TEXT};

/// Is `path` a trigger file (`.wtg`), by its extension?
pub fn is_trigger_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wtg"))
}

/// Is `path` a trigger of its own (`.trg`), by its extension?
pub fn is_trg_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("trg"))
}

/// Is `path` text whose values are calls, read raw (`.wtg`'s text form,
/// a `.trg`)?
pub fn has_call_values(path: &Path) -> bool {
    is_trigger_file(path) || is_trg_file(path)
}

const NO_DATA: &str = "no UI\\TriggerData.txt in the project's game folders or map roots: it says how many parameters each function takes, which the file doesn't";

/// Converts for the editor: the bytes of the file `name` to its text form
/// (`TO_TEXT`), or a text form to its bytes (`TO_BYTES`), with `data`.
/// The error is a message to show.
pub fn convert(op: u8, name: &str, bytes: &[u8], data: &TriggerData) -> Result<Vec<u8>, String> {
    let file = Path::new(name).file_name().map_or(name.into(), |n| n.to_string_lossy());
    match op {
        TO_TEXT => {
            if data.is_empty() {
                return Err(format!("Can't read {file}: {NO_DATA}"));
            }
            // Either layout: 1.31's tree as the same text and what it adds.
            let f = wtg_rs::parse_any(bytes, data).map_err(|e| format!("Can't read {file}: {e}"))?;
            wtg_rs::text::to_text_any(&f).map(String::into_bytes).map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(bytes).map_err(|_| "the text isn't UTF-8".to_string())?;
            let p = wtg_rs::text::from_text(text, (!data.is_empty()).then_some(data));
            let Some(f) = p.to_file() else {
                let e = &p.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
            if let Some(i) = script_lines(text).first() {
                return Err(format!("Can't save {file}: line {}: {ONE_LINE}", i + 1));
            }
            f.to_bytes().map_err(|e| format!("Can't save {file}: {e}"))
        }
        _ => Err(format!("unknown op {op}")),
    }
}

/// What would stop the text form at `path` from saving, past its BNI
/// errors and past what `UI\TriggerData.txt` says; for a trigger of its
/// own (`.trg`), what's wrong with it. Nothing for other BNI.
pub fn diagnostics(path: Option<&Path>, doc: &bni_rs::Document, text: &str) -> Vec<parser_rs::ParseError> {
    let bni = &doc.errors;
    if path.is_some_and(is_trg_file) {
        return wtg_rs::text::trigger_from_text(text, None).errors.into_iter().filter(|e| !bni.contains(e)).collect();
    }
    if !path.is_some_and(is_trigger_file) || !wtg_rs::text::is_trigger_text(doc) {
        return Vec::new();
    }
    let mut errors: Vec<parser_rs::ParseError> = wtg_rs::text::from_bni(doc, text, None).errors.into_iter().filter(|e| !bni.contains(e)).collect();
    for i in script_lines(text) {
        let width = text.lines().nth(i).map_or(1, |l| l.chars().count());
        errors.push(parser_rs::ParseError { message: ONE_LINE.into(), line: i + 1, col: 1, end_col: width + 1 });
    }
    errors
}

/// Why custom script of more lines isn't in a map's triggers.
const ONE_LINE: &str = "custom script in a map's triggers (war3map.wtg) is one line, as World Editor's: more lines are a trigger file's (.trg)";

/// The lines (from 0) of the text `text` whose ECA is custom script of
/// more than one line (`CustomScriptCode` with a line break in its
/// string): a trigger file of its own (`.trg`) has them, World Editor's
/// triggers don't.
fn script_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| eca_line(l).is_some_and(|(_, n)| n == "CustomScriptCode"))
        .filter(|(_, l)| {
            let (_, rest) = l.split_once('=').expect("an ECA's line");
            wtg_rs::text::read_call(rest.trim()).is_ok_and(|(_, params)| params.iter().any(|p| p.kind == 3 && p.value.contains(['\n', '\r'])))
        })
        .map(|(i, _)| i)
        .collect()
}

/// The triggers of the text form `text` (of a `.wtg`) as files, as JSON:
/// `{"folders": ["Init", ...], "files": [["Init/Spawn.trg", "[trigger]..."],
/// ["_variables.trg", "[variable]..."]]}` (`wtg_rs::text::to_files`: a
/// folder per category, a `.trg` per trigger, the variables at the top),
/// or `{"error": "..."}` when the text has errors.
///
/// With the map's custom script beside it (`wct`, its text form), each
/// trigger of custom text's code goes with it, as code of the project:
/// beside its `.trg` (`Spawn.j`), read as vJASS (a `// @dialect` line
/// first), standing for its piece of `war3map.wct` in a build; a trigger
/// that's off keeps its code commented out, as World Editor leaves it out
/// of the map's script (it's often no code). The map's own custom script
/// stays the map's, `war3map.wct` (a build reads it, wherever the
/// triggers are).
pub fn extract(text: &str, wct: Option<&str>) -> String {
    let js = crate::objects::json_string;
    let p = wtg_rs::text::from_text(text, None);
    let result = match &p.file {
        Some(f) => extract_files(f, wct),
        None => Err(p.errors.first().map_or("the text has errors".into(), |e| format!("line {}: {}", e.line, e.message))),
    };
    match result {
        Ok(made) => {
            let folders: Vec<String> = made.folders.iter().map(|f| js(f)).collect();
            let files: Vec<String> = made.files.iter().map(|(p, t)| format!("[{},{}]", js(p), js(t))).collect();
            format!("{{\"folders\":[{}],\"files\":[{}]}}", folders.join(","), files.join(","))
        }
        Err(e) => format!("{{\"error\":{}}}", js(&format!("Can't extract the triggers: {e}"))),
    }
}

/// The files [`extract`] makes of the triggers `f` and the text form of
/// the map's custom script beside them (`wct`).
pub fn extract_files(f: &wtg_rs::Triggers, wct: Option<&str>) -> Result<wtg_rs::text::Files, String> {
    let mut made = wtg_rs::text::to_files(f)?;
    let Some(wct) = wct else { return Ok(made) };
    let comments: Vec<bool> = f.triggers.iter().map(|t| t.is_comment != 0).collect();
    let pieces = crate::customtext::pieces(wct).of_triggers(&comments)?;
    let lines: Vec<&str> = wct.split_inclusive('\n').collect();
    let code = |from: usize, to: usize| lines[from..to].concat();
    let head = "// @dialect vjass\n";
    let mut out = Vec::new();
    // The triggers' files follow the variables', in their order.
    let first = made.files.len() - f.triggers.len();
    for (k, (t, piece)) in f.triggers.iter().zip(&pieces).enumerate() {
        let Some((marker, end, _)) = piece else { continue };
        if t.is_custom == 0 || t.is_comment != 0 {
            continue;
        }
        let path = made.files[first + k].0.trim_end_matches(".trg").to_string() + ".j";
        let body = code(marker + 1, *end);
        let body = if t.enabled != 0 {
            body
        } else {
            format!("// The trigger is off: its code is left out of the map's script.\n{}", body.lines().map(|l| format!("// {l}\n")).collect::<String>())
        };
        out.push((path, format!("{head}{body}")));
    }
    made.files.extend(out);
    Ok(made)
}

// ---------------------------------------------------- a trigger of its own
//
// A `.trg` is one trigger as text (`wtg_rs::text::trigger_from_text`),
// and opens as World Editor shows it: the view is the server's JSON of
// the text ([`view`]), and what's done in it comes back as edits of the
// text ([`edit`]), which the editor applies like any other: the text
// stays the document. Edits are of whole lines: an ECA and what's inside
// it (the lines below it with more `>`) move together.

/// The view of the trigger `text`, as JSON:
///
/// ```json
/// {"trigger": true, "variables": [{"line": 1, "name": "hero", "type": "unit",
///  "type_name": "Unit", "array": true, "size": 12, "initial": null}],
///  "name": "Spawn", "description": "", "enabled": 1, "initially_off": 0,
///  "run_on_init": 0, "custom_text": 0, "comment": 0, "data": true,
///  "errors": [[3, "`action`: ..."]],
///  "labels": ["Events", "Conditions", "Actions", "Variables"],
///  "events": [node], "conditions": [...], "actions": [...]}
/// ```
///
/// A node: `{"line": 4, "kind": 2, "enabled": true, "name": "CreateNUnitsAtLoc",
/// "known": true, "icon": "ReplaceableTextures\\...", "hint": "...",
/// "text": [span], "blocks": [{"label": "Then - Actions", "group": 1,
/// "kind": 2, "nodes": [...]}]}`; a span a string, or a value
/// `{"r": "p", "p": "2.0", "s": [span]}` (`r`: `p` preset, `v` variable,
/// `l` a value typed in, `c` a call, `u` unknown, and a variable
/// `globals` doesn't have, with an error; `p` its path, see
/// `wtg_rs::ui::Span::Value`). `data` is whether the editor's files
/// were found; the errors are checked against them when they were, but
/// the tree isn't: a function they don't have shows, as unknown.
pub fn view(text: &str, names: &crate::names::Names, visible: &[Declared], triggers: &Triggers) -> String {
    use wtg_rs::ui::Ui;
    let data = (!names.triggers.is_empty()).then_some(&names.triggers);
    let mut p = wtg_rs::text::trigger_from_text(text, None);
    if data.is_some() {
        p.errors = wtg_rs::text::trigger_from_text(text, data).errors;
    }
    let ui: &Ui = &names.trigger_ui;
    let has_trigger = p.trigger.is_some();
    let t = p.trigger.clone().unwrap_or_default();
    let mut v = ui.render(&t, &p.lines);
    let js = crate::objects::json_string;
    let mut errors: Vec<(usize, String)> = p.errors.iter().map(|e| (e.line, e.message.clone())).collect();
    // A variable no trigger file of the project declares, or a local
    // where it isn't seen: unknown.
    let seen = Seen::of(visible, triggers);
    undeclared(&mut v.events, &seen, false, &mut errors);
    undeclared(&mut v.conditions, &seen, false, &mut errors);
    undeclared(&mut v.actions, &seen, true, &mut errors);
    // Named as another: World Editor's name of it may mean either.
    let alike = triggers.named_alike(&triggers.me);
    if !alike.is_empty() {
        let line = text.lines().position(|l| l.trim_start().starts_with("name=")).map_or(1, |i| i + 1);
        let files: Vec<String> = alike.iter().map(|f| file_name(f)).collect();
        errors.push((line, format!(
            "the map's script would name {} gg_trg_{} too: rename one",
            files.join(", "),
            triggers.mine
        )));
    }
    errors.sort_by_key(|e| e.0);
    let errors: Vec<String> = errors.iter().map(|(line, m)| format!("[{line},{}]", js(m))).collect();
    let labels = ["WESTRING_EVENTS", "WESTRING_CONDITIONS", "WESTRING_ACTIONS", "WESTRING_VARIABLES"].map(|k| js(&ui.label(k)));
    // Custom script read as the trigger's code it's part of.
    let code = script_code(&t, &p.lines, &p.variables, visible, ui);
    let nodes = |n: &[wtg_rs::ui::Node]| format!("[{}]", n.iter().map(|n| node_json(n, &code)).collect::<Vec<_>>().join(","));
    // Each variable with the line of its section.
    let doc = wtg_rs::text::parse_bni(text);
    let var_lines = doc.sections.iter().filter(|s| s.name.text == wtg_rs::text::VARIABLE).map(|s| s.line);
    let variables: Vec<String> = p
        .variables
        .iter()
        .zip(var_lines)
        .map(|(d, line)| {
            let var = &d.variable;
            let type_name = ui.types.get(&var.kind).cloned().unwrap_or_else(|| var.kind.clone());
            format!(
                "{{\"line\":{line},\"scope\":\"{}\",\"name\":{},\"type\":{},\"type_name\":{},\"array\":{},\"size\":{},\"initial\":{}}}",
                d.scope.word().unwrap_or("global"),
                js(&var.name),
                js(&var.kind),
                js(&type_name),
                var.is_array != 0,
                var.array_size,
                if var.has_initial != 0 { js(&var.initial) } else { "null".into() },
            )
        })
        .collect();
    format!(
        "{{\"trigger\":{has_trigger},\"script\":{},\"variables\":[{}],\"name\":{},\"description\":{},\"enabled\":{},\"initially_off\":{},\"run_on_init\":{},\"custom_text\":{},\"comment\":{},\"data\":{},\"errors\":[{}],\"labels\":[{}],\"events\":{},\"conditions\":{},\"actions\":{}}}",
        js(&format!("gg_trg_{}", triggers.mine)),
        variables.join(","),
        js(&t.name),
        js(&t.description),
        t.enabled,
        t.initially_off,
        t.run_on_init,
        t.is_custom,
        t.is_comment,
        data.is_some(),
        errors.join(","),
        labels.join(","),
        nodes(&v.events),
        nodes(&v.conditions),
        nodes(&v.actions),
    )
}

/// The names of the variables a trigger file sees: the globals (the
/// project's and its own private ones), and its locals.
struct Seen<'a> {
    globals: std::collections::HashSet<&'a str>,
    locals: std::collections::HashSet<&'a str>,
    triggers: &'a Triggers,
}

impl<'a> Seen<'a> {
    fn of(visible: &'a [Declared], triggers: &'a Triggers) -> Seen<'a> {
        let names = |local: bool| visible.iter().filter(|d| (d.scope == Scope::Local) == local).map(|d| d.variable.name.as_str()).collect();
        Seen { globals: names(false), locals: names(true), triggers }
    }

    /// What's wrong with a reference to a trigger (`gg_trg_...`): it means
    /// none of the project's, or several.
    fn trigger_problem(&self, name: &str) -> Option<String> {
        let files = |f: &[&str]| f.iter().map(|id| file_name(id)).collect::<Vec<_>>().join(", ");
        match &self.triggers.resolve(name)[..] {
            [_] => None,
            [] => Some(format!("no trigger of the project is named `{name}`")),
            many => Some(format!(
                "`{name}` may mean {} triggers ({}): rename one",
                many.len(),
                files(many)
            )),
        }
    }
}

/// Marks the variables of `nodes` (and what's inside them) that aren't
/// seen unknown, each with an error at its line. `locals`: are the
/// trigger's locals seen here (in its actions, every block inside
/// included)?
fn undeclared(nodes: &mut [wtg_rs::ui::Node], seen: &Seen, locals: bool, errors: &mut Vec<(usize, String)>) {
    use wtg_rs::ui::{Role, Span};
    fn walk(spans: &mut [Span], line: usize, seen: &Seen, locals: bool, errors: &mut Vec<(usize, String)>) {
        for s in spans {
            if let Span::Value { role, spans, .. } = s {
                if *role == Role::Variable
                    && let Some(Span::Text(name)) = spans.first()
                    && name.starts_with("gg_trg_")
                    && let Some(problem) = seen.trigger_problem(name)
                {
                    errors.push((line, problem));
                    *role = Role::Unknown;
                } else if *role == Role::Variable
                    && let Some(Span::Text(name)) = spans.first()
                    && !name.starts_with("gg_")
                    && !seen.globals.contains(name.as_str())
                    && !(locals && seen.locals.contains(name.as_str()))
                {
                    let message = if seen.locals.contains(name.as_str()) {
                        format!("the local `{name}` isn't seen here: a trigger's locals are its actions'")
                    } else {
                        format!("the variable `{name}` isn't declared in the project's trigger files ([variable])")
                    };
                    errors.push((line, message));
                    *role = Role::Unknown;
                }
                walk(spans, line, seen, locals, errors);
            }
        }
    }
    for n in nodes {
        walk(&mut n.text, n.line, seen, locals, errors);
        for b in &mut n.blocks {
            undeclared(&mut b.nodes, seen, locals, errors);
        }
    }
}

/// Does the ECA at index `i` see its trigger's locals: is it in the
/// actions (its top line an action), whatever block it's in?
fn sees_locals(doc: &Doc, mut i: usize) -> bool {
    loop {
        let Some((key, _)) = doc.eca(i) else { return false };
        if key.depth == 0 {
            return key.kind == 2;
        }
        match (doc.section.start..i).rev().find(|&j| doc.eca(j).is_some_and(|(k, _)| k.depth + 1 == key.depth)) {
            Some(p) => i = p,
            None => return false,
        }
    }
}

fn node_json(n: &wtg_rs::ui::Node, code: &HashMap<usize, ScriptCode>) -> String {
    let js = crate::objects::json_string;
    let blocks: Vec<String> = n
        .blocks
        .iter()
        .map(|b| {
            let nodes: Vec<String> = b.nodes.iter().map(|n| node_json(n, code)).collect();
            format!("{{\"label\":{},\"group\":{},\"kind\":{},\"nodes\":[{}]}}", js(&b.label), b.group, b.kind, nodes.join(","))
        })
        .collect();
    // A custom script's: its tokens, `[line, start, end, "type mods"]`
    // (its lines from 0, columns in characters from 0), and its errors.
    let script = code.get(&n.line).map_or(String::new(), |c| {
        let tokens: Vec<String> = c.tokens.iter().map(|(k, a, b, ty)| format!("[{k},{a},{b},{}]", js(ty))).collect();
        let errors: Vec<String> = c.errors.iter().map(|(k, m)| format!("[{k},{}]", js(m))).collect();
        format!(",\"code\":{{\"tokens\":[{}],\"errors\":[{}]}}", tokens.join(","), errors.join(","))
    });
    format!(
        "{{\"line\":{},\"kind\":{},\"enabled\":{},\"name\":{},\"known\":{},\"icon\":{},\"hint\":{},\"text\":{},\"blocks\":[{}]{script}}}",
        n.line,
        n.kind,
        n.enabled,
        js(&n.name),
        n.known,
        js(&n.icon),
        js(&n.hint),
        spans_json(&n.text),
        blocks.join(",")
    )
}

/// A custom script action's code (`CustomScriptCode`) as the trigger's
/// code reads it: highlighting tokens of each of its lines, and the errors
/// on them.
#[derive(Debug, Default, PartialEq)]
pub struct ScriptCode {
    /// Line of the code (from 0), start and end column (characters, from
    /// 0), the token's type and modifiers (`"function defaultLibrary"`).
    pub tokens: Vec<(usize, usize, usize, String)>,
    /// Line of the code (from 0) and what's wrong there.
    pub errors: Vec<(usize, String)>,
}

/// The marker a line of custom script is built with, to be found in the
/// trigger's JASS: `// @cs <line of the action> <line of its code>`.
const SCRIPT_MARK: &str = "// @cs ";

/// Each custom script action of trigger `t` (by its line in the text,
/// `lines` the line of each ECA as a walk of the tree meets them), read
/// as part of the trigger's code: the trigger built into JASS
/// (`wtg_rs::jass::compile`, the build's), each line of custom script
/// marked, so a construct over several of them (an `if` in one, its
/// `endif` in another, the trigger's actions between) and the locals one
/// declares for the others read as they will. A line the build changes
/// (a local declared again) or leaves out (a disabled action) is read
/// alone.
pub fn script_code(t: &wtg_rs::Trigger, lines: &[usize], own: &[Declared], visible: &[Declared], ui: &wtg_rs::ui::Ui) -> HashMap<usize, ScriptCode> {
    fn mark(ecas: &mut [wtg_rs::Eca], lines: &[usize], at: &mut usize, code: &mut Vec<(usize, Vec<String>)>) {
        for e in ecas {
            let line = lines.get(*at).copied().unwrap_or(0);
            *at += 1;
            if e.name == "CustomScriptCode"
                && let Some(p) = e.params.first_mut()
            {
                let source: Vec<String> = p.value.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
                p.value = source.iter().enumerate().map(|(k, l)| if l.trim().is_empty() { l.clone() } else { format!("{l} {SCRIPT_MARK}{line} {k}") }).collect::<Vec<_>>().join("\n");
                code.push((line, source));
            }
            mark(&mut e.children, lines, at, code);
        }
    }
    let mut marked = t.clone();
    let mut code = Vec::new();
    mark(&mut marked.ecas, lines, &mut 0, &mut code);
    if code.is_empty() {
        return HashMap::new();
    }
    let built = wtg_rs::jass::compile(Some(&marked), "t", own, visible, ui);
    // Each built line's place: the action's line, its code's line, how
    // far right it's moved; the JASS without the markers.
    let mut places: HashMap<usize, (usize, usize, usize)> = HashMap::new();
    let mut jass = String::new();
    let source = |line: usize, k: usize| code.iter().find(|(l, _)| *l == line).and_then(|(_, s)| s.get(k));
    for (i, l) in built.lines().enumerate() {
        let found = l.rfind(SCRIPT_MARK).and_then(|at| {
            let mut words = l[at + SCRIPT_MARK.len()..].split_whitespace().map(|w| w.parse::<usize>().ok());
            let (line, k) = (words.next()??, words.next()??);
            Some((at, line, k))
        });
        match found {
            Some((at, line, k)) => {
                let text = l[..at].trim_end();
                // As written (moved right), not as the build changed it.
                if let Some(src) = source(line, k).map(|s| s.trim_end()).filter(|s| text.ends_with(*s) && text[..text.len() - s.len()].trim().is_empty()) {
                    places.insert(i + 1, (line, k, text[..text.len() - src.len()].chars().count()));
                }
                jass += text;
            }
            None => jass += l,
        }
        jass += "\n";
    }
    let tokens = jass_rs::semantic::semantic_tokens_with(&jass, jass_rs::Dialect::Jass);
    let first_error = jass_rs::parse(&jass).err();
    let mut out: HashMap<usize, ScriptCode> = HashMap::new();
    let ty = |t: &parser_rs::SemanticToken| {
        let mods: Vec<&str> = parser_rs::SemanticModifier::ALL.iter().filter(|m| t.mods.contains(**m)).map(|m| m.as_str()).collect();
        std::iter::once(t.ty.as_str()).chain(mods).collect::<Vec<_>>().join(" ")
    };
    for t in tokens.iter().filter(|t| t.line == t.end_line) {
        if let Some(&(line, k, shift)) = places.get(&t.line) {
            let c = out.entry(line).or_default();
            c.tokens.push((k, t.start_col.saturating_sub(1 + shift), t.end_col.saturating_sub(1 + shift), ty(t)));
        }
    }
    // The trigger's own code reads: what doesn't is its custom script's,
    // the nearest line of it at or above (a missing `endif` is found at
    // `endfunction`).
    if let Some(e) = first_error
        && let Some((_, &(line, k, _))) = places.iter().filter(|(l, _)| **l <= e.line).max_by_key(|(l, _)| **l)
    {
        out.entry(line).or_default().errors.push((k, e.message));
    }
    // Lines the built code hasn't as written: read alone.
    for (line, src) in &code {
        for (k, l) in src.iter().enumerate() {
            if l.trim().is_empty() || places.values().any(|p| p.0 == *line && p.1 == k) {
                continue;
            }
            let c = out.entry(*line).or_default();
            for t in jass_rs::semantic::semantic_tokens_with(l, jass_rs::Dialect::Jass).iter().filter(|t| t.line == 1 && t.end_line == 1) {
                c.tokens.push((k, t.start_col - 1, t.end_col - 1, ty(t)));
            }
        }
    }
    for c in out.values_mut() {
        c.tokens.sort();
    }
    out
}

fn spans_json(spans: &[wtg_rs::ui::Span]) -> String {
    use wtg_rs::ui::{Role, Span};
    let items: Vec<String> = spans
        .iter()
        .map(|s| match s {
            Span::Text(t) => crate::objects::json_string(t),
            Span::Value { role, path, spans } => {
                let r = match role {
                    Role::Preset => "p",
                    Role::Variable => "v",
                    Role::Value => "l",
                    Role::Call => "c",
                    Role::Unknown => "u",
                };
                // What it highlights as, in the words of code's tokens (the
                // custom script's too: one way of coloring both).
                let text: String = spans.iter().map(|s| if let Span::Text(t) = s { t.as_str() } else { "" }).collect();
                let t = match role {
                    Role::Variable => "variable",
                    Role::Preset => "enumMember",
                    Role::Value if text.trim().parse::<f64>().is_ok() => "number",
                    Role::Value => "string",
                    Role::Call | Role::Unknown => "",
                };
                format!("{{\"r\":\"{r}\",\"t\":\"{t}\",\"p\":{},\"s\":{}}}", crate::objects::json_string(path), spans_json(spans))
            }
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// The operations of [`edit`].
pub mod op {
    /// Turn the ECA at the line on or off (`!`); with the argument, lines
    /// (from 1, separated by spaces) of more ECAs, turned as it is.
    pub const TOGGLE: u32 = 0;
    /// Delete the ECA at the line, and what's inside it (and the
    /// argument's, as for `TOGGLE`); or the variable whose `[variable]`
    /// is at the line.
    pub const DELETE: u32 = 1;
    /// Move the ECA at the line, and what's inside it, to the place the
    /// argument says: `parent group index` (see [`super::edit`]); after a
    /// line break, lines of more ECAs to move with it, as for `TOGGLE`,
    /// which go there in the text's order.
    pub const MOVE: u32 = 2;
    /// Insert the lines after the argument's first line (a place, as for
    /// `MOVE`): ECAs, each at the top (no `>`, no block), with what's
    /// inside them, as `COPY` gives them.
    pub const PASTE: u32 = 3;
    /// The lines of the ECA at the line and what's inside it, as if at the
    /// top (and the argument's, as for `TOGGLE`, in the text's order):
    /// the answer's `text`.
    pub const COPY: u32 = 4;
    /// Set a field of the trigger: the argument is its name, a line break,
    /// its value (`name` and `description` as typed; the flags a number).
    /// A flag set to its default goes, and so does an empty name (the
    /// trigger's is then its file's) or description.
    pub const FIELD: u32 = 5;
    /// Set a parameter of the ECA at the line: the argument is its path
    /// (`wtg_rs::ui::Span::Value`), a line break, what it becomes
    /// (`preset`, `variable`, `array` for an array variable, `call`,
    /// `value`), a line break, its key, name or value. A call gets its
    /// default parameters, an array variable index 0.
    pub const SET_PARAM: u32 = 6;
    /// A new ECA: the argument is a place (as for `MOVE`), a line break,
    /// its kind (0 event, 1 condition, 2 action) and function name; its
    /// parameters are the function's defaults.
    pub const NEW: u32 = 7;
    /// A new variable: the argument is its name, type, `1` for an array,
    /// its size, its scope (`global`, `private`, `local`; see
    /// `wtg_rs::text::Scope`), and (if any) its initial value, a line
    /// each. It goes after the file's last variable, or before its
    /// trigger; a local brings the trigger when there's none.
    pub const NEW_VARIABLE: u32 = 8;
    /// The ECA at the line becomes a call of another function, the
    /// argument (of its kind), with its default parameters; it keeps its
    /// place and whether it's on. What's inside it stays in the blocks the
    /// new function has (by block and kind), the rest goes.
    pub const REPLACE: u32 = 9;
    /// The variable whose `[variable]` is at the line becomes what the
    /// argument says, as for `NEW_VARIABLE`. Renamed, its uses are
    /// renamed too, in every trigger file that sees it (the answer's
    /// `files`). Refused when its new name or scope would clash with
    /// another variable. See [`super::variable_change`].
    pub const SET_VARIABLE: u32 = 10;
    /// The trigger was renamed from the argument (its name before; empty
    /// for none): the references to it in the project that said its old
    /// name in the script say its new one (the answer's `files` for other
    /// files). Refused, changing nothing, when the script would name
    /// another trigger as it now. See [`super::retarget_project`].
    pub const RENAMED: u32 = 12;
    /// What changing the variable whose `[variable]` is at the line, as
    /// `SET_VARIABLE`'s argument says, or deleting it (an empty
    /// argument), would do, changing nothing: the answer is
    /// `{"renamed": n, "places": [[id, line, why], ...]}`, the uses
    /// renamed and the places that would need fixing by hand.
    pub const CHECK_VARIABLE: u32 = 11;
}

const KINDS: [&str; 3] = ["event", "condition", "action"];

/// What an ECA line's key says.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Key {
    depth: usize,
    enabled: bool,
    kind: u32,
    group: Option<u32>,
}

impl Key {
    fn text(&self) -> String {
        let off = if self.enabled { "" } else { "!" };
        let group = self.group.map(|g| format!(":{g}")).unwrap_or_default();
        format!("{}{off}{}{group}", ">".repeat(self.depth), KINDS[self.kind as usize])
    }
}

/// A line's key and function name, when it's an ECA's.
fn eca_line(line: &str) -> Option<(Key, &str)> {
    let (key, value) = line.split_once('=')?;
    let (depth, enabled, kind, group) = wtg_rs::text::eca_key(key.trim())?;
    let name = value.split('(').next().unwrap_or("").trim();
    Some((Key { depth, enabled: enabled != 0, kind, group }, name))
}

/// `line` with its key changed by `f` (an ECA's line: `None` otherwise).
fn rekey(line: &str, f: impl FnOnce(&mut Key)) -> Option<String> {
    let (mut key, _) = eca_line(line)?;
    let (_, rest) = line.split_once('=')?;
    f(&mut key);
    Some(format!("{}={rest}", key.text()))
}

/// A blank line or a comment: it belongs to no ECA.
fn filler(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with("//") || t.starts_with(';')
}

/// The text as lines, each with its line break (the last may have none).
struct Doc {
    lines: Vec<String>,
    eol: &'static str,
    /// The lines of the `[trigger]` section, past its header (empty at
    /// the end when there's none).
    section: std::ops::Range<usize>,
    /// Is there a `[trigger]`?
    has_section: bool,
}

/// Where in a trigger: under `parent` (the line, from 1, of the ECA; 0 for
/// the top), in its block `group` (at the top, the kind: 0 events,
/// 1 conditions, 2 actions), before what is now at `index` in it (past
/// the end: at the end).
#[derive(Debug, Clone, Copy)]
struct Place {
    parent: usize,
    group: u32,
    index: usize,
}

impl Place {
    fn parse(s: &str) -> Result<Place, String> {
        let n: Vec<usize> = s.split_whitespace().map(|w| w.parse().map_err(|_| format!("not a place: {s:?}"))).collect::<Result<_, _>>()?;
        match n[..] {
            [parent, group, index] => Ok(Place { parent, group: group as u32, index }),
            _ => Err(format!("not a place: {s:?}")),
        }
    }
}

impl Doc {
    fn new(text: &str) -> Doc {
        let lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let header = |l: &String| l.trim_start().starts_with('[');
        let found = lines.iter().position(|l| l.trim().strip_prefix('[').is_some_and(|r| r.starts_with("trigger]")));
        let start = found.map_or(lines.len(), |i| i + 1);
        let end = lines[start..].iter().position(header).map_or(lines.len(), |i| start + i);
        Doc { lines, eol, section: start..end, has_section: found.is_some() }
    }

    /// Adds a `[trigger]` at the end when there's none: a file of
    /// variables, or a new one, gets its trigger with the first thing put
    /// in it.
    fn ensure_trigger(&mut self) {
        if self.has_section {
            return;
        }
        if let Some(last) = self.lines.last_mut()
            && !last.ends_with('\n')
        {
            last.push_str(self.eol);
        }
        if self.lines.last().is_some_and(|l| !filler(l)) {
            self.lines.push(self.eol.to_string());
        }
        self.lines.push(format!("[{}]{}", wtg_rs::text::TRIGGER, self.eol));
        let eol = self.eol;
        *self = Doc::new(&self.lines.concat());
        self.eol = eol;
    }

    /// The `[variable]` section whose header is at `line` (from 1): its
    /// lines.
    fn variable_at(&self, line: usize) -> Option<std::ops::Range<usize>> {
        let i = line.checked_sub(1)?;
        let is_var = |l: &str| l.trim().strip_prefix('[').is_some_and(|r| r.starts_with("variable]"));
        if !self.lines.get(i).is_some_and(|l| is_var(l)) {
            return None;
        }
        let end = self.lines[i + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(self.lines.len(), |k| i + 1 + k);
        Some(i..end)
    }

    fn eca(&self, i: usize) -> Option<(Key, &str)> {
        if !self.section.contains(&i) {
            return None;
        }
        eca_line(&self.lines[i])
    }

    /// The ECA at `line` (from 1): its index, or why not.
    fn at(&self, line: usize) -> Result<(usize, Key), String> {
        let i = line.wrapping_sub(1);
        self.eca(i).map(|(k, _)| (i, k)).ok_or_else(|| format!("line {line} isn't an event, condition or action"))
    }

    /// The end (exclusive) of the ECA at `i` and what's inside it: the
    /// last line below with more `>` before one with as many or fewer, or
    /// a field; blank lines and comments between are inside.
    fn end(&self, i: usize) -> usize {
        let depth = self.eca(i).map_or(0, |(k, _)| k.depth);
        let mut last = i;
        for j in i + 1..self.section.end {
            match self.eca(j) {
                Some((k, _)) if k.depth > depth => last = j,
                Some(_) => break,
                None if filler(&self.lines[j]) => {}
                None => break,
            }
        }
        last + 1
    }

    /// The ECA at `line` and those at the lines `more` lists (from 1,
    /// separated by spaces): each's index and end, in the text's order;
    /// one inside another counts with it.
    fn selection(&self, line: usize, more: &str) -> Result<Vec<(usize, usize)>, String> {
        let mut at = vec![self.at(line)?.0];
        for w in more.split_whitespace() {
            let l: usize = w.parse().map_err(|_| format!("not a line: {w:?}"))?;
            at.push(self.at(l)?.0);
        }
        at.sort();
        at.dedup();
        let mut out: Vec<(usize, usize)> = Vec::new();
        for i in at {
            if out.last().is_some_and(|&(_, end)| i < end) {
                continue;
            }
            out.push((i, self.end(i)));
        }
        Ok(out)
    }

    /// The ECAs right under the one at `parent` (an index; `None` for the
    /// top): each's index, key and end.
    fn children(&self, parent: Option<usize>) -> Vec<(usize, Key, usize)> {
        let (from, to, depth) = match parent {
            Some(p) => (p + 1, self.end(p), self.eca(p).map_or(0, |(k, _)| k.depth + 1)),
            None => (self.section.start, self.section.end, 0),
        };
        let mut out = Vec::new();
        let mut i = from;
        while i < to {
            match self.eca(i) {
                Some((k, _)) if k.depth == depth => {
                    let end = self.end(i);
                    out.push((i, k, end));
                    i = end;
                }
                _ => i += 1,
            }
        }
        out
    }

    /// Where `place` is: the index to insert at, the depth and block what
    /// goes there has, and the kind it must be.
    fn place(&self, place: Place) -> Result<(usize, usize, Option<u32>, u32), String> {
        let (parent, depth, group, kind) = if place.parent == 0 {
            if place.group > 2 {
                return Err(format!("no kind {} at the top", place.group));
            }
            (None, 0, None, place.group)
        } else {
            let (p, key) = self.at(place.parent)?;
            let name = self.eca(p).map_or("", |(_, n)| n);
            let Some(block) = wtg_rs::ui::blocks(key.kind, name).iter().find(|b| b.group == place.group) else {
                return Err(format!("`{name}` has no block {}", place.group));
            };
            (Some(p), key.depth + 1, Some(place.group), block.kind)
        };
        // A block's own, and the ones before and after it: at the top by
        // kind, inside by block.
        let order = |k: &Key| if parent.is_some() { k.group.unwrap_or(0) } else { k.kind };
        let rank = if parent.is_some() { place.group } else { kind };
        let all = self.children(parent);
        let own: Vec<_> = all.iter().filter(|c| order(&c.1) == rank).collect();
        let at = if let Some(c) = own.get(place.index) {
            c.0
        } else if let Some(c) = own.last() {
            c.2
        } else if let Some(c) = all.iter().rev().find(|c| order(&c.1) < rank) {
            c.2
        } else if let Some(c) = all.iter().find(|c| order(&c.1) > rank) {
            c.0
        } else if let Some(p) = parent {
            p + 1
        } else {
            // An empty trigger: after its fields.
            (self.section.start..self.section.end).rev().find(|&i| !filler(&self.lines[i])).map_or(self.section.start, |i| i + 1)
        };
        Ok((at, depth, group, kind))
    }

    /// Lines `from..to` as if the first were at the top: as many `>`
    /// fewer as it has, and no block.
    fn lift(&self, from: usize, to: usize) -> Vec<String> {
        let depth = self.eca(from).map_or(0, |(k, _)| k.depth);
        (from..to)
            .map(|i| {
                let line = &self.lines[i];
                rekey(line, |k| {
                    k.depth -= depth.min(k.depth);
                    if i == from {
                        k.group = None;
                    }
                })
                .unwrap_or_else(|| line.clone())
            })
            .collect()
    }

    /// Inserts `lines` (lifted: their ECAs at the top have no `>`) at `at`,
    /// at `depth` in block `group`.
    fn insert(&mut self, at: usize, lines: &[String], depth: usize, group: Option<u32>) {
        if at == self.lines.len()
            && let Some(last) = self.lines.last_mut()
            && !last.ends_with('\n')
        {
            last.push_str(self.eol);
        }
        let eol = self.eol;
        let new = lines.iter().map(|line| {
            let l = rekey(line, |k| {
                if k.depth == 0 {
                    k.group = group;
                }
                k.depth += depth;
            })
            .unwrap_or_else(|| line.clone());
            // The text's own line breaks, whatever the lines came with.
            format!("{}{eol}", l.trim_end_matches(['\n', '\r']))
        });
        self.lines.splice(at..at, new.collect::<Vec<_>>());
    }

    /// Sets the trigger's field `key` to `value` (as the text writes it),
    /// or takes it out (`None`); a field that's new goes after the others
    /// (`name` and `description` first).
    fn set_field(&mut self, key: &str, value: Option<String>) {
        let line = value.map(|v| format!("{key}={v}{}", self.eol));
        if line.is_some() {
            self.ensure_trigger();
        }
        match (self.field(key), line) {
            (Some(i), Some(l)) => {
                let keep_eol = self.lines[i].ends_with('\n');
                self.lines[i] = if keep_eol { l } else { l.trim_end().to_string() };
            }
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (None, Some(l)) => {
                let mut fields = self.section.clone().filter(|&i| !filler(&self.lines[i]) && self.eca(i).is_none());
                let at = if matches!(key, "name" | "description") {
                    self.section.start
                } else {
                    fields.next_back().map_or(self.section.start, |i| i + 1)
                };
                if at == self.lines.len()
                    && let Some(last) = self.lines.last_mut()
                    && !last.ends_with('\n')
                {
                    last.push_str(self.eol);
                }
                self.lines.insert(at, l);
            }
            (None, None) => {}
        }
    }

    /// The trigger's field `key`'s line, if it has one.
    fn field(&self, key: &str) -> Option<usize> {
        self.section.clone().find(|&i| self.lines[i].split_once('=').is_some_and(|(k, _)| k.trim() == key))
    }
}

/// Does `text` (lifted lines, as [`op::COPY`] gives them) hold ECAs, each
/// of `kind`? Their kinds, or why not.
fn pasted(text: &str) -> Result<Vec<u32>, String> {
    let p = wtg_rs::text::trigger_from_text(&format!("[trigger]\n{text}"), None);
    let lines: Vec<&str> = text.lines().collect();
    if let Some(e) = p.errors.first() {
        return Err(format!("not events, conditions or actions: line {}: {}", e.line.saturating_sub(1), e.message));
    }
    if let Some(l) = lines.iter().find(|l| !filler(l) && eca_line(l).is_none()) {
        return Err(format!("not an event, condition or action: {l:?}"));
    }
    let ecas = p.trigger.map(|t| t.ecas).unwrap_or_default();
    if ecas.is_empty() {
        return Err("nothing to paste".into());
    }
    Ok(ecas.iter().map(|e| e.kind).collect())
}

const KIND_NAMES: [&str; 3] = ["an event", "a condition", "an action"];

fn fits(kinds: &[u32], kind: u32) -> Result<(), String> {
    match kinds.iter().find(|&&k| k != kind) {
        Some(&k) => Err(format!("{} can't go where {} goes", KIND_NAMES[k as usize], KIND_NAMES[kind as usize])),
        None => Ok(()),
    }
}

/// Does an operation of the view on the trigger `text`: `op` (see
/// [`op`]) on the ECA at `line` (from 1), with `arg`. The answer is JSON:
/// `{"edits": [[startLine, startChar, endLine, endChar, "text"]]}` in
/// editor coordinates (one edit, of whole lines, or none) and `"line"`,
/// where what was moved, pasted or turned on or off is after them (from
/// 1; 0 for none); `{"text": "..."}` for `COPY`; `{"error": "..."}` when
/// it can't be done.
pub fn edit(text: &str, op: u32, line: usize, arg: &str, ui: &wtg_rs::ui::Ui) -> String {
    edit_in(text, op, line, arg, ui, &Project::default())
}

/// The trigger file being edited (`me`, its id) and the other files of
/// its project (id, text, and the names the index found in it), for what
/// changes a variable.
#[derive(Default)]
pub struct Project<'a> {
    pub me: &'a str,
    pub others: Vec<(&'a str, &'a str, &'a [parser_rs::Occurrence])>,
}

/// [`edit`], in its project: an edit that renames a variable's uses in
/// other files adds `"files": [[id, startLine, startChar, endLine,
/// endChar, "text"], ...]`.
pub fn edit_in(text: &str, op: u32, line: usize, arg: &str, ui: &wtg_rs::ui::Ui, project: &Project) -> String {
    let js = crate::objects::json_string;
    let edit_of = |text: &str, new: &[String]| {
        let old: Vec<&str> = text.split_inclusive('\n').collect();
        diff(text, &old, new).map(|(r, t)| format!("{},{},{},{},{}", r.start.line, r.start.character, r.end.line, r.end.character, js(&t)))
    };
    match apply(text, op, line, arg, ui, project) {
        Ok(Done::Text(t)) => format!("{{\"text\":{}}}", js(&t)),
        Ok(Done::Json(t)) => t,
        Ok(Done::Lines(new, at, files)) => {
            let edits = edit_of(text, &new).map_or("[]".into(), |e| format!("[[{e}]]"));
            let files: Vec<String> = files
                .iter()
                .filter_map(|(id, old, new)| {
                    let lines: Vec<String> = new.split_inclusive('\n').map(String::from).collect();
                    edit_of(old, &lines).map(|e| format!("[{},{e}]", js(id)))
                })
                .collect();
            let files = if files.is_empty() { String::new() } else { format!(",\"files\":[{}]", files.join(",")) };
            format!("{{\"edits\":{edits},\"line\":{}{files}}}", at.map_or(0, |i| i + 1))
        }
        Err(e) => format!("{{\"error\":{}}}", js(&e)),
    }
}

enum Done {
    Text(String),
    /// An answer of its own.
    Json(String),
    /// The new lines, the index of the one to show, and other files
    /// changed: id, old text, new text.
    Lines(Vec<String>, Option<usize>, Vec<(String, String, String)>),
}

fn apply(text: &str, op: u32, line: usize, arg: &str, ui: &wtg_rs::ui::Ui, project: &Project) -> Result<Done, String> {
    let mut files = Vec::new();
    let mut doc = Doc::new(text);
    let mut shown = None;
    match op {
        op::TOGGLE => {
            // Several: all as the first becomes.
            let (i, key) = doc.at(line)?;
            for (j, _) in doc.selection(line, arg)? {
                doc.lines[j] = rekey(&doc.lines[j], |k| k.enabled = !key.enabled).expect("an ECA");
            }
            shown = Some(i);
        }
        op::DELETE => {
            if let Some(range) = doc.variable_at(line) {
                doc.lines.drain(range);
            } else {
                for (i, end) in doc.selection(line, arg)?.into_iter().rev() {
                    doc.lines.drain(i..end);
                }
            }
        }
        op::COPY => {
            let mut out: String = doc.selection(line, arg)?.into_iter().map(|(i, end)| doc.lift(i, end).concat()).collect();
            if !out.ends_with('\n') {
                out.push_str(doc.eol);
            }
            return Ok(Done::Text(out));
        }
        op::MOVE => {
            let (place, more) = arg.split_once('\n').unwrap_or((arg, ""));
            let place = Place::parse(place)?;
            let moved = doc.selection(line, more)?;
            let inside = |j: usize| moved.iter().any(|&(i, end)| (i..end).contains(&j));
            if place.parent != 0 && inside(place.parent - 1) {
                return Err("can't move it into itself".into());
            }
            let (at, depth, group, kind) = doc.place(place)?;
            let kinds: Vec<u32> = moved.iter().map(|&(i, _)| doc.eca(i).expect("an ECA").0.kind).collect();
            fits(&kinds, kind)?;
            if moved.iter().any(|&(i, end)| i < at && at < end) {
                return Err("can't move it into itself".into());
            }
            // Lifted in the text's order, taken out from the bottom up;
            // the place moves up by what was above it.
            let lines: Vec<String> = moved.iter().flat_map(|&(i, end)| doc.lift(i, end)).collect();
            let mut at = at;
            for &(i, end) in moved.iter().rev() {
                doc.lines.drain(i..end);
                if at >= end {
                    at -= end - i;
                }
            }
            doc.insert(at, &lines, depth, group);
            shown = Some(at);
        }
        op::PASTE => {
            doc.ensure_trigger();
            let (place, body) = arg.split_once('\n').unwrap_or((arg, ""));
            let kinds = pasted(body)?;
            let (at, depth, group, kind) = doc.place(Place::parse(place)?)?;
            fits(&kinds, kind)?;
            let lines: Vec<String> = body.split_inclusive('\n').map(String::from).collect();
            doc.insert(at, &lines, depth, group);
            shown = (at..doc.lines.len()).find(|&i| doc.eca(i).is_some());
        }
        op::FIELD => {
            let (key, value) = arg.split_once('\n').unwrap_or((arg, ""));
            let value = match key {
                // Empty: none (a trigger without a name is its file's).
                "name" | "description" => (!value.is_empty()).then(|| wtg_rs::text::jass_string(value)),
                "enabled" | "initially_off" | "run_on_init" | "comment" | "custom_text" => {
                    let n: u32 = value.trim().parse().map_err(|_| format!("`{key}` is a number, not {value:?}"))?;
                    let default = if key == "enabled" { 1 } else { 0 };
                    (n != default).then(|| n.to_string())
                }
                _ => return Err(format!("no field `{key}`")),
            };
            doc.set_field(key, value);
        }
        op::RENAMED => {
            // The text as it was named before, to see what references meant.
            let mut was = Doc::new(text);
            was.set_field("name", (!arg.is_empty()).then(|| wtg_rs::text::jass_string(arg)));
            let was = was.lines.concat();
            let pairs: Vec<(&str, &str)> = project.others.iter().map(|&(id, t, _)| (id, t)).collect();
            let after = Triggers::of(project.me, text, &pairs);
            let now = format!("gg_trg_{}", after.mine);
            let alike = after.named_alike(project.me);
            if !alike.is_empty() {
                let old = format!("gg_trg_{}", Triggers::of(project.me, &was, &pairs).mine);
                let files: Vec<String> = alike.iter().map(|f| file_name(f)).collect();
                return Err(format!(
                    "the map's script names {} {now} too: references to {old} were left as they are, rename it to a name of its own",
                    files.join(", ")
                ));
            }
            let (mine, others) = retarget_project(project.me, &was, text, project);
            doc = Doc::new(&mine);
            files = others;
        }
        op::SET_PARAM => {
            let (i, _) = doc.at(line)?;
            let mut parts = arg.splitn(3, '\n');
            let (path, what, value) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let new = match what {
                "preset" => value.to_string(),
                "variable" => format!("${value}"),
                "array" => format!("${value}[\"0\"]"),
                "call" => ui.new_call(wtg_rs::FunctionKind::Call, value),
                "value" => wtg_rs::text::jass_string(value),
                _ => return Err(format!("a parameter can't become {what:?}")),
            };
            let new = wtg_rs::text::read_param(&new)?;
            let old = &doc.lines[i];
            let (key, rest) = old.split_once('=').expect("an ECA's line");
            let eol = &rest[rest.trim_end_matches(['\n', '\r']).len()..];
            let (name, mut params) = wtg_rs::text::read_call(rest.trim_end_matches(['\n', '\r']))?;
            *param_at(&mut params, path)? = new;
            doc.lines[i] = format!("{key}={}{eol}", wtg_rs::text::call_text(&name, &params)?);
            shown = Some(i);
        }
        op::NEW => {
            doc.ensure_trigger();
            let (place, what) = arg.split_once('\n').unwrap_or((arg, ""));
            let (kind, name) = what.split_once(' ').ok_or("a new line: its kind and function")?;
            let kind: u32 = kind.parse().map_err(|_| format!("no kind {kind:?}"))?;
            let fk = wtg_rs::FunctionKind::of(kind).filter(|_| kind < 3).ok_or(format!("no kind {kind}"))?;
            let (at, depth, group, wanted) = doc.place(Place::parse(place)?)?;
            fits(&[kind], wanted)?;
            let line = format!("{}={}", KINDS[kind as usize], ui.new_call(fk, name));
            doc.insert(at, &[line], depth, group);
            shown = Some(at);
        }
        op::NEW_VARIABLE => {
            let d = variable_arg(arg, text, None)?;
            let scope = d.scope;
            // A local is its trigger's: the file gets one.
            if scope == Scope::Local {
                doc.ensure_trigger();
            }
            let eol = doc.eol;
            let section: Vec<String> = wtg_rs::text::declared_to_text(&[d])?.lines().map(|l| format!("{l}{eol}")).collect();
            // After the last variable, else before the trigger, else at
            // the end; a blank line between sections.
            let headers: Vec<usize> = (0..doc.lines.len()).filter(|&i| doc.variable_at(i + 1).is_some()).collect();
            let (at, before) = match headers.last() {
                Some(&h) => {
                    let end = doc.variable_at(h + 1).expect("a header").end;
                    let at = (h..end).rev().find(|&i| !doc.lines[i].trim().is_empty()).map_or(end, |i| i + 1);
                    (at, true)
                }
                None if doc.has_section => (doc.section.start - 1, false),
                None => (doc.lines.len(), doc.lines.iter().any(|l| !filler(l))),
            };
            if at == doc.lines.len()
                && let Some(last) = doc.lines.last_mut()
                && !last.ends_with('\n')
            {
                last.push_str(eol);
            }
            let mut block = Vec::new();
            if before {
                block.push(eol.to_string());
            }
            block.extend(section);
            if !before && at < doc.lines.len() {
                block.push(eol.to_string());
            }
            shown = Some(at + before as usize);
            doc.lines.splice(at..at, block);
        }
        op::REPLACE => {
            let (i, key) = doc.at(line)?;
            let name = arg.trim();
            let fk = wtg_rs::FunctionKind::of(key.kind).ok_or(format!("no kind {}", key.kind))?;
            if name.is_empty() {
                return Err("a function to become".into());
            }
            let old = doc.eca(i).map_or("", |(_, n)| n).to_string();
            if old != name {
                // Inside it: what has no block in the new function goes.
                let blocks = wtg_rs::ui::blocks(key.kind, name);
                for (c, k, end) in doc.children(Some(i)).into_iter().rev() {
                    if !blocks.iter().any(|b| Some(b.group) == k.group && b.kind == k.kind) {
                        doc.lines.drain(c..end);
                    }
                }
                let eol = &doc.lines[i][doc.lines[i].trim_end_matches(['\n', '\r']).len()..];
                doc.lines[i] = format!("{}={}{eol}", key.text(), ui.new_call(fk, name));
            }
            shown = Some(i);
        }
        op::CHECK_VARIABLE => {
            let old = variable_of(&doc, line)?;
            let new = if arg.trim().is_empty() { None } else { Some(variable_arg(arg, text, Some(&old.variable.name))?) };
            let change = variable_change(text, &old, new.as_ref(), ui, project)?;
            let js = crate::objects::json_string;
            let places: Vec<String> = change.places.iter().map(|(id, l, why)| format!("[{},{l},{}]", js(id), js(why))).collect();
            return Ok(Done::Json(format!("{{\"renamed\":{},\"places\":[{}]}}", change.renamed, places.join(","))));
        }
        op::SET_VARIABLE => {
            let old = variable_of(&doc, line)?;
            let d = variable_arg(arg, text, Some(&old.variable.name))?;
            let change = variable_change(text, &old, Some(&d), ui, project)?;
            // The uses renamed: lines here change, none come or go.
            let eol = doc.eol;
            doc = Doc::new(&change.me);
            doc.eol = eol;
            files = change.files;
            if d.scope == Scope::Local {
                doc.ensure_trigger();
            }
            let range = doc.variable_at(line).expect("still there");
            // Its own lines: not the blank ones before the next section.
            let end = range.clone().rev().find(|&i| !doc.lines[i].trim().is_empty()).map_or(range.end, |i| i + 1);
            let eol = doc.eol;
            let mut section: Vec<String> = wtg_rs::text::declared_to_text(&[d])?.lines().map(|l| format!("{l}{eol}")).collect();
            if end == doc.lines.len() && !doc.lines[end - 1].ends_with('\n') {
                // The file ended without a line break: so does it still.
                if let Some(last) = section.last_mut() {
                    *last = last.trim_end().to_string();
                }
            }
            doc.lines.splice(range.start..end, section);
            shown = Some(range.start);
        }
        _ => return Err(format!("unknown operation {op}")),
    }
    Ok(Done::Lines(doc.lines, shown, files))
}

/// The variable whose `[variable]` is at `line`.
fn variable_of(doc: &Doc, line: usize) -> Result<Declared, String> {
    let range = doc.variable_at(line).ok_or_else(|| format!("line {line} isn't a variable's"))?;
    wtg_rs::text::trigger_from_text(&doc.lines[range].concat(), None).variables.into_iter().next().ok_or_else(|| "no variable there".into())
}

/// What changing a variable does to the project: the file's text with its
/// uses renamed (`me`), the other files whose uses are renamed (id, old
/// text, new text), how many uses are, and the places that need fixing
/// by hand (id, line from 1, why).
struct Change {
    me: String,
    files: Vec<(String, String, String)>,
    renamed: usize,
    places: Vec<(String, usize, String)>,
}

/// A variable's use in an ECA line.
struct Use {
    id: String,
    /// From 1.
    line: usize,
    /// In the trigger's actions, where its locals are seen.
    in_actions: bool,
    indexed: bool,
    /// The type the parameter takes: empty when unknown, `AnyGlobal` for
    /// any variable.
    expects: String,
    /// A parameter after it takes its type (`SetVariable`'s value).
    sets: bool,
}

/// The name a build gives a variable in the map's script (see
/// `wtg_rs::jass`): `udg_<name>`, a private one's `udg_<trigger>__<name>`
/// (`trigger`: what the build names its file's trigger by), a local its
/// name.
fn jass_name(d: &Declared, trigger: &str) -> String {
    let name = &d.variable.name;
    match d.scope {
        Scope::Global => format!("udg_{name}"),
        Scope::Local => name.clone(),
        Scope::Private => format!("udg_{trigger}__{name}"),
    }
}

fn file_name(id: &str) -> String {
    Path::new(id).file_name().map_or(id.to_string(), |n| n.to_string_lossy().into_owned())
}

/// Does the JASS `code` (custom script) name `name`: a name token, as the
/// JASS lexer reads it (not in a string or a comment)?
fn names_in_jass(code: &str, name: &str) -> bool {
    let lexed = parser_rs::tokenize_lossy(code, &jass_rs::lexer::VJASS_SPEC);
    lexed.tokens.iter().any(|t| matches!(&t.kind, parser_rs::TokenKind::Ident(n) if n == name))
}

/// The ECA lines of a trigger file: index, key, and the parsed call.
fn calls(doc: &Doc) -> impl Iterator<Item = (usize, Key, String, Vec<wtg_rs::Param>)> + '_ {
    doc.section.clone().filter_map(|i| {
        let (key, _) = doc.eca(i)?;
        let (_, rest) = doc.lines[i].split_once('=')?;
        let (name, params) = wtg_rs::text::read_call(rest.trim_end_matches(['\n', '\r'])).ok()?;
        Some((i, key, name, params))
    })
}

/// The uses of the variable `name` in the trigger file `text` (`only_actions`:
/// in its actions only, where a local is), and the lines whose strings
/// (custom script) name `jass`.
fn uses_in(
    id: &str,
    text: &str,
    name: &str,
    jass: Option<&str>,
    only_actions: bool,
    ui: &wtg_rs::ui::Ui,
    kind_of: &dyn Fn(&str) -> Option<String>,
) -> (Vec<Use>, Vec<usize>) {
    use wtg_rs::{FunctionKind, Param};
    struct Walk<'a> {
        name: &'a str,
        jass: Option<&'a str>,
        ui: &'a wtg_rs::ui::Ui,
        kind_of: &'a dyn Fn(&str) -> Option<String>,
        /// indexed, expects, sets
        found: Vec<(bool, String, bool)>,
        script: bool,
    }
    impl Walk<'_> {
        fn list(&mut self, params: &[Param], function: Option<&wtg_rs::ui::Function>) {
            let arg = |j: usize| function.and_then(|f| f.args.get(j)).cloned().unwrap_or_default();
            for (j, p) in params.iter().enumerate() {
                let mut ty = arg(j);
                if ty == "Null" {
                    let before = j.checked_sub(1).and_then(|k| params.get(k)).filter(|b| b.kind == 1);
                    ty = before.and_then(|b| (self.kind_of)(&b.value)).unwrap_or_default();
                }
                self.one(p, ty, arg(j + 1) == "Null");
            }
        }
        fn one(&mut self, p: &Param, ty: String, sets: bool) {
            if p.kind == 1 && p.value == self.name {
                self.found.push((p.index.is_some(), ty.clone(), sets));
            }
            if p.kind == 3 && ty == "scriptcode" && self.jass.is_some_and(|j| names_in_jass(&p.value, j)) {
                self.script = true;
            }
            if let Some(ix) = &p.index {
                self.one(ix, "integer".into(), false);
            }
            if let Some(c) = &p.call {
                let f = self.ui.function(FunctionKind::of(c.kind).unwrap_or(FunctionKind::Call), &c.name);
                self.list(&c.params, f);
            }
        }
    }
    let doc = Doc::new(text);
    let mut uses = Vec::new();
    let mut script = Vec::new();
    for (i, key, call, params) in calls(&doc) {
        let in_actions = sees_locals(&doc, i);
        let mut w = Walk { name, jass, ui, kind_of, found: Vec::new(), script: false };
        w.list(&params, ui.function(FunctionKind::of(key.kind).unwrap_or(FunctionKind::Action), &call));
        if w.script {
            script.push(i + 1);
        }
        if only_actions && !in_actions {
            continue;
        }
        for (indexed, expects, sets) in w.found {
            uses.push(Use { id: id.to_string(), line: i + 1, in_actions, indexed, expects, sets });
        }
    }
    (uses, script)
}

/// The trigger file `text` with `$old` made `$new` on `lines` (from 1):
/// the variable tokens its reader finds there (`wtg_rs::text`), so a
/// string or a comment that says `$old` stays as it is.
fn rename_on_lines(text: &str, lines: &[usize], old: &str, new: &str) -> String {
    let tokens = wtg_rs::text::trigger_from_text(text, None).tokens;
    let wanted = format!("${old}");
    text.split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            if !lines.contains(&(i + 1)) {
                return line.to_string();
            }
            let mut chars: Vec<char> = line.chars().collect();
            // Columns from 1, in characters; from the right, so the ones
            // before stay where they are.
            let mut at: Vec<(usize, usize)> = tokens
                .iter()
                .filter(|t| t.line == i + 1 && t.ty == parser_rs::SemanticType::Variable)
                .map(|t| (t.start_col - 1, t.end_col - 1))
                .filter(|&(a, b)| chars.get(a..b).is_some_and(|c| c.iter().collect::<String>() == wanted))
                .collect();
            at.sort();
            for (a, b) in at.into_iter().rev() {
                chars.splice(a..b, format!("${new}").chars());
            }
            chars.into_iter().collect()
        })
        .collect()
}

/// What changing the variable `old` of the trigger file `text` into
/// `new` (`None`: deleting it) does: its uses are those that see it, in
/// this file (a local's in the actions only) and, for a global, in the
/// project's other trigger files that have no variable of that name of
/// their own.
///
/// - Renamed, its uses are renamed (safe: they mean the same variable).
/// - These need fixing by hand, and are the places: a use that won't see
///   it any more (a narrower scope), that indexes it and it's no array or
///   the other way round, whose parameter takes another type (or whose
///   `SetVariable` value is of the old type), every use of a deleted one;
///   and custom script naming it in the map's script when that name
///   changes (`udg_<name>` ...), or it goes: a `scriptcode` parameter
///   whose JASS has it as a name token, a name the index found in the
///   project's code (JASS, Zinc, Lua ...).
/// - Refused: a new name or scope that clashes with a variable another
///   trigger file declares (a global of the same name; a private or local
///   one would hide a global of the project).
fn variable_change(text: &str, old: &Declared, new: Option<&Declared>, ui: &wtg_rs::ui::Ui, project: &Project) -> Result<Change, String> {
    let me = project.me;
    let name = old.variable.name.as_str();
    let trg: Vec<(&str, &str, Vec<Declared>)> = project
        .others
        .iter()
        .filter(|(id, _, _)| is_trg_file(Path::new(id)))
        .map(|&(id, t, _)| (id, t, wtg_rs::text::trigger_from_text(t, None).variables))
        .collect();
    let own = wtg_rs::text::trigger_from_text(text, None).variables;
    // A clash: only when the name or the scope changes.
    if let Some(n) = new
        && (n.variable.name != name || n.scope != old.scope)
    {
        let new_name = n.variable.name.as_str();
        for (id, _, vars) in &trg {
            let Some(d) = vars.iter().find(|d| d.variable.name == new_name) else { continue };
            if n.scope == Scope::Global {
                return Err(format!("{} declares a variable {new_name} already", file_name(id)));
            }
            if d.scope == Scope::Global {
                return Err(format!("{new_name} is a global of {}: this file's would hide it", file_name(id)));
            }
        }
    }
    let globals: Vec<&Declared> = trg.iter().flat_map(|(_, _, v)| v.iter()).filter(|d| d.scope == Scope::Global).collect();
    let kind_in = |vars: &[Declared], v: &str| {
        vars.iter().chain(globals.iter().copied()).find(|d| d.variable.name == v).map(|d| d.variable.kind.clone())
    };
    let pairs: Vec<(&str, &str)> = project.others.iter().map(|&(id, t, _)| (id, t)).collect();
    let trigger = Triggers::of(me, text, &pairs).mine;
    let old_jass = jass_name(old, &trigger);
    let jass_changes = new.is_none_or(|n| jass_name(n, &trigger) != old_jass);
    let jass = jass_changes.then_some(old_jass.as_str());

    let (mut uses, mut script) = uses_in(me, text, name, jass, old.scope == Scope::Local, ui, &|v| kind_in(&own, v));
    let mut script: Vec<(String, usize)> = script.drain(..).map(|l| (me.to_string(), l)).collect();
    if old.scope == Scope::Global {
        for (id, t, vars) in &trg {
            if vars.iter().any(|d| d.variable.name == name) {
                continue;
            }
            let (u, s) = uses_in(id, t, name, jass, false, ui, &|v| kind_in(vars, v));
            uses.extend(u);
            script.extend(s.into_iter().map(|l| (id.to_string(), l)));
        }
        // The project's code (JASS, Zinc, Lua ...): the names its index
        // found, as each language's parser reads them.
        if let Some(j) = jass {
            for (id, _, names) in project.others.iter().filter(|(id, _, _)| !is_trg_file(Path::new(id))) {
                script.extend(names.iter().filter(|o| o.name == j).map(|o| (id.to_string(), o.line)));
            }
        }
    }

    let mut places = Vec::new();
    let v = &old.variable;
    for u in &uses {
        let mut why = |w: String| places.push((u.id.clone(), u.line, w));
        let Some(n) = new else {
            why(format!("uses {name}, which is deleted"));
            continue;
        };
        let nv = &n.variable;
        let sees = match n.scope {
            Scope::Global => true,
            Scope::Private => u.id == me,
            Scope::Local => u.id == me && u.in_actions,
        };
        if !sees {
            why(match n.scope {
                Scope::Local if u.id == me => format!("{} is a local now: a trigger's locals are its actions'", nv.name),
                Scope::Local => format!("{} is a local of {} now", nv.name, file_name(me)),
                _ => format!("{} is private to {} now", nv.name, file_name(me)),
            });
        }
        if u.indexed && nv.is_array == 0 {
            why(format!("indexes {}, which isn't an array now", nv.name));
        } else if !u.indexed && nv.is_array != 0 && v.is_array == 0 {
            why(format!("{} is an array now: it needs an index", nv.name));
        }
        if nv.kind != v.kind {
            let ty = |t: &str| ui.types.get(t).cloned().unwrap_or_else(|| t.to_string());
            if !u.expects.is_empty() && u.expects != "AnyGlobal" && u.expects == v.kind {
                why(format!("takes {}, and {} is {} now", ty(&u.expects), nv.name, ty(&nv.kind)));
            }
            if u.sets {
                why(format!("sets {} to {} (its old type), and it's {} now", nv.name, ty(&v.kind), ty(&nv.kind)));
            }
        }
    }
    for (id, line) in script {
        let why = match new {
            None => format!("custom script uses {old_jass}, which is deleted"),
            Some(n) => format!("custom script uses {old_jass}, which is {} now", jass_name(n, &trigger)),
        };
        places.push((id, line, why));
    }
    places.sort();
    places.dedup();

    // Renamed: every use, where it is.
    let mut out = Change { me: text.to_string(), files: Vec::new(), renamed: 0, places };
    if let Some(n) = new.filter(|n| n.variable.name != name) {
        let new_name = n.variable.name.as_str();
        out.renamed = uses.len();
        let rewrite = |t: &str, id: &str| -> String {
            let lines: Vec<usize> = uses.iter().filter(|u| u.id == id).map(|u| u.line).collect();
            rename_on_lines(t, &lines, name, new_name)
        };
        out.me = rewrite(text, me);
        for (id, t, _) in &trg {
            if uses.iter().any(|u| u.id == *id) {
                out.files.push((id.to_string(), t.to_string(), rewrite(t, id)));
            }
        }
    }
    Ok(out)
}

/// The variable an argument of `NEW_VARIABLE` or `SET_VARIABLE` says:
/// name, type, `1` for an array, size, scope, initial value, a line each.
/// Its name mustn't be another of the file's (`except`: its own, when it's
/// one already).
fn variable_arg(arg: &str, text: &str, except: Option<&str>) -> Result<Declared, String> {
    let parts: Vec<&str> = arg.split('\n').collect();
    let [name, kind, array, size, scope, ref rest @ ..] = parts[..] else {
        return Err("a variable: its name, type, array, size, scope".into());
    };
    let scope = match scope.trim() {
        "" | "global" => Scope::Global,
        "private" => Scope::Private,
        "local" => Scope::Local,
        w => return Err(format!("scope {w:?}: global, private or local")),
    };
    let name = name.trim();
    let ident =
        name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ident {
        return Err(format!("{name:?} isn't a variable's name: a letter, then letters, digits and _"));
    }
    let declared = wtg_rs::text::trigger_from_text(text, None).variables;
    let other = |n: &str| except.is_none_or(|e| !e.eq_ignore_ascii_case(n));
    if declared.iter().any(|d| d.variable.name.eq_ignore_ascii_case(name) && other(&d.variable.name)) {
        return Err(format!("this file has a variable {name} already"));
    }
    if kind.trim().is_empty() {
        return Err("a variable needs a type".into());
    }
    let is_array = array.trim() == "1";
    let size: u32 = if is_array { size.trim().parse().map_err(|_| format!("an array's size is a number, not {size:?}"))? } else { 1 };
    let initial = rest.first().filter(|v| !v.is_empty());
    let variable = wtg_rs::Variable {
        name: name.into(),
        kind: kind.trim().into(),
        unknown: 1,
        is_array: is_array as u32,
        array_size: size,
        has_initial: initial.is_some() as u32,
        initial: initial.map_or(String::new(), |v| v.to_string()),
    };
    Ok(Declared { variable, scope })
}

/// What the parameter at `path` (see `wtg_rs::ui::Span::Value`) of the
/// ECA at `line` can be, for the editor's dialog, as JSON:
///
/// ```json
/// {"type": "player", "type_name": "Player",
///  "current": {"kind": "preset", "value": "Player00"},
///  "presets": [["Player00", "Player 1 (Red)"], ...],
///  "variables": [["hero", "Unit", true], ...],
///  "calls": [["GetOwningPlayer", "Owner Of Unit", "Unit"], ...],
///  "value": false, "objects": [["hfoo", "Footman"], ...]}
/// ```
///
/// `objects`: for a type whose values are objects (`unitcode` ...:
/// `names::OBJECT_TYPES`), each with its name, by name.
///
/// The type is the function's parameter's (`UI\TriggerData.txt`); an
/// array's index is an integer; `AnyGlobal` takes any variable, and
/// `Null` (the value `SetVariable` sets) the type of the variable before
/// it. `variables` are the global variables `.trg` files declare
/// (`[variable]`): name, type's name, is it an array. `current.kind` is
/// `preset`, `variable`, `call`, `value` or `other`. `{"error": "..."}`
/// when there's no such parameter.
pub fn choices(text: &str, line: usize, path: &str, ui: &wtg_rs::ui::Ui, variables: &[Declared]) -> String {
    let js = crate::objects::json_string;
    match choices_of(text, line, path, ui, variables) {
        Ok(c) => c,
        Err(e) => format!("{{\"error\":{}}}", js(&e)),
    }
}

fn choices_of(text: &str, line: usize, path: &str, ui: &wtg_rs::ui::Ui, declared: &[Declared]) -> Result<String, String> {
    use wtg_rs::FunctionKind;
    let js = crate::objects::json_string;
    let doc = Doc::new(text);
    let (i, key) = doc.at(line)?;
    // The variables seen here: a local in the actions only.
    let locals = sees_locals(&doc, i);
    let seen: Vec<&Declared> = declared.iter().filter(|d| d.scope != Scope::Local || locals).collect();
    let variables: Vec<&wtg_rs::Variable> = seen.iter().map(|d| &d.variable).collect();
    let (_, rest) = doc.lines[i].split_once('=').expect("an ECA's line");
    let (name, params) = wtg_rs::text::read_call(rest.trim_end_matches(['\n', '\r']))?;
    // The type, step by step: the function's parameter's, a call's
    // parameter's, an index's.
    let mut function = ui.function(FunctionKind::of(key.kind).unwrap_or(FunctionKind::Action), &name);
    let mut list: &[wtg_rs::Param] = &params;
    let mut ty = String::new();
    let mut current: Option<&wtg_rs::Param> = None;
    for step in path.split('.') {
        if step == "i" {
            let p = current.ok_or("no parameter")?;
            current = Some(p.index.as_deref().ok_or_else(|| format!("no parameter {path}"))?);
            ty = "integer".into();
            continue;
        }
        if let Some(p) = current {
            let call = p.call.as_ref().ok_or_else(|| format!("no parameter {path}"))?;
            function = ui.function(FunctionKind::of(call.kind).unwrap_or(FunctionKind::Call), &call.name);
            list = &call.params;
        }
        let j: usize = step.parse().map_err(|_| format!("no parameter {path}"))?;
        // `Null`: the type of the variable before it (`SetVariable`).
        ty = function.and_then(|f| f.args.get(j)).cloned().unwrap_or_default();
        if ty == "Null" {
            let before = j.checked_sub(1).and_then(|k| list.get(k)).filter(|p| p.kind == 1);
            ty = before.and_then(|p| variables.iter().find(|v| v.name == p.value)).map_or(String::new(), |v| v.kind.clone());
        }
        current = Some(list.get(j).ok_or_else(|| format!("no parameter {path}"))?);
    }
    let p = current.ok_or("no parameter")?;
    let (kind, value) = match (p.kind, &p.call) {
        (0, _) => ("preset", p.value.clone()),
        (1, _) => ("variable", p.value.clone()),
        (2, Some(c)) => ("call", c.name.clone()),
        (3, _) => ("value", p.value.clone()),
        _ => ("other", p.value.clone()),
    };
    let any = ty == "AnyGlobal";
    let type_name = |t: &str| ui.types.get(t).cloned().unwrap_or_else(|| t.to_string());
    let presets: Vec<String> =
        if any { Vec::new() } else { ui.presets_of(&ty).iter().map(|(k, p)| format!("[{},{}]", js(k), js(&p.display))).collect() };
    let vars: Vec<String> = seen
        .iter()
        .filter(|d| any || (!ty.is_empty() && d.variable.kind == ty))
        .map(|d| {
            let v = &d.variable;
            format!("[{},{},{},\"{}\"]", js(&v.name), js(&type_name(&v.kind)), v.is_array != 0, d.scope.word().unwrap_or(""))
        })
        .collect();
    let calls: Vec<String> = if any || ty.is_empty() {
        Vec::new()
    } else {
        ui.calls_returning(&ty)
            .iter()
            .map(|(n, f)| {
                // As the editor lists them: a category whose name it hides
                // (`TC_NOTHING`) shows none.
                let category = ui.categories.get(&f.category).filter(|c| c.show_name).map_or("", |c| c.name.as_str());
                format!("[{},{},{}]", js(n), js(if f.title.is_empty() { n } else { &f.title }), js(category))
            })
            .collect()
    };
    let value_ok = !any && (ty.is_empty() || ui.takes_value(&ty));
    // A value of an object type: its objects, by name, to pick.
    let mut objects: Vec<(&String, &String)> = ui.objects.get(&ty).map(|o| o.iter().collect()).unwrap_or_default();
    objects.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(b.0)));
    let objects: Vec<String> = objects.iter().map(|(id, name)| format!("[{},{}]", js(id), js(name))).collect();
    Ok(format!(
        "{{\"type\":{},\"type_name\":{},\"current\":{{\"kind\":\"{kind}\",\"value\":{}}},\"presets\":[{}],\"variables\":[{}],\"calls\":[{}],\"value\":{value_ok},\"objects\":[{}]}}",
        js(&ty),
        js(&if any { "Any variable".into() } else { type_name(&ty) }),
        js(&value),
        presets.join(","),
        vars.join(","),
        calls.join(","),
        objects.join(","),
    ))
}

/// The functions a new ECA of `kind` (0 event, 1 condition, 2 action)
/// can be, as World Editor lists them, as JSON: `[["CreateNUnitsAtLoc",
/// "Create Units Facing Angle", "Unit", "hint", true], ...]`, by
/// category's name, then title; the last is whether the editor shows the
/// category's name before the title (not for `If/Then/Else` ...). Kind
/// 3: the types a global variable can be, `[["unit", "Unit"], ...]`.
pub fn catalog(kind: u32, ui: &wtg_rs::ui::Ui) -> String {
    let js = crate::objects::json_string;
    if kind == 3 {
        let types: Vec<String> = ui.variable_types().iter().map(|(t, n)| format!("[{},{}]", js(t), js(n))).collect();
        return format!("[{}]", types.join(","));
    }
    let Some(fk) = wtg_rs::FunctionKind::of(kind).filter(|_| kind < 3) else { return "[]".into() };
    let mut items: Vec<(String, String, &str, &str, bool)> = ui
        .functions(fk)
        .into_iter()
        .map(|(n, f)| {
            let c = ui.categories.get(&f.category);
            let category = c.map_or(String::new(), |c| c.name.clone());
            let title = if f.title.is_empty() { n.to_string() } else { f.title.clone() };
            (category, title, n, f.hint.as_str(), c.is_some_and(|c| c.show_name))
        })
        .collect();
    items.sort_by_key(|a| (a.0.to_lowercase(), a.1.to_lowercase()));
    let out: Vec<String> =
        items.iter().map(|(c, t, n, h, show)| format!("[{},{},{},{},{show}]", js(n), js(t), js(c), js(h))).collect();
    format!("[{}]", out.join(","))
}

/// The project's triggers as the map's script names them
/// (`gg_trg_<name>`, `$gg_trg_<name>` in a trigger file; World Editor's way,
/// `wtg_rs::jass::ident`).
#[derive(Debug, Clone)]
pub struct Triggers {
    /// Each trigger file with a trigger, in the order they're built (by
    /// path): its id, its trigger's name, and the script's name of it
    /// (`gg_trg_...`).
    list: Vec<(String, String, String)>,
    /// What the build names the file looked at by (its trigger; a file of
    /// variables alone, its private ones), without `gg_trg_`.
    pub mine: String,
    /// The file looked at.
    me: String,
}

impl Triggers {
    /// The triggers of the trigger file `me` (its text `text`) and the
    /// project's other files (id and text; its trigger files count). A
    /// trigger's name is its `name=`, or its file's.
    pub fn of(me: &str, text: &str, others: &[(&str, &str)]) -> Triggers {
        let mut files: Vec<(&str, &str)> = others.iter().copied().filter(|(id, _)| *id != me && is_trg_file(Path::new(id))).collect();
        files.push((me, text));
        files.sort_by(|a, b| a.0.cmp(b.0));
        let stem = |id: &str| Path::new(id).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        // Each trigger: its name, and what the script names it by.
        let list: Vec<(String, String, String)> = files
            .iter()
            .filter_map(|&(id, t)| {
                // A comment isn't a trigger: the script has nothing of it
                // (a disabled one's gg_trg_ it declares).
                let trigger = wtg_rs::text::trigger_from_text(t, None).trigger.filter(|t| t.is_comment == 0)?;
                let name = if trigger.name.is_empty() { stem(id) } else { trigger.name };
                let own = format!("gg_trg_{}", wtg_rs::jass::ident(&name));
                Some((id.to_string(), name, own))
            })
            .collect();
        let mine = wtg_rs::jass::ident(&list.iter().find(|(id, _, _)| id == me).map_or_else(|| stem(me), |(_, name, _)| name.clone()));
        Triggers { list, mine, me: me.to_string() }
    }

    /// These triggers looked at from the file `me` (one of them, or of
    /// variables alone): what [`Triggers::of`] gives for it, without
    /// reading the files again.
    pub fn looking_at(&self, me: &str) -> Triggers {
        let stem = Path::new(me).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let name = self.list.iter().find(|(id, _, _)| id == me).map_or(stem, |(_, name, _)| name.clone());
        Triggers { list: self.list.clone(), mine: wtg_rs::jass::ident(&name), me: me.to_string() }
    }

    /// The triggers (their files) the reference `r` (`gg_trg_...`) means:
    /// one, none, or several when the script would name them alike
    /// (`Шляпа` and `Кепка` are both `gg_trg_` and ten `_` and `u`).
    fn resolve(&self, r: &str) -> Vec<&str> {
        self.list.iter().filter(|(_, _, own)| own == r).map(|(id, _, _)| id.as_str()).collect()
    }

    /// What the script names the trigger of the file `id`, `gg_trg_...`.
    fn own(&self, id: &str) -> Option<&str> {
        self.list.iter().find(|(f, _, _)| f == id).map(|(_, _, own)| own.as_str())
    }

    /// The other trigger files whose trigger the script names as the file
    /// `id`'s.
    fn named_alike(&self, id: &str) -> Vec<&str> {
        let Some(own) = self.own(id) else { return Vec::new() };
        self.list.iter().filter(|(f, _, o)| f != id && o == own).map(|(f, _, _)| f.as_str()).collect()
    }

    /// Where the reference `r` must change, from these triggers (`self`,
    /// before) to `after`: the trigger it meant alone, renamed, by its new
    /// name in the script.
    fn retarget(&self, r: &str, after: &Triggers) -> Option<String> {
        let [meant] = self.resolve(r)[..] else { return None };
        let now = after.own(meant).filter(|now| *now != r)?;
        // Never to a name that means another too.
        (after.resolve(now) == [meant]).then(|| now.to_string())
    }
}

/// The trigger file `text` with its references to triggers retargeted
/// from `before` to `after` ([`Triggers::retarget`]): `$gg_trg_...`, the
/// variable tokens its reader finds, and names in custom script, the
/// tokens the JASS lexer finds in a `CustomScriptCode`'s string.
fn retarget_trg(text: &str, before: &Triggers, after: &Triggers) -> String {
    use parser_rs::SemanticType;
    let tokens = wtg_rs::text::trigger_from_text(text, None).tokens;
    text.split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            let mut chars: Vec<char> = line.chars().collect();
            let script = eca_line(line).is_some_and(|(_, n)| n == "CustomScriptCode");
            let mut edits: Vec<(usize, usize, String)> = Vec::new();
            for t in tokens.iter().filter(|t| t.line == i + 1) {
                let (a, b) = (t.start_col - 1, t.end_col - 1);
                let Some(word) = chars.get(a..b).map(|c| c.iter().collect::<String>()) else { continue };
                if t.ty == SemanticType::Variable
                    && let Some(r) = word.strip_prefix('$').filter(|r| r.starts_with("gg_trg_"))
                    && let Some(new) = before.retarget(r, after)
                {
                    edits.push((a, b, format!("${new}")));
                } else if t.ty == SemanticType::String
                    && script
                    && let Ok(p) = wtg_rs::text::read_param(&word)
                    && let Some(code) = retarget_jass(&p.value, before, after)
                {
                    edits.push((a, b, wtg_rs::text::jass_string(&code)));
                }
            }
            edits.sort_by_key(|e| e.0);
            for (a, b, new) in edits.into_iter().rev() {
                chars.splice(a..b, new.chars());
            }
            chars.into_iter().collect::<String>()
        })
        .collect()
}

/// JASS with its names of triggers retargeted (the lexer's name tokens),
/// if any is.
fn retarget_jass(code: &str, before: &Triggers, after: &Triggers) -> Option<String> {
    let lexed = parser_rs::tokenize_lossy(code, &jass_rs::lexer::VJASS_SPEC);
    let names = lexed.tokens.iter().filter_map(|t| match &t.kind {
        parser_rs::TokenKind::Ident(n) if n.starts_with("gg_trg_") => Some((t.line, t.col, t.end_col, n.as_str())),
        _ => None,
    });
    let found: Vec<(usize, usize, usize, String)> = names.filter_map(|(l, a, b, n)| Some((l, a, b, before.retarget(n, after)?))).collect();
    (!found.is_empty()).then(|| replace_at(code, &found))
}

/// `text` with each `(line, col, end_col, new)` (from 1, in characters)
/// replaced.
fn replace_at(text: &str, at: &[(usize, usize, usize, String)]) -> String {
    text.split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            let mut chars: Vec<char> = line.chars().collect();
            let mut here: Vec<&(usize, usize, usize, String)> = at.iter().filter(|e| e.0 == i + 1).collect();
            here.sort_by_key(|e| e.1);
            for (_, a, b, new) in here.into_iter().rev() {
                if *b - 1 <= chars.len() {
                    chars.splice(a - 1..b - 1, new.chars());
                }
            }
            chars.into_iter().collect::<String>()
        })
        .collect()
}

/// The project's files whose references to triggers change when the
/// trigger file `me` becomes `new` (from `old`): trigger files
/// ([`retarget_trg`]), code (the names the index found): id, old text,
/// new text; and `me`'s new text.
fn retarget_project(me: &str, old: &str, new: &str, project: &Project) -> (String, Vec<(String, String, String)>) {
    let pairs: Vec<(&str, &str)> = project.others.iter().map(|&(id, t, _)| (id, t)).collect();
    let before = Triggers::of(me, old, &pairs);
    let after = Triggers::of(me, new, &pairs);
    let mine = retarget_trg(new, &before, &after);
    let mut files = Vec::new();
    for &(id, t, names) in &project.others {
        let changed = if is_trg_file(Path::new(id)) {
            retarget_trg(t, &before, &after)
        } else {
            let at: Vec<(usize, usize, usize, String)> = names
                .iter()
                .filter(|o| o.name.starts_with("gg_trg_"))
                .filter_map(|o| Some((o.line, o.col, o.end_col, before.retarget(&o.name, &after)?)))
                .collect();
            if at.is_empty() { continue } else { replace_at(t, &at) }
        };
        if changed != t {
            files.push((id.to_string(), t.to_string(), changed));
        }
    }
    (mine, files)
}

/// The JASS the trigger file `me` (its text `text`) is built into
/// (`wtg_rs::jass::compile`; a trigger without a name is named as the
/// file), with the variables it sees (`visible`: its own and the
/// project's globals).
pub fn jass(text: &str, me: &str, names: &crate::names::Names, visible: &[Declared]) -> String {
    let p = wtg_rs::text::trigger_from_text(text, None);
    let mut out = String::new();
    if !p.errors.is_empty() {
        out += &format!("// The text has {} problem(s); what reads is built:\n", p.errors.len());
        for e in p.errors.iter().take(5) {
            out += &format!("//   line {}: {}\n", e.line, e.message);
        }
        out += "\n";
    }
    let stem = Path::new(me).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    out + &wtg_rs::jass::compile(p.trigger.as_ref(), &stem, &p.variables, visible, &names.trigger_ui)
}

/// A trigger's JASS ([`jass`], [`wtg_jass`]) in `script` (Lua, UjAPI's
/// AngelScript), as a build makes it (`wc3_build`): the game's natives and
/// functions from `common_j` and `blizzard_j`, `common.j`'s names in
/// AngelScript's `Jass::`. Only the trigger's own code is written:
/// `Blizzard.j`'s functions it calls aren't (a build writes them once, for
/// the whole map). What's wrong (an `ExecuteFunc` whose function can't be
/// told) is said at the top, the JASS's lines with it; what can't be
/// built, instead of the code.
pub fn converted(jass: &str, script: wc3_build::jass::Script, common_j: &str, blizzard_j: &str) -> String {
    use wc3_build::jass::Script;
    let comment = if script == Script::Lua { "--" } else { "//" };
    let said = |e: &wc3_build::ir::Error| if e.origin.line > 0 { format!("line {} of its JASS: {}", e.origin.line, e.message) } else { e.message.clone() };
    let source = wc3_build::from_jass::Source { name: "trigger.j", text: jass };
    let game: Vec<wc3_build::ir::Signature> = [common_j, blizzard_j].iter().flat_map(|t| wc3_build::from_jass::signatures(t)).collect();
    let options = wc3_build::jass::Options { game, script, common_j: if script == Script::AngelScript { common_j.to_string() } else { String::new() }, ..Default::default() };
    match wc3_build::from_jass::module(&[source]).and_then(|m| wc3_build::jass::build_with(&m, &options)) {
        Ok(built) => {
            let mut out = String::new();
            if !built.problems.is_empty() {
                out += &format!("{comment} {} error(s):\n", built.problems.len());
                for e in &built.problems {
                    out += &format!("{comment}   {}\n", said(e));
                }
                out += "\n";
            }
            out + &built.script
        }
        Err(errors) => {
            let mut out = format!("{comment} It can't be built:\n");
            for e in &errors {
                out += &format!("{comment}   {}\n", said(e));
            }
            out
        }
    }
}

/// The trigger editor's icons, as JSON: `{"ReplaceableTextures\\WorldEditUI\\Actions-Unit":
/// "data:image/png;base64,...", "SEIcon_Event": "data:...", ...}` (see
/// `names::Names::trigger_icons`), by key in order.
pub fn icons(names: &crate::names::Names) -> String {
    let js = crate::objects::json_string;
    let mut keys: Vec<&String> = names.trigger_icons.keys().collect();
    keys.sort();
    let items: Vec<String> = keys.iter().map(|k| format!("{}:{}", js(k), js(&names.trigger_icons[*k]))).collect();
    format!("{{{}}}", items.join(","))
}

// ------------------------------------------------- the triggers of a map
//
// The text of a `war3map.wtg` opens as World Editor shows it too: a tree
// of its categories, triggers and variables, and the trigger picked in it
// as a `.trg` shows. Each trigger's `[trigger]` section is cut out of the
// text ([`Slice`], without its `category=`, which a trigger of its own
// doesn't have) and is, for what's done to it, a trigger file of its own
// in a project of them all ([`Map::project`]): the picked one is seen and
// edited as a `.trg` is ([`view`], [`apply`]), a rename retargets the
// others' references, a variable's change renames its uses in them; then
// what changed is put back, and the edits are of the whole text. The page
// speaks in the lines of the picked trigger's section; `at` maps them to
// the text's. Only what World Editor's triggers have is offered: the
// variables are the file's globals, none private or local.
//
// The tree's own operations ([`wtg_edit`]'s `NEW_TRIGGER` ...) change the
// text's sections: a trigger, a category, a variable.

/// A trigger's `[trigger]` section of the text of a `.wtg`.
struct Slice {
    /// Its lines in the text's, its header first.
    range: std::ops::Range<usize>,
    /// Its `category=` line, left out of `text`.
    category: Option<usize>,
    /// The lines left out of `text` (`category=`; a 1.31 tree's `id=` and
    /// `element=`, the file's, not a `.trg`'s), in order.
    hidden: Vec<usize>,
    /// The section as a `.trg`'s text.
    text: String,
    /// Its name, as `name=` says.
    name: String,
}

impl Slice {
    /// The text's line (from 1) of the section's line `k` (from 1).
    fn line(&self, k: usize) -> usize {
        let mut i = self.range.start + k - 1;
        for &h in &self.hidden {
            if h <= i {
                i += 1;
            }
        }
        i + 1
    }

    /// The section's lines in `lines` (the text's) become `new` (as
    /// `text`'s), its `category=` put back where it was, before the first
    /// ECA at the latest; and where `new`'s index `shown` is in the text.
    fn put(&self, lines: &mut Vec<String>, new: Vec<String>, shown: Option<usize>) -> Option<usize> {
        let mut section = new;
        let mut shown = shown;
        for &c in &self.hidden {
            let first_eca = (1..section.len()).find(|&i| eca_line(&section[i]).is_some()).unwrap_or(section.len());
            let at = (c - self.range.start).min(first_eca).max(1).min(section.len());
            section.insert(at, lines[c].clone());
            shown = shown.map(|s| if s >= at { s + 1 } else { s });
        }
        lines.splice(self.range.clone(), section);
        shown.map(|s| s + self.range.start)
    }

    /// The section's lines in `lines` become `new`, line for line (a
    /// rename: lines change, none come or go).
    fn put_lines(&self, lines: &mut [String], new: &str) {
        for (k, l) in new.split_inclusive('\n').enumerate() {
            lines[self.line(k + 1) - 1] = l.to_string();
        }
    }
}

/// The text of a `.wtg`, cut into its sections.
struct Map {
    lines: Vec<String>,
    eol: &'static str,
    /// Each section: its name and its lines (header first, up to the next
    /// header: the blank lines after it are its).
    sections: Vec<(String, std::ops::Range<usize>)>,
    /// Each trigger, in order.
    triggers: Vec<Slice>,
}

/// The name of a section's header line (`[trigger]`: `trigger`).
fn header_name(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix('[')?;
    Some(rest.split_once(']').map_or(rest, |(n, _)| n).trim())
}

/// The value of the field `key` in the lines `range` of `lines`, raw.
fn field_in<'a>(lines: &'a [String], range: std::ops::Range<usize>, key: &str) -> Option<(usize, &'a str)> {
    range.into_iter().find(|&i| field_key(&lines[i]) == Some(key)).map(|i| {
        let (_, v) = lines[i].split_once('=').expect("a field");
        (i, v.trim_end_matches(['\n', '\r']).trim())
    })
}

impl Map {
    fn of(text: &str) -> Map {
        let lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let starts: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].trim_start().starts_with('[')).collect();
        let sections: Vec<(String, std::ops::Range<usize>)> = starts
            .iter()
            .enumerate()
            .map(|(k, &s)| {
                let end = starts.get(k + 1).copied().unwrap_or(lines.len());
                (header_name(&lines[s]).unwrap_or("").to_string(), s..end)
            })
            .collect();
        let triggers = sections
            .iter()
            .filter(|(n, _)| n == wtg_rs::text::TRIGGER)
            .map(|(_, r)| {
                let category = field_in(&lines, r.start + 1..r.end, "category").map(|(i, _)| i);
                let hidden: Vec<usize> = (r.start + 1..r.end).filter(|&i| matches!(field_key(&lines[i]), Some("category" | "id" | "element"))).collect();
                let text = r.clone().filter(|i| !hidden.contains(i)).map(|i| lines[i].as_str()).collect();
                let name = field_in(&lines, r.start + 1..r.end, "name").and_then(|(_, v)| wtg_rs::text::read_string(v)).unwrap_or_default();
                Slice { range: r.clone(), category, hidden, text, name }
            })
            .collect();
        Map { lines, eol, sections, triggers }
    }

    /// Which trigger `nth` (from 1) picks: itself, or the first; from 1,
    /// 0 when there's none.
    /// Whether it's 1.31's layout (`tree=1` in `[wtg]`).
    fn is_tree(&self) -> bool {
        self.sections.iter().find(|(n, _)| n == "wtg").is_some_and(|(_, r)| field_in(&self.lines, r.start + 1..r.end, "tree").is_some_and(|(_, v)| v != "0"))
    }

    /// Whether each trigger is a comment.
    fn comments(&self) -> Vec<bool> {
        self.triggers
            .iter()
            .map(|t| field_in(&self.lines, t.range.start + 1..t.range.end, "comment").is_some_and(|(_, v)| v != "0"))
            .collect()
    }

    fn pick(&self, nth: usize) -> usize {
        if (1..=self.triggers.len()).contains(&nth) { nth } else { (!self.triggers.is_empty()) as usize }
    }

    /// The trigger file a trigger is in the project of them all.
    fn id(&self, nth: usize) -> String {
        format!("{} #{nth}.trg", self.triggers[nth - 1].name)
    }

    /// Which trigger the file `id` of [`Map::id`] is.
    fn nth_of(id: &str) -> Option<usize> {
        id.strip_suffix(".trg")?.rsplit_once('#')?.1.parse().ok()
    }

    /// The triggers as a project's trigger files, but `except` (from 1).
    fn project<'a>(&'a self, ids: &'a [String], except: usize) -> Vec<(&'a str, &'a str, &'a [parser_rs::Occurrence])> {
        (1..=self.triggers.len())
            .filter(|&n| n != except)
            .map(|n| (ids[n - 1].as_str(), self.triggers[n - 1].text.as_str(), &[][..]))
            .collect()
    }

    /// The `[variable]` sections, as one trigger file's text, and the
    /// text's line (from 1) of each of its lines.
    fn variables_file(&self) -> (String, Vec<usize>) {
        let mut text = String::new();
        let mut at = Vec::new();
        for (_, r) in self.sections.iter().filter(|(n, _)| n == wtg_rs::text::VARIABLE) {
            for i in r.clone() {
                text.push_str(&self.lines[i]);
                at.push(i + 1);
            }
            if !text.ends_with('\n') {
                text.push_str(self.eol);
            }
        }
        (text, at)
    }

    /// Puts the lines of a new section in at `at`, a blank line between it
    /// and the sections around it.
    fn insert(&mut self, at: usize, section: &str) {
        let eol = self.eol;
        if let Some(last) = at.checked_sub(1).map(|i| &mut self.lines[i])
            && !last.ends_with('\n')
        {
            last.push_str(eol);
        }
        let mut block = Vec::new();
        if at > 0 && !self.lines[at - 1].trim().is_empty() {
            block.push(eol.to_string());
        }
        block.extend(section.lines().map(|l| format!("{l}{eol}")));
        if at < self.lines.len() {
            block.push(eol.to_string());
        }
        self.lines.splice(at..at, block);
    }

    /// Takes the section `range` out (what's left of it, when the text
    /// ended before its end: sections are taken out from the bottom up);
    /// at the end of the text, the blank lines before it too.
    fn remove(&mut self, range: std::ops::Range<usize>) {
        let end = range.end.min(self.lines.len());
        let mut start = range.start.min(end);
        if end == self.lines.len() {
            while start > 0 && self.lines[start - 1].trim().is_empty() {
                start -= 1;
            }
        }
        self.lines.drain(start..end);
    }

    /// The categories, in order: index, name, and the section's lines.
    fn categories(&self) -> Vec<(i32, String, std::ops::Range<usize>)> {
        self.sections
            .iter()
            .filter(|(n, _)| n == "category")
            .map(|(_, r)| {
                let index = field_in(&self.lines, r.start + 1..r.end, "index").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
                let name = field_in(&self.lines, r.start + 1..r.end, "name").and_then(|(_, v)| wtg_rs::text::read_string(v)).unwrap_or_default();
                (index, name, r.clone())
            })
            .collect()
    }

    /// Where a trigger goes at the end of the category `index`: after its
    /// last trigger, or the last of a category before it (one the text
    /// hasn't is at the top); and which trigger it then is, from 1.
    fn end_of_category(&self, index: i32) -> (usize, usize) {
        let categories = self.categories();
        let place = |i: i32| categories.iter().position(|c| c.0 == i);
        let mine = place(index);
        let before = self.triggers.iter().enumerate().rev().find(|(_, t)| {
            let theirs = place(self.category_of(t));
            theirs.is_none() || (mine.is_some() && theirs <= mine)
        });
        match before {
            Some((k, t)) => (t.range.end, k + 2),
            None => (self.triggers.first().map_or(self.lines.len(), |t| t.range.start), 1),
        }
    }

    /// The category of the trigger `t`.
    fn category_of(&self, t: &Slice) -> i32 {
        t.category.and_then(|c| field_in(&self.lines, c..c + 1, "category")).and_then(|(_, v)| v.parse().ok()).unwrap_or(0)
    }

    /// Where a section after the last of `names` goes: past it, else
    /// before the first trigger, else at the end.
    fn after_last(&self, names: &[&str]) -> usize {
        let last = self.sections.iter().rev().find(|(n, _)| names.contains(&n.as_str()));
        let first_trigger = self.triggers.first().map(|t| t.range.start);
        match last {
            Some((_, r)) => r.end,
            None => first_trigger.unwrap_or(self.lines.len()),
        }
    }
}

/// The key of a `key=value` line, trimmed.
fn field_key(line: &str) -> Option<&str> {
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    (!key.is_empty() && !key.starts_with(['[', ';', '>', '!'])).then_some(key)
}

/// The variables of the text of a `.wtg` (`doc` read from `text`), each
/// with the line (from 1) of its `[variable]`: the file's globals.
fn wtg_variables(text: &str, doc: &bni_rs::ast::Document) -> Vec<(usize, Declared)> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let headers: Vec<usize> = doc.sections.iter().map(|s| s.line).collect();
    let mut out = Vec::new();
    for s in doc.sections.iter().filter(|s| s.name.text == wtg_rs::text::VARIABLE) {
        let end = headers.iter().copied().find(|&h| h > s.line).map_or(lines.len(), |h| h - 1);
        let own: String = lines[s.line - 1..end].concat();
        if let Some(d) = wtg_rs::text::trigger_from_text(&own, None).variables.into_iter().next() {
            out.push((s.line, d));
        }
    }
    out
}

/// The view of the text of a `.wtg` (`text`), its `nth` trigger (from 1;
/// 0: the first) picked, as JSON:
///
/// ```json
/// {"categories": [{"line": 3, "index": 0, "name": "Init", "comment": 0}],
///  "triggers": [{"line": 9, "name": "Spawn", "category": 0, "enabled": 1,
///   "comment": 0, "custom": 0}],
///  "variables": [{"line": 6, "scope": "global", "name": "hero", "type": "unit",
///   "type_name": "Unit", "array": false, "size": 1, "initial": null}],
///  "picked": 1, "at": [9, 10, 12], "view": {...}}
/// ```
///
/// `picked` is which trigger is (from 1; 0: none), `view` its view as
/// [`view`] gives a `.trg`'s, in the lines of its section (`null` without
/// one), and `at` the text's line of each (`at[k - 1]` for its line `k`).
/// A trigger is picked by its place among the triggers, not by its line:
/// typing in the text above it doesn't pick another.
///
/// With the text form of the map's custom script beside it (`wct`, the
/// `war3map.wct` the triggers' code of custom text is in, by the
/// triggers' places): `"wct": {"header": [from, to], "code": [[from, to,
/// none], ...], "problem": ...}`, where in it the map's custom script and
/// each trigger's piece are (lines from 0: the marker's, and the end of
/// its code; `none` a piece that stores none), and what's wrong when the
/// two don't go together (`null` when they do).
pub fn wtg_view(text: &str, nth: usize, names: &crate::names::Names, wct: Option<&str>) -> String {
    let js = crate::objects::json_string;
    let doc = wtg_rs::text::parse_bni(text);
    let get = |s: &bni_rs::ast::Section, k: &str| s.items.iter().find(|i| i.key.as_ref().is_some_and(|n| n.text.trim() == k)).map(|i| i.value_text());
    let int = |s: &bni_rs::ast::Section, k: &str, default: i64| get(s, k).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
    let string = |s: &bni_rs::ast::Section, k: &str| get(s, k).and_then(|v| wtg_rs::text::read_string(&v)).unwrap_or_default();
    let categories: Vec<String> = doc
        .sections
        .iter()
        .filter(|s| s.name.text == "category")
        .map(|s| format!("{{\"line\":{},\"index\":{},\"name\":{},\"comment\":{}}}", s.line, int(s, "index", 0), js(&string(s, "name")), int(s, "comment", 0)))
        .collect();
    let triggers: Vec<String> = doc
        .sections
        .iter()
        .filter(|s| s.name.text == wtg_rs::text::TRIGGER)
        .map(|s| {
            format!(
                "{{\"line\":{},\"name\":{},\"category\":{},\"enabled\":{},\"comment\":{},\"custom\":{}}}",
                s.line,
                js(&string(s, "name")),
                int(s, "category", 0),
                int(s, "enabled", 1),
                int(s, "comment", 0),
                int(s, "custom_text", 0),
            )
        })
        .collect();
    let declared = wtg_variables(text, &doc);
    let ui = &names.trigger_ui;
    let variables: Vec<String> = declared
        .iter()
        .map(|(line, d)| {
            let var = &d.variable;
            let type_name = ui.types.get(&var.kind).cloned().unwrap_or_else(|| var.kind.clone());
            format!(
                "{{\"line\":{line},\"scope\":\"global\",\"name\":{},\"type\":{},\"type_name\":{},\"array\":{},\"size\":{},\"initial\":{}}}",
                js(&var.name),
                js(&var.kind),
                js(&type_name),
                var.is_array != 0,
                var.array_size,
                if var.has_initial != 0 { js(&var.initial) } else { "null".into() },
            )
        })
        .collect();
    let visible: Vec<Declared> = declared.into_iter().map(|(_, d)| d).collect();
    let map = Map::of(text);
    let picked = map.pick(nth);
    let (at, view_json) = match picked {
        0 => (String::new(), "null".into()),
        n => {
            let slice = &map.triggers[n - 1];
            let count = slice.text.split_inclusive('\n').count();
            let at: Vec<String> = (1..=count).map(|k| slice.line(k).to_string()).collect();
            (at.join(","), view(&slice.text, names, &visible, &wtg_triggers(&doc, n)))
        }
    };
    let wct_json = match wct {
        None => "null".to_string(),
        Some(w) => {
            let p = crate::customtext::pieces(w);
            let header = p.header.map_or("null".into(), |(a, b)| format!("[{a},{b}]"));
            // Each trigger's (a 1.31 comment's: none, null).
            let (code, problem): (Vec<String>, _) = match p.of_triggers(&map.comments()) {
                Ok(each) => (each.iter().map(|x| x.map_or("null".into(), |(a, b, none)| format!("[{a},{b},{none}]"))).collect(), "null".into()),
                Err(e) => (p.triggers.iter().map(|(a, b, none)| format!("[{a},{b},{none}]")).collect(), js(&e)),
            };
            format!("{{\"header\":{header},\"code\":[{}],\"problem\":{problem}}}", code.join(","))
        }
    };
    format!(
        "{{\"categories\":[{}],\"triggers\":[{}],\"variables\":[{}],\"picked\":{picked},\"at\":[{at}],\"view\":{view_json},\"wct\":{wct_json}}}",
        categories.join(","),
        triggers.join(","),
        variables.join(","),
    )
}

/// What's wrong when the custom script's pieces `p` don't go with the
/// map's triggers (`comments`, whether each is one): each has one, in the
/// same order (in 1.31's layout, but comments).
fn wct_problem(p: &crate::customtext::Pieces, comments: &[bool]) -> Option<String> {
    p.of_triggers(comments).err()
}

/// The triggers of the text of a `.wtg` (`doc`), looked at from its `me`th
/// (from 1), as [`Triggers`] has a project's files (named as [`Map::id`]),
/// read from the sections at once.
fn wtg_triggers(doc: &bni_rs::ast::Document, me: usize) -> Triggers {
    let mut mine = String::new();
    let mut me_id = String::new();
    let list: Vec<(String, String, String)> = doc
        .sections
        .iter()
        .filter(|s| s.name.text == wtg_rs::text::TRIGGER)
        .enumerate()
        .filter_map(|(k, s)| {
            let get = |key: &str| s.items.iter().find(|i| i.key.as_ref().is_some_and(|n| n.text.trim() == key)).map(|i| i.value_text());
            let name = get("name").and_then(|v| wtg_rs::text::read_string(&v)).unwrap_or_default();
            let id = format!("{name} #{}.trg", k + 1);
            if k + 1 == me {
                mine = wtg_rs::jass::ident(&name);
                me_id = id.clone();
            }
            // A comment isn't a trigger: the script has nothing of it.
            if get("comment").is_some_and(|v| v.trim() != "0") {
                return None;
            }
            let own = format!("gg_trg_{}", wtg_rs::jass::ident(&name));
            Some((id, name, own))
        })
        .collect();
    Triggers { list, mine, me: me_id }
}

/// The operations of a map's tree, past [`op`]'s, in a `.wtg`'s text only
/// ([`wtg_edit`]).
pub mod map_op {
    /// A new trigger: the argument is its category's index, a line break,
    /// its name. It goes after the category's last trigger (or the last
    /// trigger of a category before it); the answer's `pick` is it.
    pub const NEW_TRIGGER: u32 = 20;
    /// A new category: the argument is its name; its index is one past
    /// the greatest. It goes after the last category.
    pub const NEW_CATEGORY: u32 = 21;
    /// Delete the picked trigger; the answer's `pick` is the one now in
    /// its place (or the last).
    pub const DELETE_TRIGGER: u32 = 22;
    /// Delete the category whose index is the argument, and its triggers.
    pub const DELETE_CATEGORY: u32 = 23;
    /// Rename the category whose index is the argument's first line to its
    /// second.
    pub const RENAME_CATEGORY: u32 = 24;
    /// Delete the variable whose `[variable]` is at the line (of the
    /// text). `CHECK_VARIABLE` with an empty argument says what it breaks.
    pub const DELETE_VARIABLE: u32 = 25;
    /// Move a trigger: the argument is which (from 1), a line break, the
    /// category it goes in (its index), a line break, the trigger it goes
    /// before (from 1, counted before the move; 0: at the end of the
    /// category). The answer's `pick` is it, where it is then.
    pub const MOVE_TRIGGER: u32 = 26;
    /// Convert to Custom Text: the trigger (a `.trg`, or the one picked in
    /// a map's) made custom text, its code the JASS World Editor's script
    /// has of it ([`convert_trg`], [`convert_wtg`]).
    pub const CONVERT: u32 = 27;
}

// ------------------------------------------------ convert to custom text

/// The code a trigger is when made custom text, as World Editor's Convert
/// to Custom Text makes it: the game's JASS of its functions (as the
/// build makes them: `wtg_rs::jass::compile`, then `wc3_build`), its set
/// up an `InitTrig_<name>` that makes `gg_trg_<name>` first (which World
/// Editor's script declares); a disabled one's as if it were on. `own` and
/// `visible` as [`jass`]'s: a private variable of its file is declared in
/// the code (vJASS's `globals`), a local is a local.
pub fn custom_text_code(t: &wtg_rs::Trigger, stem: &str, own: &[Declared], visible: &[Declared], ui: &wtg_rs::ui::Ui) -> Result<String, String> {
    if t.is_comment != 0 {
        return Err("a comment isn't a trigger: it has no code".into());
    }
    if t.is_custom != 0 {
        return Err("the trigger is custom text already".into());
    }
    let mut on = t.clone();
    on.enabled = 1;
    let jass = wtg_rs::jass::compile(Some(&on), stem, own, visible, ui);
    let source = wc3_build::from_jass::Source { name: "trigger.j", text: &jass };
    let mut m = wc3_build::from_jass::module(&[source]).map_err(|e| e.first().map_or("its JASS doesn't read".into(), |e| format!("its JASS: line {}: {}", e.origin.line, e.message)))?;
    let id = wtg_rs::jass::ident(if t.name.is_empty() { stem } else { &t.name });
    let trig = format!("gg_trg_{id}");
    let create = m.globals.iter().find(|g| g.name == trig).and_then(|g| g.init.clone()).unwrap_or_else(|| wc3_build::ir::Expr::call("CreateTrigger", Vec::new()));
    m.globals.retain(|g| g.name != trig);
    for f in m.functions.iter_mut().filter(|f| f.sig.name == "main") {
        f.sig.name = format!("InitTrig_{id}");
        f.body.insert(0, wc3_build::ir::Stmt::set(trig.clone(), create.clone()));
    }
    // Its heading, as World Editor's: the comment lines before its code.
    let heading: String = jass.lines().take_while(|l| l.starts_with("//")).map(|l| format!("{l}\n")).collect();
    Ok(format!("{heading}{}", wc3_build::jass::print_code(&m)))
}

/// Convert to Custom Text of a `.trg` (`text`, its file `path`): its
/// edits, the trigger custom text, its ECAs gone, the variables private or
/// local to it gone (its code declares them), its globals kept; and its
/// code as the file beside it (`X.j`, as Extract writes one: `"create":
/// [path, text]`). `visible`: the project's globals and its own.
pub fn convert_trg(text: &str, path: &str, visible: &[Declared], ui: &wtg_rs::ui::Ui) -> String {
    let js = crate::objects::json_string;
    let p = wtg_rs::text::trigger_from_text(text, None);
    let made = (|| {
        if let Some(e) = p.errors.first() {
            return Err(format!("line {}: {}", e.line, e.message));
        }
        let t = p.trigger.as_ref().ok_or("the file has no trigger")?;
        let code_path = Path::new(path).with_extension("j");
        if code_path.exists() {
            return Err(format!("{} is there already: its code would be lost", code_path.display()));
        }
        let stem = Path::new(path).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let own: Vec<Declared> = p.variables.iter().filter(|d| d.scope != Scope::Global).cloned().collect();
        let code = custom_text_code(t, &stem, &own, visible, ui)?;
        let mut t = t.clone();
        // Run at map initialization: as custom text, by its flag.
        if t.ecas.iter().any(|e| e.kind == 0 && e.enabled != 0 && e.name == "MapInitializationEvent") {
            t.run_on_init = 1;
        }
        t.is_custom = 1;
        t.ecas.clear();
        let globals: Vec<Declared> = p.variables.iter().filter(|d| d.scope == Scope::Global).cloned().collect();
        let head = if globals.is_empty() { String::new() } else { wtg_rs::text::declared_to_text(&globals)? + "\n" };
        let new = head + &wtg_rs::text::trigger_to_text(&t)?;
        Ok((new, code_path.to_string_lossy().into_owned(), format!("// @dialect vjass\n{code}")))
    })();
    match made {
        Ok((new, code_path, code)) => {
            let end = text.split_inclusive('\n').count();
            format!("{{\"edits\":[[0,0,{end},0,{}]],\"line\":0,\"create\":[{},{}]}}", js(&new), js(&code_path), js(&code))
        }
        Err(e) => format!("{{\"error\":{}}}", js(&format!("Can't convert to custom text: {e}"))),
    }
}

/// Convert to Custom Text of the trigger picked in a map's text (`text`,
/// `nth` from 1), its custom script `wct` beside (the text form): the
/// edits of both, its section custom text without ECAs, its piece of
/// `war3map.wct` its code.
pub fn convert_wtg(text: &str, nth: usize, ui: &wtg_rs::ui::Ui, wct: Option<&str>) -> String {
    let js = crate::objects::json_string;
    let made = (|| {
        let map = Map::of(text);
        let nth = map.pick(nth);
        let slice = map.triggers.get(nth.wrapping_sub(1)).ok_or("no trigger picked")?;
        let wct = wct.ok_or("the map has no custom script (war3map.wct) beside its triggers: its code would have nowhere to go")?;
        let pieces = crate::customtext::pieces(wct);
        let each = pieces.of_triggers(&map.comments())?;
        let p = wtg_rs::text::trigger_from_text(&slice.text, None);
        if let Some(e) = p.errors.first() {
            return Err(format!("line {}: {}", slice.line(e.line), e.message));
        }
        let t = p.trigger.as_ref().ok_or("the section has no trigger")?;
        let visible: Vec<Declared> = wtg_variables(text, &wtg_rs::text::parse_bni(text)).into_iter().map(|(_, d)| d).collect();
        let code = custom_text_code(t, &slice.name, &[], &visible, ui)?;
        let mut t = t.clone();
        // Run at map initialization: as custom text, by its flag.
        if t.ecas.iter().any(|e| e.kind == 0 && e.enabled != 0 && e.name == "MapInitializationEvent") {
            t.run_on_init = 1;
        }
        t.is_custom = 1;
        t.ecas.clear();
        let mut new: Vec<String> = wtg_rs::text::trigger_to_text(&t)?.split_inclusive('\n').map(String::from).collect();
        // The blank line after its section, as it had.
        if slice.text.ends_with("\n\n") || slice.text.ends_with("\n\r\n") {
            new.push("\n".into());
        }
        let mut lines = map.lines.clone();
        slice.put(&mut lines, new, None);
        // Its piece of the custom script: the code.
        let (marker, end, _) = each[nth - 1].ok_or("a comment has no code")?;
        let mut code_lines: Vec<String> = wct.split_inclusive('\n').map(String::from).collect();
        let piece = format!("{} @wct trigger\n{}\n", pieces.comment, code.trim_end_matches('\n'));
        code_lines.splice(marker..end, piece.split_inclusive('\n').map(String::from));
        Ok((lines, wct.to_string(), code_lines, nth))
    })();
    let edit = |text: &str, new: &[String]| {
        let old: Vec<&str> = text.split_inclusive('\n').collect();
        diff(text, &old, new).map_or("[]".into(), |(r, t)| format!("[[{},{},{},{},{}]]", r.start.line, r.start.character, r.end.line, r.end.character, js(&t)))
    };
    match made {
        Ok((lines, wct, code_lines, nth)) => format!("{{\"edits\":{},\"line\":0,\"pick\":{nth},\"wct\":{}}}", edit(text, &lines), edit(&wct, &code_lines)),
        Err(e) => format!("{{\"error\":{}}}", js(&format!("Can't convert to custom text: {e}"))),
    }
}

/// What the operations of [`edit`] do in a `.wtg`'s text: `arg` is which
/// trigger is picked (from 1, see [`wtg_view`]), a line break, and the
/// operation's argument; `line` is a line of its section (see
/// [`wtg_view`]), or for a variable's operations (`NEW_VARIABLE`,
/// `SET_VARIABLE`, `CHECK_VARIABLE`, [`map_op::DELETE_VARIABLE`]) the
/// text's line of its `[variable]`. And the tree's own ([`map_op`]). The
/// edits are of the whole text; the answer's `line` is of the picked
/// trigger's section, and `pick`, when there is one, the trigger to pick
/// after it. The triggers are a project of trigger files to each other:
/// renaming one retargets the others' references to it, and a variable's
/// change renames its uses in all of them.
///
/// With the text form of the map's custom script beside it (`wct`, see
/// [`wtg_view`]), an operation that adds, deletes or moves triggers moves
/// their pieces of it with them: the answer's `wct` is its edits (as
/// `edits` are the text's), or `warning` says why it couldn't be.
pub fn wtg_edit(text: &str, op: u32, line: usize, arg: &str, ui: &wtg_rs::ui::Ui, wct: Option<&str>) -> String {
    let js = crate::objects::json_string;
    let (nth, arg) = arg.split_once('\n').unwrap_or((arg, ""));
    let map = Map::of(text);
    let comments = map.comments();
    let nth = map.pick(nth.trim().parse().unwrap_or(0));
    let mut order = None;
    // A trigger renamed: its names in the script, before and after (the
    // name field is set already; the argument is the name it had).
    let renamed = (op == op::RENAMED && nth > 0).then(|| (wtg_rs::jass::ident(arg), wtg_rs::jass::ident(&map.triggers[nth - 1].name)));
    let answer = match op {
        op::NEW_VARIABLE | op::SET_VARIABLE | op::CHECK_VARIABLE | map_op::DELETE_VARIABLE => wtg_variable(map, op, line, arg, ui),
        map_op::NEW_TRIGGER..=map_op::RENAME_CATEGORY | map_op::MOVE_TRIGGER => wtg_tree(map, op, nth, arg).map(|(a, o)| {
            order = o;
            a
        }),
        _ if nth == 0 => Err("no trigger to edit: the file has none".into()),
        _ => wtg_trigger(map, op, nth, line, arg, ui),
    };
    let edit = |text: &str, new: &[String]| {
        let old: Vec<&str> = text.split_inclusive('\n').collect();
        diff(text, &old, new).map_or("[]".into(), |(r, t)| format!("[[{},{},{},{},{}]]", r.start.line, r.start.character, r.end.line, r.end.character, js(&t)))
    };
    match answer {
        Ok(Answer::Json(t)) => t,
        Ok(Answer::Lines(lines, at, pick)) => {
            let edits = edit(text, &lines);
            let pick = pick.map_or(String::new(), |p| format!(",\"pick\":{p}"));
            // The custom script's pieces with the triggers.
            let custom = match (order, wct) {
                (Some(order), Some(w)) => match wct_problem(&crate::customtext::pieces(w), &comments) {
                    Some(problem) => format!(",\"warning\":{}", js(&format!("{problem}; war3map.wct is left as it was"))),
                    None => match crate::customtext::reorder(w, &crate::customtext::pieces(w).order(&order, &comments)) {
                        Some(new) => format!(",\"wct\":{}", edit(w, &new.split_inclusive('\n').map(String::from).collect::<Vec<_>>())),
                        None => String::new(),
                    },
                },
                // Renamed: the custom script's names of it follow (its code
                // of custom text declares and sets them; any code may use
                // them).
                (None, Some(w)) => match renamed.as_ref().filter(|(old, new)| old != new) {
                    Some((old, new)) => {
                        let changed = rename_trigger_in_code(w, old, new);
                        if changed == w { String::new() } else { format!(",\"wct\":{}", edit(w, &changed.split_inclusive('\n').map(String::from).collect::<Vec<_>>())) }
                    }
                    None => String::new(),
                },
                _ => String::new(),
            };
            format!("{{\"edits\":{edits},\"line\":{at}{pick}{custom}}}")
        }
        Err(e) => format!("{{\"error\":{}}}", js(&e)),
    }
}

/// `code` with a trigger's names in the script, `old` its name there
/// before (`wtg_rs::jass::ident`), now `new`: `gg_trg_<old>` and
/// `InitTrig_<old>`, whole names only, wherever they're written (a
/// comment's or a string's too, as World Editor's own text has them).
pub fn rename_trigger_in_code(code: &str, old: &str, new: &str) -> String {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    'scan: while !rest.is_empty() {
        for (from, to) in [(format!("gg_trg_{old}"), format!("gg_trg_{new}")), (format!("InitTrig_{old}"), format!("InitTrig_{new}"))] {
            let before_ok = !out.chars().next_back().is_some_and(word);
            if before_ok && rest.starts_with(&from) && !rest[from.len()..].chars().next().is_some_and(word) {
                out.push_str(&to);
                rest = &rest[from.len()..];
                continue 'scan;
            }
        }
        let c = rest.chars().next().expect("not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// What an operation in a map's text comes to.
enum Answer {
    Json(String),
    /// The text's new lines, the line (of the picked trigger's section) to
    /// select, the trigger to pick.
    Lines(Vec<String>, usize, Option<usize>),
}

/// An operation of [`edit`] on the picked trigger, `nth`.
fn wtg_trigger(map: Map, op: u32, nth: usize, line: usize, arg: &str, ui: &wtg_rs::ui::Ui) -> Result<Answer, String> {
    let ids: Vec<String> = (1..=map.triggers.len()).map(|n| map.id(n)).collect();
    let project = Project { me: &ids[nth - 1], others: map.project(&ids, nth) };
    let slice = &map.triggers[nth - 1];
    match apply(&slice.text, op, line, arg, ui, &project)? {
        Done::Text(t) => Ok(Answer::Json(format!("{{\"text\":{}}}", crate::objects::json_string(&t)))),
        Done::Json(t) => Ok(Answer::Json(t)),
        Done::Lines(new, shown, files) => {
            // Its custom script one line, as World Editor's (what was so
            // already isn't this edit's to refuse).
            if script_lines(&new.concat()).len() > script_lines(&slice.text).len() {
                return Err(ONE_LINE.into());
            }
            let mut lines = map.lines.clone();
            // The others first: line for line, so the picked one's lines
            // stay where they are.
            for (id, _, changed) in &files {
                if let Some(other) = Map::nth_of(id).and_then(|n| map.triggers.get(n - 1)) {
                    other.put_lines(&mut lines, changed);
                }
            }
            slice.put(&mut lines, new, None);
            Ok(Answer::Lines(lines, shown.map_or(0, |i| i + 1), None))
        }
    }
}

/// A variable's operation in a map's text: its variables are the
/// `[variable]` sections, one trigger file of variables alone to the
/// project of the triggers; only globals.
fn wtg_variable(map: Map, op: u32, line: usize, arg: &str, ui: &wtg_rs::ui::Ui) -> Result<Answer, String> {
    let js = crate::objects::json_string;
    let (vars, at) = map.variables_file();
    let ids: Vec<String> = (1..=map.triggers.len()).map(|n| map.id(n)).collect();
    let project = Project { me: wtg_rs::text::VARIABLES_FILE, others: map.project(&ids, 0) };
    let global = |d: Declared| match d.scope {
        Scope::Global => Ok(d),
        _ => Err("a map's variables are globals: private and local ones are a trigger file's (.trg)".to_string()),
    };
    // The variable whose `[variable]` is at the text's `line`, and its
    // section's lines there.
    let at_line = |line: usize| -> Result<(Declared, std::ops::Range<usize>), String> {
        let (_, r) = map.sections.iter().find(|(n, r)| n == wtg_rs::text::VARIABLE && r.start + 1 == line).ok_or_else(|| format!("line {line} isn't a variable's"))?;
        let d = wtg_rs::text::trigger_from_text(&map.lines[r.clone()].concat(), None).variables.into_iter().next().ok_or("no variable there")?;
        Ok((d, r.clone()))
    };
    // A place a change breaks, in the text: its line.
    let place = |id: &str, l: usize| match Map::nth_of(id).and_then(|n| map.triggers.get(n - 1)) {
        Some(t) => t.line(l),
        None => at.get(l - 1).copied().unwrap_or(l),
    };
    // Reign of Chaos's file (version 4) has no array sizes.
    let v4 = map.sections.first().filter(|(n, _)| n == "wtg").and_then(|(_, r)| field_in(&map.lines, r.clone(), "version")).is_some_and(|(_, v)| v == "4");
    let section = |d: &Declared| -> Result<String, String> {
        let text = wtg_rs::text::declared_to_text(std::slice::from_ref(d))?;
        Ok(if v4 { text.lines().filter(|l| field_key(l) != Some("size")).map(|l| format!("{l}\n")).collect() } else { text })
    };
    let mut lines = map.lines.clone();
    match op {
        op::NEW_VARIABLE => {
            let d = global(variable_arg(arg, &vars, None)?)?;
            let at = map.after_last(&[wtg_rs::text::VARIABLE, "category", "wtg"]);
            let mut m = map;
            m.insert(at, &section(&d)?);
            lines = m.lines;
        }
        op::CHECK_VARIABLE | op::SET_VARIABLE | map_op::DELETE_VARIABLE => {
            let (old, range) = at_line(line)?;
            let new = match op {
                map_op::DELETE_VARIABLE => None,
                _ if arg.trim().is_empty() => None,
                _ => Some(global(variable_arg(arg, &vars, Some(&old.variable.name))?)?),
            };
            let change = variable_change(&vars, &old, new.as_ref(), ui, &project)?;
            if op == op::CHECK_VARIABLE {
                let places: Vec<String> = change.places.iter().map(|(id, l, why)| format!("[\"\",{},{}]", place(id, *l), js(why))).collect();
                return Ok(Answer::Json(format!("{{\"renamed\":{},\"places\":[{}]}}", change.renamed, places.join(","))));
            }
            for (id, _, changed) in &change.files {
                if let Some(t) = Map::nth_of(id).and_then(|n| map.triggers.get(n - 1)) {
                    t.put_lines(&mut lines, changed);
                }
            }
            match new {
                Some(d) => {
                    // Its own lines: not the blank ones before the next section.
                    let end = range.clone().rev().find(|&i| !lines[i].trim().is_empty()).map_or(range.end, |i| i + 1);
                    let eol = map.eol;
                    lines.splice(range.start..end, section(&d)?.lines().map(|l| format!("{l}{eol}")));
                }
                None => {
                    let mut m = Map { lines, ..map };
                    m.remove(range);
                    lines = m.lines;
                }
            }
        }
        _ => unreachable!("a variable's operation"),
    }
    Ok(Answer::Lines(lines, 0, None))
}

/// The triggers after a change, each the one it was (from 0) or a new
/// one (`None`).
type Order = Vec<Option<usize>>;

/// One of the tree's own operations ([`map_op`]); `nth` the picked
/// trigger. And the triggers' order after it, when it changes.
fn wtg_tree(mut map: Map, op: u32, nth: usize, arg: &str) -> Result<(Answer, Option<Order>), String> {
    let count = map.triggers.len();
    let all = || (0..count).map(Some).collect::<Order>();
    let categories = map.categories();
    let index_arg = |s: &str| s.trim().parse::<i32>().map_err(|_| format!("not a category's index: {s:?}"));
    match op {
        map_op::NEW_TRIGGER => {
            let (index, name) = arg.split_once('\n').unwrap_or((arg, ""));
            let index = index_arg(index)?;
            let name = name.trim();
            if name.is_empty() {
                return Err("a trigger needs a name".into());
            }
            let own = wtg_rs::jass::ident(name);
            if let Some(t) = map.triggers.iter().find(|t| wtg_rs::jass::ident(&t.name) == own) {
                return Err(format!("the map's script would name {:?} gg_trg_{own} too: name it otherwise", t.name));
            }
            let (at, pick) = map.end_of_category(index);
            let section = format!("[{}]\nname={}\ndescription=\"\"\ncategory={index}\n", wtg_rs::text::TRIGGER, wtg_rs::text::jass_string(name));
            map.insert(at, &section);
            let mut order = all();
            order.insert(pick - 1, None);
            Ok((Answer::Lines(map.lines, 0, Some(pick)), Some(order)))
        }
        map_op::NEW_CATEGORY => {
            let name = arg.trim();
            if name.is_empty() {
                return Err("a category needs a name".into());
            }
            // 1.31's: an id of a category's type (0x02 its high byte), not
            // the map's 0.
            let first = if map.is_tree() { 0x0200_0000 } else { 0 };
            let index = categories.iter().map(|c| c.0).max().map_or(first, |m| m + 1);
            let at = map.after_last(&["category", "wtg"]);
            map.insert(at, &format!("[category]\nindex={index}\nname={}\n", wtg_rs::text::jass_string(name)));
            Ok((Answer::Lines(map.lines, 0, Some(nth)), None))
        }
        map_op::DELETE_TRIGGER => {
            let t = map.triggers.get(nth.wrapping_sub(1)).ok_or("no trigger to delete")?;
            let pick = nth.min(map.triggers.len() - 1);
            let range = t.range.clone();
            map.remove(range);
            let mut order = all();
            order.remove(nth - 1);
            Ok((Answer::Lines(map.lines, 0, Some(pick)), Some(order)))
        }
        map_op::DELETE_CATEGORY => {
            let index = index_arg(arg)?;
            let (_, _, range) = categories.iter().find(|c| c.0 == index).ok_or(format!("no category {index}"))?.clone();
            let mut ranges: Vec<std::ops::Range<usize>> = map.triggers.iter().filter(|t| map.category_of(t) == index).map(|t| t.range.clone()).collect();
            let gone_before = map.triggers.iter().take(nth).filter(|t| map.category_of(t) == index).count();
            let left = map.triggers.len() - ranges.len();
            let order: Order = map.triggers.iter().enumerate().filter(|(_, t)| map.category_of(t) != index).map(|(k, _)| Some(k)).collect();
            ranges.push(range);
            ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
            for r in ranges {
                map.remove(r);
            }
            Ok((Answer::Lines(map.lines, 0, Some(nth.saturating_sub(gone_before).max(1).min(left))), Some(order)))
        }
        map_op::RENAME_CATEGORY => {
            let (index, name) = arg.split_once('\n').unwrap_or((arg, ""));
            let index = index_arg(index)?;
            let (_, _, range) = categories.iter().find(|c| c.0 == index).ok_or(format!("no category {index}"))?.clone();
            let line = format!("name={}{}", wtg_rs::text::jass_string(name.trim()), map.eol);
            match field_in(&map.lines, range.start + 1..range.end, "name") {
                Some((i, _)) => map.lines[i] = line,
                None => map.lines.insert(range.start + 1, line),
            }
            Ok((Answer::Lines(map.lines, 0, Some(nth)), None))
        }
        map_op::MOVE_TRIGGER => {
            let mut parts = arg.split('\n');
            let mut number = |what: &str| parts.next().and_then(|v| v.trim().parse::<i64>().ok()).ok_or_else(|| format!("a trigger's move: {what}"));
            let moved = number("which")? as usize;
            let index = number("its category")? as i32;
            let before = number("before which")? as usize;
            let t = map.triggers.get(moved.wrapping_sub(1)).ok_or(format!("no trigger {moved}"))?;
            if before == moved {
                return Ok((Answer::Lines(map.lines, 0, Some(moved)), None));
            }
            if before > map.triggers.len() {
                return Err(format!("no trigger {before}"));
            }
            // Its lines, its category the new one, without the blank lines after it.
            let eol = map.eol;
            let mut section: Vec<String> = map.lines[t.range.clone()].to_vec();
            while section.len() > 1 && section.last().is_some_and(|l| l.trim().is_empty()) {
                section.pop();
            }
            let category = format!("category={index}{eol}");
            match field_in(&section, 1..section.len(), "category") {
                Some((i, _)) => section[i] = category,
                None => section.insert(1, category),
            }
            if let Some(last) = section.last_mut()
                && !last.ends_with('\n')
            {
                last.push_str(eol);
            }
            let range = t.range.clone();
            map.remove(range);
            let mut map = Map::of(&map.lines.concat());
            // The one it goes before, counted without it.
            let before = if before > moved { before - 1 } else { before };
            let (at, pick) = match before {
                0 => map.end_of_category(index),
                b => (map.triggers[b - 1].range.start, b),
            };
            map.insert(at, &section.concat());
            let mut order = all();
            order.remove(moved - 1);
            order.insert(pick - 1, Some(moved - 1));
            Ok((Answer::Lines(map.lines, 0, Some(pick)), Some(order)))
        }
        _ => Err(format!("unknown operation {op}")),
    }
}

/// The JASS the `nth` trigger (from 1; 0: the first) of a `.wtg`'s text
/// is built into, as [`jass`] gives a `.trg`'s: the map's variables are
/// its globals.
pub fn wtg_jass(text: &str, nth: usize, names: &crate::names::Names) -> String {
    let map = Map::of(text);
    let nth = map.pick(nth);
    if nth == 0 {
        return "// The map has no triggers.\n".into();
    }
    let slice = &map.triggers[nth - 1];
    let visible: Vec<Declared> = wtg_variables(text, &wtg_rs::text::parse_bni(text)).into_iter().map(|(_, d)| d).collect();
    let p = wtg_rs::text::trigger_from_text(&slice.text, None);
    let mut out = String::new();
    if !p.errors.is_empty() {
        out += &format!("// The trigger has {} problem(s); what reads is built:\n", p.errors.len());
        for e in p.errors.iter().take(5) {
            out += &format!("//   line {}: {}\n", slice.line(e.line), e.message);
        }
        out += "\n";
    }
    out + &wtg_rs::jass::compile(p.trigger.as_ref(), &slice.name, &[], &visible, &names.trigger_ui)
}

/// What the parameter at `path` of the ECA at `line` of the picked
/// trigger can be ([`choices`]): `arg` is which trigger it is (from 1), a
/// line break, and the path.
pub fn wtg_choices(text: &str, line: usize, arg: &str, ui: &wtg_rs::ui::Ui) -> String {
    let (nth, path) = arg.split_once('\n').unwrap_or((arg, ""));
    let map = Map::of(text);
    let nth = map.pick(nth.trim().parse().unwrap_or(0));
    if nth == 0 {
        return format!("{{\"error\":{}}}", crate::objects::json_string("no trigger: the file has none"));
    }
    let visible: Vec<Declared> = wtg_variables(text, &wtg_rs::text::parse_bni(text)).into_iter().map(|(_, d)| d).collect();
    choices(&map.triggers[nth - 1].text, line, path, ui, &visible)
}

/// The parameter at `path` (see `wtg_rs::ui::Span::Value`) of `params`.
fn param_at<'a>(params: &'a mut [wtg_rs::Param], path: &str) -> Result<&'a mut wtg_rs::Param, String> {
    let missing = || format!("no parameter {path}");
    let mut steps = path.split('.');
    let first: usize = steps.next().and_then(|s| s.parse().ok()).ok_or_else(missing)?;
    let mut p = params.get_mut(first).ok_or_else(missing)?;
    for step in steps {
        p = if step == "i" {
            p.index.as_deref_mut().ok_or_else(missing)?
        } else {
            let j: usize = step.parse().map_err(|_| missing())?;
            p.call.as_mut().and_then(|c| c.params.get_mut(j)).ok_or_else(missing)?
        };
    }
    Ok(p)
}

/// The one edit, of whole lines, that turns `old` (the lines of `text`)
/// into `new`: its range in the editor's coordinates and its text; `None`
/// when they're the same.
fn diff(text: &str, old: &[&str], new: &[String]) -> Option<(crate::document::Range, String)> {
    use crate::document::{Position, Range};
    let prefix = old.iter().zip(new).take_while(|(a, b)| **a == b.as_str()).count();
    if prefix == old.len() && prefix == new.len() {
        return None;
    }
    let most = old.len().min(new.len()) - prefix;
    let suffix = old.iter().rev().zip(new.iter().rev()).take(most).take_while(|(a, b)| **a == b.as_str()).count();
    let index = crate::document::LineIndex::new(text);
    // The position where line `k` starts; past the last, the end.
    let pos = |k: usize| {
        if k < old.len() {
            return Position::new(k as u32, 0);
        }
        match old.last() {
            None => Position::new(0, 0),
            Some(l) if l.ends_with('\n') => Position::new(old.len() as u32, 0),
            Some(_) => {
                let last = old.len() - 1;
                Position::new(last as u32, index.line(text, last).encode_utf16().count() as u32)
            }
        }
    };
    let range = Range::new(pos(prefix), pos(old.len() - suffix));
    Some((range, new[prefix..new.len() - suffix].concat()))
}

#[cfg(test)]
#[path = "triggers.test.rs"]
mod tests;
