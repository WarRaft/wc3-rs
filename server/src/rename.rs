//! Renaming a symbol: every occurrence the index finds (see `index`), in
//! every file of the project, open or not. Refused, with the reason, when
//! the rename can't be done safely:
//!
//! - **nothing to rename**: no symbol under the cursor;
//! - **not ours**: the symbol has no declaration in the project (`print`,
//!   or a native whose `common.j` isn't part of it), or it's a JASS
//!   `native`, the game's own API;
//! - **ambiguous member**: a field or method declared in several places
//!   (two structs' `x`). Members are matched by name, so renaming one
//!   would rename the others too;
//! - **invalid name**: not a plain name in every language the symbol is
//!   used in. Each language's own lexer decides (a keyword doesn't lex as
//!   a name), plus JASS and Zinc names start with a letter and JASS
//!   names don't end with `_`;
//! - **conflict**: the new name is already declared in the same scope
//!   (for a project-wide name, the same namespace);
//! - **the game's**: an occurrence is in one of the game's scripts
//!   (`Blizzard.j`, say; see `workspace`);
//! - **in a read-only map**: an occurrence is in a file inside a map
//!   that can't be written (see `mapfs`). Files in other maps are renamed
//!   like any; the editor asks before changing a map.
//!
//! An AngelScript class is renamed with its constructors and destructor,
//! which are members named like it; renaming a constructor renames its
//! class.

use parser_rs::{LexSpec, Occurrence, Scope, SymbolKind, TokenKind};

use crate::document::{Document, Language, Position};
use crate::index::{at, matches, Answer, Out};
use crate::map;
use crate::workspace::Workspace;

/// Whether the name under the cursor can be renamed: its location and
/// current name (a constructor's is its class's), or why not.
pub fn prepare(ws: &Workspace, id: &str, pos: Position) -> Answer {
    if let Err(why) = plan(ws, id, pos) {
        return refused(why);
    }
    let mut out = Out::default();
    if let (Some((doc, _)), Some(o)) = (ws.file(id), at(ws, id, pos)) {
        out.push(id, doc, o);
    }
    out.finish()
}

/// Every occurrence to replace with `new_name`, or why not.
pub fn rename(ws: &Workspace, id: &str, pos: Position, new_name: &str) -> Answer {
    let found = match plan(ws, id, pos) {
        Ok(found) => found,
        Err(why) => return refused(why),
    };
    let target = found.iter().find(|f| f.2.declaration).expect("plan checked for a declaration").2;
    if new_name == target.name {
        return Answer::default();
    }
    let mut languages: Vec<Language> = found.iter().map(|f| f.1.language).collect();
    languages.dedup();
    if let Err(why) = valid_name(new_name, &languages) {
        return refused(why);
    }
    if let Some((file, o)) = conflict(ws, id, target, new_name) {
        return refused(format!(
            "`{new_name}` is already declared in {}:{}.",
            file_name(file),
            o.line
        ));
    }
    let mut out = Out::default();
    for (file, doc, o) in found {
        out.push(file, doc, o);
    }
    out.finish()
}

fn refused(why: String) -> Answer {
    Answer { locations: Vec::new(), text: why }
}

type Found<'a> = (&'a str, &'a Document, &'a Occurrence);

/// Every occurrence the rename would replace, if it can be renamed.
fn plan<'a>(ws: &'a Workspace, id: &'a str, pos: Position) -> Result<Vec<Found<'a>>, String> {
    let Some(mut target) = at(ws, id, pos) else {
        return Err("There's no symbol here to rename.".into());
    };
    // A constructor or destructor stands for its class.
    if target.scope == Scope::Member
        && let Some(class) = declarations(ws, id, |o| o.kind == SymbolKind::Class && o.name == target.name).next()
    {
        target = class.2;
    }
    if target.kind == SymbolKind::Native {
        return Err(format!("`{}` is a native, part of the game's API: it can't be renamed.", target.name));
    }
    let mut found = matches(ws, id, target);
    if target.kind == SymbolKind::Class {
        let name = target.name.clone();
        found.extend(declarations(ws, id, move |o| {
            o.scope == Scope::Member && o.kind == SymbolKind::Method && o.name == name
        }));
    }
    let decls: Vec<&Found> = found.iter().filter(|f| f.2.declaration).collect();
    if decls.is_empty() {
        return Err(format!(
            "`{}` isn't declared in this project, so it can't be renamed.",
            target.name
        ));
    }
    if decls.iter().any(|f| f.2.kind == SymbolKind::Native) {
        return Err(format!("`{}` is a native, part of the game's API: it can't be renamed.", target.name));
    }
    if let Some(f) = found.iter().find(|f| ws.is_game(f.0)) {
        return Err(format!(
            "`{}` is used in the game's {}, so it can't be renamed.",
            target.name,
            file_name(f.0)
        ));
    }
    let mut checked = Vec::new();
    for (archive, name) in found.iter().filter_map(|f| map::split(f.1.path.as_deref()?)) {
        if checked.contains(&archive) {
            continue;
        }
        checked.push(archive);
        if let Some(reason) = crate::mapfs::readonly(archive) {
            return Err(format!(
                "`{}` is used in {name} inside {}, which can't be changed ({reason}), so it can't be renamed.",
                target.name,
                archive.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
            ));
        }
    }
    if target.scope == Scope::Member && decls.len() > 1 {
        let places: Vec<String> = decls.iter().map(|f| format!("{}:{}", file_name(f.0), f.2.line)).collect();
        return Err(format!(
            "`{}` is declared {} times ({}). Fields and methods are matched by name, \
             so renaming it would rename all of them.",
            target.name,
            decls.len(),
            places.join(", ")
        ));
    }
    Ok(found)
}

/// Every declaration `id` sees (see `Workspace::scope`) matching `keep`.
fn declarations<'a>(ws: &'a Workspace, id: &str, keep: impl Fn(&Occurrence) -> bool + 'a) -> impl Iterator<Item = Found<'a>> + 'a {
    ws.scope(id)
        .flat_map(|(id, doc, symbols)| symbols.iter().map(move |o| (id, doc, o)))
        .filter(move |(_, _, o)| o.declaration && keep(o))
}

/// A declaration of `name` in the same scope as `target` (and, for a
/// project-wide one, the same namespace), if any.
fn conflict<'a>(ws: &'a Workspace, id: &str, target: &Occurrence, name: &str) -> Option<(&'a str, &'a Occurrence)> {
    let scope = target.scope;
    ws.scope(id)
        .flat_map(|(file, _, symbols)| symbols.iter().map(move |o| (file, o)))
        .find(|(file, o)| {
            o.declaration
                && o.name == name
                && o.scope == scope
                && (!matches!(scope, Scope::Local(_)) || *file == id)
                && (scope != Scope::Project || o.namespace == target.namespace)
        })
}

/// Is `name` a plain name in each of `languages`?
fn valid_name(name: &str, languages: &[Language]) -> Result<(), String> {
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !plain {
        return Err(format!("`{name}` isn't a valid name: use letters, digits and `_`, not starting with a digit."));
    }
    for &language in languages {
        let Some(spec) = spec(language) else { continue };
        let starts_with_letter = name.starts_with(|c: char| c.is_ascii_alphabetic());
        if matches!(language, Language::Jass | Language::Zinc) && !starts_with_letter {
            return Err(format!("`{name}` isn't a valid {} name: it must start with a letter.", label(language)));
        }
        if language == Language::Jass && name.ends_with('_') {
            return Err(format!("`{name}` isn't a valid JASS name: it can't end with `_`."));
        }
        let lexed = parser_rs::tokenize_lossy(name, spec);
        // One name, then the end of input.
        let is_name = matches!(
            lexed.tokens.as_slice(),
            [t, end] if t.kind == TokenKind::Ident(name.to_string()) && end.kind == TokenKind::Eof
        );
        let reserved = language == Language::AngelScript && matches!(name, "this" | "super");
        if !is_name || reserved {
            return Err(format!("`{name}` is a keyword in {}.", label(language)));
        }
    }
    Ok(())
}

/// The lexer that decides what's a name. JASS is checked as vJASS, the
/// bigger keyword set, so a renamed name works in either dialect.
fn spec(language: Language) -> Option<&'static LexSpec> {
    match language {
        Language::Jass => Some(&jass_rs::lexer::VJASS_SPEC),
        Language::Zinc => Some(&zinc_rs::lexer::SPEC),
        Language::Lua => Some(&lua_rs::lexer::SPEC),
        Language::AngelScript => Some(&as_rs::lexer::SPEC),
        Language::Bni | Language::Slk | Language::Wts => None,
    }
}

fn label(language: Language) -> &'static str {
    match language {
        Language::Jass => "JASS",
        Language::Zinc => "Zinc",
        Language::Lua => "Lua",
        Language::AngelScript => "AngelScript",
        Language::Bni => "BNI",
        Language::Slk => "SLK",
        Language::Wts => "WTS",
    }
}

fn file_name(id: &str) -> &str {
    std::path::Path::new(id).file_name().and_then(|n| n.to_str()).unwrap_or(id)
}

#[cfg(test)]
#[path = "rename.test.rs"]
mod tests;
