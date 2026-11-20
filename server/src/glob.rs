//! Path masks for `wc3.bni` (`sources=`, `exclude=`): `*` is any part of
//! a name (not `/`), `?` one character of it, `**` any number of folders,
//! none included. Case doesn't matter, as in Windows and the game. `/` and
//! `\` both separate folders.
//!
//! A mask matches a path when it matches the path itself or a folder the
//! path is in: `**/old` covers everything inside every `old` folder, and
//! `*.w3x` the files inside matching maps.

use std::path::{Path, PathBuf};

use crate::project::resolve;

#[derive(Debug, Clone, PartialEq)]
pub struct Glob {
    /// As written in the manifest.
    pub text: String,
    /// The absolute mask, lowercase, split at the separators.
    parts: Vec<String>,
    /// The folder before the first wildcard: everything the mask can
    /// match is inside it.
    pub base: PathBuf,
}

/// Does `text` use wildcards, so it's a mask rather than a path?
pub fn is_mask(text: &str) -> bool {
    text.contains(['*', '?'])
}

impl Glob {
    /// The mask `text`, relative to `root` unless absolute.
    pub fn new(root: &Path, text: &str) -> Glob {
        // `resolve` keeps wildcard parts as they are and settles `.`/`..`.
        let full = resolve(root, text);
        let parts = split(&full);
        let literal: Vec<&str> = full
            .iter()
            .filter_map(|p| p.to_str())
            .take_while(|p| !is_mask(p))
            .collect();
        let mut base = PathBuf::new();
        base.extend(literal);
        Glob { text: text.to_string(), parts, base }
    }

    /// Does the mask match `path` (absolute) or a folder it's in?
    pub fn matches(&self, path: &Path) -> bool {
        let path = split(path);
        (1..=path.len()).any(|n| match_parts(&self.parts, &path[..n]))
    }
}

/// A path's parts, lowercase, the root as an empty first part.
fn split(path: &Path) -> Vec<String> {
    let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
    text.split('/').map(String::from).collect()
}

fn match_parts(mask: &[String], path: &[String]) -> bool {
    match mask.split_first() {
        None => path.is_empty(),
        Some((m, rest)) if m == "**" => (0..=path.len()).any(|skip| match_parts(rest, &path[skip..])),
        Some((m, rest)) => path
            .split_first()
            .is_some_and(|(p, path)| match_name(m.as_bytes(), p.as_bytes()) && match_parts(rest, path)),
    }
}

/// One name against one mask part, `*` and `?` within it. Bytes, but `?`
/// steps over a whole UTF-8 character.
fn match_name(mask: &[u8], name: &[u8]) -> bool {
    match mask.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|i| match_name(rest, &name[i..])),
        Some((b'?', rest)) => {
            let Some(&first) = name.first() else { return false };
            let len = match first {
                b if b < 0x80 => 1,
                b if b >= 0xF0 => 4,
                b if b >= 0xE0 => 3,
                _ => 2,
            };
            name.len() >= len && match_name(rest, &name[len..])
        }
        Some((c, rest)) => name.first() == Some(c) && match_name(rest, &name[1..]),
    }
}

#[cfg(test)]
#[path = "glob.test.rs"]
mod tests;
