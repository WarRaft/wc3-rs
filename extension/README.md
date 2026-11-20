# WC3

Warcraft III language support for VS Code.

## Features

- JASS syntax highlighting (`.j`, `.pld`, `.ai`): comments, string/rawcode
  literals, bracket matching, indent-on-keyword rules for
  `function`/`if`/`loop`/`globals` blocks.
- Zinc syntax highlighting (`.zn`): keywords, declarations, line/block
  comments, literals, operators, brace-based indentation.
- Lua (`.lua`, the Warcraft III Reforged script language; VS Code's
  built-in Lua grammar plus our icon) and AngelScript (`.as`, own grammar):
  semantic highlighting and diagnostics from [`lua-rs`](https://github.com/WarRaft/lua-rs) /
  [`as-rs`](https://github.com/WarRaft/as-rs).
- Semantic highlighting for all four languages, using VS Code's standard
  token types and modifiers: functions and natives (`defaultLibrary`),
  parameters, locals, globals and constants (`readonly`), types and
  primitives, libraries (`namespace`), structs, interfaces, methods,
  fields (`static`), assignments (`modification`), plus keywords,
  literals, operators and comments. It works on half-typed code too,
  because classification runs on an error-tolerant token stream rather
  than the AST.
- Per-file dialects. A `.j` file is plain JASS unless its header says
  `// @dialect vjass`. A vJASS file gets vJASS keywords, `/* */`
  comments, and `//! zinc` … `//! endzinc` blocks, which are parsed,
  linted and highlighted as Zinc with file-accurate positions. One project
  can mix both.
- BNI (`.bni`, including `wc3.bni`) via [`bni-rs`](https://github.com/WarRaft/bni-rs):
  semantic highlighting, syntax errors and lint.
- Projects: a `wc3.bni` file marks a project root and holds its settings,
  such as the default JASS dialect. Every file of the project is analyzed,
  open or not, so its problems are listed right away (see the repository
  README).
- `wc3.bni` paths: `game=`, `sources=` and `exclude=` complete from the disk as you
  type, and each folder along a path is a link that reveals it.
- Maps: the scripts inside `.w3x` / `.w3m` maps a project's `sources=` matches are project
  files too (`war3map.j`, `war3map.lua`, ...), analyzed and searchable
  like any other; they open read-only.
- WC3 Maps, a tree in the Explorer: every `.w3x` / `.w3m` in the
  workspace opens like a folder into the files inside it. Text files
  (scripts, `.fdf`, `.txt`, `.slk`, ...) open read-only; binary ones are
  marked as such. It follows maps being saved, added and removed.
- Symbol navigation across the whole project, in every language: go to
  definition, find references, hover with the declaration, outline, go
  to symbol in workspace, and rename (F2), which refuses with a reason
  when the rename wouldn't be safe. Parameters and locals resolve within their
  function; everything else across all project files.
- Live diagnostics: parse errors and lint warnings
  shown in the Problems panel as you type —
  - JASS via [`jass-rs`](https://github.com/WarRaft/jass-rs): duplicate declarations, unused
    locals, `loop` without `exitwhen`, missing `return`, ...
  - Zinc via [`zinc-rs`](https://github.com/WarRaft/zinc-rs): duplicate libraries/members/
    locals, unused locals, `while (true)` without `break`/`return`,
    `break` outside a loop, empty `if`/`else`, missing `return`, ...

## Architecture

All language logic lives in Rust; the extension is a thin shell with no
dependencies besides the VS Code API.

- `../server`: the `wc3-server` binary. It wraps `jass-rs`, `zinc-rs`,
  `lua-rs`, `as-rs` and `bni-rs`, all built on the shared
  [`parser-rs`](https://github.com/WarRaft/parser-rs), so one code path
  turns their errors and lint into diagnostics.
- **Protocol.** stdio, binary, the same framing both ways:
  `u32 body length | u8 kind | body` (little-endian fields, length-prefixed
  UTF-8 strings). It's specified in `server/src/protocol.rs`;
  `protocol.js` is its JavaScript twin. Unknown kinds are skipped, so
  either side can add messages.
- **The editor only reports**: workspace folders (`init`), and what
  happens to documents: `open`, `edit` (incremental changes), `lang`
  (language changed, text kept), `close`. It never asks for anything.
- **The server keeps every project file in memory**, open or not, and
  answers each change with one `file` message per affected file:
  everything it knows about it at that version (diagnostics, plus semantic
  tokens for open documents). Project files that were never opened are
  reported too, so their problems show up right away. A burst of edits
  is analyzed once. It sends the token legend first, and watches the disk
  itself (`notify`): source files and `wc3.bni`.
- **Ids.** A document is identified by its path on disk (`fsPath`) when
  it's a file, so it matches the files the server finds itself, and by its
  URI otherwise.
- **Queries.** Only what depends on the cursor is asked (`query` →
  `answer`): definition, references, hover, document and workspace
  symbols, rename, answered from the server's symbol index after every earlier
  change is analyzed.
- `extension.js` applies diagnostics as they arrive and serves
  highlighting from the last tokens received. If VS Code asks for a
  version the server hasn't answered yet, it waits for that answer (up to
  a second). The server exits when its stdin closes.

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
