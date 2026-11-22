//! The `wc3-server` binary: the language backend alone (the extension
//! ships `wc3`, whose `wc3 server` is the same: `server::run_stdio`).

fn main() {
    wc3_server::server::run_stdio();
}
