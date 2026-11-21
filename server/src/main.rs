//! `wc3-server`: the language backend of the WC3 VS Code extension.
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

mod analysis;
mod document;
mod game;
mod glob;
mod index;
mod map;
mod mapfs;
mod objects;
mod paths;
mod project;
mod protocol;
mod rename;
mod server;
mod sheet;
mod texture;
mod workspace;

use std::io::{BufWriter, stdin, stdout};
use std::sync::mpsc;

fn main() {
    let (events, inbox) = mpsc::channel();
    let reader = events.clone();
    std::thread::spawn(move || server::read_events(stdin().lock(), &reader));

    let out = BufWriter::new(stdout().lock());
    if let Err(err) = server::Server::new(out, Some(events)).run(inbox) {
        // Most likely the editor stopped reading: nothing left to do.
        eprintln!("wc3-server: {err}");
    }
}
