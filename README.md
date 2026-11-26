# wc3-rs
The ultimate high-performance Rust ecosystem for Warcraft III map development, modding, and asset parsing.

<a href="https://send.monobank.ua/jar/7uKvD6jK8T"><img src="extension/images/donate.svg" alt="Donate via monobank jar" width="200" height="52"></a>

## Open tasks

What's known to be next, so work can pick up where it stopped (each
item's details are in its section below):

- **The build** (see [Building a map](#building-a-map)).
  - Of JassHelper's manual (its every example run by `wc3-build`'s
    `tests/manual.rs`): all of it is read and built (`//! external` runs
    its tool on the map: `[external]`); what its examples still fail on
    is the manual's own (typos, errors shown on purpose, fragments).
  - Neither Lua nor AngelScript is run in the game yet (only `luac`, a
    test's `lua`, `as-rs`): the work is on macOS, where neither a game
    of 1.31 and later is run from here nor UjAPI (Windows). JassHelper's own manual is in
    `../JassHelper-manual/Writerside/topics`: what's left of vJASS is
    checked against it.
- **The World Editor** (see [World Editor](#world-editor)): its steps,
  in order; the first not done is next. Done: 1 (the shell), 2 (terrain), 3 (cliffs), 4 (doodads and units),
  5 (overlays), 6a (terrain brushes), 6b (things placed). Next: 6c.
- **Tests in the editor** ([below](#tests-in-the-editor)): every new
  piece of the editor gets a step in `extension/e2e/suite.js`.
- **Data**: the real maps tested are in `../maps` (Diablo III beta, The
  Castle, CruiserCommand, LastGuards, three-corridors). three-corridors'
  custom script is vJASS and Zinc as real libraries write them (95
  textmacros, 13 Zinc blocks, 43 libraries: Nestharus', Bribe's,
  BPower's) and builds clean (`wc3-build`'s README, Tests); none uses
  hooks or Zinc's arrays of several dimensions, which only tests
  cover. The
  build's and the server's tests on real maps take their paths from
  environment variables (`WC3_PROJECT`, `MAP`, `EXTRACT_MAP` ...: see
  each test).

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
  files until `[code]` or `[map]` names them.
- **Three sections, three things.** `[project]` is the game the project
  is for (`game=`), `[code]` its code (`sources=`, `exclude=`), `[map]`
  its map (`root=`). Code is found by masks and relative paths; the map
  by its roots, whose top is the map's top.

```ini
// wc3.bni
[project]
game="C:/Games/Warcraft III"
game=../ujapi

[code]
sources=src,lib
exclude=**/old/**,test.j

[map]
root=base.w3x
root=../../multi_project_source
root=icons
root=scripts

[build]
name=debug

[build]
name=release
out=release/map.w3x
uglify=1

[jass]
dialect=vjass
```

| section     | key       | value                                    | meaning                                                                 |
|-------------|-----------|------------------------------------------|-------------------------------------------------------------------------|
| `[project]` | `game`    | one path per line, absolute or relative to the project root | a game folder; repeat for more, each over the ones before |
| `[code]`    | `sources` | masks (a plain path too), comma-separated, relative to the project root | the project's code |
| `[code]`    | `exclude` | masks, comma-separated, relative to the project root | what isn't code, even inside `sources`; the roots aren't touched |
| `[map]`     | `root`    | one path per line, absolute or relative to the project root: a folder or a map (`.w3x`, `.w3m`) | a layer of the map: its top is the map's top; repeat for more, each over the ones before |
| `[build]`   | `name`    | a name, unique in the project            | a build scenario (debug, release ...), a `[build]` each: see [Building a map](#building-a-map); one without a name is the default (one at most), built when none is named, its map `build/map.w3x` |
| `[build]`   | `out`     | a path, relative to the project root     | where the map is written; `build/<name>.w3x` without it; never inside a `[map]` root or the `[code] sources` (an error: the next build would read it) |
| `[build]`   | `uglify`  | `1` or `0`                               | the script's names made short (see "Shorter names" below); `0` without it |
| `[build]`   | `inline`  | `1` or `0`                               | one-line functions' calls inlined, as JassHelper does (see "What a build does to the script" below); `0` without it |
| `[build]`   | `dead_code` | `1` or `0`                             | the functions and globals nothing reachable uses left out; `0` without it |
| `[build]`   | `string_hash` | `1` or `0`                           | `StringHash("text")` of a literal worked out as the game does; `0` without it |
| `[jass]`    | `dialect` | `jass` or `vjass` (case-insensitive)     | the dialect of `.j` files that don't declare one in a `// @dialect` header |
| `[lint]`    | a rule or group id | `off`, `warning` or `error`     | turn a lint rule (or every rule of a group) off or on, at that level (see [Lint](#lint)) |
| `[external]` | a tool's name | its program and first arguments, comma-separated BNI strings (`"wine","tools/ObjectMerger.exe"`; a program with a `/` relative to the project root) | the tool JassHelper's `//! external NAME` runs on the map built (see below) |

- **`[external]`: JassHelper's tools.** A build runs each `//! external
  NAME args` and `//! externalblock [extension=x] NAME args` ... `//! i
  line` ... `//! endexternalblock` of the code on the map it wrote, in
  order, as JassHelper does: the program `[external] NAME=` names, its
  arguments there, then the map's path, the lookup paths (`;` between,
  `/` for `\`: the project's folder, its roots, its game folders) and
  the directive's own arguments (`"..."` one); a block's `//! i` lines
  are the tool's input, or the file `$FILENAME$` names (with its
  `extension=`), run in the project's folder. A tool that ends with
  other than 0 fails the build, with what it said at its line; what it
  says otherwise is a note. Code alone (no `[map]`) makes no map: an
  error.
- **`dialect`.** A file's own header always wins over the project, so a
  project can still mix dialects. Without either, a file is plain JASS.
  A `.vj` file is vJASS unless its header says otherwise. The order is
  header, then `.vj`, then project, then plain JASS.
- **`[code]` and `[map]`: code and the map.** The two say different
  things, and it's worth keeping them apart:
  - **`root`** is the map: everything the map is made of (models,
    textures, icons, `war3map.w3i`, the object data, the tables, the
    scripts that go into it as they are). Anything that has to know where
    the map's top is (an asset at `war3mapImported\icon.blp`, a table at
    `Units\UnitFunc.txt`) is in a root, and is found through the
    project's lookup (see "Finding a file" below).
  - **`[code] sources`** is code: the scripts the build compiles into
    the map's script, in any folders, at any paths (relative paths are
    all code needs).
  - The roots are part of the project as well as the code: what's in
    them is analyzed, checked and shown with hints. So `sources` names
    only what's outside the map.
  - Maps a developer keeps in the repository to take things from are
    neither: they aren't the project's map, and needn't be named
    anywhere. A file opened from one is read with the project's settings
    and that map alone over the game (see "Inside a map: two cases"
    below).
  - `sources`, `exclude` in `[project]` (where they used to be) are an
    error that says to move them to `[code]`.
- **`sources`** is the project's code (see the project model below):
  every value is a **mask**, and a file is code when a mask matches it
  or a folder or map it's in. A plain path is a mask without wildcards:
  `src` is everything in `src`, `.` the whole project folder,
  `**/*.j` every `.j` outside maps. Without `sources` the project has no
  code (its roots are still its, and its settings still apply to files
  opened from under it). `./src` and `a/../src` mean `src`. Quote a path
  that contains a comma: `sources="my, folder"`.
- **Masks** (`server/src/glob.rs`): `*` is any part of a name, `?` one
  character, `**` any number of folders (none included). They're
  relative to the project folder unless absolute, case doesn't matter,
  and `/` and `\` both separate folders. A mask matching a folder or a
  map covers everything in it: `**/old` and `**/old/**` both drop every
  `old` folder, and `**/war3map.lua` a file inside every map.
- **`exclude`** drops what its masks match from the code:
  `exclude=test.j,**/old/**`. It wins over `sources`, and leaves the
  roots alone: everything in a root is the map's. Repeat the line to add more. An excluded file
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
- **One project, one map.** `[map]` lists the map's **roots**: folders
  and maps whose top is the map's top (`war3map.w3i`,
  `Units\UnitFunc.txt`, `war3mapImported\icon.blp`). Folders are the
  way to work: the aim is a map kept as files in folders, with no `.w3x`
  while it's made; a `.w3x` root is there for a base map, or a protected
  one that only gets scripts written in. A map can be put together from
  several: a base map, a folder shared by projects, the project's own
  icons and scripts. Each `root=` goes over the ones
  before. What's in a root is part of the project, so `[code] sources`
  is needed only for files outside the map (scripts the build compiles). A build puts the roots' files together into
  the map in a place of its own (`[build] out=`): a copy of the first
  root when it's a map, with the rest and the script written in, else a
  map made of the files (see [Building a map](#building-a-map)). Campaigns, several maps at once, will be a project above
  projects.
- **Finding a file.** Every file the server reads for a project
  (`UI\WorldEditStrings.txt`, `TerrainArt\Weather.slk`, the metadata
  tables) is looked up the same way (`server/src/game.rs`, `Lookup`):
  through the `game=` folders, then the `[map]` roots, in order, and the
  **last** one that has it wins, the way the game loads a map's files
  over its own. So any layer can replace (shadow) any file below it: a
  root with its own `UI\WorldEditStrings.txt` renames what the editor
  shows. Names are matched in any case, `/` or `\`. Nothing the game's
  files say is built into the server: the names shown for weather
  effects and tilesets come from them (`server/src/names.rs`). The one
  exception is a file inside a map that isn't a root: the game's folders
  and that map alone (see "Inside a map: two cases").
- **Patches: changing an editor file instead of replacing it.** Some
  BNI files are read with patches: `UI\TriggerData.patch.txt` in any
  layer changes `UI\TriggerData.txt` by meaning instead of shadowing it
  whole (`Game::read_patched`, applied by
  [`bni_rs::patch`](../bni-rs/README.md#patches)): a key it has replaces
  that key's lines (all of them: a function's name and phrase in
  `TriggerStrings.txt` go together), a new key goes to the end of its
  section, a new section to the end of the file. So a map made with a
  modified editor (a JNGP with natives added to the GUI) needs only a
  root with its additions, not a copy of the editor's whole files.
  - **Which patches:** those in the layer the file comes from and in the
    layers over it, lowest first, each over the ones before. A patch
    below that layer was a change to a file that layer replaces, so it's
    shadowed with it. With no file anywhere, the patches go on an empty
    one.
  - **Which files: the ones a reader asks for, not every file.** The
    editor's data and strings, the files a modified editor extends
    (`server/src/names.rs`, `PATCHED`): `UI\TriggerData.txt`,
    `UI\TriggerStrings.txt`, `UI\WorldEditStrings.txt`,
    `UI\WorldEditGameStrings.txt`, `UI\WorldEditData.txt`, wherever
    they're read: names and triggers (`names.rs`) and object data's field
    names (`objects.rs`). Tables,
    binary files and scripts are read as they are: code that needs more
    natives declares them in its own files (a map script can declare a
    `native`).
  - **The name:** `.patch` before the extension, so the file keeps its
    highlighting: a patch to a BNI file opens as BNI
    (`TriggerData.patch.txt`, as `TriggerData.txt`). A patch is the editor's data, for the project's tools:
    the game never reads one, so a build leaves them out of the map.
- **Settings apply below the manifest.** A file under the project folder
  but outside `sources` isn't loaded as part of the project, but when it's
  opened it's still read with the project's settings.

**Paths in the editor.** In `game=`, `sources=`, `exclude=` and `root=` values, every folder
along the path is a link (Cmd/Ctrl+click) that reveals that folder: in
the Explorer when it's in the workspace, in the system's file manager
otherwise. Typing a path completes it from the disk: folders for `game`,
folders and files for `sources`, `exclude` and `root`, starting from the project
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
- `root` with no value or several on one line (an error), a root that
  doesn't exist, is a file but not a map, or is listed twice (a
  warning), a second `[map]` (a warning: its roots are added);
- a `[build]` with two `name`s, a name another build has, or a second
  one without a name (the default is one) (an error); repeated
  `[build]` sections are no problem, each is a build; a build whose map
  would be written inside a `[map]` root or among the `[code] sources`
  (an error, at its `out=` or its `[build]`; a build won't run either);
  `uglify` that isn't `1` or `0` (an error);
- an unknown lint rule or group (a warning), or a setting that isn't `off`,
  `warning` or `error` (an error). A rule set twice takes the last value.

**The project model.** `wc3-server` keeps every file of every project in
memory (`server/src/workspace.rs`) and analyzes them all, so the
Problems panel shows the whole project's errors without opening anything.

- **Which projects.** Every `wc3.bni` under the workspace folders, and
  the project of any file opened from elsewhere (even a `wc3.bni` above
  the workspace folders).
- **Which files.** What the project's `[code] sources` match, less what
  `exclude` matches, everything in its `[map]` roots, plus
  `wc3.bni` itself. Only known extensions count (`.j`, `.vj`, `.pld`,
  `.ai`, `.zn`, `.lua`, `.as`, `.angelscript`, `.bni`, `.slk`, `.wts`, `.trg`),
  and the `.txt`
  files that are BNI: a map's `war3map*.txt` (`war3mapSkin.txt`,
  `war3mapMisc.txt`, `war3mapExtra.txt`) and the game's own by name
  (`Units\HumanUnitFunc.txt`, `UI\WorldEditStrings.txt`, ...; the list
  is `BNI_FILES` in `server/src/document.rs`, the same as `package.json`).
  The game's `.txt` files that aren't BNI (`HelpStrings.txt`, the
  captions, the licenses) and `config.txt`, too common a name, aren't;
  hidden folders
  (`.git`, `.idea`, ...) are skipped; a nested project owns its own files.
  `sources` may point outside the project folder (`sources=src,../shared`).
- **Game files.** The game's `Scripts\common.j`, `Scripts\Blizzard.j` and
  `Scripts\common.ai`, looked up like any other file (see "Finding a
  file": the `game=` folders, then the `[map]` roots, the last one wins),
  at the path they're found at: on disk (`../ujapi/Scripts/common.j`) or inside an archive
  (`War3Patch.mpq/Scripts/Blizzard.j`, opened read-only: the game's
  archives are never written). They get no diagnostics, being the game's, and a rename that
  would change them is refused. Projects with the same `game=` folders
  and roots share them.
- **The game's scripts follow the same lookup, on purpose.** The game
  loads `common.j` and `Blizzard.j` under every map script, and `common.j`
  and `common.ai` under every AI script, by themselves. A map script
  that wants a `common.ai` function declares it as a `native` in its own
  code, so nothing about these three needs a lookup of their own: a root
  with its own `Scripts\Blizzard.j` (an optimized one, say) replaces the
  game's for the whole project, as any file in it would replace the
  game's. If a map maker breaks something that way (a `common.j` missing
  natives the code uses, a `Blizzard.j` without functions it calls), the
  build is where it's caught, not the lookup. Settled: don't add special
  cases here.
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
- **Maps.** A map (`.w3x`, `.w3m`) that's a root (`root=Main.w3x`), or
  in a folder that's a root, or one `[code] sources` matches, is
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
  are MPQ archives, read with [`storm-rs`](../storm-rs). A map saved
  again is re-read; a map that can't be read adds nothing.
- **Inside a map: two cases.** A project is one map, so a `.w3x` or
  `.w3m` in it is one of two things, and a file inside it is read
  accordingly (`Workspace::lookup`, `Workspace::scope`):
  1. **The map is a `[map] root`.** It's part of the project's map, and
     its files go through the **whole chain**: the `game=` folders, then
     every root in order, the last one that has a file wins. Its
     `Scripts\Blizzard.j` (or `common.j`, `common.ai`) is the project's
     own, for every file of the project, code in `src` included, unless
     a later root replaces it in turn; the names its hints show come
     from the files the whole chain gives.
  2. **The map isn't a root** (a map kept in the repository to take
     things from, named nowhere or matched by `[code] sources`; known by
     its path, a file inside a `.w3x` or `.w3m`). It isn't the project's
     map, so
     its files see a **short chain**: the `game=` folders, then that map
     alone; the roots don't apply to it, and it doesn't apply to anything
     else. Its own `Scripts\Blizzard.j`, `Scripts\common.j` and
     `Scripts\common.ai` (maps often carry an optimized `Blizzard.j`, which
     the game loads instead of its own) replace the game's for that
     map's scripts only; they get no diagnostics, and no other file sees
     them. Hints and checks of its other files (its regions' weather, its
     object data's fields) read the game's files with the map's own
     over them the same way.

  A rename may change a map's files either way (see Rename below).
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

## Building a map

A build makes the map's script and the map out of everything a project
has: `wc3 build [name] [--project <path>]`, **WC3: Build Map…** in the
editor, or `wc3-mcp`'s `project_build` (`server/src/build.rs`). What follows is the plan, what's
decided about it, and how far it's come.

### One model, several outputs

Every source goes into one model of the code (the IR, in a crate of its
own, `wc3-build`, which the server and `wc3-mcp` use as a library), and
each output is made from it:

```
 this toolchain's JASS ─┐
 vJASS ─────────────────┤                          ┌─▶ JASS, as the game reads it
 Zinc ──────────────────┼─▶ the IR ─▶ checks ─▶ ───┼─▶ AngelScript (UjAPI)
 triggers (.wtg, .trg) ─┤                          └─▶ Lua
 custom script (.wct) ──┤
 the map's data ────────┘
```

- **The IR keeps what's object-oriented**: libraries, structs, methods,
  interfaces. Only the JASS output turns structs into arrays and an
  allocator; AngelScript gets classes. Lowered early, AngelScript
  couldn't be had.
- **The map's data is code too, made into the IR's nodes, not into JASS
  text** (so every output gets the same tree, and converts it):
  the players, forces, alliances and start locations of `war3map.w3i`,
  its fog, sky, weather and music; what `.doo` places (units, items and
  what they drop, the destructables code names); the regions of `.w3r`
  with their weather and sounds; the cameras of `.w3c`, the sounds of
  `.w3s`; the variables and triggers of `.wtg` and `.trg`; the custom
  script of `.wct`, its header and the triggers of custom text. So every
  output gets it the same.
- **One set of checks** for the editor, the build and MCP: the same
  rules, the same `[lint]` settings, the same messages.

### Outputs

- **JASS, as the game reads it.** The functions sorted so that none
  calls one below it (by their calls, a graph sorted topologically).
  Calls that can't be sorted (A calls B, B calls A) go through
  `TriggerEvaluate`, as JassHelper's `.evaluate`: the arguments and the
  result through globals of the function's own. JASS runs one thing at
  a time and an evaluated function takes no arguments, so two calls
  can't race for those globals. `main` and `config`, declared any number
  of times, are merged into one each, and the triggers of map
  initialization run after every `main` (as World Editor's
  `RunInitializationTriggers`). `set`, `call`, `local` and `takes nothing
  returns nothing` are written; locals go to the top of their function,
  globals into `globals`. What it makes passes the rules of the group
  `pure`.
- **Which one: `[build] script=jass|lua|as`** (`jass` without it). Every
  source (JASS, vJASS, Zinc, the triggers) goes through the IR to the
  one asked for: all of a map's JASS is transpiled, nothing is written
  twice. A map built as `lua` gets `war3map.lua` and the Lua flag of its
  `war3map.w3i`.
- **AngelScript, for UjAPI** (decided with UjAPI's author, 2026-11-24):
  the natives of `common.j` are in the namespace `Jass` (`Jass::CreateUnit`);
  every type extending `handle` is a global type of its own and is a
  `uint` (a number, not a reference), its empty value `nil` (not
  `null`); `'hfoo'` is written as in JASS. UjAPI ships no `Blizzard.j`,
  so the build takes the functions and globals of it the code uses (and
  what they use: tree-shaking) and writes them in AngelScript too.
  Classes for structs. `integer`, `real`, `boolean`, `string` are `int`,
  `float`, `bool`, `string`; AngelScript has no `code`: UjAPI's two
  funcdefs, `void CallbackFunc()` and `bool BoolexprFunc()` (`Condition`,
  `Filter`), `function F` `@F` (`TimerStart(t, 1.0f, false, @F)`,
  `Condition(@F)`). The script runs after the natives are set up, so a
  global's initial value may call them. The map's
  script is `war3map.as`, its entries `void main()` and `void config()`.
  An array (AngelScript's `array<T>` has a size, a JASS one none) is a
  class of the namespace `WC3` the build writes for each type of element
  used (`WC3::int_array`), indexed as JASS's (`a[i] = x`): any index
  read, what's never set its type's empty value, growing as it's set.
  A struct is a class and its value a handle (`S@`, decided 2026-11-25);
  where vJASS takes it as an integer (`SaveInteger(ht, k, 0, this)`,
  `S(LoadInteger(...))`) its number goes and comes back through its
  family's table, `0` is `null`, so instance 0 is no object
  (`thistype(0).next` is an error). AngelScript has no `ExecuteFunc`:
  `ExecuteFunc(name)` is a call of the function named when the name can
  be worked out before the game runs (literals, `+` of them, constants
  made of them); else it's left as it is and is an error the build
  reports, the map written all the same (`wc3 build` exits with 2; the
  editor says so; `project_build`'s `errors`).
- **Lua** (UjAPI runs it too): the natives and `Blizzard.j`'s functions
  as the game gives them (its own Lua has `Blizzard.j`, so it isn't
  written again: its `bj_` globals are the game's state).
- **A project of code alone** (no `[map] root`): a build writes the
  script only, `build/war3map.j` (`.lua`, `.as` by `script`), with no
  map's part: `main` and `config` are the code's.
- **A trigger in each**: Show JASS, Show Lua and Show AngelScript, live
  from the text as it is (not saved). Done: see Show JASS below.

### vJASS and Zinc

The most used of each first, not all of JassHelper: libraries and
scopes with `requires` and initializers, `private`/`public`, structs
with methods, `static` members, `create`/`destroy`/`onDestroy`,
interfaces, `.evaluate`/`.execute`, `static if` on constants. The rest
(`textmacro`, `hook`, operators, `delegate`, `stub`, modules ...) when
a map needs it. Zinc's parser is complete; it goes into the same IR.

### In order

0. **Names and types.** Every name resolved: the game's `common.j` and
   `Blizzard.j` (through the project's lookup) and the project's files.
   An unknown function, variable or type is an error, and so is a call
   with too many or too few arguments; then types, the `handle` tree
   from `common.j`. In the editor, the build and MCP (`code_check`; the
   natives: see `wc3-mcp`'s README). Done: names and types in the
   editor and in `code_check`, the natives in `jass_natives`.
1. **This toolchain's JASS into the game's.** The output above, for
   the triggers and hand-written JASS: the first thing that runs. Done
   (`wc3-build`'s `jass`; `wc3-mcp`'s `jass_build`): the three maps'
   scripts build as they were, and The Castle's 752 triggers, variables
   and custom script build into one script that lacks only the map's
   data.
2. **The map's script out of the map's files** (the table above). Done
   (`wc3-build`'s `map`, made as the IR's nodes): `config`, the players
   and forces, the setting up in `main`, the sounds (how long each plays
   worked out as World Editor does), regions, cameras, `InitGlobals`,
   the triggers of map initialization, upgrades, the tech tree, random
   groups and item tables (`tables`), and what's placed (destructables,
   items, units and buildings, random ones too, what each drops when it
   dies; `placed`, the types read by `objects` from the game's tables
   and the map's object data). World Editor's call for call on The
   Castle and CruiserCommand (The Castle's whole `main`, 1475 calls) and
   on 104 of Blizzard's 108 melee maps; the other four are World
   Editor's for what their files don't say (`wc3-build`'s README).
   Saving a `.w3x` makes its `war3map.j` again, and so does a build.
   Checked against the `war3map.j` of real maps by what each part does
   (its calls), not byte for byte; taking a file out of a map into a
   folder doesn't change what's built.
3. **vJASS and Zinc into the IR**, the most used first. Done: the IR has
   libraries and scopes, structs, interfaces, modules, function
   interfaces and who owns each item; `jass-rs` reads vJASS's grammar
   (libraries, scopes, structs, methods and operators, interfaces with
   `defaults`, modules, `a.b`, `.x`, `this`, `thistype`, `super`,
   `static if`), and its lint, names and the editor's tree know it;
   `wc3-build` reads it and Zinc into the IR (Zinc's loops, `+=` and
   anonymous functions into JASS's forms). JassHelper's preprocessor
   is run over all the code first (`wc3-build`'s `preprocess`:
   `textmacro`/`runtextmacro`, `novjass`, `import`), what's said of the
   code said of the lines written; `hook` (`h__Name`, the game's
   natives' signatures from `common.j` and `Blizzard.j`); Zinc's arrays
   of several dimensions (one array). In the editor a textmacro's body
   and `novjass` are a comment. `//! inject`, `dovjassinit`,
   `loaddata` are built; `external` runs its tool on the map built
   (`[external]` in Projects above).
4. **Their objects in JASS**: structs as arrays and an allocator,
   interfaces through `TriggerEvaluate`, libraries in the order of
   `requires`. Done so far (`wc3-build`'s `lower` and `structs`):
   libraries and scopes (order, private and public names, initializers),
   `static if`, `%`, structs with fields, static members, methods,
   `create`/`allocate`/`destroy`/`onDestroy`, `onInit`, modules, casts;
   interfaces, a struct extending another, `stub` (by the instance's
   type, through a trigger of each type, the arguments in globals),
   `super`, `typeid`, `.evaluate`/`.execute` through triggers;
   function interfaces (`I.f` a number, run through a trigger of
   each), operators (`x`, `x=`, `[]`, `[]=`, `<`), `delegate`; what's
   built checks clean as the game's JASS; operators of interfaces and
   `stub` ones by the instance's type, and more index space than an array
   (split into the game's arrays). Everything of JassHelper's manual
   (`wc3-build`'s README).
5. **AngelScript and Lua** past their stubs (see Outputs above for
   what's decided): the build of code alone and `script=` first, then
   Lua, then AngelScript with `Blizzard.j` tree-shaken, then Show Lua and
   Show AngelScript for a trigger. Done: `script=` (`wc3_build::jass::Script`,
   `jass::Options::script`); a project of code alone builds
   `build/war3map.<ext>`; Lua (`wc3-build`'s `lua`, after the same steps
   as JASS, the types told by the declarations: integer division cut
   toward zero, strings' `+` as `..` with a null one empty, `and`/`or`
   in parentheses where they bind otherwise, arrays as tables with their
   empty value, `'hfoo'` a number, `return` as `do return ... end`):
   Diablo III beta's whole map builds as Lua that `luac -p` reads, and a
   test runs a script in `lua` to see it does what the JASS did. A map
   built as Lua gets `war3map.lua` and its `war3map.w3i` the Lua flag
   (from 1.31's layout; an older one is told so). AngelScript
   (`wc3-build`'s `angelscript`, as decided above; `Blizzard.j`
   tree-shaken): Diablo III beta's whole map builds as 27064 lines that
   `as-rs` reads, every call a function written or a native of `Jass::`;
   structs and interfaces as classes (`wc3-build`'s README); Show Lua and
   Show AngelScript of a trigger (below, Show JASS).
6. **The map**: `[build]` scenarios, the roots' layers, the `.w3x`
   written (`storm-rs`); optimizations (dead code) after. Done
   (`server/src/build.rs`, `wc3-build`'s `whole`):
   - a `[build]` says where the map goes (`out=`) and whether its names
     are short (`uglify=`); `wc3 build` without a name builds the
     default (the `[build]` without a name), else the first, and a
     project without a `[build]` as `build/map.w3x`; never into a root
     or the code;
   - **the map's files** are the roots', the last that has a file
     winning, less what's the project's: the patches (`X.patch.txt`),
     `.trg` files, the project's code, `wc3.bni`, maps inside a root
     folder, and the script, made anew (`war3map.j`, and a
     `Scripts\war3map.j` dropped: the game would take it first);
   - **the script**: the map's part of the files the lookup finds; the
     triggers, the project's `.trg` files when it has any (in the order
     of their paths; the map's globals declared by `InitGlobals`, a
     file's private ones by the file), else the roots' `war3map.wtg`; the
     map's custom script (`war3map.wct`: its header, and a custom text
     trigger's code found by the trigger's name in the roots'
     `war3map.wtg`), vJASS unless `[jass] dialect=jass`; the project's
     code (`.j`, `.vj`, `.zn`) in the project's dialect;
   - **the archive**: when the first root is a map, a copy of it with the
     rest written in, so the files of a protected map nobody knows the
     names of stay; else a new one, behind a map's header (`HM3W`: its
     name, flags and players from `war3map.w3i`). Written beside `out`
     first, so a build that fails leaves the last map as it was.
   Diablo III beta (238 `.trg`), The Castle (753 `.trg`) and
   CruiserCommand (its `war3map.wtg`, a root of JNGP's patches) build;
   their scripts have no problem their own `war3map.j` hasn't, and every
   file of their maps comes out the same (`build`'s test
   `a_real_project_built`, `WC3_PROJECT`). Diablo's map built from a
   copy of it and from its files taken out into a folder is the same,
   file for file, its script too. A copy is compacted after
   (`storm_rs::archive::compact`): the space of the files it replaces
   given back, the files nobody knows the names of kept (moved by the
   hash table, an encrypted one encrypted again for its place); one that
   can't be is left as it is, with a note. A build from `.trg` files
   gives the map `war3map.wtg` and `war3map.wct` made of them. A map
   with no triggers (a protected one, its script only `Scripts\war3map.j`)
   in a project with no code keeps its own script as it is: made of its
   files alone it would have none of its code, and the game would run
   that one first (LastGuards); with code, the script made replaces it,
   and a note says so. CruiserCommand and LastGuards built so: 0
   problems.

**In the editor**: **WC3: Build Map…** (the command palette: the project
of the active file, else the workspace's; the Explorer's menu of a
`wc3.bni`), the ▶ button over an open `wc3.bni`, and a lens over each of
its `[build]`s (**Build debug**, **Build (default)**). Run with no
build named (the palette, the button): the default; with several and no
default, it asks which. The project's files open with changes are saved first (the build
reads the disk); the build runs as a process of its own (`wc3 build`,
the binary the extension ships), so the editor goes on answering, and
what it says goes to the output **WC3 Build** (shown when it fails; a
map built offers to reveal it; one built with errors in its script, as
an AngelScript `ExecuteFunc` that can't be worked out, says so: `wc3
build` exits with 2). For AI assistants: `wc3-mcp`'s
`project_build`.

**What a build does to the script**, each by its own name in `[build]`
(none without it), in this order, before names are shortened, for every
output (`wc3-build`'s modules of the same names); the build says how
much of each in a note. There's no `optimize` (a project that still
says it is warned with these names):

- `string_hash=1`: `StringHash("text")` of a literal is the number the
  game gives, worked out as the game's `SStrHash2` does (pjass's
  `sstrhash.c`, taken from the game: Jenkins' lookup2, seed 0, `a`-`z` as
  `A`-`Z`, `/` as `\`), so the game hashes nothing as it runs. Only an
  ASCII literal shorter than 1024 bytes (the game's hashing of what isn't
  ASCII changed with versions); `StringHash` of anything else is the
  game's to work out.
- `inline=1`: as JassHelper's (its manual, Script optimization), a call
  of a one-line function is what its line does, only where nothing can
  change (each parameter read once, in order, any call after them; not a
  `set` of a parameter; not where the caller has a local named as a name
  the line reads).
- `dead_code=1`: the functions nothing reachable from `main`, `config`,
  `ExecuteFunc("name")` (any of a name told only as the game runs keeps
  them all) or a class's methods calls or names as code, and the globals
  nothing reachable reads or sets, are left out. Diablo III beta with
  `inline=1` and `dead_code=1`: 53 functions and 66 globals of its JASS
  left out, its script checking clean as before.

**Shorter names (uglify).** The finished tree knows every name and every
reference to it, so a build can rename everything that isn't the game's
(not a native, nor a function, global or type of `common.j`,
`Blizzard.j` or `common.ai`) to the shortest names free: `a`, `b`, ...,
`a1` ... A function's locals and parameters take the shortest names again
in each function. A name the code gives as text is renamed with it where
the text is a literal (`ExecuteFunc("Name")`, the variable of
`TriggerRegisterVariableEvent`); where it isn't, what it may name keeps
its name, and the build says why. Done (`wc3-build`'s `uglify`,
`wc3-mcp`'s `jass_build` with `uglify`): three maps' scripts keep their
problems rule by rule at 64% to 76% of their size.

## Textures

A `.blp` opens in the editor's texture viewer, from disk or from inside a
map, and so does a `.tga` (map previews, loading screens, model
textures: decoded with the `image` crate, every one of the game's 383). The server decodes it with [`blp-rs`](../blp-rs/README.md)
(`server/src/texture.rs`) without its encoder, so the server stays pure
Rust: every BLP1 and BLP2 kind (JPEG, palette with 0/1/4/8-bit alpha,
DXT1/3/5, raw BGRA) and every mip, each on its own, so a damaged or
missing mip leaves the others viewable. BLP0, the Warcraft III beta's
format, keeps its mips in separate `.b00`… files, and isn't shown.

A map's pathing, `war3map.wpm`, has no text: it opens in the same viewer
as a picture ([`wpm-rs`](../wpm-rs/README.md)). A pixel is a cell (32
world units, four to a terrain cell), north up, drawn as the game's
pathing textures draw pathing: red where units can't walk, green where
they can't fly, blue where nothing can be built, so white forbids all
three. **Show** switches to one flag at a time (no walking, no flying,
no building, blight, no water, boundary, the two unused bits), white
where it's set; under the cursor, the cell (from the bottom left, as the
file counts) and its flags by name. Save PNG saves the picture.

A map's shadow map, `war3map.shd`, opens there too ([`shd-rs`](../shd-rs/README.md)):
the file has no header, so its size comes from the terrain beside it
(`war3map.w3e`, on disk or in the same map; 16 pixels to a terrain
cell). Dark where trees and buildings cast shadow.

## Object data

Object data (`.w3a` abilities, `.w3b` destructables, `.w3d` doodads,
`.w3h` buffs, `.w3q` upgrades, `.w3t` items, `.w3u` units) opens as text,
from disk or from inside a map, and saves back into the file. The text is
BNI, written and read by [`w3abdhqtu-rs`](../w3abdhqtu-rs/README.md)
(`server/src/objects.rs`):

```ini
[w3a]
version=2

[ACwe]
reqLevel=6                          Required Level: int

[A000]
_parent=AHbz
Name="Blizzard"                     Name: string
Ubertip[2]="Line one"\n"Line two"   Tooltip - Normal - Extended: string
DataA[3]=12                         Number of Waves: int
DataB[3]=0.25                       Damage: unreal
Cool[3]=6.0                         Cooldown: unreal
```

`[ACwe]` is a changed standard object, `[A000]` a custom one with the
object it's based on. The names and types on the right aren't in the
text: they're inlay hints, shown after each field but not part of what
you edit (`QUERY.INLAY_HINTS`, `objects::hints`). The editor asks only for
the lines in view, and as many again above and below for scrolling: a
whole file's hints at once would slow it down.

A field's key is its name in the game's data tables (`Cool`, `DataA`,
`HP`: the metadata's `field` column; a shared one with its `index` as a
letter, `ButtonposA` and `ButtonposB` for `abpx` and `abpy`), or its
four-character id where nothing tells the field apart or there's no
metadata; then `[level]` for abilities, upgrades and doodads, and, after
an id, `.column` (`Data A` is `.A`). Both forms read; the hover shows the
id. A number takes
its field's type (integer, real, unreal) from the game's metadata, shown
in the inlay hint and the hover; only a value of another type, or an unreal
without metadata, says it (`unreal 0.25`). The `end` id after each field,
which the game doesn't read, is left out, and saving writes 0. The
format and its rarer parts (sets) are in the `w3abdhqtu-rs` README.

The hint after a field is the name World Editor gives it and its type, from the
game's metadata tables (`Units\AbilityMetaData.slk` ...) and its strings
(`UI\WorldEditStrings.txt`), found in the `game=` folders of the file's
project, over each other in order, like the game's scripts: the project
whose `sources` take the file in, else the nearest `wc3.bni` above it (a
map next to its `wc3.bni` is that project's, `sources` or not). They're
looked up only in the game folders, never in the project or the map,
which may legally replace them with their own. The game's version decides
the fields, so there's no guessing: a file with no `wc3.bni` above it, or
in a project without `game=`, gets no names and no checks. They're read once per set of
`game=` folders and kind, and again when an archive or one of the tables
changes on disk.

While the text is edited, its fields are checked against the same
metadata: a value stored as the wrong type (an error: the game misreads
it), a number past World Editor's limits, an unknown field, a level or
data column that doesn't fit, a field of another object. Each is a lint
rule (`object-...` below), so the project's `[lint]` switches it.

Hovering over a field's key shows what the metadata says about it: its
name and id, its type and how it's written, World Editor's limits,
whether it has levels and which data column it is in, and whose field it
is (`Data of AHbz, ACbz`; `AIbz has the data of AHbz`, by its code).

The editor reads the
text as BNI, so it's highlighted and checked like any BNI, plus what
would stop it from saving (a value that isn't one, a level on a unit
field). Saving such text fails with the reason, and the file stays as it
was. Comments and spacing aren't kept: the file has no room for them, so
the next time it's opened the text is as the server writes it.

A collection (`.w3o`: World Editor's File > Export All Object Data)
opens as text too ([`w3o-rs`](../w3o-rs/README.md)): a `[w3o]` section,
then each kind's text form as its own file's, from its section (`[w3u]`,
`[w3t]` ...). Each block gets its kind's names, hints, hover and checks.

## Cameras

A map's cameras, `war3map.w3c`, open as text the same way, from disk or
from inside a map, and save back into the file. The text is BNI, written
and read by [`w3c-rs`](../w3c-rs/README.md) (`server/src/cameras.rs`): a
`[w3c]` section, then a section per camera named after it (the script's
`gg_cam_<name>`), its fields in file order.

```ini
[w3c]
version=0

[Close Arthas]
target_x=-2728.98
target_y=1922.12
z_offset=0
rotation=83.74
angle_of_attack=330.62
distance=830.11
roll=0
field_of_view=90
far_z=2500
near_z=100
```

The file doesn't say its layout: since 1.31 a camera also has
`local_pitch`, `local_yaw` and `local_roll`. The map's `war3map.w3i` next
to the file says which game it's for (its first four bytes, the format
version: 28 and later is 1.31 and later); without one, it's 1.27a's. A
field left out takes World Editor's default; a field the layout doesn't
have, a value that isn't a number, an unknown or repeated field are
errors while the text is edited, and saving refuses them.

## Regions

A map's regions, `war3map.w3r`, open as text the same way
([`w3r-rs`](../w3r-rs/README.md), `server/src/regions.rs`): a `[w3r]`
section, then a section per region named after it (the script's
`gg_rct_<name>`).

```ini
[w3r]
version=5

[RegionDust]
index=12
left=4544
bottom=4000
right=4608
top=4992
weather=WNcw
sound="gg_snd_N06Furion30"
color=255,255,255,255
```

`weather` is an effect's id, `0x00000000` for none; its name (`Wind
(Heavy)`, `none`) shows after it as an inlay hint, from the game's files
through the project's layers (`TerrainArt\Weather.slk` and
`UI\WorldEditStrings.txt`; see "Finding a file"). `color` is red, green,
blue, alpha: a swatch before it opens the color picker, and a picked
color is written back as the four numbers. `sound` is a sound variable,
quoted. A field left out takes its default (no weather, no
sound, white, the next index); a value that isn't one, an unknown or
repeated field are errors while the text is edited, and saving refuses
them.

## Sounds

A map's sounds, `war3map.w3s`, open as text the same way
([`w3s-rs`](../w3s-rs/README.md), `server/src/sounds.rs`): a `[w3s]`
section, then a section per sound named after its variable
(`gg_snd_<name>`).

```ini
[w3s]
version=1

[gg_snd_ArthasSummon]
file="Abilities\Spells\Undead\DarkSummoning\DarkSummoningTarget1.wav"
eax="SpellsEAX"
flags=0x00000006
fade_in=10
fade_out=10
volume=default
pitch=default
channel=default
...
```

`default` is World Editor's "use default" (the game's own value). As
inlay hints: the EAX setting's name (`Spells`, from
`UI\SoundInfo\EAXDefs.slk`; an empty one is the default setting's), the
channel's (`Combat`, from `UI\WorldEditData.txt` `[SoundChannels]`) and
the flags' (`3D sound, stop when out of range, imported`, World Editor's
strings for them), all from the game's files through the project's
layers (see "Finding a file"), so in the game's language. Channels and
flags are built into the game, so `w3s-rs` has them too
(`w3s_rs::CHANNELS`, `w3s_rs::FLAGS`), with English names that show
without the game's files. A field left out takes World Editor's value for a
new sound; a value that isn't one, an unknown or repeated field are
errors while the text is edited, and saving refuses them.

## Terrain

A map's terrain, `war3map.w3e`, opens as text too, but only its header:
version, tileset, the ground and cliff textures, the grid's size, its
position ([`w3e-rs`](../w3e-rs/README.md), `server/src/terrain.rs`). The
points (heights, textures, cliffs, water) aren't text. What the values
mean shows after them as inlay hints: the tileset's name, each ground
and cliff texture's name with how many points use it (the server reads
the file's points once, on disk or in the map, and again when it
changes), the map's size in cells, where it spans in world coordinates,
which games write the version. Names come from the game's files
(`UI\WorldEditData.txt` `[TileSets]`, `TerrainArt\Terrain.slk`,
`TerrainArt\CliffTypes.slk`).

```ini
[w3e]
version=11
tileset=L
custom_tilesets=0
ground=Ldrt,Ldro,Lgrs
cliffs=CLdi
width=97
height=97
offset_x=-6144
offset_y=-6144
```

Saving puts the header onto the file as it is now, on disk or in the map,
its points kept. So the size can't change, and a texture list only so
that every point still finds its texture at its place: replacing a code
retextures its points, removing one some point uses is refused with how
many use it. A new version (11 ↔ 12) moves every point's flags. A field
left out keeps the file's value.

## Custom script

A map's custom script, `war3map.wct` (the trigger editor's custom
script, and the triggers converted to custom text), is code, and it's
treated as code ([`wct-rs`](../wct-rs/README.md),
`server/src/customtext.rs`). Its text form is the code, one piece after
another, each after a marker line (a comment, so the text is a valid
script):

```jass
// @wct version=1
// @wct comment ""
// @wct header
function Helper takes nothing returns nothing
endfunction
// @wct trigger
function Trig_Init_Actions takes nothing returns nothing
    call Helper()
endfunction
// @wct trigger none
```

- **In the model.** A `war3map.wct` the project has (in a `[map]` root,
  folder or map, or matched by `[code] sources`) is a document of the
  model in that text form: JASS, or Lua when the map's `war3map.w3i`
  says the map is Lua. It's analyzed, indexed and seen like any other
  script, so the project's code finds the map's custom script's
  functions, and go to definition leads into it.
- **In the editor.** It opens as that text (`wc3obj:`), checked as JASS
  (with the project's dialect) and highlighted, each `trigger` marker
  with its number as an inlay hint. While it's open, the text stands for
  the file in the model: its code is there once, as edited. Saving turns
  it back into the file's bytes (`\r\n` line breaks, as World Editor
  writes them); a wrong marker is an error while editing, and saving
  refuses it.

## Map info

A map's info, `war3map.w3i` (the Scenario menu: name, description,
players, forces, tech tree, random tables ...), opens as text too
([`w3i-rs`](../w3i-rs/README.md), `server/src/mapinfo.rs`): a `[w3i]`
section, then a section per player, force, upgrade, tech, random unit
table and random item table, in file order.

```ini
[w3i]
version=25
name="TRIGSTR_001"
flags=0x0001DC68
tileset=C

[player]
number=0
type=1
race=1
name="TRIGSTR_005"

[force]
flags=0x0000000B
players=0x000001FF
name="TRIGSTR_004"
```

What a value means shows after it as an inlay hint, not in the text:
the tilesets' names, the flags set, a player's number, type and race,
the players of a mask, a force's flags, an upgrade's availability, a
random table's columns, the fog style, what a list's numbers are. The
words are the game's where it has them (World Editor's strings, through
the project's layers, so in the game's language; the keys are in
`w3i-rs`'s tables), English otherwise. The version
(18 Reign of Chaos, 25 The Frozen Throne, 28 1.31, 31 1.32) decides which
fields there are: one it doesn't have is an error while the text is
edited, and saving refuses it. A field left out takes its default. A
file a map optimizer cut short says so (`cut=forces`) and saves back the
same.

## Imports and minimap icons

A map's list of imported files, `war3map.imp`, opens as text
([`imp-rs`](../imp-rs/README.md), `server/src/imports.rs`): `[imp]
version=1`, then a `file=<flags>,"path"` line per file. Where the file
is in the map (`war3mapImported\` and its name, unless the flags say a
custom path) and the flags' names show as inlay hints.

Its minimap icons, `war3map.mmp`, too ([`mmp-rs`](../mmp-rs/README.md),
`server/src/minimap.rs`): an `[icon]` per icon with `kind`, `x`, `y`
and `color=r,g,b,a`; the kind's name is a hint, the color has a swatch
and the color picker.

## Doodads and units

What a map places, `war3map.doo` (doodads, destructables, the doodads
cliffs place) and `war3mapUnits.doo` (units, buildings, items, start
locations), opens as text ([`doo-rs`](../doo-rs/README.md),
`server/src/placement.rs`): a header section, then a `[doodad]`,
`[special]` or `[unit]` per thing placed, in file order, with its type,
position, angle, scale, owner, hit points, items, abilities, item drops
and how it's random. Since 1.32 everything placed has a skin: the
`war3map.w3i` beside the file says which layout it is (without one, the
layout the file reads in).

## Triggers

A map's GUI triggers, `war3map.wtg`, open as text ([`wtg-rs`](../wtg-rs/README.md),
`server/src/triggers.rs`): `[wtg]`, then a section per category,
variable and trigger, and a trigger's events, conditions and actions as
lines of calls:

```ini
[trigger]
name="Hint Draenei"
description=""
category=1
event=MapInitializationEvent()
action=IfThenElseMultiple()
>condition:0=OperatorCompareBoolean($DraeneiUsed, OperatorEqualENE, "false")
>action:1=TriggerSleepAction("30.00")
>!action:2=DoNothing()
```

`!` marks one disabled; `>` and `:block` one inside another (an
if/then/else's condition 0, then 1, else 2). How many parameters each
function takes isn't in the file but in World Editor's
`UI\TriggerData.txt`, found like every game file (see "Finding a file"):
a modded editor's in a root replaces the game's. Opening needs it, and
saving checks every function's name and parameter count against it.

The file opens as World Editor shows it (`wc3.wtgTriggers`, over that
text: **Open as Text** and **Open as Triggers** switch, both the same
document): on the left the tree, drawn by the view since there are no
files to show it (each category with its triggers, a disabled one
dimmed, a comment in italics, then the variables); on the right the
trigger picked in it, as a `.trg` shows (below), and edited the same
way. The tools above the tree make a new category, a new trigger (in
the category selected, after its last), a new variable, and rename
(F2) or delete (Delete, after asking) what's selected: a category with
its triggers, a trigger, a variable. A trigger moves by dragging it:
onto a trigger, before it, in its category; onto a category, to its
end; Alt+Up and Alt+Down move the selected one in its category. A
variable opens World Editor's dialog of it, without a scope: **a map's
variables are globals**, private and local ones are a `.trg`'s. And
**custom script is one line there**, as World Editor's: its dialog has
a line to type, not lines, an edit that would make one of more lines is
refused, and the text with one is a problem at its line and doesn't
save; custom script of many lines is a `.trg`'s.

The server cuts each trigger's `[trigger]` out of the text, without its
`category=`, and to what's done the triggers are a project of trigger
files of their own: the picked one is seen and edited as a `.trg`, a
rename retargets the other triggers' references to it, a variable's
change renames its uses in all of them and lists what it breaks, at
their lines in the text. What changed is put back: every edit is of the
text, so undo and save are the text's, and saving writes `war3map.wtg`
as before (`triggers::wtg_view`, `triggers::wtg_edit`,
`triggers::map_op`). A trigger is picked by its place among the
triggers, not its line, so typing in the text above it doesn't pick
another. **Show JASS** shows the picked trigger's JASS, as a `.trg`'s
(below), and follows the picking (`war3map.wtg.j`,
`triggers::wtg_jass`): the map's variables are its globals. A comment
(`comment=1`) is set apart, in the tree and in the view: its text
alone, no events, conditions or actions.
A trigger's `category=` shows its category's name as an inlay hint.

### A trigger of its own: `.trg`

**Triggers live in one of two places, and a map moves from the first to
the second in small steps.** Nobody starts by turning a whole map into
folders: that's a lot of work to do before seeing anything come of it,
and it's frightening. So the first step is nothing more than opening a
`.w3x` and editing its triggers; the rest comes one file at a time,
when it's worth it, and no step is one that can't be taken back.

- **In the map, `war3map.wtg`: World Editor's triggers.** They're edited
  in the map itself, and the view draws their tree (categories,
  triggers, variables) on its own, since there are no files to show
  it. Only what World Editor's trigger editor can do is offered here:
  `war3map.wtg`'s layout (and `UI\TriggerData.txt`'s rules) can't hold
  any more, and what can't be stored would be lost. Saving writes
  `war3map.wtg` and makes the map's script, `war3map.j`, again from
  the map's files, as World Editor does: the game runs the script,
  `war3map.wtg` is only World Editor's storage of the triggers. The
  script behaves as World Editor's would; it isn't World Editor's
  text (its conditions, for one, are written plainly).
- **In files, `.trg`: the triggers of their own.** **Extract
  Triggers…** takes all of the map's `war3map.wtg` out at once (not
  some triggers here and some there), a file per trigger and a folder
  per category, and each trigger of custom text's code beside it, in a
  `.j` of its name (one that's off: commented out, as World Editor
  leaves it out of the script; it's often no code), read as vJASS
  (`// @dialect vjass` first): the project's code, standing for that
  trigger's piece of the map's `war3map.wct` (the project's file over
  the map's, as everywhere). The map's own custom script stays in
  `war3map.wct`, which a build reads while the map has it, whatever the
  triggers are; a trigger of custom text with neither is an error. From
  then on the files are the triggers, and
  everything this editor has beyond World Editor is theirs: local and
  private variables, custom script of many lines, and what comes next.
- **A map open is a project of one layer**, the map itself (see
  "Projects": the project's own files win over the map's). Moving to folders is taking files out of the map into that
  layer, one by one: the triggers, the object data, the rest.
- **Development is hybrid, and stays so as long as one likes**: a map's
  binary files and the project's files of their own work together in
  one project and one build. The triggers as `.trg` while `war3map.wct`,
  the object data and the terrain are still the map's; a root folder
  holding `war3map.w3u` as it is beside text files of other kinds; a
  base `.w3x` under folders. Every file is read where it is, in the
  layers' order, and none needs another to have moved first. Nothing
  changes its meaning by what else is there (no file is a sign to read
  others differently): what's in the project is what's built, and what
  isn't needed is deleted by hand. A folder of extracted triggers that
  isn't to be built (kept to look at, say) stays out of `[code]
  sources`, or goes in `exclude`: then the map's `war3map.wtg` is the
  triggers. **Taking
  a file out doesn't change the script a build makes**, byte for byte:
  the map's script and a project's are made by the same build, only
  the files come from different places. That's what makes each step
  safe, and what a test checks.
- **And back:** a `.trg` that uses nothing World Editor lacks can be
  written into a map's `war3map.wtg` again, so a map is never held in
  this tool; one that does use more is told what (a local here, a
  private global there). The triggers' order hardly matters to them,
  so little is lost going either way.

**A map's triggers and its custom script go together.** The code of a
trigger of custom text isn't in `war3map.wtg` but in `war3map.wct`, and
by the trigger's place, not its name (each trigger has a piece there,
`// @wct trigger`, most of them `none`). So the two are one thing to
edit, kept as the two files they are (each keeps its own text form, and
the code stays JASS, with everything the editor does for JASS):

- **Opening either opens the triggers**: `war3map.wtg` or
  `war3map.wct` (when a `war3map.wtg` is beside it) opens the trigger
  editor with the tree. The custom script is opened beside it as a
  document, so what's edited and not saved in it counts.
- **The code is the custom script's text, a piece at a time**: **Map
  Script** at the top of the tree, and a trigger of custom text picked
  (or double clicked, or **Open Code**), open that code alone beside, as
  a document of its own (`wc3code:…/war3map.wct/<trigger>.j`), so the
  other triggers' code is out of reach. It's a view of those lines of
  `war3map.wct`, not a file: typing in it changes the custom script's
  text at once (a piece that stored none gets its marker's `none`
  dropped), a change there comes back to it, and saving it saves the
  custom script (and with it the triggers). The server never gets it (its
  functions would be declared twice): its diagnostics, highlighting,
  hover, go to definition, references, outline and rename are the custom
  script's, their lines moved. A trigger of custom text shows no events,
  conditions or actions, only a line saying where its code is. **Open as
  Triggers** over the custom script's whole text goes back to the tree.
- **The tree's changes move the code with the triggers**: a new trigger
  gets an empty piece in its place, deleting one (or a category) takes
  its piece out, moving one moves its piece; one edit of both files,
  undone as one (`triggers::wtg_edit`'s `wct`,
  `customtext::reorder`). A custom script whose pieces don't go with the
  triggers (as many as there are) is said so once, and left alone.
- **Maps of 1.31 and later** open the same: their `war3map.wtg` (a tree
  of elements with ids) is the same text with the tree's own keys
  (`tree=1`, each section's `id=`, a category's `parent=`; see wtg-rs),
  its sections in the tree's order; a trigger's view leaves `id=` out as
  it does `category=`, and a category new in it gets a category's id.
  Saved, it's 1.31's again, the ids kept, new ones of their type. Its
  `war3map.wct` (`@wct magic`) has a piece only for each trigger that
  isn't a comment, and the pieces go with those
  (`customtext::Pieces::of_triggers`, `order`). The MCP's `wtg_` tools
  read and write it so too; the build reads its triggers as the classic
  ones and writes World Editor's classic layout back.
- **A trigger renamed, its names in the code follow**: what World
  Editor's script makes of a trigger of custom text is its variable
  (`gg_trg_<name>`) and the call of `InitTrig_<name>`; its functions
  are its code's. So renaming one renames those two names wherever the
  custom script writes them (whole names: `gg_trg_AB` isn't `gg_trg_A`'s;
  `triggers::rename_trigger_in_code`), in the same edit, and a piece of
  its code open is reopened as its new name's where it was. Its other
  functions (`Trig_<name>_Actions` ...) are its code's own: they keep
  their names.
- **Saved together**: saving one saves the other, so the files on disk
  never put code to the wrong trigger.
- **Read as code as World Editor builds it**: the custom script is vJASS
  unless the project says `[jass] dialect=jass`; the code of a trigger
  that's off, or a comment, isn't code (often it's a readme), and its
  lines are read as empty; and it sees what World Editor's script
  declares for it, the triggers' `gg_trg_` and the variables' `udg_`,
  and what the map's other files place and name, as a build names them
  (`wc3_build::map::declared_names`: regions `gg_rct_`, cameras
  `gg_cam_`, sounds `gg_snd_`, destructables `gg_dest_`, and what
  `war3mapUnits.doo` places as `gg_unit_` or `gg_item_`, which it is as
  a build tells (`wc3_build::objects::Objects`: the game's tables, an
  item's in `Units\ItemData.slk`, and the map's object data over them;
  both names only without the game's files), read again when those
  files change. A name the map hasn't is unknown, as any other.

Of this, the view of `.trg` files, Extract, and editing a map's
`war3map.wtg` in place (its tree: new, renamed and deleted categories,
triggers and variables; a trigger picked in it edited as a `.trg`;
moved between categories; saved back), making `war3map.j` from a map's
files, **Convert to Custom Text**, and the way back are done. Convert to
Custom Text (the trigger's tools, `</>`; asked first) makes the trigger
custom text, its events, conditions and actions the game's JASS World
Editor's script has of it (`triggers::custom_text_code`: the build's,
`InitTrig_<name>` making `gg_trg_<name>`, a trigger run at map
initialization `run_on_init=1`): a map's in its piece of
`war3map.wct`, a `.trg`'s in the `.j` beside it (its locals and private
variables declared in it); its code shown after. One undo takes it back. The way back is the build's: a map
built from `.trg` files gets a `war3map.wtg` and a `war3map.wct` made of
them (`wtg_rs::text::from_files`), so World Editor opens it with the
triggers it runs: the custom script's header the map's, a trigger of
custom text's code the `X.j` beside it (else the map's), a comment's
text the map's. What World Editor's files can't hold (a variable
private or local to its file, a trigger outside a category ...) leaves
the map's own two as they are, and a note of the build says why.
Taken out of Diablo III beta and The Castle and built back, the triggers
are the map's again (the categories numbered anew), the custom script's
code too (server test `triggers_taken_out_with_their_code_build`).

The `.trg` format is free to grow: a new field, a new kind of line,
more than `UI\TriggerData.txt` describes, with nothing to keep in
step with World Editor's binary layout. **A new field is optional,
and its absence means what World Editor's trigger would be**: a
`[variable]` without `scope=` is a public global, as every one of
World Editor's is; so a file written before a field existed, or
extracted from a map, reads the same. A file holds one `[trigger]`:
a second is an error, not ignored, so nothing typed by hand is lost
without a word.

`war3map.wtg` itself still opens as its text and saves back exactly
(above).

A `.trg` is one trigger, kept as a file: the map as files in folders has
a file per trigger, and a folder per category. Its text is one
`[trigger]` of the text above, without `category=` (the folder is the
category), always of The Frozen Throne's version (lines inside others):

```ini
[trigger]
name="Spawn"
description="Creates footmen"
event=MapInitializationEvent()
action=CreateNUnitsAtLoc("1", "hfoo", Player00, GetRectCenter(GetPlayableMapRect()), RealUnitFacing)
action=ForLoopAMultiple("1", "10")
>action:0=DisplayTextToForce(GetPlayersAll(), "Hello")
```

A `.trg` may also declare the map's global variables, `[variable]`
sections as above (before or after its `[trigger]`); a file of them
alone holds the map's, `_variables.trg` at the top of the triggers'
folder. The view lists them (name, type, array size, initial value);
One project is one map, so its globals are the map's: every trigger
file of the project sees every variable any of them declares. A
variable's `scope=` narrows that:

| `scope=` | what it is | seen |
|---|---|---|
| (none) | a global of the map | by every trigger file of the project |
| `private` | a global of this file | by this file only; a build names it so that no other file's clashes |
| `local` | a local of this trigger's actions, which are one function | in the actions, every block inside included; not in the events and conditions |

```ini
[variable]
name="i"
type="integer"
scope=local
size=1
initial="0"
```

The actions are one function, every block too: a build makes the
blocks World Editor runs as functions of their own loops of it
instead (`ForGroupMultiple`: over a copy of the group, `FirstOfGroup`
and `GroupRemoveUnit`; `ForForceMultiple`: over the player slots,
`IsPlayerInForce`; `Enum...Multiple`: the items or destructables
gathered first, then a loop over them), so a local is seen in them
too, and *Picked unit* and the like are the loop's.

A local needs its file's `[trigger]`, and `scope=` is a `.trg`'s own (the
text of `war3map.wtg` has globals only). A parameter's dialog offers
what's seen where the parameter is (of its type), and a `$name` not seen
there shows as unknown, with a problem at its line: one no trigger file
declares, or a local outside its actions. **New variable** declares one in the file (name, type, scope, array
and size, initial value; a local brings the file's `[trigger]` when it
has none), after its last variable, and **Delete** on one removes
it. The trigger always shows: an empty file (a new one) is fine, and a
file without a `[trigger]` gets one with the first thing put in it.

**Extract Triggers…** (the button in the title of a `war3map.wtg`'s
text, or the Explorer's context menu of a `.wtg`) asks for a folder and
puts the map's triggers there as files: a folder per category (empty
ones too), a `.trg` per trigger, the variables in `_variables.trg`. The
game builds them top to bottom, so the order is in the names: a folder
is its category's number and name (`000_Init`), a file its trigger's
number in the category and name (`000_Spawn.trg`); the trigger's name
stays in its `name=`. Names a file can't have are made safe (`a/b?` is
`a_b_`); files already there are replaced after asking. It splits the text as it is in the editor, saved or not
(`wtg_rs::text::to_files`).

It opens as World Editor shows it (**Open as Text** and **Open as
Trigger** switch, as Reopen Editor With does): Events, Conditions and
Actions, each line its phrase from the editor's files
(`UI\TriggerData.txt`, `UI\TriggerStrings.txt` and its strings, found
like every game file, so in the game's language, and a modded editor's
in a root replaces them), each line with its category's icon and the
headings and blocks with the editor's own (World Editor's textures,
`ReplaceableTextures\WorldEditUI\*`, found the same way: the
categories' in `UI\TriggerData.txt`, the rest in
`UI\WorldEditData.txt`'s `[WorldEditArt]`), parameters colored by what they are (a preset,
a variable, a value, a call), what's inside an if/then/else or a loop in
its blocks (If - Conditions, Then - Actions, Else - Actions, Loop -
Actions), a disabled line dimmed. A function the files don't have shows
as its call, marked.

The view is the same document as the text, as the SLK table is: the text
stays what's saved, and what's done in the view the server turns into
edits of the text (`server/src/triggers.rs`), so undo, save and the
dirty mark are the text's. In the view:

- the name, the comment (`description`), **Enabled** and **Initially
  On** edit the trigger's fields. The name and the comment go into the
  text as they're typed, as any edit of a line does: it shows changed,
  and the text, the JASS and everything else follow it at once, saved or
  not (**Enter** leaves the name, **Escape** puts back what it was).
  The name isn't the file's: the file's name is only what an empty one
  means (shown in the empty box), and an empty name or comment leaves
  its field out; renaming either doesn't rename the other. What the
  map's script names the trigger by (`gg_trg_...`, in the name's
  tooltip) is World Editor's name of it, made of its name alone: `_`
  for each byte that can't be in a JASS name, so a Cyrillic letter is
  two, and a `u` after a `_` at the end (`wtg_rs::jass::ident`);
- references to triggers (`$gg_trg_<name>`) are checked and kept true.
  One that means no trigger of the project, or several (`Шляпа` and
  `Кепка` are both `gg_trg_` and ten `_` and `u`), is marked, and so is
  a trigger the script names as another. Once a name is done (**Enter**,
  or the focus gone; typed, it's in the text at once), the references
  that meant the trigger by its name before say its new one:
  `$gg_trg_...` in every trigger file, names in **Custom Script** (the
  JASS lexer's tokens, not in its comments or strings), and in the
  project's code (the names its index found), each file an edit of its
  own; nothing found by searching text. When the script names another
  trigger as it now, they're left as they are (marked, then) and it
  says so: never moved to mean another;
- **Ctrl+E** turns a line on or off, **Delete** deletes it (with what's
  inside), **Alt+Up** / **Alt+Down** move it in its block, and dragging
  moves it before or after another line, or to the end of a heading or a
  block; an action can't go where a condition goes;
- **Shift** with a click or **Up**/**Down** selects several lines, those
  beside each other in one heading or block from the line selected
  before (a far end in another block stops at the first's); **Ctrl**
  (**Cmd** on a Mac) with a click adds a line or takes it out, from
  anywhere. Copy, cut, delete and enable/disable take them all (enabled
  or disabled as the first becomes), paste goes after the last; dragging
  one of them drags them all, which land together in the text's order;
  **Alt+Up**/**Down** moves them a step when they're in one heading or
  block. A plain click on one of them selects it alone;
- **Ctrl+C**, **Ctrl+X** and **Ctrl+V** copy, cut and paste lines with
  what's inside them, through the clipboard as lines of the text (so
  they paste into a text editor too, and back): after the selected line,
  or at the end of the selected heading or block;
- a click on a parameter opens World Editor's dialog for it, of the
  parameter's type: a **Preset**, a **Variable** (the globals the
  project's `.trg` files declare), a **Function** (a call, filtered as
  you type; it comes with its default parameters, which are links in
  turn) or a **Value**, whichever the type has; an empty value shows its
  name (`Value`) so it can be clicked. A value of objects (a unit type,
  an ability, an item ...: `unitcode`, `abilcode`, `heroskillcode` (a
  hero's), `buffcode`, `destructablecode`, `doodadcode`, `itemcode`,
  `techcode` (a unit or an upgrade); `names::OBJECT_TYPES`) shows as its
  object's name (`hfoo`: Footman), and its **Value** is the objects to
  pick, by name with the id beside, filtered as you type (an id typed
  that none has is taken as it is). The names are World Editor's, from
  the game's tables and profile (`w3abdhqtu_rs::ObjectNames`), with the
  map's object data over them (its custom units by their names, a
  `TRIGSTR_` from `war3map.wts`), colors left out. **Custom Script**'s value is lines
  of JASS (a box with Tab and **Ctrl+Enter** for OK): one line of the
  text still, the lines `\n` in its string, and a build writes each
  indented as the action is (World Editor's is one line; there's no way
  back to it, so it needn't be);
- **New** (+, **Ctrl+Shift+N**) adds an event, condition or action
  after the selected line or at the end of the selected heading or
  block: a category, then a function, its hint below; it comes with its
  default parameters (`UI\TriggerData.txt`'s `_Defaults`);
- a click on a line selects it, a double click (or **Enter**) opens its
  dialog, as World Editor's does (a click on a parameter is the
  parameter's, at once): the function it calls, picked as for **New**; another
  one comes with its default parameters, keeps the line's place and
  whether it's on, and keeps what's inside only in the blocks it has
  (an if/then/else's Then - Actions don't survive becoming a loop). A
  double click on a variable opens its own: name, type, scope, array and
  size, initial value, rewritten in place. A double click on a heading
  or a block folds it. **Show in Text** in either dialog shows the line
  in the text;
- changing or deleting a variable is safe where it can be and asks where
  it can't. Its uses are the `$name`s that mean it: in its file (a
  local's in the actions only) and, for a global, in every other trigger
  file of the project that has no variable of that name of its own.
  Renamed, they're renamed with it, in every file; a size or an initial
  value changes nothing else. Nothing is found by searching text: a use
  is a `$name` the trigger reader parses (`wtg_rs::text`'s tokens, so a
  string or a comment that says `$name` isn't one), and a name in code
  is a name token, the JASS lexer's in a `scriptcode` parameter, the
  index's (each language's parser) in the project's files. What
  breaks a use is shown first, each place with why, and done only once
  confirmed: deleting a used variable, a narrower scope (a private one
  isn't seen by other files, a local by the events and conditions),
  another type where a parameter takes the old one (or `SetVariable`
  gives it a value of the old one), becoming an array or no longer one,
  and custom script (a **Custom Script** in a trigger file, the
  project's JASS, Zinc, Lua ...) that names it in the map's script (`udg_<name>`,
  a private one's `udg_<trigger>__<name>`, a local's `<name>`) when that
  name changes or goes. After, **Show Places** goes to them one by one.
  A new name or scope that clashes is refused: another file's global of
  that name, or a private or local one that would hide a project's
  global;
- arrows move and fold, and the problems count goes to the first one.

**Show JASS**, **Show Lua**, **Show AngelScript** (the view's buttons,
each its language's icon with a tooltip of the page's own, styled as the
editor's hovers; the editor's title; the Explorer's context menu of a
`.trg`) show what the trigger is built into, each beside it (`<name>.j`,
`.lua`, `.as`; a map's `war3map.wtg.j` ... of the picked trigger),
analyzed in its language. Lua and AngelScript are that JASS made by
`wc3-build` as a build makes it (`triggers::converted`; AngelScript with
the project's `common.j` in `Jass::`), the trigger's own code only (the
`Blizzard.j` functions it calls a build writes once, for the whole map);
what's wrong with it (an `ExecuteFunc` that can't be worked out) is said
at its top. Show JASS shows the JASS the trigger is built into,
beside it and read-only, following the text as it changes, saved or
not (the text is the one truth; the view is only another way to edit it)
(`wtg_rs::jass`). It's this toolchain's JASS, which the build makes the
game's of (`jass-rs`'s README): no `set`, `call` and `local`, no
`takes nothing returns nothing`, globals at the top level. Functions are named as World
Editor's (`Trig_<name>_Conditions`, `Trig_<name>_Actions`; the `<name>`
World Editor's, see above and `wtg_rs`'s README); its `InitTrig_<name>`
is a `main` of the trigger's own, which the build merges with every
other, and the trigger is made where it's declared:

```jass
trigger gg_trg_Gate1 = CreateTrigger()

function main
    TriggerRegisterDeathEvent(gg_trg_Gate1, gg_dest_B000_0261)
    TriggerAddAction(gg_trg_Gate1, function Trig_Gate1_Actions)
endfunction
```

The file's globals are declared (`udg_<name>`, a private one
`udg_<trigger>__<name>`) and its locals at the top of the actions; a
preset as its code (`bj_UNIT_FACING`), a function by its script name
(`_Name_ScriptName`), an object's id in single quotes (`'hfoo'`); the
loops of `ForGroupMultiple`, `ForForceMultiple` and `Enum...Multiple`
inside the actions, *Picked unit* and the like the loop's (a
`ForGroupMultiple`'s loop takes `FirstOfGroup` until it's `null`: the
group's first unit that's still there, so a removed one doesn't end it
early). What's disabled isn't in it. It isn't a file of the project,
but the server analyzes it as JASS without a file: its highlighting
and its diagnostics (lint: an unused local, say), in the Problems panel
while it's open.

While it's edited, the text gets its errors (a field that isn't there,
a call that doesn't read), and the view counts them with what
`UI\TriggerData.txt` says (a function it doesn't have, a parameter too
many).

## txt2

XGM's markup (`.txt2`, articles like `w3-file-format.txt2`) is
highlighted by the server with [`txt2-rs`](../txt2-rs/README.md): bold
as bold, accents in their colors, headings, links, code, block tags.
Unclosed blocks (`((cut` or `((кат` without `))`, `>>` without `<<`) are
errors. The site's keywords are Russian; each has an English twin
(`((code`, `((layout` …), read the same.
**Open Preview to the Side** (the button in the editor's title,
`Cmd+K V`; `Cmd+Shift+V` in place) shows the page as the site would,
updated as you type; the preview and the text follow each other's
scrolling, a double click in the preview puts the cursor on that line,
and links open in the browser.

## SLK tables

A `.slk` (the game's data tables: `Units\UnitBalance.slk`, the
`*MetaData.slk`, ...) opens as a table to edit. **Open as Text** (the
editor's title bar, or Reopen Editor With) shows the text, highlighted and
checked by [`slk-rs`](../slk-rs/README.md): records, cell values, formulas,
comments; parse errors and lint as you type. **Open as Table** (the title
bar, the Explorer's context menu) goes back.

The table is the same document: the text stays what's saved, so undo,
save and the dirty mark are the text's. The server reads the sheet from
the text and turns a written cell into the fewest text edits
(`server/src/sheet.rs`, `slk_rs::set_cell`): the cell's `K` changes, a new
cell gets a record in reading order, a cleared one loses its record, and
the records around keep their place (`X` and `Y` carry over from record to
record). Typed input becomes a number, `TRUE` / `FALSE`, an error like
`#VALUE!`, or text; a text cell stays text, and `"quoted"` input is always
text.

In the table:

- the column letters, row 1 (the field names; **Row 1 as header**), the
  row numbers and column A (**Freeze column A**) stay in view;
- **Find rows** keeps the rows with a match and marks it;
- arrows, Page Up / Down, Home / End (with Ctrl / Cmd: the edges) move;
  Enter, F2, a double click or typing edits; Enter / Tab write and move,
  Escape cancels; Delete clears;
- copy and paste a cell, or a tab-separated block from a spreadsheet;
- numbers are right-aligned, `TRUE` / `FALSE` and errors colored, a cell
  with a formula italic, one with a comment marked; hovering shows a cell's
  whole text, formula and comment.

It's drawn on a canvas, only what's in view (`webview/slk.js`), so a table
of 50,000 cells scrolls like a small one.

## World Editor

The map's world (terrain, cliffs, water, doodads, units, regions,
cameras, pathing) seen and edited in a page of VS Code, the way World
Editor and [HiveWE](../HiveWE-main) show it. HiveWE (C++, Qt, Vulkan) is
the reference for *what* to compute (tile variations, cliff meshes,
water colours, the camera); none of its code is taken as is.

### Shape

- **Rust does the work, the page draws.** A process per editor, `wc3
  editor --project <root>` (a mode of the one binary, see "One binary" under MCP),
  holds the map open: it reads every file through the project's lookup
  (`game::Lookup`: game folders, then the roots, last wins), decodes
  textures (BLP) and models (MDX), works out what is drawn (corner
  heights, each tile's ground layers, cliff and doodad instances) and
  applies every edit (brushes, placement, undo). The page
  (`extension/webview/editor/`) is WebGL2 and input: it uploads what it
  is sent and sends back what the user does (a brush stroke at a
  point), never edits the map itself.
- **The editor protocol.** The same framing as `server/src/protocol.rs`
  (`u32` length | `u8` kind | body, little-endian), its own kinds,
  documented in its Rust module, its JavaScript twin in the page. The
  extension passes frames between the process and the page as bytes,
  without reading them: big arrays (heights, RGBA, vertices) go to the
  page as they come, with no JSON on the way.
- **One project, one map, one editor.** An editor is per project
  (`wc3.bni`): opening it again shows the one open. Across VS Code
  windows too: a lock (the process's pid) under the system's temp
  folder, keyed by the project's path; a second window says where it is
  open.
- **The files are the truth.** The editor reads the files the build
  reads (development is hybrid, see Triggers): a file in
  a folder root and the same file inside a `.w3x` root are layers like
  anywhere else. An edit is written to the layer the file comes from
  (a folder root: the file; a map root: inside it); a file only the
  game has goes to the top root. A change on disk (VS Code, git, the
  language server) is read again. The editor's unsaved state and an
  open text form of the same file (`war3map.doo` as text ...) stay in
  step: what that means exactly is decided at step 7.
- **Code.** The editor's state and protocol: a package of this
  repository, `editor/` (lib `wc3_editor`, over `wc3_server`'s project
  model), run by `cli` as `wc3 editor`. MDX models: a crate of their own,
  `../mdx-rs` (`/Volumes/TrascendSSD/Work/mdx-rs`), a repository like
  every other format's.
- **MCP tools where they make sense.** What the editor works out that an
  assistant can use without the page (a model's summary, a picture of
  the terrain, what stands at a point, a brush applied to the files)
  becomes a tool of `wc3 mcp` too, over the same Rust code; each step
  says which ones it adds.

### In the extension

- A view **WC3 Editors** beside **WC3 Maps**: every project (`wc3.bni`)
  in the workspace, its folder's name, and whether its editor is open; a
  click opens it or shows it; a menu closes it.
- A button over an open `wc3.bni` (beside Build), the Explorer's menu of
  a `wc3.bni`, and the command palette (the active file's project):
  **Open World Editor**.
- Every new piece gets a step in `extension/e2e/suite.js`.

### Steps

Each step ends working: built, tested (Rust tests on `../maps`, an e2e
step), this list updated.

1. **The shell** (done). `wc3 editor` with the editor protocol: open the
   project, answer what the map is (name from `war3map.w3i`, size,
   tileset) or why it can't be opened; errors as messages, never a
   crash. The extension: the WC3 Editors view, the commands and the
   button, one panel and one process per project (closing the panel ends
   the process; a crashed process is said in the panel with a way to
   reopen), the cross-window lock. The page: an empty canvas and the
   map's name. As built: `editor/` (`lock.rs`: an OS file lock, so a
   crash leaves none; `session.rs`; `protocol.rs`: `Hello`, `Map`,
   `Error`), `extension/world-editor.js`, `extension/webview/editor/`;
   no MCP tool (nothing yet that the files' tools don't give).
2. **Terrain** (done). `war3map.w3e` through the lookup; the tileset's ground
   textures by `TerrainArt\Terrain.slk` (never a hardcoded list), BLPs
   decoded in Rust, each sent once and cached by the page; per tile up
   to four ground layers with their variations (HiveWE's
   `get_tile_variation` and blending order), corner heights with the
   layer height; the camera (HiveWE's `camera.ixx`: pan, zoom, rotate,
   the game's angle); a grid. Water: the flat surface at its height,
   colours from `TerrainArt\Water.slk`. Boundary and blight flags shown.
   As built: `editor/src/tileset.rs` (the tables), `terrain.rs` (heights,
   each tile's four layers as `u32` texture | cell << 16, corner and tile
   flags: a cliff tile, a ramp's foot), `textures.rs` (every ground
   texture cut into its cells, one array of layers of the biggest cell's
   size; the water's frames), messages `Terrain`, `Ground`, `Water`; the
   page's `camera.js` and `terrain.js` (WebGL2: heights and layers in
   textures read with `texelFetch`, a quad per tile drawn instanced). A
   cliff tile is still drawn as a slope of ground until step 3 puts its
   model there and leaves its ground out (`tile::CLIFF`,
   `RAMP_ENTRANCE` say where). MCP: `w3e_image`, the terrain from above
   (`editor/src/picture.rs`), in `wc3-mcp`.
3. **Cliffs** (done). `../mdx-rs` reads geosets and textures (enough for a
   static mesh); `TerrainArt\CliffTypes.slk`; each cliff tile's model
   picked by its corners' layers (`Doodads\Terrain\Cliffs\Cliffs<ABCD><n>.mdx`,
   as HiveWE's `terrain.ixx`), ramps, the cliff texture per tile.
   As built: `mdx-rs` reads every chunk (geosets, materials' layers,
   textures, sequences; the rest as bytes) and writes each model back
   byte for byte (all 3320 of 1.26a); `editor/src/cliffs.rs` places
   ramps and cliffs as HiveWE's `update_cliff_meshes` does (column by
   column, a ramp marking its tiles `tile::ROMP` first), each model in
   its cliff type's folder (`cliffModelDir`, `rampModelDir`: `Cliffs`,
   `CityCliffs` ...), a variation the game has (its files say which, no
   table); the `Cliffs` message: the cliff types' textures, each
   model's first geoset and where it stands; the page draws them
   instanced (a quarter turn round, each vertex on the ground under it,
   HiveWE's `cliff.vert`) and no ground under them. MCP: `mdx_info`.
4. **Doodads and units** (done). `war3map.doo` and `war3mapUnits.doo`; their
   models from the game's tables with the map's object data
   (`.w3d`/`.w3b`/`.w3u`, the obj pipeline the server already has);
   drawn instanced, at their first `Stand` frame; picking and selection
   (Rust answers what is under a point).
   As built: `editor/src/types.rs` (a type's fields: the game's
   tables and `Units\ItemFunc.txt`, the map's object data over them,
   each change under its field's key by the metadata), `placed.rs`
   (each thing's model as HiveWE finds it, `placed::model_of`: a
   doodad's variation after its name, a destructable's in place of the
   extension, its `texFile` for its replaceable texture; a unit's or
   item's `file`; geosets hidden at the start of `Stand` left out, as
   `mdx-rs`'s `GEOA` says; textures at 256 or less; team colour the
   average of `TeamColor<nn>.blp`; units on the ground, doodads at their
   height; `Objects::pick`, a ray against each thing's box), messages
   `Objects` and `Pick`/`Picked`; the page's `objects.js` (instanced,
   the game's filter modes, the picked one lighter, said under the
   view). Models are drawn as the file has their vertices: no bones,
   so no animation and no pose (step 8); start locations (`sloc`) have
   no model in the tables, said in a note. MCP: `doo_list` names each
   thing and its model with the game's files
   (`wc3_server::names::with_objects`).
5. **Overlays** (done). Pathing (`war3map.wpm`), regions (`war3map.w3r`),
   cameras (`war3map.w3c`), the map's bounds (`war3map.w3i`), a
   minimap.
   As built: `editor/src/overlays.rs`: the pathing (`war3map.wpm`, a
   byte a cell) and the regions (`war3map.w3r`, a picture of four
   pixels a cell, filled and outlined) laid on the ground and the
   cliffs by their shaders; the bounds (`war3map.w3i`: the camera's,
   the playable area) and the cameras (`war3map.w3c`: from where each
   is to where it looks) as lines on the ground; the minimap
   (`picture::paint`, units in their owners' colours); `ground_hit`,
   where a ray meets the ground: a click there says the point and its
   regions, a thing picked says its regions too. The page's
   `overlays.js` and buttons (and keys P, R, C, B, M), the minimap a
   click moves the camera on. MCP: `w3e_image`'s `show` (regions,
   units, bounds: `picture::mark`).
6. **Editing.** In three parts:
   - **6a, terrain brushes** (done): raise, lower, plateau, smooth,
     ground texture, cliff level up and down, water and dry, each laid
     in Rust (`editor/src/edit.rs`) where the mouse's ray meets the
     ground, a step at a time while the button is held (the next once
     the last is answered); undo and redo, a stroke each; save
     (`Session::save`): `war3map.w3e` into the layer it came from (a
     root folder's file, inside a root map through `MapFs`), one only
     the game has into the top root. The `Brush`, `StrokeEnd`, `Undo`,
     `Redo`, `Save` messages; `Palette` (the tileset's ground textures,
     `[TileSets]` after its name and blight, with pictures; the
     terrain's cliff types), `State` (unsaved, undo, redo, what a save
     said); after an edit the terrain's messages again (`Cliffs`
     without textures keeps the page's). The page's tool panel, the
     brush's ring, Ctrl+Z, Ctrl+Shift+Z, Ctrl+S, the tab's `●`; closing
     it with changes not saved says they're lost. What the terrain
     changes doesn't follow yet: the bounds' lines,
     the minimap, and the files World Editor makes with the terrain
     (`war3map.wpm`, `war3map.shd`): step 7. The whole terrain is
     worked out again after each step (fast enough for 193 by 129;
     the changed rectangle alone, when a bigger map needs it). MCP:
     `w3e_brush`.
   - **6b, things placed** (done): the object palette (every type of
     doodad, destructable, unit and item, the game's and the map's
     object data's, named as World Editor names them, with its group:
     a unit's race, a doodad's category, an item's class; those it
     hides left out), a search, the owner of a unit; Place lays the
     type where a click meets the ground (facing 270°, a doodad's
     variation by chance, a number no other has); the thing picked is
     dragged to move it, Q and E turn it, Delete takes it away; each
     an undo (a drag one), with the terrain's in one list
     (`edit::Snapshot`: the terrain, the doodads, the units); Save
     writes each file changed (`war3map.w3e`, `war3map.doo`,
     `war3mapUnits.doo`) into its layer. The session keeps the files
     (`placed::Files`) and the models read: an edit places everything
     again (some 30 ms for 13 000 things) and sends where each model
     stands (`Instances`) and only the models the page hasn't
     (`Objects` from `first_model`); so units stand on the ground as a
     brush leaves it. Messages `Place`, `Move`, `Rotate`, `Delete`,
     `Instances`, `ObjectPalette`; `Picked` says the thing's file and
     index. MCP: the `doo_*` tools already place, change and delete in
     the files.
   - **6c, regions and pathing**: regions drawn, moved, resized,
     renamed (`war3map.w3r`); the pathing brush (`war3map.wpm`).

7. **In step with the rest.** Disk changes read again piece by piece;
   the editor and open text forms of the same file; the language server
   and the build see what the editor saved.
8. **Later.** Animations and particles, lighting and shadows, the
   terrain's resize, the tile setter and pather, Reforged HD assets.

## MCP

`.mcp.json` registers [`wc3-mcp`](../wc3-mcp/README.md) with Claude Code in
this repository: an MCP server that reads, queries and edits the game's
SLK tables, from disk or from inside an MPQ archive or a map, and looks
into and changes MPQ archives and maps (list, read, extract, verify, add,
remove, rename), object data (list, read, text, check, set, add and
remove objects; `TRIGSTR_<n>` names as their text) and string tables
(`war3map.wts`: list, read, resolve, set), on the same `slk-rs`,
`storm-rs`, `w3abdhqtu-rs` and `wts-rs` as the editor. It runs the
binary the extension ships, `extension/bin/wc3-macos mcp`, so it's the
same as the editor's: build it first (`server/build-mac.sh`, or
`server/build.sh` for every system; on Linux or Windows point `.mcp.json`
at `wc3-linux` or `wc3-windows.exe`), and restart Claude Code after a
build to have the new one. Claude Code asks once whether to use it.

**One binary.** The extension ships one binary per system, `wc3`
(`cli/`, over the server's library and `wc3-mcp`'s), a mode a run:
`wc3 server` is the language backend the extension spawns, `wc3 mcp` the
MCP server. `server/build.sh` cross-compiles it into `extension/bin/`
(`wc3-{macos,linux,windows.exe}`), and the extension registers `wc3 mcp`
with VS Code (`contributes.mcpServerDefinitionProviders`, id `wc3`,
`registerMcp` in `extension.js`), so chat agents in VS Code get its tools
with nothing to install. VS Code starts it in the first workspace folder;
without a packaged binary it takes the dev build,
`cli/target/{release,debug}/wc3`. `wc3-server` and `wc3-mcp` still build
alone, for tools and tests.

**Releases.** `server/publish.sh` builds the binaries, publishes the
extension (which bumps the version in `extension/package.json`), commits
and pushes that bump as `v<version>`, and makes a GitHub release of that
name on it with the same three binaries, for MCP clients
without the extension: `wc3-<os> mcp` from
`https://github.com/WarRaft/wc3-rs/releases/latest/download/wc3-{macos,linux,windows.exe}`.

Its tools read the game's files (names, `UI\TriggerData.txt`, metadata)
the way the editor does for the same file: through `game` and `roots`
when a call gives them, else through the settings of the project the file
is in (the nearest `wc3.bni` above it, or `project`), with the same rule
for a map that isn't a root (`game::Lookup::for_file`, shared with
`Workspace::lookup`). `project_info` shows a project's settings, problems
and where the editor's files and their patches come from;
`project_write` checks a `wc3.bni` as the editor reads it before writing
it.

Planned for it: checking JASS (syntax, names, order as the game reads a
map's `war3map.j`), lint, listing natives with their comments, and
renaming symbols across the project. For that the server is a library as
well as the binary (`wc3_server`, `server/src/lib.rs`: the workspace,
`index::answer`, analysis, the game's files), which `wc3-mcp` uses
in-process: see the [`wc3-mcp` README](../wc3-mcp/README.md#planned-jass).

## String tables

A `.wts` (`war3map.wts`, `war3campaign.wts`: the text a map's object data,
triggers and info name as `TRIGSTR_<n>`) is highlighted and checked by
[`wts-rs`](../wts-rs/README.md): `STRING` headers and their numbers,
World Editor's comments, the text, and its color codes (`|cffff0000`,
`|r`, `|n`); parse errors and lint (`duplicate-string`) as you type. A
map's `war3map.wts` is one of the project's files like its scripts.

**Hovering over `TRIGSTR_<n>`**, in any file (`war3map.j`, the text of
object data, ...), shows string `n`'s text from the map's
`war3map.wts`: the one next to the file, in the same folder or inside the
same map. The text shows its colors (`|cffffcc00...|r`) and line breaks,
and under it World Editor's comments say what uses the string. A table
the editor has open is read as it is, unsaved edits and all
(`server/src/trigstr.rs`).

Files are read without their byte-order mark (World Editor writes one in
`.wts` and other files), as the editor holds them, so positions agree.

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

**Names and types are checked across the project** (`unknown-name`,
`argument-count`, `type-mismatch`, `array-access`,
`constant-assignment`, and `array-struct`'s calls; `jass_rs::names`): each JASS file against what every
file it sees declares, its project's and the game's scripts it runs with
(`common.j`, `Blizzard.j` or `common.ai`, through `game=` and the
roots; see [Projects](#projects)), and the names other languages declare
project-wide (a public Zinc function is known, its arguments not
counted). When a file's declarations change, every JASS file is checked
again, so a problem between files shows and goes as it's typed. Only
where the game's `common.j` is found: without it every native would be
unknown. The game's scripts and a map's own `war3map.j` aren't checked.
`wc3-mcp`'s `code_check` gives the same for a whole project.

A **group** switches several rules at once: `pure=error` sets every rule
of the group `pure`. A rule set by name wins over its group, wherever
the lines are, so `pure=error` with `implicit-signature=off` keeps that
one rule off.

| group  | rules                | for |
|--------|----------------------|-----|
| `pure` | `implicit-signature`, `implicit-keyword`, `merged-function`, `declaration-after-function`, `global-outside-globals` | pure JASS as the game reads it: reports what this toolchain accepts beyond the game's grammar. Off by default |
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
| `empty-branch`                   | JASS, Zinc, Lua, AngelScript | on      | an empty `then` / `else` / `if` body; an empty `else` is marked on `else` and has a quick fix that removes it |
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
| `unknown-name`                   | JASS                         | on      | a function, variable or type declared nowhere: in the file, the files of its project, the game's scripts it runs with (see below) |
| `argument-count`                 | JASS                         | on      | a call with more or fewer arguments than its function takes, or `function F` (a code value) of one that takes some |
| `type-mismatch`                  | JASS                         | on      | a value of the wrong type: an argument, a value set or returned, a condition, an operator's side |
| `array-access`                   | JASS                         | on      | an array used without its index, or an index of what isn't an array |
| `constant-assignment`            | JASS                         | on      | a constant set |
| `array-struct`                   | JASS                         | on      | what a struct `extends array` can't have or do, its instances only numbers that nothing creates or destroys: a member's default value, an array member, `onDestroy` (its own, or a module's it implements: said at the `implement`); `allocate`, `create`, `destroy`, `deallocate` not its own or a module's |
| `implicit-signature`             | JASS                         | **off** | a `function` or `native` without `takes` or `returns`, which the game requires (group `pure`) |
| `implicit-keyword`               | JASS                         | **off** | an assignment without `set`, a call without `call` or a local without `local`, which the game requires (group `pure`) |
| `merged-function`                | JASS                         | **off** | more than one `main` or `config`, which the build merges into one; the game reads one (group `pure`) |
| `declaration-after-function`     | JASS                         | **off** | a global, `globals` block, `native` or `type` after a function; the game reads them only before the first one (group `pure`) |
| `global-outside-globals`         | JASS                         | **off** | a variable at the top level outside `globals ... endglobals`, where the game doesn't read it (group `pure`) |
| `object-value-type`              | Object data                  | on      | a value stored as another type than the game's metadata gives the field (a number for a string, a type written out that isn't the field's: `int 6` for an unreal); an error, the game misreads it. Quick fix: the right type |
| `object-value-range`             | Object data                  | on      | a number outside World Editor's limits for the field, or a flag other than 0 or 1 |
| `object-unknown-field`           | Object data                  | on      | a field the game's metadata doesn't have, like one of a newer game version |
| `object-field-level`             | Object data                  | on      | a level on a field without levels, none on one with, another data column than its own. Quick fix: the right key |
| `object-field-owner`             | Object data                  | on      | a field of other objects: an ability's data of another ability (by its code), a units-only field of an item |
| `duplicate-cell`                 | SLK                          | on      | a cell whose value or formula two records set; the later one wins |
| `cell-outside-bounds`            | SLK                          | on      | a cell past the size the `B` record gives, which readers that trust `B` lose |
| `duplicate-string`               | WTS                          | on      | a string number defined twice in a string table (`war3map.wts`) |

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
- it's what World Editor's script makes of a map's triggers: a name
  declared only in a `war3map.j` with a `war3map.wtg` beside it (a
  variable's `udg_`, a trigger's `gg_trg_` and its GUI functions, a
  placed thing's `gg_unit_` ...: made again from the triggers when the
  map is saved or built), or a trigger's `InitTrig_<name>` in the custom
  script (called by the trigger's name). The trigger or variable is
  renamed instead. A name the custom script declares and that
  `war3map.j` copies is renamed in both, from either. The MCP's
  `jass_rename` is this rename.

When some of the places are inside maps, the editor asks first: renaming
there rewrites the map. **Rename** changes them all, **Rename Outside
Maps** only the files on disk, **Cancel** nothing.

An AngelScript class is renamed together with its constructors and
destructor; F2 on a constructor renames the class.

Nothing is cached beyond the symbols themselves: a query scans what it
needs. On the game's own `Scripts` folder (293 files: `common.j`,
`Blizzard.j`, the AI and campaign scripts) loading takes about 80 ms and a
query about 1 ms.

## Tests in the editor

`cd extension && npm run e2e [map.w3x]` (`e2e/run.js`) runs the
extension in a VS Code of its own: the one installed (`VSCODE`, else
`/Applications/Visual Studio Code.app`), with a profile and extensions
folder of its own in a temporary folder (no other extension; it opens
a window of its own for the seconds it runs), this extension from its
folder (the language server is `extension/bin/`'s: `server/build-mac.sh`
first), on a copy of a map (Diablo III beta by default) in a folder with
a `wc3.bni` naming the game (`WC3_GAME`). `e2e/suite.js` drives it as a
user would: the trigger editors take the page's messages through the
extension's `test` export (`triggerEditors`), and each step is checked
on the documents, diagnostics and the map on disk (the window fills the
screen, with no chat or tips: its profile's settings); `E2E_SUITE=probe.js
E2E_OPEN='wc3map://{map}/war3map.w3i|...'` only opens each and takes a
screenshot, to look at something; what it says goes to
`<out>/log.txt` (`E2E_OUT`, else a temporary folder), printed at the
end, and screenshots of the screen to `<out>/*.png` (`run.js` takes them,
with macOS's `screencapture`: the terminal needs Screen Recording).
Now: opening `war3map.wct` brings up the triggers, its problems have
none of World Editor's names, a trigger of custom text opens its code
alone, typing there changes its piece of `war3map.wct` and no other,
a diagnostic shows on its line in the piece, moving the trigger moves its
code, saving the piece writes the map. What only the page does is asked
of the page (the editor's `page(message)`, its answer in `state()`): the
line beside the trigger tree is dragged with the pointer's events a mouse
makes (`e2eSplitter`: the tree follows it halfway and at the end, its
width kept with the page, a double click back to its first width).
