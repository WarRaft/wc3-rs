//! `wc3-server`: the language backend of the WC3 VS Code extension, and
//! the project model other tools build on.
//!
//! The extension spawns it and talks to it over stdio in a small binary
//! protocol (`protocol`). The editor only reports what happens to
//! documents; everything else is done here: keeping every file of every
//! project in memory (`workspace`, projects are `wc3.bni`, see `project`),
//! watching the disk, parsing and linting with
//! the language crates (`jass-rs`, `zinc-rs`, `lua-rs`, `as-rs`, `bni-rs`,
//! all built on `parser-rs`), and sending the results back. It also
//! decodes BLP textures for the editor's viewer (`texture`, with
//! `blp-rs`), and turns object data (`.w3a` ...) into its text form and
//! back (`objects`, with `w3abdhqtu-rs`). The process ends when its stdin closes, i.e. when the
//! editor goes away.

//!
//! It is a library as well as the `wc3-server` binary (`main.rs`, which
//! only runs [`server::Server`] over stdio): `wc3-mcp` uses the same
//! project model in-process, so an AI assistant and the editor see the
//! same projects, files, diagnostics and symbols. The entry points:
//!
//! - [`workspace::Workspace`]: the projects under some folders and every
//!   file of theirs, in memory: `set_roots`, `open` / `edit` / `close`
//!   a document, `take_reports` to analyze what changed (diagnostics,
//!   semantic tokens, symbols), `file`, `all_files`;
//! - [`index::answer`]: the queries the editor asks (definition,
//!   references, hover, symbols, rename, ...) against a workspace;
//! - [`analysis`]: one file's diagnostics and tokens, with its project's
//!   settings ([`project`]);
//! - [`game`]: the game's files from a project's `game=` folders;
//! - [`objects`], [`cameras`], [`sheet`], [`trigstr`]: object data,
//!   cameras, SLK tables and string tables for the editor.

pub mod analysis;
pub mod cameras;
pub mod document;
pub mod game;
pub mod glob;
pub mod index;
pub mod map;
pub mod mapfs;
pub mod objects;
pub mod paths;
pub mod project;
pub mod protocol;
pub mod rename;
pub mod server;
pub mod sheet;
pub mod texture;
pub mod trigstr;
pub mod workspace;
