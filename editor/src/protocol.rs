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
//! | 2 | `Pick` | a ray from the camera: `f32` origin x, y, z, direction x, y, z (cells, layers); answered by `Picked` |
//! | 3 | `Brush` | a step of a stroke: `u8` its first, `u8` tool (`edit::TOOLS`), the ray (six `f32`), `f32` size, strength, the texture's tile id (empty for none), `i32` cliff; answered by what changed (`Ground`, `Cliffs`, `Terrain`) and `State` |
//! | 4 | `StrokeEnd` | the mouse let go; answered by `State` |
//! | 5 | `Undo` | answered by what changed and `State` |
//! | 6 | `Redo` | the same |
//! | 7 | `Save` | the terrain written where it came from; answered by `State` (where, or why not) |
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
//! | 134 | `Objects` | what's placed: see [`ObjectsMsg`] |
//! | 135 | `Picked` | what a `Pick` meets: see [`PickedMsg`] |
//! | 136 | `Overlays` | pathing, regions, bounds, cameras, minimap: see [`OverlaysMsg`] |
//! | 137 | `Palette` | the terrain palette: see [`PaletteMsg`] |
//! | 138 | `State` | the edits' state: see [`StateMsg`] |
//!
//! After an edit the terrain's messages come again: `Cliffs` with no
//! textures keeps the ones the page has.
//!
//! `Hello` is answered by `Map`, then `Ground`, `Water` and `Cliffs`
//! (what the terrain's layers index, and what stands on it), then
//! `Terrain`, then `Objects`, `Overlays`, `Palette` and `State`.

use std::io::{self, Read};

pub use wc3_server::protocol::{HEADER, MAX_BODY};

mod kind {
    pub const HELLO: u8 = 1;
    pub const PICK: u8 = 2;
    pub const BRUSH: u8 = 3;
    pub const STROKE_END: u8 = 4;
    pub const UNDO: u8 = 5;
    pub const REDO: u8 = 6;
    pub const SAVE: u8 = 7;
    pub const PALETTE: u8 = 137;
    pub const STATE: u8 = 138;
    pub const OBJECTS: u8 = 134;
    pub const PICKED: u8 = 135;
    pub const OVERLAYS: u8 = 136;
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
    /// Kind 2.
    Pick { origin: [f32; 3], dir: [f32; 3] },
    /// Kind 3.
    Brush { start: bool, origin: [f32; 3], dir: [f32; 3], brush: crate::edit::Brush },
    /// Kind 4.
    StrokeEnd,
    /// Kind 5.
    Undo,
    /// Kind 6.
    Redo,
    /// Kind 7.
    Save,
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

/// Kind 134's body: a list of textures (path, `u32` width, height, RGBA
/// array, error), a list of models (path, `f32` box min x, y, z, max x,
/// y, z, a list of geosets: `f32` positions, normals, texture
/// coordinates, `u16` triangles, a list of layers: `i32` texture
/// (`placed::texture`), `u32` filter mode, `u32` shading, `f32` alpha;
/// then `f32` instances, `placed::INSTANCE` each; error), notes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjectsMsg {
    pub textures: Vec<crate::placed::Texture>,
    pub models: Vec<crate::placed::Model>,
    pub notes: Vec<String>,
}

/// Kind 135's body: `u8` found (a thing), `u32` model, `u32` instance,
/// what it is (a line to show: the thing, else the ground's point and
/// its regions); `u8` on the ground, its point `f32` x, y, z (cells,
/// layers).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PickedMsg {
    pub found: bool,
    pub model: u32,
    pub instance: u32,
    pub text: String,
    pub ground: Option<[f32; 3]>,
}

/// Kind 136's body: the pathing (`u32` width, height, a byte a cell, an
/// array), the regions' picture (`u32` width, height, RGBA), a list of
/// regions (name, `f32` left, bottom, right, top, `u8` r, g, b), lines of
/// the bounds and of the cameras (`f32`, `overlays::POINT` a point), a
/// list of cameras (name, `f32` target x, y, z, eye x, y, z), the
/// minimap (`u32` width, height, RGBA, north up), notes.
pub type OverlaysMsg = crate::overlays::Overlays;

/// A palette's entry: id, name, a picture (`u32` width, height, RGBA).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaletteEntry {
    pub id: String,
    pub name: String,
    pub icon: crate::textures::Icon,
}

/// Kind 137's body: a list of ground textures, a list of cliff types
/// (their order the terrain's: a brush's `cliff`), each a [`PaletteEntry`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaletteMsg {
    pub grounds: Vec<PaletteEntry>,
    pub cliffs: Vec<PaletteEntry>,
}

/// Kind 138's body: `u8` unsaved changes, `u8` can undo, `u8` can redo,
/// a message (what a save or a brush said; empty for none).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateMsg {
    pub dirty: bool,
    pub undo: bool,
    pub redo: bool,
    pub message: String,
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
    /// Kind 134.
    Objects(ObjectsMsg),
    /// Kind 135.
    Picked(PickedMsg),
    /// Kind 136.
    Overlays(OverlaysMsg),
    /// Kind 137.
    Palette(PaletteMsg),
    /// Kind 138.
    State(StateMsg),
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
    pub fn decode(kind: u8, body: &[u8]) -> Result<Option<ClientMsg>, DecodeError> {
        let mut r = Reader { buf: body };
        Ok(match kind {
            kind::HELLO => Some(ClientMsg::Hello),
            kind::PICK => Some(ClientMsg::Pick { origin: [r.f32()?, r.f32()?, r.f32()?], dir: [r.f32()?, r.f32()?, r.f32()?] }),
            kind::BRUSH => {
                let start = r.u8()? != 0;
                let tool = crate::edit::TOOLS.get(r.u8()? as usize).copied().ok_or_else(|| DecodeError("no such tool".into()))?;
                let origin = [r.f32()?, r.f32()?, r.f32()?];
                let dir = [r.f32()?, r.f32()?, r.f32()?];
                let (size, strength) = (r.f32()?, r.f32()?);
                let texture = r.string()?;
                let cliff = r.u32()? as i32;
                let texture = parser_rs::bin::Rawcode::parse(&texture);
                Some(ClientMsg::Brush { start, origin, dir, brush: crate::edit::Brush { tool, size, strength, texture, cliff } })
            }
            kind::STROKE_END => Some(ClientMsg::StrokeEnd),
            kind::UNDO => Some(ClientMsg::Undo),
            kind::REDO => Some(ClientMsg::Redo),
            kind::SAVE => Some(ClientMsg::Save),
            _ => None,
        })
    }

    /// The whole frame. The editor never sends these; tests do.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let kind = match self {
            ClientMsg::Hello => kind::HELLO,
            ClientMsg::Pick { origin, dir } => {
                origin.iter().chain(dir).for_each(|v| w.f32(*v));
                kind::PICK
            }
            ClientMsg::Brush { start, origin, dir, brush } => {
                w.u8(*start as u8);
                w.u8(crate::edit::TOOLS.iter().position(|t| *t == brush.tool).unwrap() as u8);
                origin.iter().chain(dir).for_each(|v| w.f32(*v));
                w.f32(brush.size);
                w.f32(brush.strength);
                w.string(&brush.texture.map(|t| t.to_string()).unwrap_or_default());
                w.u32(brush.cliff as u32);
                kind::BRUSH
            }
            ClientMsg::StrokeEnd => kind::STROKE_END,
            ClientMsg::Undo => kind::UNDO,
            ClientMsg::Redo => kind::REDO,
            ClientMsg::Save => kind::SAVE,
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
            ServerMsg::Objects(o) => {
                w.list(&o.textures, |w, t| {
                    w.string(&t.path);
                    w.u32(t.width);
                    w.u32(t.height);
                    w.bytes(&t.rgba);
                    w.string(&t.error);
                });
                w.list(&o.models, |w, m| {
                    w.string(&m.path);
                    m.min.iter().chain(&m.max).for_each(|v| w.f32(*v));
                    w.list(&m.geosets, |w, g| {
                        w.f32s(&g.positions);
                        w.f32s(&g.normals);
                        w.f32s(&g.uvs);
                        w.u16s(&g.indices);
                        w.list(&g.layers, |w, l| {
                            w.u32(l.texture as u32);
                            w.u32(l.filter_mode);
                            w.u32(l.shading);
                            w.f32(l.alpha);
                        });
                    });
                    w.f32s(&m.instances);
                    w.string(&m.error);
                });
                w.list(&o.notes, |w, n| w.string(n));
                kind::OBJECTS
            }
            ServerMsg::Picked(p) => {
                w.u8(p.found as u8);
                w.u32(p.model);
                w.u32(p.instance);
                w.string(&p.text);
                w.u8(p.ground.is_some() as u8);
                p.ground.unwrap_or_default().iter().for_each(|v| w.f32(*v));
                kind::PICKED
            }
            ServerMsg::Overlays(o) => {
                for g in [&o.pathing, &o.region_picture] {
                    w.u32(g.width);
                    w.u32(g.height);
                    w.bytes(&g.data);
                }
                w.list(&o.regions, |w, r| {
                    w.string(&r.name);
                    r.bounds.iter().for_each(|v| w.f32(*v));
                    r.color.iter().for_each(|v| w.u8(*v));
                });
                w.f32s(&o.bounds);
                w.f32s(&o.camera_lines);
                w.list(&o.cameras, |w, c| {
                    w.string(&c.name);
                    c.target.iter().chain(&c.eye).for_each(|v| w.f32(*v));
                });
                w.u32(o.minimap.width);
                w.u32(o.minimap.height);
                w.bytes(&o.minimap.data);
                w.list(&o.notes, |w, n| w.string(n));
                kind::OVERLAYS
            }
            ServerMsg::Palette(p) => {
                for list in [&p.grounds, &p.cliffs] {
                    w.list(list, |w, e| {
                        w.string(&e.id);
                        w.string(&e.name);
                        w.u32(e.icon.width);
                        w.u32(e.icon.height);
                        w.bytes(&e.icon.rgba);
                    });
                }
                kind::PALETTE
            }
            ServerMsg::State(st) => {
                w.u8(st.dirty as u8);
                w.u8(st.undo as u8);
                w.u8(st.redo as u8);
                w.string(&st.message);
                kind::STATE
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
            kind::OBJECTS => {
                let textures = r.list(|r| {
                    Ok(crate::placed::Texture { path: r.string()?, width: r.u32()?, height: r.u32()?, rgba: r.bytes()?, error: r.string()? })
                })?;
                let models = r.list(|r| {
                    let path = r.string()?;
                    let min = [r.f32()?, r.f32()?, r.f32()?];
                    let max = [r.f32()?, r.f32()?, r.f32()?];
                    let geosets = r.list(|r| {
                        Ok(crate::placed::Geoset {
                            positions: r.f32s()?,
                            normals: r.f32s()?,
                            uvs: r.f32s()?,
                            indices: r.bytes()?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
                            layers: r.list(|r| {
                                Ok(crate::placed::Layer { texture: r.u32()? as i32, filter_mode: r.u32()?, shading: r.u32()?, alpha: r.f32()? })
                            })?,
                        })
                    })?;
                    Ok(crate::placed::Model { path, min, max, geosets, instances: r.f32s()?, error: r.string()? })
                })?;
                ServerMsg::Objects(ObjectsMsg { textures, models, notes: r.list(|r| r.string())? })
            }
            kind::PICKED => {
                let (found, model, instance, text) = (r.u8()? != 0, r.u32()?, r.u32()?, r.string()?);
                let on = r.u8()? != 0;
                let p = [r.f32()?, r.f32()?, r.f32()?];
                ServerMsg::Picked(PickedMsg { found, model, instance, text, ground: on.then_some(p) })
            }
            kind::OVERLAYS => {
                use crate::overlays::{Camera, Grid, Overlays, Region};
                let grid = |r: &mut Reader| -> Result<Grid, DecodeError> { Ok(Grid { width: r.u32()?, height: r.u32()?, data: r.bytes()? }) };
                let pathing = grid(&mut r)?;
                let region_picture = grid(&mut r)?;
                let regions = r.list(|r| {
                    Ok(Region { name: r.string()?, bounds: [r.f32()?, r.f32()?, r.f32()?, r.f32()?], color: [r.u8()?, r.u8()?, r.u8()?] })
                })?;
                let bounds = r.f32s()?;
                let camera_lines = r.f32s()?;
                let cameras = r.list(|r| Ok(Camera { name: r.string()?, target: [r.f32()?, r.f32()?, r.f32()?], eye: [r.f32()?, r.f32()?, r.f32()?] }))?;
                let minimap = grid(&mut r)?;
                let notes = r.list(|r| r.string())?;
                ServerMsg::Overlays(Overlays { pathing, regions, region_picture, bounds, camera_lines, cameras, minimap, notes })
            }
            kind::PALETTE => {
                let entry = |r: &mut Reader| -> Result<PaletteEntry, DecodeError> {
                    Ok(PaletteEntry {
                        id: r.string()?,
                        name: r.string()?,
                        icon: crate::textures::Icon { width: r.u32()?, height: r.u32()?, rgba: r.bytes()? },
                    })
                };
                ServerMsg::Palette(PaletteMsg { grounds: r.list(entry)?, cliffs: r.list(entry)? })
            }
            kind::STATE => ServerMsg::State(StateMsg { dirty: r.u8()? != 0, undo: r.u8()? != 0, redo: r.u8()? != 0, message: r.string()? }),
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
