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
  that only marks its root: its settings apply below it, but it has no
  files until `sources` names them.

```ini
// wc3.bni
[project]
sources=src,lib,maps/*.w3x
exclude=**/old/**,test.j
game="C:/Games/Warcraft III"
game=../ujapi

[jass]
dialect=vjass
```

| section     | key       | value                                    | meaning                                                                 |
|-------------|-----------|------------------------------------------|-------------------------------------------------------------------------|
| `[project]` | `sources` | masks (a plain path too), comma-separated, relative to the project root | everything in the project; none: only `wc3.bni` |
| `[project]` | `exclude` | masks, comma-separated, relative to the project root | what isn't part of the project, even inside `sources` |
| `[project]` | `game`    | one path per line, absolute or relative to the project root | a game folder; repeat for more, each over the ones before |
| `[jass]`    | `dialect` | `jass` or `vjass` (case-insensitive)     | the dialect of `.j` files that don't declare one in a `// @dialect` header |
| `[lint]`    | a rule or group id | `off`, `warning` or `error`     | turn a lint rule (or every rule of a group) off or on, at that level (see [Lint](#lint)) |

- **`dialect`.** A file's own header always wins over the project, so a
  project can still mix dialects. Without either, a file is plain JASS.
  The order is header, then project, then plain JASS.
- **`sources`** is what makes up the project (see the project model
  below), and nothing else does: every value is a **mask**, and a file
  is in the project when a mask matches it or a folder or map it's in.
  A plain path is a mask without wildcards: `src` is everything in
  `src`, `Main.w3x` every script in that map, `.` the whole project
  folder, maps included; `maps/*.w3x` is every map in `maps`,
  `**/*.j` every `.j` outside maps. Without `sources` the project has no
  files but `wc3.bni` (its settings still apply to files opened from
  under it). `./src` and `a/../src` mean `src`. Quote a path that
  contains a comma: `sources="my, folder"`.
- **Masks** (`server/src/glob.rs`): `*` is any part of a name, `?` one
  character, `**` any number of folders (none included). They're
  relative to the project folder unless absolute, case doesn't matter,
  and `/` and `\` both separate folders. A mask matching a folder or a
  map covers everything in it: `**/old` and `**/old/**` both drop every
  `old` folder, and `**/war3map.lua` a file inside every map.
- **`exclude`** drops what its masks match from the project, whether it's
  matched by `sources`: `exclude=test.j,**/old/**`.
  It wins over `sources`. Repeat the line to add more. An excluded file
  that's opened is still read with the project's settings.
- **`game`** is the folder Warcraft III is installed in, and each further
  `game=` line adds a folder on top of it, to replace some of the game's
  files (a custom `Scripts/common.j`, say). A game file (`Scripts\common.j`,
  any case, `/` or `\`) is looked up from the top: within each folder,
  the folder itself first, then its archives in the reverse of the order
  the game opens them (`War3.mpq`, `War3x.mpq`, `War3xLocal.mpq`,
  `War3Patch.mpq`; any case on disk, missing ones skipped); then the
  folder below. So with the manifest above, `Scripts\common.j` comes from
  `../ujapi/Scripts/common.j` if it's there, and `Scripts\Blizzard.j` from
  the game's `War3Patch.mpq`. These, and `common.ai` for AI scripts, are
  part of the project's model (see below), so natives and `Blizzard.j` functions
  resolve: go to definition, hover and references reach them. Each value
  is one path, so quote it if it contains a comma. A folder that doesn't exist is only a warning and the
  setting is kept, so a manifest shared between machines still loads.
- **Settings apply below the manifest.** A file under the project folder
  but outside `sources` isn't loaded as part of the project, but when it's
  opened it's still read with the project's settings.

**Paths in the editor.** In `game=`, `sources=` and `exclude=` values, every folder
along the path is a link (Cmd/Ctrl+click) that reveals that folder: in
the Explorer when it's in the workspace, in the system's file manager
otherwise. Typing a path completes it from the disk: folders for `game`,
folders and files for `sources` and `exclude`, starting from the project
folder for a relative path; picking a folder goes straight on to its
entries. In a mask, the part before the first wildcard links and
completes the same way. Both
come from the server (`server/src/paths.rs`). In every path, `\`
separates parts like `/`.

**Checks.** In the editor, `wc3.bni` gets BNI's own syntax errors and lint
plus checks of its settings, at the exact name or value:

- an unknown section or key (a warning, so a typo like `[jas]` doesn't
  go unnoticed);
- an unknown dialect, or `dialect` with no value or several (an error);
- `dialect` set twice (a warning; the first one is used);
- a `sources` path that doesn't exist, a mask that matches nothing, or
  an empty one (a warning); an empty `exclude` mask (a warning; one that
  matches nothing isn't reported, as there may be nothing to exclude
  yet);
- `game` with no value or several on one line (an error), a `game` path
  that doesn't exist or isn't a folder (a warning);
- an unknown lint rule or group (a warning), or a setting that isn't `off`,
  `warning` or `error` (an error). A rule set twice takes the last value.

**The project model.** `wc3-server` keeps every file of every project in
memory (`server/src/workspace.rs`) and analyzes them all, so the
Problems panel shows the whole project's errors without opening anything.

- **Which projects.** Every `wc3.bni` under the workspace folders, and
  the project of any file opened from elsewhere (even a `wc3.bni` above
  the workspace folders).
- **Which files.** What the project's `sources` match, less what
  `exclude` matches, plus
  `wc3.bni` itself. Only known extensions count (`.j`, `.pld`,
  `.ai`, `.zn`, `.lua`, `.as`, `.angelscript`, `.bni`); hidden folders
  (`.git`, `.idea`, ...) are skipped; a nested project owns its own files.
  `sources` may point outside the project folder (`sources=src,../shared`).
- **Game files.** The game's `Scripts\common.j`, `Scripts\Blizzard.j` and
  `Scripts\common.ai`, looked up through the project's `game=` folders, at the path they're
  found at: on disk (`../ujapi/Scripts/common.j`) or inside an archive
  (`War3Patch.mpq/Scripts/Blizzard.j`, opened read-only: the game's
  archives are never written). They get no diagnostics, being the game's, and a rename that
  would change them is refused. Projects with the same `game=` folders
  share them.
- **One file per game path.** Game paths ignore case, and a file in a
  later `game=` folder replaces the file at the same path below it, so a
  project sees exactly one `common.j`, one `Blizzard.j` and one
  `common.ai`. An empty `Scripts/blizzard.j` in the last folder takes all
  of `Blizzard.j`'s functions away. A game file that projects with
  different `game=` folders share (the stock `Blizzard.j` under both the
  stock and a custom `common.j`) is read with the game files of the
  project the last go to definition, references or hover came from, so
  from it too every name leads to one file: the one that project sees.
- **Visibility.** A file sees the files of its own project and the game
  files the game runs it with: a map script `common.j` and `Blizzard.j`,
  an AI script (`.ai`) `common.j` and `common.ai` (the game loads neither
  `Blizzard.j` for AI nor `common.ai` for maps). An AI script runs on
  its own, so of the project's files it sees only itself, and map
  scripts don't see AI scripts. Names resolve there, references are found there,
  and two projects with different `game=` folders (one with a custom
  `common.j`) don't see each other's declarations. A file outside every
  project sees all files, AI scripts kept apart the same way.
- **Maps.** A map (`.w3x`, `.w3m`) that `sources` matches, or that's in
  a folder it matches (`sources=Main.w3x`, `sources=.`), is
  read like a folder: its scripts are project files at paths inside the
  map's, `Main.w3x/war3map.j`. Those are every file the map's
  `(listfile)` names with a known extension, plus the map script under
  the names the game looks for (`Scripts\war3map.j`, `war3map.j`,
  `war3map.lua`; a `war3map.j` beside `Scripts\war3map.j` is never run, so
  it isn't loaded), which protected maps often leave out of the list,
  and the game's own files the map carries, found by name from the
  game's full file list (`server/src/war3.listfile.txt`). The tree of
  maps also finds the files the editor keeps at the top of a map
  (`war3map.w3i`, `war3mapMisc.txt`, ...) the same way. Maps
  are MPQ archives, read with [`storm-rs`](../storm-rs). A map's own
  `Scripts\Blizzard.j` (maps often carry an optimized one, which the game
  loads instead of its own) replaces the game's `Blizzard.j` for that
  map's scripts; it gets no diagnostics, and no other file sees it. A
  `common.j` in a map isn't treated this way: it's an ordinary file of
  the map. A rename may change them (see Rename below). A map saved
  again is re-read; a map that can't be read adds nothing.
- **Maps as folders** (`server/src/mapfs.rs`). The editor shows a map as
  a folder under the `wc3map:` scheme, every file in it (found as
  above), and the server carries out what's done there: read, write,
  rename, delete, make a folder. A write goes to a copy of the map
  beside it, which replaces the map only once it reads back right: the
  old files' bytes where they were, the new ones equal to what was
  written, none missing. A map whose tables come before its files (some
  protectors do that; `storm-rs` appends after the files) and the
  game's `.mpq`s are read-only. MPQ has no folders, so an empty one made
  in the editor lasts until the server stops. Writing one file of a map
  doesn't make the editor think its other files changed on disk.
- **Text.** An open file uses the editor's text; any other file its text
  on disk, read as UTF-8 with invalid bytes replaced (old maps often use a
  legacy code page). Closing a project file goes back to its disk text.
- **Disk.** The server watches the workspace folders and every project's
  folders, sources and `game=` folders itself. A changed, created or
  deleted source file is re-read and re-analyzed (unless it's open), and
  so is a changed map. In a game folder, a change to one of its archives
  (`War3.mpq`, `War3x.mpq`, `War3xLocal.mpq`, `War3Patch.mpq`) or to its
  `Scripts/common.j`, `Blizzard.j` or `common.ai` reads the game files
  again, so a new or deleted override takes effect at once; anything else
  there (its `Maps`, say) is ignored. A changed `wc3.bni` reloads the
  projects and re-analyzes everything, since its settings and `sources`
  apply to all files.
- **Cost.** Files are analyzed in parallel: 25 MB of JASS (60 copies of
  `Blizzard.j`) loads in about 0.2 s.

## Lint

Every lint diagnostic names its **rule**, shown as the diagnostic's code
in the Problems panel. A rule has one id in every language that checks
the same thing, so one setting covers all of them. A project switches
rules in `wc3.bni`:

```ini
[lint]
infinite-loop=warning     // on, as a warning (it's off by default)
unused-local=off          // off
empty-branch=error        // on, raised to an error
```

Without a setting, a rule is on at its own level (the level it reports
in each language) unless the table says it's off by default.

A **group** switches several rules at once: `pure=error` sets every rule
of the group `pure`. A rule set by name wins over its group, wherever
the lines are, so `pure=error` with `implicit-signature=off` keeps that
one rule off.

| group  | rules                | for |
|--------|----------------------|-----|
| `pure` | `implicit-signature`, `declaration-after-function`, `global-outside-globals` | pure JASS as the game reads it: reports what this toolchain accepts beyond the game's grammar. Off by default |
 Parse errors
aren't lint: they can't be turned off. The rules are defined in
`parser_rs::RULES` (`parser-rs/src/diagnostic.rs`); this table follows it.

| rule                             | languages                    | default | finds |
|----------------------------------|------------------------------|---------|-------|
| `statement-outside-function`     | JASS, Zinc                   | on      | a statement at the top level, where only declarations belong |
| `modifier-outside-library`       | Zinc                         | on      | `private` / `public` outside a library, where it has no effect |
| `duplicate-declaration`          | JASS, Zinc, AngelScript      | on      | a name declared twice in the same scope |
| `unused-local`                   | JASS, Zinc, Lua, AngelScript | on      | a local variable (or Lua local function) that's never used |
| `shadowed-variable`              | AngelScript                  | on      | a local that hides a variable of an outer scope |
| `missing-return`                 | JASS, Zinc, AngelScript      | on      | a function (or `get` accessor) with a return type that may not return a value |
| `infinite-loop`                  | JASS, Zinc, Lua, AngelScript | **off** | a loop with no way out: a JASS `loop` without `exitwhen`, a `while (true)` without `break` or `return` |
| `empty-branch`                   | JASS, Zinc, Lua, AngelScript | on      | an empty `then` / `else` / `if` body |
| `break-outside-loop`             | Zinc, Lua, AngelScript       | on      | `break` outside a loop (or switch) |
| `continue-outside-loop`          | AngelScript                  | on      | `continue` outside a loop |
| `duplicate-label`                | Lua                          | on      | a label defined twice in the same block |
| `undefined-label`                | Lua                          | on      | a `goto` with no visible label |
| `vararg-outside-vararg-function` | Lua                          | on      | `...` in a function that doesn't take it |
| `item-outside-section`           | BNI                          | on      | a `key=value` before the first `[section]` |
| `empty-section-name`             | BNI                          | on      | a `[]` section header |
| `duplicate-section`              | BNI                          | on      | a section header repeated in the file |
| `unclosed-section-header`        | BNI                          | on      | a section header missing its `]` |
| `unterminated-string`            | BNI                          | on      | a quoted string with no closing `"` |
| `implicit-signature`             | JASS                         | **off** | a `function` or `native` without `takes` or `returns`, which the game requires (group `pure`) |
| `declaration-after-function`     | JASS                         | **off** | a global, `globals` block, `native` or `type` after a function; the game reads them only before the first one (group `pure`) |
| `global-outside-globals`         | JASS                         | **off** | a variable at the top level outside `globals ... endglobals`, where the game doesn't read it (group `pure`) |

**Why `infinite-loop` is off.** A thread that loops forever and sleeps is
normal Warcraft III code: the game's own AI scripts are full of them (161
such loops in its `Scripts` folder, e.g. `PlayGame` ends in
`SleepForever()`). Turn the rule on to find such loops, say, ones that
forgot to sleep.

## Symbol index

Every file of the project model carries its **symbols**: each name it
declares or references, with its kind and scope (`parser_rs::Occurrence`).
The language crate records them in the same pass that produces the
highlighting, so they agree, and half-typed code is indexed too. Every
language produces them: JASS, Zinc (also inside vJASS `//! zinc` blocks),
Lua and AngelScript. BNI is data and has none.

**Resolution.** By exact (case-sensitive) name, within the name's scope:

- **Parameters and locals**: their own function, in their own file. In
  Lua every `local` is a binding of its own, following Lua's scoping, so
  a shadowing `local x = x + 1` is two symbols.
- **Zinc library members** are private by default: their own library.
  `public` ones are project-wide, so JASS code can call a public Zinc
  function, but not a private one.
- **Struct and class members** (Zinc, AngelScript) and **table fields**
  (Lua): a separate namespace matched across the project, since `obj.x`
  can't be tied to its struct, class or table without type inference.
  Every `x` field and every `.x` are one symbol. In AngelScript, a bare
  name inside a class that names one of its members is that member
  (implicit `this`).
- **AngelScript namespaces.** A name declared in `namespace A { namespace
  B { ... } }` lives in `A::B`, and a namespace may be reopened in any
  file. A use is looked up as AngelScript does: what's written (`C`,
  `X::C`) from the namespace it's used in, then each enclosing one, out to
  the global namespace, and the first that declares it wins; `::X::C`
  means exactly `X::C`. `Color::Red` and `Red` are the same enum value. A
  name declared nowhere (a native, say) is in the global namespace, which
  is where every other language's names live.
- **Everything else** (JASS functions, natives, globals, constants and
  types; Zinc libraries and anything outside a library; Lua globals;
  AngelScript's namespace members, as above) is project-wide, across
  languages: every file the project sees (its own files and its game
  files, see [Visibility](#projects)) means the same symbol. So with
  `game=` set, go to definition on `CreateTimer`, from JASS, Zinc or Lua,
  lands on its `native` in the game's `common.j`.

**What the editor gets** (`server/src/index.rs`), all asked with one
`query` message and answered after every earlier change is analyzed:

| feature                          | answer                                                       |
|----------------------------------|--------------------------------------------------------------|
| go to definition (F12)           | the symbol's declarations, in any file                       |
| find references (Shift+F12)      | every occurrence, declarations included (unless VS Code asks otherwise) |
| hover                            | the declaration lines, with file, line and namespace; or "not declared in the project" |
| outline (document symbols)       | the file's declarations, except parameters and locals        |
| go to symbol in workspace (Ctrl+T) | declarations (except parameters and locals) whose name contains the text (case-insensitive) |
| rename (F2)                      | every occurrence, in every project file (open or not), or why it can't be renamed |

**Rename** (`server/src/rename.rs`) replaces exactly what find references
finds. It's refused, with the reason shown in the editor, when it can't
be done safely:

- the symbol isn't declared in the project (`print`, or a native whose
  `common.j` isn't part of it), or it's a JASS `native`, the game's API;
- it's a field or method declared in more than one place: members are
  matched by name, so renaming one would rename the others;
- the new name isn't a plain name in **every** language the symbol is used
  in: each language's own lexer decides (a keyword doesn't lex as a name;
  JASS is checked as vJASS), JASS and Zinc names start with a letter, and
  JASS names don't end with `_`;
- the new name is already declared in the same scope (for a project-wide
  name, the same namespace);
- it's used in the game's scripts, or in a map that can't be written (see
  maps as folders above).

When some of the places are inside maps, the editor asks first: renaming
there rewrites the map. **Rename** changes them all, **Rename Outside
Maps** only the files on disk, **Cancel** nothing.

An AngelScript class is renamed together with its constructors and
destructor; F2 on a constructor renames the class.

Nothing is cached beyond the symbols themselves: a query scans what it
needs. On the game's own `Scripts` folder (293 files: `common.j`,
`Blizzard.j`, the AI and campaign scripts) loading takes about 80 ms and a
query about 1 ms.
