# Icons

The extension's file icons, one per kind of Warcraft III file, all drawn
in one style by `make.py` (run `python3 make.py .` here to write them
again): a 32×32 grid, flat shapes, one color per icon (the Material
palette), white or lighter parts for detail.

- **Code** is a rounded badge with the language's mark: `jass` (`.j`,
  `.ai`, `.pld`), `vjass` (`.vj`), `zinc`, `lua`, `as` (AngelScript),
  `wct` (a map's custom script).
- **Text data** is a sheet with a folded corner and what it holds: `bni`
  (`.bni`, the game's and a map's BNI `.txt`), `slk` (tables), `wts`
  (strings), `fdf` (frame definitions), `toc` (their list), `trg` (a
  trigger of its own, `wtg`'s bolt).
- **Object data**: `w3u` units (a helmet), `w3t` items (a potion), `w3a`
  abilities (a spark), `w3h` buffs (an aura), `w3q` upgrades (an arrow
  up), `w3b` destructables (a crate), `w3d` doodad types (a tree).
- **A map's parts**: `w3e` terrain, `w3i` map info, `w3r` regions, `w3c`
  cameras, `w3s` sounds, `wpm` pathing, `wtg` triggers, `shd` shadows,
  `mmp` minimap icons, `imp` imports, `w3o` object collection, `doo`
  doodads, `units` (`war3mapUnits.doo`), `wai` AI editor data, `preview`
  (`war3mapPreview.tga`, `war3mapMap.blp`).
- **Archives**: `w3x` and `w3m` maps, `w3n` campaigns, `w3f` campaign
  info, `mpq`.
- **Media and the game's own**: `blp` and `tga` images, `mdx` models
  (`.mdx`, `.mdl`), `audio` (`.wav`, `.mp3`, `.mid`), `w3g` replays,
  `w3z` saved games, `w3v` game caches, `bin` any other binary.

Each is a language's icon in `package.json` (`contributes.languages`), so
VS Code shows it for the file whatever icon theme is on; a kind the
extension doesn't read is a language only for its icon.
