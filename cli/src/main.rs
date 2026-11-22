//! `wc3`: the one binary the extension ships for each system, a mode a
//! run:
//!
//! - `wc3 server`: the language backend the extension spawns
//!   (`wc3_server::server::run_stdio`);
//! - `wc3 mcp`: the MCP server for AI assistants (`wc3_mcp::run`).
//!
//! Both talk over stdin and stdout and end when stdin closes.

const USAGE: &str = "usage: wc3 <mode>\n\n  server   the language backend of the VS Code extension (stdio)\n  mcp      the MCP server for AI assistants (stdio)\n  version  the version\n";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "server" => wc3_server::server::run_stdio(),
        "mcp" => wc3_mcp::run(),
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
