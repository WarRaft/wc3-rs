# WC3

Warcraft III language support for VS Code.

## Features

- JASS syntax highlighting (`.j`, `.pld`, `.ai`; `.vj` is vJASS, with its
  own icon): comments, string/rawcode
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
- BNI (`.bni`, including `wc3.bni`; the game's BNI `.txt` files by name,
  `HumanUnitFunc.txt`, `WorldEditStrings.txt`, `TriggerData.txt` ...; a
  map's `war3map*.txt`) via [`bni-rs`](https://github.com/WarRaft/bni-rs):
  semantic highlighting, syntax errors and lint.
- Projects: a `wc3.bni` file marks a project root and holds its settings,
  such as the default JASS dialect. Every file of the project is analyzed,
  open or not, so its problems are listed right away (see the repository
  README).
- `wc3.bni` paths: `game=`, `sources=`, `exclude=` and `root=` complete from the disk as you
  type, and each folder along a path is a link that reveals it.
- Maps: the scripts inside a `.w3x` / `.w3m` that's a project's `[map] root=` (or that its `[code] sources=` matches) are project
  files too (`war3map.j`, `war3map.lua`, ...), analyzed and searchable
  like any other.
- Maps as folders: **Open Map as Folder** (in the Explorer's context
  menu of a `.w3x` / `.w3m`, on the page a map file opens in, or from the
  WC3 Maps list) adds the map to the workspace as a folder. Its files are
  then listed, opened, edited and saved, copied, dragged in from disk,
  renamed and deleted like any others, written into the map itself. A
  `war3map.j` the game doesn't run (there's a `Scripts\war3map.j`) is
  dimmed. The game's `.mpq`s and maps whose layout can't be written
  (some protected ones) are read-only. Search (Ctrl+Shift+F, Ctrl+P)
  doesn't look inside maps: VS Code only offers that to its own file
  systems.
- Textures: a `.blp` (on disk or inside a map) opens in a viewer that
  decodes it with [`blp-rs`](https://github.com/WarRaft/blp-rs): every
  kind Warcraft III and World of Warcraft use (JPEG, palette, DXT1/3/5,
  raw). Pick a mip; show RGBA, RGB or the alpha alone, on a checkerboard,
  black or white; zoom (Fit, 1:1, +/−, Ctrl/Cmd + wheel, keys `0` `1`
  `+` `-`, `[` `]` for the mips), read the pixel under the cursor, save
  the mip as PNG. It reloads when the file changes. A `.tga` (map
  previews, loading screens) opens there too, and a pathing map
  (`.wpm`) as a picture.
- File icons: every kind of Warcraft III file has its own, in one style
  (`icons/`, see `icons/README.md`): scripts, data tables, object data,
  a map's parts, maps and campaigns, images, models, sounds, replays.
- Object data: a `.w3a`, `.w3b`, `.w3d`, `.w3h`, `.w3q`, `.w3t` or
  `.w3u` (on disk or inside a map) opens as text, in BNI, with
  highlighting and live errors, and saving writes the file back. Every
  file goes to text and back to the same data (but the `end` ids, which
  the game doesn't read and saving writes as 0). With the project's
  `game=` folders, each field gets its World Editor name and type as an
  inlay hint (shown, not part of the text; asked for the lines in view
  only), and the fields are checked against the game's metadata as you
  type (types, limits, unknown fields, levels, whose field it is), with
  quick fixes. Hovering over a field shows its name, type, limits, levels,
  data column and whose field it is.
- String tables (`.wts`, `war3map.wts`): highlighting of headers, numbers,
  comments, text and color codes (`|cffff0000`, `|r`, `|n`), with live
  errors. Hovering over `TRIGSTR_<n>` in any file of a map shows the
  string's text, in its colors, and what uses it.
- SLK tables (`.slk`): highlighted text with live errors, or, with **Open
  as Table** (title bar, Explorer context menu), a table to edit: frozen
  header row and column A, find, keyboard editing, copy and paste. **Open
  as Text** goes back. Both are the same document.
- Symbol navigation across the whole project, in every language: go to
  definition, find references, hover with the declaration, outline, go
  to symbol in workspace, and rename (F2), which refuses with a reason
  when the rename wouldn't be safe, and asks before changing files
  inside maps (Rename, Rename Outside Maps, Cancel). Parameters and locals resolve within their
  function; everything else across all project files.
- Live diagnostics: parse errors and lint warnings
  shown in the Problems panel as you type, with quick fixes where a rule
  offers one (Remove empty 'else') —
  - JASS via [`jass-rs`](https://github.com/WarRaft/jass-rs): duplicate declarations, unused
    locals, `loop` without `exitwhen`, missing `return`, ...
  - Zinc via [`zinc-rs`](https://github.com/WarRaft/zinc-rs): duplicate libraries/members/
    locals, unused locals, `while (true)` without `break`/`return`,
    `break` outside a loop, empty `if`/`else`, missing `return`, ...

## Architecture

All language logic lives in Rust; the extension is a thin shell with no
dependencies besides the VS Code API.

- `../server`: the `wc3-server` binary, over a library (`wc3_server`) that
  other tools such as `wc3-mcp` use too. It wraps `jass-rs`, `zinc-rs`,
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
- **Textures.** The viewer (`webview/texture.js`) reads the file through
  VS Code (so a map's file system works too) and sends its bytes in a
  `texture` message; the server decodes it with `blp-rs` (decoding only,
  no C code, so cross-compiling is unaffected) and answers with the
  header and every mip as RGBA.
- **Object data.** A `.w3a` ... file opens through the `wc3obj:` file
  system, at the file's path: reading it reads the file (through VS Code,
  so inside maps too) and has the server convert it to text (`objects`
  message), and writing it has the server convert the text back, then
  writes the file. The text's language is the file's (`w3a` ...), which
  the server reads as BNI; the text is opened with the file's path, so
  the server reads it with the file's project and its game.
- **SLK tables.** The table is a custom *text* editor (`wc3.slkTable`,
  priority `option`, so the text opens by default): the document stays the
  text. The page (`webview/slk.js`) draws the grid on a canvas, only what's
  in view; the sheet comes from the server (`QUERY.SLK_TABLE`), and a
  written cell goes back as text edits (`QUERY.SLK_SET_CELL`) applied as a
  workspace edit, after which the page gets the new sheet.
- **Restored tabs.** `wc3obj:` and `wc3map:` exist only while the
  extension runs, so it activates on them (`onFileSystem:wc3obj`,
  `onFileSystem:wc3map`): a tab of either that VS Code restores after a
  reload opens instead of failing with "no file system provider".
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
