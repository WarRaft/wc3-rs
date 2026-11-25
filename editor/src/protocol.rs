//! The editor protocol: between the World Editor's page
//! (`extension/webview/editor/`) and this process. The extension passes
//! the frames between the two as bytes, without reading them.
//!
//! The framing is `wc3-server`'s (`server/src/protocol.rs`): every message
//! is one frame, the same in both directions,
//!
//! ```text
//! u32 length of the body (little-endian) | u8 kind | body
//! ```
//!
//! and a body is a sequence of fields, all little-endian: `u8`, `u32`,
//! `f32` as they are; a string is a `u32` byte length, then UTF-8; a list
//! a `u32` count, then the items. A reader skips frames of a kind it
//! doesn't know, so either side can add messages without breaking the
//! other. The JavaScript twin is `extension/webview/editor/protocol.js`;
//! keep the two in step.
//!
//! Page → editor:
//!
//! | kind | message | body |
//! |------|---------|------|
//! | 1 | `Hello` | nothing: the page (re)loaded; answered by `Map` or `Error` |
//!
//! Editor → page:
//!
//! | kind | message | body |
//! |------|---------|------|
//! | 128 | `Map`   | the map open: see [`MapInfo`] |
//! | 129 | `Error` | a message: the map can't be opened, or a frame was malformed |

use std::io::{self, Read};

pub use wc3_server::protocol::{HEADER, MAX_BODY};

mod kind {
    pub const HELLO: u8 = 1;
    pub const MAP: u8 = 128;
    pub const ERROR: u8 = 129;
}

/// Page → editor.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientMsg {
    /// Kind 1.
    Hello,
}

/// What the page is told of the map: kind 128's body, in this order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapInfo {
    /// The project's folder.
    pub project: String,
    /// The map's name (`war3map.w3i`, its `TRIGSTR_` resolved by
    /// `war3map.wts`), and its author.
    pub name: String,
    pub author: String,
    /// The terrain's points across and up (`war3map.w3e`): one more than
    /// its cells.
    pub width: u32,
    pub height: u32,
    /// The tileset's letter (`L`) and its name from the game's files.
    pub tileset: String,
    pub tileset_name: String,
    /// What was read with a problem, said and gone past.
    pub notes: Vec<String>,
}

/// Editor → page.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMsg {
    /// Kind 128.
    Map(MapInfo),
    /// Kind 129.
    Error { message: String },
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

/// Reads one frame: `Ok(None)` at a clean end of input (between frames).
pub fn read_frame(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    wc3_server::protocol::read_frame(input)
}

impl ClientMsg {
    /// Decodes a frame. `Ok(None)` for a kind this side doesn't know.
    pub fn decode(kind: u8, _body: &[u8]) -> Result<Option<ClientMsg>, DecodeError> {
        Ok(match kind {
            kind::HELLO => Some(ClientMsg::Hello),
            _ => None,
        })
    }

    /// The whole frame. The editor never sends these; tests do.
    pub fn encode(&self) -> Vec<u8> {
        let w = Writer::default();
        let kind = match self {
            ClientMsg::Hello => kind::HELLO,
        };
        w.frame(kind)
    }
}

impl ServerMsg {
    /// The whole frame.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let kind = match self {
            ServerMsg::Map(m) => {
                w.string(&m.project);
                w.string(&m.name);
                w.string(&m.author);
                w.u32(m.width);
                w.u32(m.height);
                w.string(&m.tileset);
                w.string(&m.tileset_name);
                w.list(&m.notes, |w, s| w.string(s));
                kind::MAP
            }
            ServerMsg::Error { message } => {
                w.string(message);
                kind::ERROR
            }
        };
        w.frame(kind)
    }

    /// Decodes a frame; for tests (the page decodes them).
    pub fn decode(kind: u8, body: &[u8]) -> Result<Option<ServerMsg>, DecodeError> {
        let mut r = Reader { buf: body };
        Ok(Some(match kind {
            kind::MAP => ServerMsg::Map(MapInfo {
                project: r.string()?,
                name: r.string()?,
                author: r.string()?,
                width: r.u32()?,
                height: r.u32()?,
                tileset: r.string()?,
                tileset_name: r.string()?,
                notes: r.list(|r| r.string())?,
            }),
            kind::ERROR => ServerMsg::Error { message: r.string()? },
            _ => return Ok(None),
        }))
    }
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }

    fn list<T>(&mut self, items: &[T], mut each: impl FnMut(&mut Writer, &T)) {
        self.u32(items.len() as u32);
        for item in items {
            each(self, item);
        }
    }

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
            return Err(DecodeError(format!("{n} bytes wanted, {} left", self.buf.len())));
        }
        let (head, rest) = self.buf.split_at(n);
        self.buf = rest;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| DecodeError("a string isn't UTF-8".into()))
    }

    fn list<T>(&mut self, mut each: impl FnMut(&mut Self) -> Result<T, DecodeError>) -> Result<Vec<T>, DecodeError> {
        let n = self.u32()? as usize;
        (0..n).map(|_| each(self)).collect()
    }
}

#[cfg(test)]
#[path = "protocol.test.rs"]
mod tests;
