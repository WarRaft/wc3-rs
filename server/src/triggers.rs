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
            let f = wtg_rs::parse_any(bytes, data).map_err(|e| format!("Can't read {file}: {e}"))?;
            let wtg_rs::File::Classic(f) = f else {
                return Err(format!("Can't show {file}: the layout of 1.31 and later has no text form yet"));
            };
            wtg_rs::text::to_text(&f).map(String::into_bytes).map_err(|e| format!("Can't show {file}: {e}"))
        }
        TO_BYTES => {
            let text = std::str::from_utf8(bytes).map_err(|_| "the text isn't UTF-8".to_string())?;
            let p = wtg_rs::text::from_text(text, (!data.is_empty()).then_some(data));
            let Some(f) = p.file else {
                let e = &p.errors[0];
                return Err(format!("Can't save {file}: line {}: {}", e.line, e.message));
            };
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
    wtg_rs::text::from_bni(doc, text, None).errors.into_iter().filter(|e| !bni.contains(e)).collect()
}

/// The triggers of the text form `text` (of a `.wtg`) as files, as JSON:
/// `{"folders": ["Init", ...], "files": [["Init/Spawn.trg", "[trigger]..."],
/// ["_variables.trg", "[variable]..."]]}` (`wtg_rs::text::to_files`: a
/// folder per category, a `.trg` per trigger, the variables at the top),
/// or `{"error": "..."}` when the text has errors.
pub fn extract(text: &str) -> String {
    let js = crate::objects::json_string;
    let p = wtg_rs::text::from_text(text, None);
    let result = match p.file {
        Some(f) => wtg_rs::text::to_files(&f),
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
    let nodes = |n: &[wtg_rs::ui::Node]| format!("[{}]", n.iter().map(node_json).collect::<Vec<_>>().join(","));
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

fn node_json(n: &wtg_rs::ui::Node) -> String {
    let js = crate::objects::json_string;
    let blocks: Vec<String> = n
        .blocks
        .iter()
        .map(|b| {
            let nodes: Vec<String> = b.nodes.iter().map(node_json).collect();
            format!("{{\"label\":{},\"group\":{},\"kind\":{},\"nodes\":[{}]}}", js(&b.label), b.group, b.kind, nodes.join(","))
        })
        .collect();
    format!(
        "{{\"line\":{},\"kind\":{},\"enabled\":{},\"name\":{},\"known\":{},\"icon\":{},\"hint\":{},\"text\":{},\"blocks\":[{}]}}",
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
                format!("{{\"r\":\"{r}\",\"p\":{},\"s\":{}}}", crate::objects::json_string(path), spans_json(spans))
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
///  "value": false}
/// ```
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
    Ok(format!(
        "{{\"type\":{},\"type_name\":{},\"current\":{{\"kind\":\"{kind}\",\"value\":{}}},\"presets\":[{}],\"variables\":[{}],\"calls\":[{}],\"value\":{value_ok}}}",
        js(&ty),
        js(&if any { "Any variable".into() } else { type_name(&ty) }),
        js(&value),
        presets.join(","),
        vars.join(","),
        calls.join(","),
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
                let trigger = wtg_rs::text::trigger_from_text(t, None).trigger?;
                let name = if trigger.name.is_empty() { stem(id) } else { trigger.name };
                let own = format!("gg_trg_{}", wtg_rs::jass::ident(&name));
                Some((id.to_string(), name, own))
            })
            .collect();
        let mine = wtg_rs::jass::ident(&list.iter().find(|(id, _, _)| id == me).map_or_else(|| stem(me), |(_, name, _)| name.clone()));
        Triggers { list, mine, me: me.to_string() }
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
