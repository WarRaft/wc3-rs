//! The wire protocol between the VS Code extension and `wc3-server`.
//!
//! Binary, over stdio, the same in both directions. Every message is one
//! frame:
//!
//! ```text
//! u32 length of the body (little-endian) | u8 kind | body
//! ```
//!
//! A body is a sequence of fields, all little-endian:
//!
//! | field  | encoding                                   |
//! |--------|--------------------------------------------|
//! | `u8`, `u32`, `i32` | as is                          |
//! | string | `u32` byte length, then UTF-8 bytes        |
//! | list   | `u32` count, then the items                |
//! | range  | four `u32`: start line, start character, end line, end character (0-based, UTF-16 characters, as VS Code counts) |
//!
//! The editor only reports what happens to documents; the server answers
//! each change with everything it knows about the file in one `File`
//! message, and also sends `File` for project files that aren't open.
//!
//! Questions only the cursor can ask (go to definition, find references,
//! hover, symbol search) are a `Query`; the server answers it after
//! analyzing every change that arrived before it, so the answer is never
//! stale.
//!
//! A document is identified by an **id**: its path on disk for a file
//! (VS Code's `fsPath`), its URI otherwise (an untitled document). A file
//! inside a map has a path inside the map's, `<map.w3x>/war3map.j`; the
//! editor shows it under the `wc3map:` scheme with that path, a file
//! system it reaches through `Fs` messages (see `mapfs`). A texture the
//! editor shows is sent as its bytes in a `Texture` message and comes back
//! decoded (see `texture`); object data (`.w3a` ...) goes to its text form
//! and back the same way, in `Objects` messages (see `objects`). The
//! server reports files the editor never opened, and a path is what both
//! sides know them by. A reader skips frames of a kind it doesn't know, so either
//! side can add messages without breaking the other. The JavaScript twin
//! of this file is `extension/protocol.js`; keep the two in step.

use std::io::{self, Read};

use parser_rs::{Level, SymbolKind};

use crate::analysis::Diagnostic;
use crate::document::{Position, Range};
use crate::index::{Location, QueryKind};
use crate::mapfs::{FsError, Kind, Stat};
use crate::texture::Texture;

/// Header size: `u32` body length + `u8` kind.
pub const HEADER: usize = 5;

/// The largest body accepted, so a corrupt length can't make us allocate
/// gigabytes.
pub const MAX_BODY: usize = 1 << 30;

/// Editor → server.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientMsg {
    /// Kind 1. The workspace folders, sent at startup and whenever they
    /// change: the server loads every project in them and watches them.
    Init { roots: Vec<String> },
    /// Kind 2. A document was opened (or switched to a language the
    /// server knows). `path` is empty for a document with no file.
    Open {
        id: String,
        path: String,
        lang: String,
        version: i32,
        text: String,
    },
    /// Kind 3. Edits, applied in order, each against the result of the
    /// previous one.
    Edit {
        id: String,
        version: i32,
        changes: Vec<(Range, String)>,
    },
    /// Kind 4. The document's language changed; its text didn't.
    Lang { id: String, lang: String },
    /// Kind 5. The document was closed.
    Close { id: String },
    /// Kind 6. A question about the name at `pos` in `id`, or (workspace
    /// symbols) about `text`. `request` is echoed in the `Answer`. The
    /// query kind is a `u8`: 0 definition, 1 references, 2 hover,
    /// 3 document symbols, 4 workspace symbols, 5 prepare rename,
    /// 6 rename (`text` is the new name), 7 content (the answer's `text`
    /// is the file's text), 8 path completion and 9 path links in a
    /// `wc3.bni`, 10 an SLK table, 11 an SLK cell written, 12 inlay
    /// hints, 13 colors, 14 a txt2 preview, 15 a trigger's view, 16 an
    /// operation in it, 17 triggers as files, 18 a parameter's choices,
    /// 19 the functions of a new line, 20 the trigger editor's icons (see
    /// `index::QueryKind`). An unknown one gets an
    /// empty answer.
    Query {
        request: u32,
        kind: Option<QueryKind>,
        id: String,
        pos: Position,
        text: String,
    },
    /// Kind 7. A file system operation inside a map (see `mapfs`), answered
    /// at once by `Fs`: `u8` op (0 stat, 1 read folder, 2 read, 3 write,
    /// 4 delete, 5 rename, 6 make folder), `path`, `to` (rename's target),
    /// `u8` flags (1 create, 2 overwrite, 4 recursive), `data` (write's
    /// bytes: `u32` length, then the bytes).
    Fs {
        request: u32,
        op: Option<FsOp>,
        path: String,
        to: String,
        flags: u8,
        data: Vec<u8>,
    },
    /// Kind 8. A picture file's path (a string: what it is, and where
    /// what it needs is, a shadow map its terrain) and bytes (`u32`
    /// length, then the bytes), to decode for the viewer; answered at once
    /// by `Texture`.
    Texture { request: u32, name: String, data: Vec<u8> },
    /// Kind 9. Object data to convert: `u8` op (0: the file's bytes to
    /// its text form; 1: a text form, as UTF-8, to the file's bytes), the
    /// file's name (its extension says what it holds), `data`. Answered at
    /// once by `Objects`.
    Objects { request: u32, op: u8, name: String, data: Vec<u8> },
}

/// The operations of `ClientMsg::Fs`, in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsOp {
    Stat,
    ReadDir,
    Read,
    Write,
    Delete,
    Rename,
    CreateDir,
}

const FS_OPS: [FsOp; 7] = [FsOp::Stat, FsOp::ReadDir, FsOp::Read, FsOp::Write, FsOp::Delete, FsOp::Rename, FsOp::CreateDir];

pub mod fs_flags {
    pub const CREATE: u8 = 1;
    pub const OVERWRITE: u8 = 2;
    pub const RECURSIVE: u8 = 4;
}

/// Server → editor.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMsg {
    /// Kind 129. Sent once, first: the semantic token types and modifiers
    /// (two lists of strings) that `File` tokens index. The legend lives
    /// in one place, `parser_rs::semantic`.
    Legend {
        types: Vec<String>,
        modifiers: Vec<String>,
    },
    /// Kind 128. Everything known about one file at `version`:
    /// diagnostics (`u8` severity, 0 error / 1 warning, as
    /// `vscode.DiagnosticSeverity`; range; message; the lint rule's id, as
    /// `vscode.Diagnostic.code`, empty for a parse error; its quick fix:
    /// title, empty for none, and edits, each a range and its new text) and semantic tokens (a
    /// list of `u32`, five per token, ready for `vscode.SemanticTokens`).
    /// Tokens are only sent for documents open in the editor; a project
    /// file that isn't open has version 0 and diagnostics only. A file
    /// that left the model (closed and not a project file, deleted, no
    /// longer in `sources`) gets one with both lists empty.
    File {
        id: String,
        version: i32,
        diagnostics: Vec<Diagnostic>,
        tokens: Vec<u32>,
    },
    /// Kind 130. The answer to `Query` number `request`: `text` (Markdown
    /// for a hover; for a rename, why it's refused, empty if it isn't) and
    /// locations (id, range, name, container: the namespace a declaration
    /// lives in, `u8` symbol kind:
    /// 0 function, 1 native, 2 global, 3 constant, 4 local, 5 parameter,
    /// 6 type, 7 library, 8 struct, 9 interface, 10 module, 11 method,
    /// 12 field, 13 namespace, 14 class, 15 enum, 16 enum member).
    Answer {
        request: u32,
        text: String,
        locations: Vec<Location>,
    },
    /// Kind 131. The answer to `ClientMsg::Fs` number `request`: `u8`
    /// error (0 none, 1 not found, 2 exists, 3 not a folder, 4 a folder,
    /// 5 no permission, 6 other) and its message; `u8` stat kind (0 none,
    /// 1 file, 2 folder), `u64` size, `u64` time (ms), `u8` read-only, note;
    /// the folder's entries (name, `u8` kind); the file's bytes.
    Fs {
        request: u32,
        error: Option<FsError>,
        stat: Option<Stat>,
        entries: Vec<(String, Kind)>,
        data: Vec<u8>,
    },
    /// Kind 132. The answer to `ClientMsg::Texture` number `request`:
    /// error (empty if it decoded), version (`BLP1`), encoding (`JPEG`),
    /// `u8` alpha bits, `u32` width, `u32` height, then the mips, the base
    /// one first: `u32` width, `u32` height, error, RGBA bytes (empty for
    /// a mip that's missing or doesn't decode).
    Texture { request: u32, texture: Texture },
    /// Kind 133. The answer to `ClientMsg::Objects` number `request`:
    /// error (empty if it converted), then the result (the text as UTF-8,
    /// or the file's bytes).
    Objects { request: u32, error: String, data: Vec<u8> },
}

fn kind_code(kind: Kind) -> u8 {
    match kind {
        Kind::File => 1,
        Kind::Dir => 2,
    }
}

const QUERY_KINDS: [QueryKind; 21] = [
    QueryKind::Definition,
    QueryKind::References,
    QueryKind::Hover,
    QueryKind::DocumentSymbols,
    QueryKind::WorkspaceSymbols,
    QueryKind::PrepareRename,
    QueryKind::Rename,
    QueryKind::Content,
    QueryKind::PathCompletion,
    QueryKind::PathLinks,
    QueryKind::SlkTable,
    QueryKind::SlkSetCell,
    QueryKind::InlayHints,
    QueryKind::Colors,
    QueryKind::Txt2Html,
    QueryKind::TriggerView,
    QueryKind::TriggerEdit,
    QueryKind::TriggerExtract,
    QueryKind::TriggerChoices,
    QueryKind::TriggerCatalog,
    QueryKind::TriggerIcons,
];

const SYMBOL_KINDS: [SymbolKind; 17] = [
    SymbolKind::Function,
    SymbolKind::Native,
    SymbolKind::Global,
    SymbolKind::Constant,
    SymbolKind::Local,
    SymbolKind::Parameter,
    SymbolKind::Type,
    SymbolKind::Library,
    SymbolKind::Struct,
    SymbolKind::Interface,
    SymbolKind::Module,
    SymbolKind::Method,
    SymbolKind::Field,
    SymbolKind::Namespace,
    SymbolKind::Class,
    SymbolKind::Enum,
    SymbolKind::EnumMember,
];

fn symbol_kind_code(kind: SymbolKind) -> u8 {
    SYMBOL_KINDS.iter().position(|k| *k == kind).expect("every kind is listed") as u8
}

mod kind {
    pub const INIT: u8 = 1;
    pub const OPEN: u8 = 2;
    pub const EDIT: u8 = 3;
    pub const LANG: u8 = 4;
    pub const CLOSE: u8 = 5;
    pub const QUERY: u8 = 6;
    pub const FS: u8 = 7;
    pub const TEXTURE: u8 = 8;
    pub const OBJECTS: u8 = 9;
    pub const FILE: u8 = 128;
    pub const LEGEND: u8 = 129;
    pub const ANSWER: u8 = 130;
    pub const FS_ANSWER: u8 = 131;
    pub const TEXTURE_ANSWER: u8 = 132;
    pub const OBJECTS_ANSWER: u8 = 133;
}

/// Reads one frame: `Ok(None)` at a clean end of input (between frames).
pub fn read_frame(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; HEADER];
    let mut filled = 0;
    while filled < HEADER {
        match input.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => filled += n,
        }
    }
    let len = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    if len > MAX_BODY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("frame of {len} bytes")));
    }
    let mut body = vec![0u8; len];
    input.read_exact(&mut body)?;
    Ok(Some((header[4], body)))
}

/// A frame's body was shorter than its fields, or not UTF-8.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed message: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

impl ClientMsg {
    /// Decodes a frame. `Ok(None)` for a kind this server doesn't know.
    pub fn decode(kind: u8, body: &[u8]) -> Result<Option<ClientMsg>, DecodeError> {
        let mut r = Reader { buf: body };
        let msg = match kind {
            kind::INIT => ClientMsg::Init {
                roots: r.list(|r| r.string())?,
            },
            kind::OPEN => ClientMsg::Open {
                id: r.string()?,
                path: r.string()?,
                lang: r.string()?,
                version: r.i32()?,
                text: r.string()?,
            },
            kind::EDIT => ClientMsg::Edit {
                id: r.string()?,
                version: r.i32()?,
                changes: r.list(|r| Ok((r.range()?, r.string()?)))?,
            },
            kind::LANG => ClientMsg::Lang {
                id: r.string()?,
                lang: r.string()?,
            },
            kind::CLOSE => ClientMsg::Close { id: r.string()? },
            kind::QUERY => ClientMsg::Query {
                request: r.u32()?,
                kind: QUERY_KINDS.get(r.u8()? as usize).copied(),
                id: r.string()?,
                pos: Position::new(r.u32()?, r.u32()?),
                text: r.string()?,
            },
            kind::FS => ClientMsg::Fs {
                request: r.u32()?,
                op: FS_OPS.get(r.u8()? as usize).copied(),
                path: r.string()?,
                to: r.string()?,
                flags: r.u8()?,
                data: r.bytes()?,
            },
            kind::TEXTURE => ClientMsg::Texture { request: r.u32()?, name: r.string()?, data: r.bytes()? },
            kind::OBJECTS => ClientMsg::Objects { request: r.u32()?, op: r.u8()?, name: r.string()?, data: r.bytes()? },
            _ => return Ok(None),
        };
        Ok(Some(msg))
    }

    /// The whole frame. The server never sends these; tests do.
    #[cfg(test)]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let kind = match self {
            ClientMsg::Init { roots } => {
                w.list(roots, |w, s| w.string(s));
                kind::INIT
            }
            ClientMsg::Open { id, path, lang, version, text } => {
                w.string(id);
                w.string(path);
                w.string(lang);
                w.i32(*version);
                w.string(text);
                kind::OPEN
            }
            ClientMsg::Edit { id, version, changes } => {
                w.string(id);
                w.i32(*version);
                w.list(changes, |w, (range, text)| {
                    w.range(*range);
                    w.string(text);
                });
                kind::EDIT
            }
            ClientMsg::Lang { id, lang } => {
                w.string(id);
                w.string(lang);
                kind::LANG
            }
            ClientMsg::Close { id } => {
                w.string(id);
                kind::CLOSE
            }
            ClientMsg::Query { request, kind, id, pos, text } => {
                w.u32(*request);
                w.u8(kind.map_or(u8::MAX, |k| QUERY_KINDS.iter().position(|q| *q == k).unwrap() as u8));
                w.string(id);
                w.u32(pos.line);
                w.u32(pos.character);
                w.string(text);
                kind::QUERY
            }
            ClientMsg::Fs { request, op, path, to, flags, data } => {
                w.u32(*request);
                w.u8(op.map_or(u8::MAX, |o| FS_OPS.iter().position(|x| *x == o).unwrap() as u8));
                w.string(path);
                w.string(to);
                w.u8(*flags);
                w.bytes(data);
                kind::FS
            }
            ClientMsg::Texture { request, name, data } => {
                w.u32(*request);
                w.string(name);
                w.bytes(data);
                kind::TEXTURE
            }
            ClientMsg::Objects { request, op, name, data } => {
                w.u32(*request);
                w.u8(*op);
                w.string(name);
                w.bytes(data);
                kind::OBJECTS
            }
        };
        w.frame(kind)
    }
}

impl ServerMsg {
    /// The whole frame, ready to write.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let kind = match self {
            ServerMsg::File { id, version, diagnostics, tokens } => {
                w.string(id);
                w.i32(*version);
                w.list(diagnostics, |w, d| {
                    w.u8(match d.level {
                        Level::Error => 0,
                        Level::Warning => 1,
                    });
                    w.range(d.range);
                    w.string(&d.message);
                    w.string(d.rule);
                    match &d.fix {
                        Some(fix) => {
                            w.string(&fix.title);
                            w.list(&fix.edits, |w, (range, text)| {
                                w.range(*range);
                                w.string(text);
                            });
                        }
                        None => {
                            w.string("");
                            w.u32(0);
                        }
                    }
                });
                w.list(tokens, |w, t| w.u32(*t));
                kind::FILE
            }
            ServerMsg::Legend { types, modifiers } => {
                w.list(types, |w, s| w.string(s));
                w.list(modifiers, |w, s| w.string(s));
                kind::LEGEND
            }
            ServerMsg::Answer { request, text, locations } => {
                w.u32(*request);
                w.string(text);
                w.list(locations, |w, l| {
                    w.string(&l.id);
                    w.range(l.range);
                    w.string(&l.name);
                    w.string(&l.container);
                    w.u8(symbol_kind_code(l.kind));
                });
                kind::ANSWER
            }
            ServerMsg::Fs { request, error, stat, entries, data } => {
                w.u32(*request);
                let (code, message) = match error {
                    None => (0, ""),
                    Some(FsError::NotFound) => (1, ""),
                    Some(FsError::Exists) => (2, ""),
                    Some(FsError::NotADirectory) => (3, ""),
                    Some(FsError::IsADirectory) => (4, ""),
                    Some(FsError::NoPermissions(m)) => (5, m.as_str()),
                    Some(FsError::Other(m)) => (6, m.as_str()),
                };
                w.u8(code);
                w.string(message);
                match stat {
                    Some(s) => {
                        w.u8(kind_code(s.kind));
                        w.u64(s.size);
                        w.u64(s.mtime);
                        w.u8(s.readonly as u8);
                        w.string(&s.note);
                    }
                    None => {
                        w.u8(0);
                        w.u64(0);
                        w.u64(0);
                        w.u8(0);
                        w.string("");
                    }
                }
                w.list(entries, |w, (name, kind)| {
                    w.string(name);
                    w.u8(kind_code(*kind));
                });
                w.bytes(data);
                kind::FS_ANSWER
            }
            ServerMsg::Texture { request, texture: t } => {
                w.u32(*request);
                w.string(&t.error);
                w.string(&t.version);
                w.string(&t.encoding);
                w.u8(t.alpha_bits);
                w.u32(t.width);
                w.u32(t.height);
                w.list(&t.mips, |w, m| {
                    w.u32(m.width);
                    w.u32(m.height);
                    w.string(&m.error);
                    w.bytes(&m.rgba);
                });
                w.bytes(&t.cells);
                w.list(&t.flags, |w, (bit, name)| {
                    w.u8(*bit);
                    w.string(name);
                });
                kind::TEXTURE_ANSWER
            }
            ServerMsg::Objects { request, error, data } => {
                w.u32(*request);
                w.string(error);
                w.bytes(data);
                kind::OBJECTS_ANSWER
            }
        };
        w.frame(kind)
    }

    /// Decodes a frame. The extension does this in JavaScript; tests do
    /// it here.
    #[cfg(test)]
    pub fn decode(kind: u8, body: &[u8]) -> Result<Option<ServerMsg>, DecodeError> {
        let mut r = Reader { buf: body };
        let msg = match kind {
            kind::FILE => ServerMsg::File {
                id: r.string()?,
                version: r.i32()?,
                diagnostics: r.list(|r| {
                    let level = match r.u8()? {
                        0 => Level::Error,
                        _ => Level::Warning,
                    };
                    let range = r.range()?;
                    let message = r.string()?;
                    // Rule ids are static: decoding (tests only) maps a
                    // known one back to it.
                    let rule = r.string()?;
                    let rule = parser_rs::find_rule(&rule).map_or("", |known| known.id);
                    let title = r.string()?;
                    let edits = r.list(|r| Ok((r.range()?, r.string()?)))?;
                    let fix = (!title.is_empty()).then_some(crate::analysis::Fix { title, edits });
                    Ok(Diagnostic { level, range, message, rule, fix })
                })?,
                tokens: r.list(|r| r.u32())?,
            },
            kind::LEGEND => ServerMsg::Legend {
                types: r.list(|r| r.string())?,
                modifiers: r.list(|r| r.string())?,
            },
            kind::ANSWER => ServerMsg::Answer {
                request: r.u32()?,
                text: r.string()?,
                locations: r.list(|r| {
                    Ok(Location {
                        id: r.string()?,
                        range: r.range()?,
                        name: r.string()?,
                        container: r.string()?,
                        kind: *SYMBOL_KINDS
                            .get(r.u8()? as usize)
                            .ok_or_else(|| DecodeError("unknown symbol kind".into()))?,
                    })
                })?,
            },
            kind::FS_ANSWER => {
                let request = r.u32()?;
                let error = match (r.u8()?, r.string()?) {
                    (0, _) => None,
                    (1, _) => Some(FsError::NotFound),
                    (2, _) => Some(FsError::Exists),
                    (3, _) => Some(FsError::NotADirectory),
                    (4, _) => Some(FsError::IsADirectory),
                    (5, m) => Some(FsError::NoPermissions(m)),
                    (_, m) => Some(FsError::Other(m)),
                };
                let code = r.u8()?;
                let (size, mtime, readonly, note) = (r.u64()?, r.u64()?, r.u8()? != 0, r.string()?);
                let kind_of = |c: u8| if c == 2 { Kind::Dir } else { Kind::File };
                let stat = (code != 0).then(|| Stat { kind: kind_of(code), size, mtime, readonly, note });
                let entries = r.list(|r| Ok((r.string()?, kind_of(r.u8()?))))?;
                ServerMsg::Fs { request, error, stat, entries, data: r.bytes()? }
            }
            kind::TEXTURE_ANSWER => ServerMsg::Texture {
                request: r.u32()?,
                texture: Texture {
                    error: r.string()?,
                    version: r.string()?,
                    encoding: r.string()?,
                    alpha_bits: r.u8()?,
                    width: r.u32()?,
                    height: r.u32()?,
                    mips: r.list(|r| Ok(crate::texture::Mip { width: r.u32()?, height: r.u32()?, error: r.string()?, rgba: r.bytes()? }))?,
                    cells: r.bytes()?,
                    flags: r.list(|r| Ok((r.u8()?, r.string()?)))?,
                },
            },
            kind::OBJECTS_ANSWER => ServerMsg::Objects { request: r.u32()?, error: r.string()?, data: r.bytes()? },
            _ => return Ok(None),
        };
        Ok(Some(msg))
    }
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }

    fn list<T>(&mut self, items: &[T], mut item: impl FnMut(&mut Writer, &T)) {
        self.u32(items.len() as u32);
        for i in items {
            item(self, i);
        }
    }

    fn range(&mut self, r: Range) {
        for v in [r.start.line, r.start.character, r.end.line, r.end.character] {
            self.u32(v);
        }
    }

    /// Prepends the header.
    fn frame(self, kind: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.buf.len());
        out.extend_from_slice(&(self.buf.len() as u32).to_le_bytes());
        out.push(kind);
        out.extend_from_slice(&self.buf);
        out
    }
}

struct Reader<'a> {
    buf: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        if self.buf.len() < n {
            return Err(DecodeError(format!("needed {n} more bytes, {} left", self.buf.len())));
        }
        let (head, rest) = self.buf.split_at(n);
        self.buf = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    #[cfg(test)]
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let len = self.u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError("string is not UTF-8".into()))
    }

    fn list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Vec<T>, DecodeError> {
        let count = self.u32()? as usize;
        // Every item takes at least a byte, so a corrupt count can't
        // reserve more than the body holds.
        let mut out = Vec::with_capacity(count.min(self.buf.len()));
        for _ in 0..count {
            out.push(item(self)?);
        }
        Ok(out)
    }

    fn range(&mut self) -> Result<Range, DecodeError> {
        Ok(Range::new(
            Position::new(self.u32()?, self.u32()?),
            Position::new(self.u32()?, self.u32()?),
        ))
    }
}

#[cfg(test)]
#[path = "protocol.test.rs"]
mod tests;
