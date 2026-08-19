# WC3

Warcraft III language support for VS Code.

## Features

- JASS syntax highlighting (`.j`, `.pld`, `.ai`): comments, string/rawcode
  literals, bracket matching, indent-on-keyword rules for
  `function`/`if`/`loop`/`globals` blocks.
- Live diagnostics: parse errors and lint warnings (duplicate
  declarations, unused locals, `loop` without `exitwhen`, missing
  `return`, ...) from [`jass-rs`](../../jass-rs), shown in the Problems
  panel as you type.

## Architecture

- `../server` — a small Rust binary (`wc3-server`) that wraps `jass-rs`.
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
# 1. Build the backend
cd ../server && cargo build

# 2. Open extension/ in VS Code and press F5
#    (Run and Debug → "Extension Development Host")
```

The extension looks for the backend at `../server/target/debug/wc3-server`
relative to itself — there's no packaged/prebuilt binary yet.
