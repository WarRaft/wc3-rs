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

pub mod anim;
pub mod cliffs;
pub mod constants;
pub mod edit;
pub mod light;
pub mod lock;
pub mod objdata;
pub mod overlays;
pub mod pathing;
pub mod particles;
pub mod picture;
pub mod placed;
pub mod protocol;
pub mod resize;
pub mod session;
pub mod terrain;
pub mod textures;
pub mod tiles;
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
    // The page's frames and the disk's changes, one after another.
    let (tx, rx) = std::sync::mpsc::channel::<Event>();
    let frames = tx.clone();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        loop {
            let event = match protocol::read_frame(&mut input) {
                Ok(Some((kind, body))) => Event::Frame(kind, body),
                Ok(None) => Event::End,
                Err(e) => Event::Failed(e),
            };
            let end = !matches!(event, Event::Frame(..));
            if frames.send(event).is_err() || end {
                return;
            }
        }
    });
    let _watcher = watch(root, tx);
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut editor = Editor { root: root.to_path_buf(), session: None };
    let result = (|| -> io::Result<()> {
        // An event that came while the disk's were gathered: next.
        let mut next: Option<Event> = None;
        while let Some(event) = next.take().or_else(|| rx.recv().ok()) {
            match event {
                Event::Frame(kind, body) => editor.frame(kind, &body, &mut output)?,
                Event::Disk(mut paths) => {
                    // A save writes several files: the rest of them first.
                    loop {
                        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                            Ok(Event::Disk(more)) => paths.extend(more),
                            Ok(other) => {
                                next = Some(other);
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    editor.disk(&paths, &mut output)?;
                }
                Event::End => return Ok(()),
                Event::Failed(e) => return Err(e),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("wc3 editor: {e}");
            1
        }
    }
}

/// What the editor answers: a frame of the page's, a change on disk.
enum Event {
    Frame(u8, Vec<u8>),
    Disk(Vec<std::path::PathBuf>),
    End,
    Failed(io::Error),
}

/// Watches the project's roots (a folder and what's in it; a map, its
/// file) and its `wc3.bni`; `None` when the system can't.
fn watch(root: &Path, tx: std::sync::mpsc::Sender<Event>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let src = std::fs::read_to_string(root.join("wc3.bni")).ok()?;
    let (project, _) = wc3_server::project::load(root, &src);
    let mut watcher = notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
        if let Ok(e) = e
            && !matches!(e.kind, notify::EventKind::Access(_))
        {
            let _ = tx.send(Event::Disk(e.paths));
        }
    })
    .ok()?;
    for r in &project.roots {
        let (at, mode) = if r.is_dir() { (r.as_path(), notify::RecursiveMode::Recursive) } else { (r.parent()?, notify::RecursiveMode::NonRecursive) };
        if let Err(e) = watcher.watch(at, mode) {
            eprintln!("wc3 editor: can't watch {}: {e}", at.display());
        }
    }
    Some(watcher)
}

/// Answers frames from `input` on `output` until `input` ends (no disk
/// watched: for tests).
pub fn serve(input: &mut impl io::Read, output: &mut impl Write, root: &Path) -> io::Result<()> {
    let mut editor = Editor { root: root.to_path_buf(), session: None };
    while let Some((kind, body)) = protocol::read_frame(input)? {
        editor.frame(kind, &body, output)?;
    }
    Ok(())
}

/// The project's map open, or not yet.
pub struct Editor {
    root: std::path::PathBuf,
    session: Option<session::Session>,
}

impl Editor {
    /// Files of the project changed on disk: the map's pieces read again
    /// (`Session::disk_changed`).
    pub fn disk(&mut self, paths: &[std::path::PathBuf], output: &mut impl Write) -> io::Result<()> {
        let Some(s) = self.session.as_mut() else { return Ok(()) };
        for msg in s.disk_changed(paths) {
            write(output, &msg)?;
        }
        Ok(())
    }

    /// Answers one frame.
    pub fn frame(&mut self, kind: u8, body: &[u8], output: &mut impl Write) -> io::Result<()> {
        let (root, session) = (&self.root, &mut self.session);
        let root = root.as_path();
        let msg = match ClientMsg::decode(kind, body) {
            Ok(Some(msg)) => msg,
            // A kind this side doesn't know yet: skipped.
            Ok(None) => return Ok(()),
            Err(e) => {
                write(output, &ServerMsg::Error { message: e.to_string() })?;
                return Ok(());
            }
        };
        match msg {
            ClientMsg::Hello | ClientMsg::Reload => {
                // The page (re)loaded: the map open, with its changes
                // (unsaved ones too); the first time, or to drop them,
                // read.
                let reload = matches!(msg, ClientMsg::Reload);
                let opened = match session.take() {
                    Some(s) if !reload => Ok(s),
                    _ => session::Session::open(root),
                };
                match opened {
                    Ok(mut s) => {
                        // The map's name first, for the page to show
                        // while the rest is worked out.
                        write(output, &s.map_msg())?;
                        for msg in s.draw() {
                            write(output, &msg)?;
                        }
                        *session = Some(s);
                    }
                    Err(message) => write(output, &ServerMsg::Error { message })?,
                }
            }
            ClientMsg::Pick { origin, dir } => {
                let picked = session.as_ref().map(|s| s.pick(origin, dir)).unwrap_or_default();
                write(output, &ServerMsg::Picked(picked))?;
            }
            edit => {
                let Some(s) = session.as_mut() else { return Ok(()) };
                let answers = match edit {
                    ClientMsg::Brush { start, origin, dir, brush } => s.brush(origin, dir, &brush, start),
                    ClientMsg::StrokeEnd => s.stroke_end(),
                    ClientMsg::Undo => s.undo(false),
                    ClientMsg::Redo => s.undo(true),
                    ClientMsg::Save => s.save(),
                    ClientMsg::Place { kind, id, player, origin, dir } => s.place(kind, &id, player, origin, dir),
                    ClientMsg::Move { start, file, index, origin, dir } => s.move_thing(file, index, origin, dir, start),
                    ClientMsg::Rotate { file, index, delta } => s.rotate(file, index, delta),
                    ClientMsg::Delete { file, index } => s.delete(file, index),
                    ClientMsg::Icons(wanted) => vec![ServerMsg::Icons(s.icons(&wanted))],
                    ClientMsg::Region { start, mode, index, from, to } => s.region_drag(start, mode, index, from, to),
                    ClientMsg::RegionRename { index, name } => s.region_rename(index, &name),
                    ClientMsg::RegionDelete { index } => s.region_delete(index),
                    ClientMsg::Camera { start, index, origin, dir } => s.camera_drag(start, index, origin, dir),
                    ClientMsg::CameraView { index, view } => s.camera_view(index, view),
                    ClientMsg::CameraRename { index, name } => s.camera_rename(index, &name),
                    ClientMsg::CameraDelete { index } => s.camera_delete(index),
                    ClientMsg::CameraSet { index, field, value } => s.camera_set(index, &field, value),
                    ClientMsg::ObjectList { kind } => s.object_list_first(kind),
                    ClientMsg::ObjectAsk { kind, id } => vec![s.object_fields(kind, &id)],
                    ClientMsg::ObjectSet { kind, id, field, level, column, value } => s.object_set(kind, &id, &field, level, column, &value),
                    ClientMsg::ObjectNew { kind, base } => s.object_new(kind, &base),
                    ClientMsg::ObjectDelete { kind, id } => s.object_delete(kind, &id),
                    ClientMsg::SoundNew { file } => s.sound_new(&file),
                    ClientMsg::SoundSet { index, field, value } => s.sound_set(index, &field, &value),
                    ClientMsg::SoundDelete { index } => s.sound_delete(index),
                    ClientMsg::SoundPlay { file } => vec![s.sound_data(&file)],
                    ClientMsg::MapFind { name } => vec![s.map_find(&name)],
                    ClientMsg::Copy { things } => s.copy(&things),
                    ClientMsg::GroupMove { start, things, origin, dir } => s.group_move(start, &things, origin, dir),
                    ClientMsg::GroupRotate { things, delta } => s.group_rotate(&things, delta),
                    ClientMsg::GroupDelete { things } => s.group_delete(&things),
                    ClientMsg::ConstantsAsk { book } => crate::constants::Book::of(book).map(|b| vec![s.constants_msg(b)]).unwrap_or_default(),
                    ClientMsg::ConstantSet { book, id, value } => crate::constants::Book::of(book).map(|b| s.constant_set(b, &id, &value)).unwrap_or_default(),
                    ClientMsg::Paste { origin, dir } => s.paste(origin, dir),
                    ClientMsg::MapName { kind, index, origin, dir } => vec![s.map_name(&kind, index, origin, dir)],
                    ClientMsg::SoundFiles => vec![ServerMsg::SoundFiles(s.sound_files())],
                    ClientMsg::InfoAsk => vec![s.info_msg()],
                    ClientMsg::InfoSet { field, value } => s.info_set(&field, &value),
                    ClientMsg::ImportList => vec![s.imports_msg()],
                    ClientMsg::ImportAdd { source, path } => s.import_add(&source, &path),
                    ClientMsg::ImportSet { index, path } => s.import_set(index, &path),
                    ClientMsg::ImportDelete { index } => s.import_delete(index),
                    ClientMsg::ImportPreview { index } => vec![s.import_preview(index)],
                    ClientMsg::ThingSetText { file, index, field, value } => s.thing_set_text(file, index, &field, &value),
                    ClientMsg::ThingAsk { file, index } => vec![ServerMsg::Thing(s.thing(file, index))],
                    ClientMsg::ThingSet { file, index, field, value } => s.thing_set(file, index, &field, value),
                    ClientMsg::Text { path, text } => s.text_changed(&path, &text),
                    ClientMsg::MapAsk => vec![ServerMsg::MapSettings(s.map_settings())],
                    ClientMsg::Resize { sides, margins } => s.resize(sides, margins),
                    ClientMsg::Tiles { ids, to, tileset } => s.set_tiles(&ids, &to, tileset),
                    ClientMsg::TilePathing { index, forbids } => s.tile_pathing(index, forbids),
                    ClientMsg::TextClosed { path } => {
                        s.text_closed(&path);
                        Vec::new()
                    }
                    ClientMsg::Hello | ClientMsg::Reload | ClientMsg::Pick { .. } => unreachable!("answered above"),
                };
                // What changed of the pieces with a text form open, to
                // them, before the state (which says who has it unsaved).
                let synced = s.sync_docs();
                let (state, rest): (Vec<_>, Vec<_>) = answers.into_iter().partition(|m| matches!(m, ServerMsg::State(_)));
                for msg in rest.iter().chain(&synced).chain(&state) {
                    write(output, msg)?;
                }
            }
        }
        Ok(())
    }
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
