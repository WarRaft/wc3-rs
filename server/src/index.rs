//! The symbol index: answers about names, from the symbols every file of
//! the model already carries (`Workspace`). Nothing is precomputed or
//! cached beyond them: a query scans the occurrences it needs, which is
//! fast enough for any real map (a whole-project scan is a few
//! milliseconds), and can never go stale.
//!
//! Names resolve by scope (see `parser_rs::symbols`): a local name within
//! its own file and scope; a struct member (in its own namespace) across
//! every file of the model; a project-wide name across every file too, in
//! the namespace it resolves to (AngelScript's `A::B::name`, looked up
//! from where it's used outwards); always by exact (case-sensitive) name.

use std::collections::{HashMap, HashSet};

use parser_rs::{Occurrence, Scope, SymbolKind};

use crate::document::{Document, LineIndex, Position, Range};
use crate::workspace::Workspace;

/// What the editor asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// Where the name under the cursor is declared.
    Definition,
    /// Every occurrence of that symbol, its declarations included.
    References,
    /// A description of that symbol: its declaration lines.
    Hover,
    /// The declarations of one file, for the outline: everything but
    /// parameters and locals.
    DocumentSymbols,
    /// The declarations (but parameters and locals) whose name contains
    /// the query text, in every file.
    WorkspaceSymbols,
    /// Can the name under the cursor be renamed? Its location, or why not.
    PrepareRename,
    /// Every occurrence to replace to rename the symbol to the query
    /// text, or why it can't be.
    Rename,
    /// The text of a file, as the model has it: for the editor to show a
    /// file it can't read itself, one inside a map.
    Content,
    /// In a `wc3.bni`: the entries completing the path at the cursor
    /// (see `paths`). Each location's `name` is an entry, its `id` the
    /// entry's path, its `range` what the entry replaces, its `container`
    /// `folder` or `file`.
    PathCompletion,
    /// In a `wc3.bni`: every folder along every path, a location whose
    /// `range` is the path part and `id` the folder.
    PathLinks,
    /// In a `.slk`: its sheet, as JSON in the text (`sheet::table`).
    SlkTable,
    /// In a `.slk`: the edits that write the query text into the cell at
    /// row `line`, column `character` (both from 1), as JSON in the text
    /// (`sheet::set_cell`).
    SlkSetCell,
    /// In the text form of object data: the inlay hints (fields' names and
    /// types) of lines `line` to `character` (0-based), as JSON in the
    /// text (`objects::hints`); in those of regions, terrain and map info,
    /// the names of weathers and tilesets (`hints::hints`).
    InlayHints,
    /// In the text form of regions: their colors, for the color picker, as
    /// JSON in the text (`regions::colors`).
    Colors,
    /// In a txt2 text: its preview, the stylesheet and the HTML (each
    /// block with the `data-line` it starts at) in the text.
    Txt2Html,
    /// In a trigger of its own (`.trg`): the view, as World Editor shows
    /// it, as JSON in the text (`triggers::view`). In the text of a map's
    /// triggers (`.wtg`): its tree and its trigger number `line` picked
    /// (from 1; `triggers::wtg_view`); the other trigger queries take that
    /// number before their text there (`triggers::wtg_edit`,
    /// `triggers::wtg_choices`).
    TriggerView,
    /// In a trigger of its own: the edits that do operation `character`
    /// on the ECA at line `line` (from 1) with the query text, as JSON in
    /// the text (`triggers::edit`).
    TriggerEdit,
    /// In the text form of triggers (`.wtg`): every trigger as a file of
    /// its own, as JSON in the text (`triggers::extract`).
    TriggerExtract,
    /// In a trigger of its own: what the parameter at the query text's
    /// path of the ECA at line `line` can be, as JSON in the text
    /// (`triggers::choices`).
    TriggerChoices,
    /// In a trigger of its own: the functions a new ECA of kind
    /// `character` can be, as JSON in the text (`triggers::catalog`).
    TriggerCatalog,
    /// In a trigger of its own: the trigger editor's icons, as JSON in
    /// the text (`triggers::icons`).
    TriggerIcons,
    /// In a trigger of its own: the JASS it's built into, as the text
    /// (`triggers::jass`); in a map's triggers (`.wtg`), the trigger whose
    /// number (from 1) is the query text's (`triggers::wtg_jass`).
    TriggerJass,
    /// [`QueryKind::TriggerJass`]'s JASS in the game's Lua
    /// (`triggers::converted`).
    TriggerLua,
    /// [`QueryKind::TriggerJass`]'s JASS in UjAPI's AngelScript
    /// (`triggers::converted`).
    TriggerAngelScript,
    /// In a frame file (FDF), in a string, or in JASS in a string that
    /// names a frame (`BlzCreateFrame("|", ...)`): the names it can be, each a
    /// location with its `name`, `kind` (a frame, a key), `container`
    /// (where it's declared: the file's name), and the `range` it
    /// replaces (the string's text); `id` the declaring file.
    Completion,
}

/// The variables the trigger file `id` sees: every global its project's
/// trigger files declare (`[variable]`; one project is one map, so every
/// trigger sees them all), and its own private ones and locals, as it's
/// being edited.
fn project_variables(ws: &Workspace, id: &str) -> Vec<wtg_rs::text::Declared> {
    use wtg_rs::text::{Scope, trigger_from_text};
    let mut out: Vec<wtg_rs::text::Declared> = ws
        .scope(id)
        .filter(|(fid, d, _)| *fid != id && d.path.as_deref().is_some_and(crate::triggers::is_trg_file))
        .flat_map(|(_, d, _)| trigger_from_text(&d.text, None).variables)
        .filter(|d| d.scope == Scope::Global)
        .collect();
    if let Some((doc, _)) = ws.file(id) {
        out.extend(trigger_from_text(&doc.text, None).variables);
    }
    out
}

/// In a frame file (FDF): each string that's the path of a file the
/// file sees (`IncludeFile "UI\\FrameDef\\UI\\EscMenuTemplates.fdf"`), a
/// location whose `range` is the path and `id` the file: the game's
/// path names it relative to the game's top, so the file whose path
/// ends with it, the project's over the game's.
fn frame_links(ws: &Workspace, id: &str) -> Answer {
    let Some((doc, _)) = ws.file(id) else { return Answer::default() };
    let text = doc.text.as_str();
    let lines = LineIndex::new(text);
    let norm = |p: &str| p.replace('\\', "/").to_lowercase();
    let files: Vec<(String, &str)> = ws.scope(id).map(|(fid, _, _)| (norm(fid), fid)).collect();
    let locations = fdf_rs::paths(&fdf_rs::parse(text))
        .into_iter()
        .filter_map(|p| {
            let tail = format!("/{}", norm(&p.text).trim_start_matches('/'));
            let target = files.iter().filter(|(n, _)| n.ends_with(&tail)).min_by_key(|(_, f)| ws.is_game(f))?.1;
            Some(Location {
                id: target.to_string(),
                range: lines.range(text, p.span.start_line, p.span.start_col, p.span.end_col),
                name: p.text,
                kind: SymbolKind::Namespace,
                container: "file".into(),
            })
        })
        .collect();
    Answer { locations, text: String::new() }
}

/// In a frame file, in a string (typed or not: an open `"` before the
/// cursor on its line; strings have no escapes): every frame and key the
/// file sees, once each, the project's over the game's. Nothing elsewhere.
fn frame_completion(ws: &Workspace, id: &str, pos: Position) -> Answer {
    let Some((doc, _)) = ws.file(id) else {
        return Answer::default();
    };
    let text = doc.text.as_str();
    let lines = LineIndex::new(text);
    let range = if doc.language == crate::document::Language::Fdf {
        match fdf_string_at(text, &lines, pos) {
            Some(range) => range,
            None => return Answer::default(),
        }
    } else {
        // Code: a string the names found naming a frame.
        match at(ws, id, pos).filter(|o| o.scope == Scope::Data && !o.declaration) {
            Some(o) => lines.range(text, o.line, o.col, o.end_col),
            None => return Answer::default(),
        }
    };
    let mut found: Vec<(&str, &Occurrence)> = ws
        .scope(id)
        .flat_map(|(fid, _, symbols)| symbols.iter().map(move |o| (fid, o)))
        .filter(|(_, o)| o.declaration && o.scope == Scope::Data)
        .collect();
    found.sort_by_key(|(fid, o)| (o.name.as_str(), ws.is_game(fid), *fid, o.line));
    found.dedup_by(|a, b| a.1.name == b.1.name);
    let locations = found
        .into_iter()
        .map(|(fid, o)| Location {
            id: fid.to_string(),
            range,
            name: o.name.clone(),
            kind: o.kind,
            container: std::path::Path::new(fid).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        })
        .collect();
    Answer { locations, text: String::new() }
}

/// In a frame file, the text of the string at `pos` (typed or not: an
/// open `"` before it on its line; strings have no escapes), outside
/// comments.
fn fdf_string_at(text: &str, lines: &LineIndex, pos: Position) -> Option<Range> {
    let line_text = lines.line(text, pos.line as usize);
    let col = lines.column(text, pos);
    let chars: Vec<char> = line_text.chars().collect();
    let before = &chars[..(col - 1).min(chars.len())];
    // In a comment, or not in a string: nothing.
    let code_end = line_text.find("//").map_or(chars.len(), |b| line_text[..b].chars().count());
    if before.len() > code_end || before.iter().filter(|c| **c == '"').count() % 2 == 0 {
        return None;
    }
    let start = before.iter().rposition(|c| *c == '"').expect("an odd count") + 2;
    let end = chars[before.len()..].iter().position(|c| *c == '"').map_or(chars.len(), |i| before.len() + i) + 1;
    Some(lines.range(text, pos.line as usize + 1, start, end))
}

/// Path completion and links in a manifest; nothing for any other file.
fn manifest_paths(ws: &Workspace, kind: QueryKind, id: &str, pos: Position) -> Answer {
    let Some((doc, _)) = ws.file(id) else {
        return Answer::default();
    };
    let Some(root) = doc.path.as_deref().filter(|p| crate::project::is_manifest(p)).and_then(|p| p.parent()) else {
        return Answer::default();
    };
    let text = doc.text.as_str();
    let lines = LineIndex::new(text);
    let location = |id: &std::path::Path, range: Range, name: String, container: &str| Location {
        id: id.to_string_lossy().into_owned(),
        range,
        name,
        kind: SymbolKind::Namespace,
        container: container.to_string(),
    };
    let locations = if kind == QueryKind::PathLinks {
        crate::paths::links(root, text)
            .into_iter()
            .map(|l| location(&l.target, lines.range(text, l.line, l.col, l.end_col), String::new(), "folder"))
            .collect()
    } else {
        let line = pos.line as usize + 1;
        let col = lines.column(text, pos);
        match crate::paths::complete(root, text, line, col) {
            Some(c) => {
                let range = lines.range(text, c.line, c.col, c.end_col);
                c.entries
                    .into_iter()
                    .map(|e| location(&e.path, range, e.name, if e.is_dir { "folder" } else { "file" }))
                    .collect()
            }
            None => Vec::new(),
        }
    };
    Answer { locations, text: String::new() }
}

/// One place in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub id: String,
    pub range: Range,
    pub name: String,
    pub kind: SymbolKind,
    /// The namespace a declaration lives in (`A::B`); empty for the
    /// global one and for anything that isn't a declaration.
    pub container: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Answer {
    pub locations: Vec<Location>,
    /// Markdown for a hover; for a rename, why it was refused (empty when
    /// it's fine).
    pub text: String,
}

/// The most symbols a workspace search returns.
const MAX_SEARCH: usize = 1000;

/// The most declarations a hover shows.
const MAX_HOVER: usize = 5;

pub fn answer(ws: &Workspace, kind: QueryKind, id: &str, pos: Position, query: &str) -> Answer {
    // A `TRIGSTR_<n>`, in any file: its text from the map's string table.
    if kind == QueryKind::Hover
        && let Some(text) = crate::trigstr::hover(ws, id, pos)
    {
        return Answer { locations: Vec::new(), text };
    }
    let mut out = Out::default();
    match kind {
        QueryKind::PrepareRename => return crate::rename::prepare(ws, id, pos),
        QueryKind::Rename => return crate::rename::rename(ws, id, pos, query),
        QueryKind::Content => {
            let text = match ws.file(id) {
                Some((doc, _)) => doc.text.clone(),
                None => crate::map::text(std::path::Path::new(id)).unwrap_or_default(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Completion => return frame_completion(ws, id, pos),
        QueryKind::PathLinks if ws.file(id).is_some_and(|(d, _)| d.language == crate::document::Language::Fdf) => return frame_links(ws, id),
        QueryKind::PathCompletion | QueryKind::PathLinks => return manifest_paths(ws, kind, id, pos),
        QueryKind::SlkTable | QueryKind::SlkSetCell => {
            let Some((doc, _)) = ws.file(id).filter(|(d, _)| d.language == crate::document::Language::Slk) else {
                return Answer::default();
            };
            let text = match kind {
                QueryKind::SlkTable => crate::sheet::table(&doc.text),
                _ => crate::sheet::set_cell(&doc.text, pos.line, pos.character, query),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::InlayHints if ws.file(id).and_then(|(d, _)| d.path.as_deref()).is_some_and(crate::customtext::is_custom_text_file) => {
            let (doc, _) = ws.file(id).expect("just checked");
            return Answer { locations: Vec::new(), text: crate::customtext::hints(&doc.text, pos.line, pos.character) };
        }
        QueryKind::InlayHints => {
            let text = match (ws.object_text(id), ws.names_text(id)) {
                (Some((doc, meta)), _) => crate::objects::hints(&doc.text, pos.line, pos.character, meta),
                (None, Some((doc, names, usage))) => match doc.path.as_deref() {
                    Some(path) => crate::hints::hints(&doc.text, path, names, usage, pos.line, pos.character),
                    None => "[]".into(),
                },
                (None, None) => "[]".into(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Txt2Html => {
            let text = match ws.file(id).filter(|(d, _)| d.language == crate::document::Language::Txt2) {
                Some((doc, _)) => {
                    let parsed = txt2_rs::parse(&doc.text);
                    format!("<style>{}</style>\n{}", txt2_rs::html::STYLE, txt2_rs::html::render(&parsed))
                }
                None => String::new(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::TriggerExtract => {
            let text = match ws.file(id).filter(|(d, _)| d.path.as_deref().is_some_and(crate::triggers::is_trigger_file)) {
                Some((doc, _)) => crate::triggers::extract(&doc.text, ws.custom_script_beside(id).as_ref().map(|(_, t)| t.as_str())),
                None => "{\"error\":\"not the text of triggers (.wtg)\"}".into(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::TriggerJass | QueryKind::TriggerLua | QueryKind::TriggerAngelScript => {
            let Some((doc, names, _)) = ws.names_text(id).filter(|(d, _, _)| d.path.as_deref().is_some_and(crate::triggers::has_call_values)) else {
                return Answer::default();
            };
            let jass = if doc.path.as_deref().is_some_and(crate::triggers::is_trigger_file) {
                crate::triggers::wtg_jass(&doc.text, query.trim().parse().unwrap_or(0), names)
            } else {
                crate::triggers::jass(&doc.text, id, names, &project_variables(ws, id))
            };
            let script = match kind {
                QueryKind::TriggerLua => wc3_build::jass::Script::Lua,
                QueryKind::TriggerAngelScript => wc3_build::jass::Script::AngelScript,
                _ => return Answer { locations: Vec::new(), text: jass },
            };
            // The game's scripts the trigger runs with, as its project sees them.
            let game = |which: crate::game::Script| ws.scope(id).find(|(fid, _, _)| crate::game::Script::of(std::path::Path::new(fid)) == Some(which)).map(|(_, d, _)| d.text.as_str()).unwrap_or_default();
            let text = crate::triggers::converted(&jass, script, game(crate::game::Script::Common), game(crate::game::Script::Blizzard));
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::TriggerIcons => {
            let text = match ws.names_text(id).filter(|(d, _, _)| d.path.as_deref().is_some_and(crate::triggers::has_call_values)) {
                Some((_, names, _)) => crate::triggers::icons(names),
                None => "{}".into(),
            };
            return Answer { locations: Vec::new(), text };
        }
        // The text of a map's triggers (`.wtg`): its tree, and the trigger
        // picked in it as a trigger of its own (`triggers::wtg_view`).
        QueryKind::TriggerView | QueryKind::TriggerEdit | QueryKind::TriggerChoices | QueryKind::TriggerCatalog
            if ws.file(id).is_some_and(|(d, _)| d.path.as_deref().is_some_and(crate::triggers::is_trigger_file)) =>
        {
            let Some((doc, names, _)) = ws.names_text(id) else {
                return Answer::default();
            };
            let ui = &names.trigger_ui;
            // The map's custom script beside it: the code of its triggers of custom text.
            let wct = ws.custom_script_beside(id);
            let text = match kind {
                QueryKind::TriggerView => crate::triggers::wtg_view(&doc.text, pos.line as usize, names, wct.as_ref().map(|(_, t)| t.as_str())),
                QueryKind::TriggerEdit if pos.character == crate::triggers::map_op::CONVERT => {
                    let nth = query.split('\n').next().unwrap_or("").trim().parse().unwrap_or(0);
                    crate::triggers::convert_wtg(&doc.text, nth, ui, wct.as_ref().map(|(_, t)| t.as_str()))
                }
                QueryKind::TriggerEdit => crate::triggers::wtg_edit(&doc.text, pos.character, pos.line as usize, query, ui, wct.as_ref().map(|(_, t)| t.as_str())),
                QueryKind::TriggerChoices => crate::triggers::wtg_choices(&doc.text, pos.line as usize, query, ui),
                _ => crate::triggers::catalog(pos.character, ui),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::TriggerChoices | QueryKind::TriggerCatalog => {
            let Some((doc, names, _)) = ws.names_text(id).filter(|(d, _, _)| d.path.as_deref().is_some_and(crate::triggers::is_trg_file)) else {
                return Answer::default();
            };
            let text = if kind == QueryKind::TriggerCatalog {
                crate::triggers::catalog(pos.character, &names.trigger_ui)
            } else {
                crate::triggers::choices(&doc.text, pos.line as usize, query, &names.trigger_ui, &project_variables(ws, id))
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::TriggerView | QueryKind::TriggerEdit => {
            let Some((doc, names, _)) = ws.names_text(id).filter(|(d, _, _)| d.path.as_deref().is_some_and(crate::triggers::is_trg_file)) else {
                return Answer::default();
            };
            let text = match kind {
                QueryKind::TriggerView => {
                    let others: Vec<(&str, &str)> = ws.scope(id).filter(|(fid, _, _)| *fid != id).map(|(fid, d, _)| (fid, d.text.as_str())).collect();
                    let triggers = crate::triggers::Triggers::of(id, &doc.text, &others);
                    crate::triggers::view(&doc.text, names, &project_variables(ws, id), &triggers)
                }
                _ if pos.character == crate::triggers::map_op::CONVERT => {
                    let path = doc.path.as_deref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                    crate::triggers::convert_trg(&doc.text, &path, &project_variables(ws, id), &names.trigger_ui)
                }
                _ => {
                    // The project's other files: a variable's uses, renamed or checked.
                    let others = ws.scope(id).filter(|(fid, _, _)| *fid != id && !ws.is_game(fid)).map(|(fid, d, names)| (fid, d.text.as_str(), names)).collect();
                    let project = crate::triggers::Project { me: id, others };
                    crate::triggers::edit_in(&doc.text, pos.character, pos.line as usize, query, &names.trigger_ui, &project)
                }
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Colors => {
            let text = match ws.file(id) {
                Some((doc, _)) => match doc.path.as_deref().filter(|p| crate::hints::has_colors(p)) {
                    Some(path) => crate::hints::colors(&doc.text, path),
                    None => "[]".into(),
                },
                None => "[]".into(),
            };
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Hover if ws.object_text(id).is_some() => {
            let (doc, meta) = ws.object_text(id).expect("just checked");
            let text = crate::objects::hover(&doc.text, pos, meta).unwrap_or_default();
            return Answer { locations: Vec::new(), text };
        }
        QueryKind::Definition | QueryKind::References | QueryKind::Hover => {
            ws.note_context(id);
            let Some(target) = at(ws, id, pos) else {
                return Answer::default();
            };
            let mut found = matches(ws, id, target);
            if kind != QueryKind::References {
                found.retain(|(_, _, o)| o.declaration);
            }
            if kind == QueryKind::Hover {
                return Answer { locations: Vec::new(), text: hover(target, &found) };
            }
            for (id, doc, o) in found {
                out.push(id, doc, o);
            }
        }
        QueryKind::DocumentSymbols => {
            if let Some((doc, symbols)) = ws.file(id) {
                for o in symbols.iter().filter(|o| o.declaration && !o.kind.is_local()) {
                    out.push(id, doc, o);
                }
            }
        }
        QueryKind::WorkspaceSymbols => {
            let query = query.to_lowercase();
            let mut found: Vec<(&str, &Document, &Occurrence)> = ws
                .all_files()
                .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
                .filter(|(_, _, o)| {
                    o.declaration && !o.kind.is_local() && o.name.to_lowercase().contains(&query)
                })
                .collect();
            found.sort_by(|a, b| (&a.2.name, a.0, a.2.line).cmp(&(&b.2.name, b.0, b.2.line)));
            for (id, doc, o) in found.into_iter().take(MAX_SEARCH) {
                out.push(id, doc, o);
            }
        }
    }
    out.finish()
}

/// The occurrence under the cursor.
pub(crate) fn at<'a>(ws: &'a Workspace, id: &str, pos: Position) -> Option<&'a Occurrence> {
    let (doc, symbols) = ws.file(id)?;
    let col = LineIndex::new(&doc.text).column(&doc.text, pos);
    let line = pos.line as usize + 1;
    // A cursor between two names (`a+b` at `+`) is on both: prefer the one
    // it's inside.
    symbols
        .iter()
        .filter(|o| o.covers(line, col))
        .min_by_key(|o| (o.end_col == col) as u8)
}

/// Every occurrence of `target`'s symbol that `id` sees (see
/// `Workspace::scope`), sorted by file and position.
pub(crate) fn matches<'a>(ws: &'a Workspace, id: &'a str, target: &Occurrence) -> Vec<(&'a str, &'a Document, &'a Occurrence)> {
    let mut found: Vec<(&str, &Document, &Occurrence)> = match target.scope {
        Scope::Local(_) => ws
            .file(id)
            .into_iter()
            .flat_map(|(doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
            .filter(|(_, _, o)| o.same_symbol(target))
            .collect(),
        // Members, and a data format's names (FDF's frames): by name across
        // the files `id` sees.
        Scope::Member | Scope::Data => ws
            .scope(id)
            .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
            .filter(|(_, _, o)| o.same_symbol(target))
            .collect(),
        Scope::Project => {
            // The namespaces that declare the name, to resolve every
            // occurrence of it against.
            let declared: HashSet<&str> = ws
                .scope(id)
                .flat_map(|(_, _, symbols)| symbols)
                .filter(|o| o.declaration && o.same_symbol(target))
                .map(|o| o.namespace.as_str())
                .collect();
            let declares = |ns: &str| declared.contains(ns);
            let namespace = target.resolve(declares);
            ws.scope(id)
                .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
                .filter(|(_, _, o)| o.same_symbol(target) && o.resolve(declares) == namespace)
                .collect()
        }
    };
    found.sort_by_key(|(id, _, o)| (*id, o.line, o.col));
    found
}

/// The declaration lines, as code in the file's language, each with the
/// file and line it's from. A name with no declaration in the project
/// (a native from a `common.j` that isn't part of it, say) says so.
fn hover(target: &Occurrence, decls: &[(&str, &Document, &Occurrence)]) -> String {
    // A data format's string that names nothing is just a string.
    if decls.is_empty() && target.scope == Scope::Data {
        return String::new();
    }
    if decls.is_empty() {
        return format!(
            "{} `{}`\n\n*not declared in the project*",
            target.kind.as_str(),
            target.name
        );
    }
    let mut parts = Vec::new();
    for (id, doc, o) in decls.iter().take(MAX_HOVER) {
        let lines = LineIndex::new(&doc.text);
        let line = lines.line(&doc.text, o.line - 1).trim();
        let file = std::path::Path::new(id).file_name().map_or(*id, |n| n.to_str().unwrap_or(id));
        let place = if o.namespace.is_empty() {
            format!("{file}:{}", o.line)
        } else {
            format!("{file}:{}, in `{}`", o.line, o.namespace)
        };
        parts.push(format!("```{}\n{line}\n```\n*{place}*", doc.language.id()));
    }
    if decls.len() > MAX_HOVER {
        parts.push(format!("*and {} more declarations*", decls.len() - MAX_HOVER));
    }
    parts.join("\n\n---\n\n")
}

/// Builds locations, indexing each file's lines once.
#[derive(Default)]
pub(crate) struct Out<'a> {
    lines: HashMap<&'a str, LineIndex>,
    locations: Vec<Location>,
}

impl<'a> Out<'a> {
    pub(crate) fn push(&mut self, id: &'a str, doc: &Document, o: &Occurrence) {
        let lines = self.lines.entry(id).or_insert_with(|| LineIndex::new(&doc.text));
        self.locations.push(Location {
            id: id.to_string(),
            range: lines.range(&doc.text, o.line, o.col, o.end_col),
            name: o.name.clone(),
            kind: o.kind,
            container: if o.declaration { o.namespace.clone() } else { String::new() },
        });
    }

    pub(crate) fn finish(self) -> Answer {
        Answer { locations: self.locations, text: String::new() }
    }
}

#[cfg(test)]
#[path = "index.test.rs"]
mod tests;
