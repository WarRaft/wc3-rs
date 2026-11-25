//! The World Editor's backend (see the wc3-rs README, "World Editor"):
//! one process per project, `wc3 editor --project <root>`, holding the
//! project's map open and answering the editor's page over stdio in the
//! editor protocol ([`protocol`]). The page draws; this does the work.
//!
//! - [`lock`]: one editor per project, across VS Code windows;
//! - [`session`]: the map open, read through the project's lookup;
//! - [`tileset`]: the game's tables a terrain is drawn with;
//! - [`terrain`]: what the page draws of the terrain;
//! - [`textures`]: the ground's and the water's pictures, decoded;
//! - [`protocol`]: the frames.

pub mod cliffs;
pub mod lock;
pub mod picture;
pub mod placed;
pub mod protocol;
pub mod session;
pub mod terrain;
pub mod textures;
pub mod tileset;
pub mod types;

use std::io::{self, Write};
use std::path::Path;

use protocol::{ClientMsg, ServerMsg};

/// The exit code when another editor has the project open.
pub const EXIT_LOCKED: i32 = 3;

/// Runs the editor of the project at `root` on stdio until stdin closes;
/// the process's exit code: 0, [`EXIT_LOCKED`], or 1 when stdio fails.
pub fn run(root: &Path) -> i32 {
    // Absolute: it's said to the page and in messages.
    let root = std::path::absolute(root).map_or_else(|_| root.to_path_buf(), |p| wc3_server::project::normalize(&p));
    let root = root.as_path();
    let _lock = match lock::Lock::take(root) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("wc3 editor: {e}");
            return EXIT_LOCKED;
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    match serve(&mut stdin.lock(), &mut stdout.lock(), root) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("wc3 editor: {e}");
            1
        }
    }
}

/// Answers frames from `input` on `output` until `input` ends.
pub fn serve(input: &mut impl io::Read, output: &mut impl Write, root: &Path) -> io::Result<()> {
    let mut session: Option<session::Session> = None;
    while let Some((kind, body)) = protocol::read_frame(input)? {
        let msg = match ClientMsg::decode(kind, &body) {
            Ok(Some(msg)) => msg,
            // A kind this side doesn't know yet: skipped.
            Ok(None) => continue,
            Err(e) => {
                write(output, &ServerMsg::Error { message: e.to_string() })?;
                continue;
            }
        };
        match msg {
            ClientMsg::Hello => {
                // The page (re)loaded: the map as it is now, read again.
                match session::Session::open(root) {
                    Ok(mut s) => {
                        // The map's name first, for the page to show
                        // while the rest is worked out.
                        write(output, &s.map_msg())?;
                        for msg in s.draw() {
                            write(output, &msg)?;
                        }
                        session = Some(s);
                    }
                    Err(message) => write(output, &ServerMsg::Error { message })?,
                }
            }
            ClientMsg::Pick { origin, dir } => {
                let picked = session.as_ref().map(|s| s.pick(origin, dir)).unwrap_or_default();
                write(output, &ServerMsg::Picked(picked))?;
            }
        }
    }
    drop(session);
    Ok(())
}

fn write(output: &mut impl Write, msg: &ServerMsg) -> io::Result<()> {
    output.write_all(&msg.encode())?;
    output.flush()
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;

#[cfg(test)]
mod testdir;
