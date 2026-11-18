# WC3

Warcraft III language support for VS Code.

## Features

- JASS syntax highlighting (`.j`, `.pld`, `.ai`): comments, string/rawcode
  literals, bracket matching, indent-on-keyword rules for
  `function`/`if`/`loop`/`globals` blocks.
- Zinc syntax highlighting (`.zn`): keywords, declarations, line/block
  comments, literals, operators, brace-based indentation.
- Semantic highlighting for both languages, using VS Code's standard
  token types and modifiers: functions and natives (`defaultLibrary`),
  parameters, locals, globals and constants (`readonly`), types and
  primitives, libraries (`namespace`), structs, interfaces, methods,
  fields (`static`), assignments (`modification`), plus keywords,
  literals, operators and comments. It works on half-typed code too,
  because classification runs on an error-tolerant token stream rather
  than the AST.
- Live diagnostics for both languages: parse errors and lint warnings
  shown in the Problems panel as you type —
  - JASS via [`jass-rs`](../../jass-rs): duplicate declarations, unused
    locals, `loop` without `exitwhen`, missing `return`, ...
  - Zinc via [`zinc-rs`](../../zinc-rs): duplicate libraries/members/
    locals, unused locals, `while (true)` without `break`/`return`,
    `break` outside a loop, empty `if`/`else`, missing `return`, ...

## Architecture

- `../server` — a small Rust binary (`wc3-server`) that wraps `jass-rs`
  and `zinc-rs` (both built on the shared [`parser-rs`](../../parser-rs),
  so the server converts their errors/diagnostics with one code path). The
  extension sends each document's `languageId` (`jass`/`zinc`) along with
  its text, and the server picks the parser by it.
  It's spawned as a subprocess and speaks plain HTTP on
  `127.0.0.1:<random port>`, guarded by a `?token=` printed (along with
  the port) as its first line of stdout. It exits when its stdin closes,
  i.e. when the extension process that spawned it dies.
- `serverClient.js` / `httpTransport.js` implement that transport. They're
  unmodified-in-spirit copies from the older `JASS-Tree-sitter-Rust`
  extension — none of that code is JASS-specific, so there was nothing to
  rewrite (one pre-existing bug — an uncleared startup timeout that could
  throw after `stop()` — was fixed here).
- `extension.js` is new: it only wires up `/document/update` → Problems
  panel. None of the older extension's other features (hover, completion,
  map editor, MPQ browsing, ...) have a counterpart here yet.

This is otherwise a fresh extension — it does not share JASS-parsing code
with the older `JASS-Tree-sitter-Rust` extension (only the TextMate
grammar, language configuration, and file icon were carried over from it,
verbatim).

## Running it locally

```sh
# 1. Build the backend (quick, host-only, for iterating)
cd ../server && cargo build

# 2. Open extension/ in VS Code and press F5
#    (Run and Debug → "Extension Development Host")
```

`serverBinaryPath()` in `extension.js` looks for the backend in this order:
1. `./bin/wc3-server-{macos,linux,windows.exe}` — the packaged,
   cross-compiled binary produced by `server/build.sh` / `server/build-mac.sh`
   (same naming as the older `JASS-Tree-sitter-Rust` extension's own
   `bin/`). It lives inside `extension/`, not `server/`, because `vsce`
   only packages files under the folder that holds `package.json`.
2. `../server/target/release/wc3-server`
3. `../server/target/debug/wc3-server` — the plain `cargo build` output;
   what you get from step 1 above.

### Producing a packaged binary

One-time environment setup — installs the cross-compilation toolchain
(rustup targets/components, `jq`, `mingw-w64`, `musl-cross`):

```sh
cd ../server && ./build-conf.sh
```

Then, either:

- `./build.sh` — ported from the older extension's own `build.sh`.
  Cross-compiles a universal macOS binary plus Linux (musl) and Windows
  (gnu) builds into `extension/bin/`. See `server/.cargo/config.toml` for
  the linkers it expects.
- `./build-mac.sh` — quick host-only debug build (current Mac target
  only) into `extension/bin/`, for local iteration without the full
  cross-compile.

```sh
cd ../server && ./build.sh       # full cross-compile
cd ../server && ./build-mac.sh   # quick local mac build
```
