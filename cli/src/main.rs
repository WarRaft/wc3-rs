//! `wc3`: the one binary the extension ships for each system, a mode a
//! run:
//!
//! - `wc3 server`: the language backend the extension spawns
//!   (`wc3_server::server::run_stdio`);
//! - `wc3 mcp`: the MCP server for AI assistants (`wc3_mcp::run`);
//! - `wc3 build [name] [--project <path>]`: builds the map of the project
//!   of the current folder (or of `path`), its `[build]` `name` or its
//!   first (`wc3_server::build`).
//!
//! Both talk over stdin and stdout and end when stdin closes.

const USAGE: &str = "usage: wc3 <mode>\n\n  server   the language backend of the VS Code extension (stdio)\n  mcp      the MCP server for AI assistants (stdio)\n  build    builds the project's map: wc3 build [name] [--project <path>]\n  version  the version\n";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "server" => wc3_server::server::run_stdio(),
        "mcp" => wc3_mcp::run(),
        "build" => build(std::env::args().skip(2).collect()),
        "version" | "--version" | "-V" => println!("wc3 {}", env!("CARGO_PKG_VERSION")),
        "help" | "--help" | "-h" => print!("{USAGE}"),
        other => {
            if !other.is_empty() {
                eprintln!("wc3: no mode {other:?}");
            }
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    }
}

/// `wc3 build [name] [--project <path>]`.
fn build(args: Vec<String>) {
    let mut name = None;
    let mut project = std::path::PathBuf::from(".");
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--project" | "-p" => project = it.next().map(Into::into).unwrap_or(project),
            _ => name = Some(a),
        }
    }
    let start = std::time::Instant::now();
    match wc3_server::build::build_at(&project, name.as_deref()) {
        Ok(b) => {
            for n in &b.notes {
                eprintln!("note: {n}");
            }
            let base = b.base.map_or(String::new(), |p| format!(", a copy of {}", p.display()));
            let name = if b.name.is_empty() { "default" } else { b.name.as_str() };
            println!("{name}: {} ({} files{base}; triggers: {}; {} {} lines) in {:.1?}", b.out.display(), b.files, b.triggers, b.script, b.script_lines, start.elapsed());
        }
        Err(errors) => {
            for e in errors.iter().take(50) {
                eprintln!("error: {e}");
            }
            if errors.len() > 50 {
                eprintln!("... and {} more", errors.len() - 50);
            }
            std::process::exit(1);
        }
    }
}
