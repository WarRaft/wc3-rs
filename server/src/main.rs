//! The `wc3-server` binary: the language backend the WC3 VS Code extension
//! spawns, talking to it over stdio (see the library, `lib.rs`, for what
//! it does). The process ends when its stdin closes, i.e. when the editor
//! goes away.

use std::io::{BufWriter, stdin, stdout};
use std::sync::mpsc;

use wc3_server::server;

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
