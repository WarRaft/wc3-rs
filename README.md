# wc3-rs
The ultimate high-performance Rust ecosystem for Warcraft III map development, modding, and asset parsing.

## Projects

A folder with a **`wc3.bni`** file in it is a **project**, the way a folder
with `Cargo.toml` is a Rust package. Every file in that folder and below
belongs to the project and is read with its settings.

- **Finding the project.** For each file, the nearest `wc3.bni` in its
  folder or any folder above it wins. So a nested `wc3.bni` starts its own
  project, and a file with no `wc3.bni` above it has no project and uses
  the defaults.
- **The format** is BNI (see [`bni-rs`](../bni-rs/README.md)), so `//`
  comments, quoted values and comma-separated lists work as in any other
  BNI file. Section and key names are case-sensitive.
- **Every setting is optional.** An empty `wc3.bni` is a valid project
  that only marks its root.

```ini
// wc3.bni
[project]
sources=src,lib

[jass]
dialect=vjass
```

| section     | key       | value                                    | meaning                                                                 |
|-------------|-----------|------------------------------------------|-------------------------------------------------------------------------|
| `[project]` | `sources` | paths, comma-separated, relative to the project root | the project's source folders and files                     |
| `[jass]`    | `dialect` | `jass` or `vjass` (case-insensitive)     | the dialect of `.j` files that don't declare one in a `// @dialect` header |

- **`dialect`.** A file's own header always wins over the project, so a
  project can still mix dialects. Without either, a file is plain JASS.
  The order is header, then project, then plain JASS.
- **`sources`** is read and checked, but not used yet: it's where
  cross-file analysis will look for the project's code. Quote a path that
  contains a comma: `sources="my, folder"`.

**Checks.** In the editor, `wc3.bni` gets BNI's own syntax errors and lint
plus checks of its settings, at the exact name or value:

- an unknown section or key (a warning, so a typo like `[jas]` doesn't
  go unnoticed);
- an unknown dialect, or `dialect` with no value or several (an error);
- `dialect` set twice (a warning; the first one is used);
- a `sources` path that doesn't exist, or an empty one (a warning).

**How the editor uses it.** The extension sends each document's path with
its requests, and `wc3-server` finds and reads the manifest
(`server/src/project.rs`). Settings come from the **saved** `wc3.bni`: the
server re-reads it when its modification time or size changes, and the
extension re-analyzes and re-highlights every open document when a
`wc3.bni` in the workspace is saved, created or deleted. A `wc3.bni` above
the workspace folders is still found, but changes to it are only picked up
on the next edit of a document.
