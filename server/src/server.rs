//! The server loop. The editor only reports what happens to documents
//! (opened, edited, language changed, closed) and its workspace folders;
//! the project model (`workspace`) does the rest, the loop feeds it these
//! reports and the disk watcher's events, and sends back one `File`
//! message per changed file (see `protocol`).
//!
//! Synchronous on purpose: the work is CPU-bound parsing (a 440 KB
//! `Blizzard.j` takes ~15 ms), so one thread fed by a channel is enough.

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use crate::document::{Language, Position};
use crate::index::{self, QueryKind};
use crate::project;
use crate::protocol::{read_frame, ClientMsg, ServerMsg};
use crate::workspace::Workspace;

/// Everything the loop reacts to.
#[derive(Debug)]
pub enum Event {
    Client(ClientMsg),
    /// Files changed on disk (from the watcher).
    Disk(Vec<PathBuf>),
    /// The editor is gone: input closed or unreadable.
    Exit,
}

/// Turns the editor's input into events until it ends, then sends
/// `Exit`. A malformed or unknown message is skipped, never fatal.
pub fn read_events(mut input: impl Read, events: &Sender<Event>) {
    loop {
        match read_frame(&mut input) {
            Ok(Some((kind, body))) => match ClientMsg::decode(kind, &body) {
                Ok(Some(msg)) => {
                    if events.send(Event::Client(msg)).is_err() {
                        return;
                    }
                }
                Ok(None) => {}
                Err(err) => eprintln!("wc3-server: {err}"),
            },
            Ok(None) => break,
            Err(err) => {
                eprintln!("wc3-server: {err}");
                break;
            }
        }
    }
    let _ = events.send(Event::Exit);
}

pub struct Server<W: Write> {
    out: W,
    workspace: Workspace,
    /// Where the disk watcher sends its events; `None` disables watching
    /// (tests inject `Event::Disk` themselves).
    watch_events: Option<Sender<Event>>,
    watcher: Option<notify::RecommendedWatcher>,
    watched: HashSet<PathBuf>,
    /// Queries waiting for the changes before them to be analyzed.
    queries: Vec<Query>,
}

struct Query {
    request: u32,
    kind: Option<QueryKind>,
    id: String,
    pos: Position,
    text: String,
}

impl<W: Write> Server<W> {
    pub fn new(out: W, watch_events: Option<Sender<Event>>) -> Server<W> {
        Server {
            out,
            workspace: Workspace::default(),
            watch_events,
            watcher: None,
            watched: HashSet::new(),
            queries: Vec::new(),
        }
    }

    /// Sends the legend, then handles every event that has arrived and
    /// answers once. While the user types faster than we analyze, a burst
    /// of edits costs one analysis instead of one per keystroke. Ends on
    /// `Exit` or when the editor stops reading our output.
    pub fn run(mut self, events: Receiver<Event>) -> io::Result<()> {
        let (types, modifiers) = parser_rs::semantic::legend();
        let legend = ServerMsg::Legend {
            types: types.into_iter().map(String::from).collect(),
            modifiers: modifiers.into_iter().map(String::from).collect(),
        };
        self.out.write_all(&legend.encode())?;
        self.out.flush()?;

        while let Ok(event) = events.recv() {
            if !self.handle(event) {
                break;
            }
            while let Ok(event) = events.try_recv() {
                if !self.handle(event) {
                    return self.flush();
                }
            }
            self.flush()?;
        }
        self.flush()
    }

    /// `false` on `Exit`.
    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Client(msg) => self.on_client(msg),
            Event::Disk(paths) => self.workspace.disk_changed(&paths),
            Event::Exit => return false,
        }
        // Loading a project may have brought folders to watch.
        self.update_watch();
        true
    }

    fn on_client(&mut self, msg: ClientMsg) {
        let ws = &mut self.workspace;
        match msg {
            ClientMsg::Init { roots } => ws.set_roots(roots.into_iter().map(PathBuf::from).collect()),
            ClientMsg::Open { id, path, lang, version, text } => {
                if let Some(language) = Language::from_id(&lang) {
                    let path = (!path.is_empty()).then(|| PathBuf::from(path));
                    ws.open(id, path, language, version, text);
                }
            }
            ClientMsg::Edit { id, version, changes } => ws.edit(&id, version, &changes),
            ClientMsg::Lang { id, lang } => ws.set_language(&id, Language::from_id(&lang)),
            ClientMsg::Close { id } => ws.close(&id),
            ClientMsg::Query { request, kind, id, pos, text } => {
                self.queries.push(Query { request, kind, id, pos, text })
            }
        }
    }

    /// Watches every folder the model cares about (see
    /// `Workspace::watch_paths`) that isn't watched yet. Only manifests
    /// and source files are passed on, so a build writing thousands of
    /// other files doesn't wake the loop.
    fn update_watch(&mut self) {
        use notify::Watcher;
        let Some(events) = self.watch_events.clone() else {
            return;
        };
        let wanted: Vec<PathBuf> = self
            .workspace
            .watch_paths()
            .into_iter()
            .filter(|p| !self.watched.iter().any(|w| p.starts_with(w)))
            .collect();
        if wanted.is_empty() {
            return;
        }
        if self.watcher.is_none() {
            let handler = move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else { return };
                let paths: Vec<PathBuf> = event
                    .paths
                    .into_iter()
                    .filter(|p| project::is_manifest(p) || Language::from_path(p).is_some())
                    .collect();
                if !paths.is_empty() {
                    let _ = events.send(Event::Disk(paths));
                }
            };
            match notify::recommended_watcher(handler) {
                Ok(w) => self.watcher = Some(w),
                Err(err) => {
                    eprintln!("wc3-server: can't watch files: {err}");
                    self.watch_events = None;
                    return;
                }
            }
        }
        let watcher = self.watcher.as_mut().expect("created above");
        for path in wanted {
            match watcher.watch(&path, notify::RecursiveMode::Recursive) {
                Ok(()) => {
                    self.watched.insert(path);
                }
                Err(err) => eprintln!("wc3-server: can't watch {}: {err}", path.display()),
            }
        }
    }

    /// Sends the reports of everything changed, then answers the waiting
    /// queries against that up-to-date model.
    fn flush(&mut self) -> io::Result<()> {
        if self.workspace.is_idle() && self.queries.is_empty() {
            return Ok(());
        }
        for msg in self.workspace.take_reports() {
            self.out.write_all(&msg.encode())?;
        }
        for q in std::mem::take(&mut self.queries) {
            let answer = match q.kind {
                Some(kind) => index::answer(&self.workspace, kind, &q.id, q.pos, &q.text),
                None => index::Answer::default(),
            };
            let msg = ServerMsg::Answer { request: q.request, text: answer.text, locations: answer.locations };
            self.out.write_all(&msg.encode())?;
        }
        self.out.flush()
    }
}

#[cfg(test)]
#[path = "server.test.rs"]
mod tests;
