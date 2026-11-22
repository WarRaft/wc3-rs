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
/// {"name": "Spawn", "description": "", "enabled": 1, "initially_off": 0,
///  "run_on_init": 0, "custom_text": 0, "comment": 0, "data": true,
///  "errors": [[3, "`action`: ..."]],
///  "labels": ["Events", "Conditions", "Actions"],
///  "events": [node], "conditions": [...], "actions": [...]}
/// ```
///
/// A node: `{"line": 4, "kind": 2, "enabled": true, "name": "CreateNUnitsAtLoc",
/// "known": true, "icon": "ReplaceableTextures\\...", "hint": "...",
/// "text": [span], "blocks": [{"label": "Then - Actions", "group": 1,
/// "kind": 2, "nodes": [...]}]}`; a span a string, or a value
/// `{"r": "p", "s": [span]}` (`p` preset, `v` variable, `l` a value typed
/// in, `c` a call, `u` unknown). `data` is whether the editor's files
/// were found; the errors are checked against them when they were, but
/// the tree isn't: a function they don't have shows, as unknown.
pub fn view(text: &str, names: &crate::names::Names) -> String {
    use wtg_rs::ui::Ui;
    let data = (!names.triggers.is_empty()).then_some(&names.triggers);
    let mut p = wtg_rs::text::trigger_from_text(text, None);
    if data.is_some() {
        p.errors = wtg_rs::text::trigger_from_text(text, data).errors;
    }
    let ui: &Ui = &names.trigger_ui;
    let v = ui.render(&p.trigger, &p.lines);
    let t = &p.trigger;
    let js = crate::objects::json_string;
    let errors: Vec<String> = p.errors.iter().map(|e| format!("[{},{}]", e.line, js(&e.message))).collect();
    let labels = ["WESTRING_EVENTS", "WESTRING_CONDITIONS", "WESTRING_ACTIONS"].map(|k| js(&ui.label(k)));
    let nodes = |n: &[wtg_rs::ui::Node]| format!("[{}]", n.iter().map(node_json).collect::<Vec<_>>().join(","));
    format!(
        "{{\"name\":{},\"description\":{},\"enabled\":{},\"initially_off\":{},\"run_on_init\":{},\"custom_text\":{},\"comment\":{},\"data\":{},\"errors\":[{}],\"labels\":[{}],\"events\":{},\"conditions\":{},\"actions\":{}}}",
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
            Span::Value { role, spans } => {
                let r = match role {
                    Role::Preset => "p",
                    Role::Variable => "v",
                    Role::Value => "l",
                    Role::Call => "c",
                    Role::Unknown => "u",
                };
                format!("{{\"r\":\"{r}\",\"s\":{}}}", spans_json(spans))
            }
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// The operations of [`edit`].
pub mod op {
    /// Turn the ECA at the line on or off (`!`).
    pub const TOGGLE: u32 = 0;
    /// Delete the ECA at the line, and what's inside it.
    pub const DELETE: u32 = 1;
    /// Move the ECA at the line, and what's inside it, to the place the
    /// argument says: `parent group index` (see [`super::edit`]).
    pub const MOVE: u32 = 2;
    /// Insert the lines after the argument's first line (a place, as for
    /// `MOVE`): ECAs, each at the top (no `>`, no block), with what's
    /// inside them, as `COPY` gives them.
    pub const PASTE: u32 = 3;
    /// The lines of the ECA at the line and what's inside it, as if at the
    /// top: the answer's `text`.
    pub const COPY: u32 = 4;
    /// Set a field of the trigger: the argument is its name, a line break,
    /// its value (`name` and `description` as typed; the flags a number).
    /// A flag set to its default goes.
    pub const FIELD: u32 = 5;
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
    /// The lines of the `[trigger]` section, past its header.
    section: std::ops::Range<usize>,
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
        let start = lines.iter().position(|l| l.trim().strip_prefix('[').is_some_and(|r| r.starts_with("trigger]"))).map_or(lines.len(), |i| i + 1);
        let end = lines[start..].iter().position(header).map_or(lines.len(), |i| start + i);
        Doc { lines, eol, section: start..end }
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
    if p.trigger.ecas.is_empty() {
        return Err("nothing to paste".into());
    }
    Ok(p.trigger.ecas.iter().map(|e| e.kind).collect())
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
pub fn edit(text: &str, op: u32, line: usize, arg: &str) -> String {
    let js = crate::objects::json_string;
    match apply(text, op, line, arg) {
        Ok(Done::Text(t)) => format!("{{\"text\":{}}}", js(&t)),
        Ok(Done::Lines(new, at)) => {
            let old: Vec<&str> = text.split_inclusive('\n').collect();
            let edits = match diff(text, &old, &new) {
                Some((r, t)) => format!("[[{},{},{},{},{}]]", r.start.line, r.start.character, r.end.line, r.end.character, js(&t)),
                None => "[]".into(),
            };
            format!("{{\"edits\":{edits},\"line\":{}}}", at.map_or(0, |i| i + 1))
        }
        Err(e) => format!("{{\"error\":{}}}", js(&e)),
    }
}

enum Done {
    Text(String),
    /// The new lines, and the index of the one to show.
    Lines(Vec<String>, Option<usize>),
}

fn apply(text: &str, op: u32, line: usize, arg: &str) -> Result<Done, String> {
    let mut doc = Doc::new(text);
    let mut shown = None;
    match op {
        op::TOGGLE => {
            let (i, _) = doc.at(line)?;
            doc.lines[i] = rekey(&doc.lines[i], |k| k.enabled = !k.enabled).expect("an ECA");
            shown = Some(i);
        }
        op::DELETE => {
            let (i, _) = doc.at(line)?;
            let end = doc.end(i);
            doc.lines.drain(i..end);
        }
        op::COPY => {
            let (i, _) = doc.at(line)?;
            let mut out = doc.lift(i, doc.end(i)).concat();
            if !out.ends_with('\n') {
                out.push_str(doc.eol);
            }
            return Ok(Done::Text(out));
        }
        op::MOVE => {
            let (i, key) = doc.at(line)?;
            let end = doc.end(i);
            let place = Place::parse(arg)?;
            if place.parent != 0 && (i..end).contains(&(place.parent - 1)) {
                return Err("can't move it into itself".into());
            }
            let (at, depth, group, kind) = doc.place(place)?;
            fits(&[key.kind], kind)?;
            if i < at && at < end {
                return Err("can't move it into itself".into());
            }
            let lines = doc.lift(i, end);
            doc.lines.drain(i..end);
            let at = if at >= end { at - (end - i) } else { at };
            doc.insert(at, &lines, depth, group);
            shown = Some(at);
        }
        op::PASTE => {
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
                "name" | "description" => Some(wtg_rs::text::jass_string(value)),
                "enabled" | "initially_off" | "run_on_init" | "comment" | "custom_text" => {
                    let n: u32 = value.trim().parse().map_err(|_| format!("`{key}` is a number, not {value:?}"))?;
                    let default = if key == "enabled" { 1 } else { 0 };
                    (n != default).then(|| n.to_string())
                }
                _ => return Err(format!("no field `{key}`")),
            };
            let line = value.map(|v| format!("{key}={v}{}", doc.eol));
            match (doc.field(key), line) {
                (Some(i), Some(l)) => {
                    let keep_eol = doc.lines[i].ends_with('\n');
                    doc.lines[i] = if keep_eol { l } else { l.trim_end().to_string() };
                }
                (Some(i), None) => {
                    doc.lines.remove(i);
                }
                (None, Some(l)) => {
                    // After the fields, before the ECAs; `name` and
                    // `description` first.
                    let mut fields = doc.section.clone().filter(|&i| !filler(&doc.lines[i]) && doc.eca(i).is_none());
                    let at = if matches!(key, "name" | "description") {
                        doc.section.start
                    } else {
                        fields.next_back().map_or(doc.section.start, |i| i + 1)
                    };
                    if doc.section.start == doc.lines.len() && doc.section.start == 0 {
                        doc.lines.push(format!("[trigger]{}", doc.eol));
                        doc.lines.push(l);
                    } else {
                        if at == doc.lines.len()
                            && let Some(last) = doc.lines.last_mut()
                            && !last.ends_with('\n')
                        {
                            last.push_str(doc.eol);
                        }
                        doc.lines.insert(at, l);
                    }
                }
                (None, None) => {}
            }
        }
        _ => return Err(format!("unknown operation {op}")),
    }
    Ok(Done::Lines(doc.lines, shown))
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
