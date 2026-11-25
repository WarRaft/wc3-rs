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
//! a `u32` count, then the items; an **array** (heights, pixels) a `u32`
//! byte length, then its items as they are in memory (`f32`, `u32`
//! little-endian), for the page to view as a typed array. A reader skips frames of a kind it
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
//! | 130 | `Terrain` | the terrain as it's drawn: see [`TerrainMsg`] |
//! | 131 | `Ground` | the ground textures' layers: see [`GroundMsg`] |
//! | 132 | `Water` | the water: see [`WaterMsg`] |
//! | 133 | `Cliffs` | the cliffs and ramps: see [`CliffsMsg`] |
//!
//! `Hello` is answered by `Map`, then `Ground`, `Water` and `Cliffs`
//! (what the terrain's layers index, and what stands on it), then
//! `Terrain`.

use std::io::{self, Read};

pub use wc3_server::protocol::{HEADER, MAX_BODY};

mod kind {
    pub const HELLO: u8 = 1;
    pub const MAP: u8 = 128;
    pub const ERROR: u8 = 129;
    pub const TERRAIN: u8 = 130;
    pub const GROUND: u8 = 131;
    pub const WATER: u8 = 132;
    pub const CLIFFS: u8 = 133;
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

/// Kind 130's body, in this order: `u32` width and height (corners),
/// `f32` x and y of corner (0, 0); arrays, a corner each: `f32` final
/// heights, `f32` ground heights, `f32` water levels, `u8` flags
/// (`terrain::corner`); a tile each: `u32` ground layers, four per tile
/// (`terrain::Draw::tiles`), `u8` flags (`terrain::tile`).
pub type TerrainMsg = crate::terrain::Draw;

/// Kind 131's body: `u32` cell size, `u32` layer count, a list of the
/// textures (id, path, `u32` first layer, `u32` layers, `u8` extended,
/// error), then the layers' RGBA (an array).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroundMsg {
    pub textures: Vec<crate::textures::GroundTexture>,
    pub layers: crate::textures::Layers,
}

/// Kind 132's body: `f32` frames a second, four RGBA colours (shallow
/// least and most, deep least and most: 16 `u8`), `u32` frame size,
/// `u32` frames, the frames' RGBA (an array), error (what's missing).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaterMsg {
    pub rate: f32,
    pub colors: [[u8; 4]; 4],
    pub frames: crate::textures::Layers,
    pub error: String,
}

/// Kind 133's body: the cliff textures, `u32` size, `u32` count, their
/// RGBA (an array), a list of (path, error); then a list of the models:
/// path, `f32` positions (three a vertex, the model's units), `f32`
/// texture coordinates (two), `f32` normals (three), `u16` triangles
/// (three indices each), `f32` where each stands (four: tile x, tile y,
/// layer, texture), error; then notes (the models the game hasn't).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CliffsMsg {
    pub textures: crate::textures::Layers,
    pub texture_paths: Vec<(String, String)>,
    pub meshes: Vec<crate::cliffs::Mesh>,
    pub notes: Vec<String>,
}

/// Editor → page.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMsg {
    /// Kind 128.
    Map(MapInfo),
    /// Kind 129.
    Error { message: String },
    /// Kind 130.
    Terrain(TerrainMsg),
    /// Kind 131.
    Ground(GroundMsg),
    /// Kind 132.
    Water(WaterMsg),
    /// Kind 133.
    Cliffs(CliffsMsg),
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
            ServerMsg::Terrain(t) => {
                w.u32(t.width);
                w.u32(t.height);
                w.f32(t.offset[0]);
                w.f32(t.offset[1]);
                w.f32s(&t.heights);
                w.f32s(&t.ground);
                w.f32s(&t.water);
                w.bytes(&t.corners);
                w.u32s(t.tiles.as_flattened());
                w.bytes(&t.tile_flags);
                kind::TERRAIN
            }
            ServerMsg::Ground(g) => {
                w.u32(g.layers.size);
                w.u32(g.layers.count);
                w.list(&g.textures, |w, t| {
                    w.string(&t.id);
                    w.string(&t.path);
                    w.u32(t.first);
                    w.u32(t.count);
                    w.u8(t.extended as u8);
                    w.string(&t.error);
                });
                w.bytes(&g.layers.rgba);
                kind::GROUND
            }
            ServerMsg::Water(m) => {
                w.f32(m.rate);
                for c in &m.colors {
                    w.buf.extend_from_slice(c);
                }
                w.u32(m.frames.size);
                w.u32(m.frames.count);
                w.bytes(&m.frames.rgba);
                w.string(&m.error);
                kind::WATER
            }
            ServerMsg::Cliffs(c) => {
                w.u32(c.textures.size);
                w.u32(c.textures.count);
                w.bytes(&c.textures.rgba);
                w.list(&c.texture_paths, |w, (path, error)| {
                    w.string(path);
                    w.string(error);
                });
                w.list(&c.meshes, |w, m| {
                    w.string(&m.path);
                    w.f32s(&m.positions);
                    w.f32s(&m.uvs);
                    w.f32s(&m.normals);
                    w.u16s(&m.indices);
                    w.f32s(&m.instances);
                    w.string(&m.error);
                });
                w.list(&c.notes, |w, n| w.string(n));
                kind::CLIFFS
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
            kind::TERRAIN => ServerMsg::Terrain(crate::terrain::Draw {
                width: r.u32()?,
                height: r.u32()?,
                offset: [r.f32()?, r.f32()?],
                heights: r.f32s()?,
                ground: r.f32s()?,
                water: r.f32s()?,
                corners: r.bytes()?,
                tiles: r.u32s()?.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
                tile_flags: r.bytes()?,
            }),
            kind::GROUND => {
                let (size, count) = (r.u32()?, r.u32()?);
                let textures = r.list(|r| {
                    Ok(crate::textures::GroundTexture {
                        id: r.string()?,
                        path: r.string()?,
                        first: r.u32()?,
                        count: r.u32()?,
                        extended: r.u8()? != 0,
                        error: r.string()?,
                    })
                })?;
                ServerMsg::Ground(GroundMsg { textures, layers: crate::textures::Layers { size, count, rgba: r.bytes()? } })
            }
            kind::WATER => {
                let rate = r.f32()?;
                let mut colors = [[0u8; 4]; 4];
                for c in &mut colors {
                    c.copy_from_slice(r.take(4)?);
                }
                let (size, count) = (r.u32()?, r.u32()?);
                ServerMsg::Water(WaterMsg {
                    rate,
                    colors,
                    frames: crate::textures::Layers { size, count, rgba: r.bytes()? },
                    error: r.string()?,
                })
            }
            kind::CLIFFS => {
                let (size, count) = (r.u32()?, r.u32()?);
                let textures = crate::textures::Layers { size, count, rgba: r.bytes()? };
                let texture_paths = r.list(|r| Ok((r.string()?, r.string()?)))?;
                let meshes = r.list(|r| {
                    Ok(crate::cliffs::Mesh {
                        path: r.string()?,
                        positions: r.f32s()?,
                        uvs: r.f32s()?,
                        normals: r.f32s()?,
                        indices: r.bytes()?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
                        instances: r.f32s()?,
                        error: r.string()?,
                    })
                })?;
                ServerMsg::Cliffs(CliffsMsg { textures, texture_paths, meshes, notes: r.list(|r| r.string())? })
            }
            _ => return Ok(None),
        }))
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

    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// An array of bytes.
    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn f32s(&mut self, v: &[f32]) {
        self.u32((v.len() * 4) as u32);
        for x in v {
            self.f32(*x);
        }
    }

    fn u16s(&mut self, v: &[u16]) {
        self.u32((v.len() * 2) as u32);
        for x in v {
            self.buf.extend_from_slice(&x.to_le_bytes());
        }
    }

    fn u32s(&mut self, v: &[u32]) {
        self.u32((v.len() * 4) as u32);
        for x in v {
            self.u32(*x);
        }
    }

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

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }

    fn f32s(&mut self) -> Result<Vec<f32>, DecodeError> {
        Ok(self.bytes()?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    fn u32s(&mut self) -> Result<Vec<u32>, DecodeError> {
        Ok(self.bytes()?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect())
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
