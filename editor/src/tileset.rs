//! What the game's tables say about a terrain, read through the project's
//! lookup like any other file (a root can replace any of them; nothing is
//! hardcoded):
//!
//! - `TerrainArt\Terrain.slk`: each ground tile's texture (`dir`, `file`);
//! - `TerrainArt\CliffTypes.slk`: each cliff's ground tile (`groundTile`),
//!   the texture under a cliff;
//! - `TerrainArt\Water.slk`: a tileset's water (`<letter>Sha`): its height
//!   over the water level, its animated texture, its colours by depth;
//! - `UI\WorldEditData.txt` `[TileSets]`: a tileset's blight texture (its
//!   second field).

use std::collections::HashMap;

use wc3_server::game::Game;

/// A SLK table's rows by their first column, each a map of column name
/// (lower case) to text.
#[derive(Debug)]
pub struct Slk {
    rows: HashMap<String, HashMap<String, String>>,
}

impl Slk {
    /// `name` through the lookup; `None` when no layer has it.
    pub fn read(game: &Game, name: &str) -> Option<Slk> {
        let (bytes, _) = game.read(name)?;
        Some(Slk::parse(&String::from_utf8_lossy(&bytes)))
    }

    pub fn parse(text: &str) -> Slk {
        let table = slk_rs::Table::new(&slk_rs::parse(text));
        let names: Vec<String> = (1..=table.cols()).map(|c| table.text(1, c).trim().to_ascii_lowercase()).collect();
        let mut rows = HashMap::new();
        for row in 2..=table.rows() {
            let id = table.text(row, 1).trim().to_string();
            if id.is_empty() {
                continue;
            }
            let cells = names.iter().enumerate().map(|(c, n)| (n.clone(), table.text(row, c as u32 + 1).trim().to_string())).collect();
            rows.insert(id, cells);
        }
        Slk { rows }
    }

    /// The text of `column` (any case) in the row of `id`; `None` for a
    /// row or column it hasn't, or an empty cell.
    pub fn get(&self, id: &str, column: &str) -> Option<&str> {
        self.rows.get(id)?.get(&column.to_ascii_lowercase()).map(String::as_str).filter(|s| !s.is_empty())
    }

    /// Every row: its id and its cells by column (lower case).
    pub fn rows(&self) -> impl Iterator<Item = (&String, &HashMap<String, String>)> {
        self.rows.iter()
    }

    /// [`get`](Self::get) as a number.
    pub fn number(&self, id: &str, column: &str) -> Option<f32> {
        self.get(id, column)?.parse().ok()
    }
}

/// A tileset's water (`Water.slk`).
#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    /// Over the points' water level, in layers (128 units).
    pub height: f32,
    /// The animation's frames: `<texFile>00` ... `.blp`, `count` of them,
    /// `rate` a second.
    pub texture: String,
    pub count: u32,
    pub rate: f32,
    /// RGBA, 0-255: shallow at its least and most depth, deep at its least
    /// and most.
    pub colors: [[u8; 4]; 4],
    /// The first frame's average colour (what `colors` tint in the game),
    /// when it's found.
    pub average: Option<[u8; 3]>,
}

/// The game's data a terrain is drawn with.
#[derive(Debug, Default)]
pub struct Tileset {
    pub terrain: Option<Slk>,
    pub cliffs: Option<Slk>,
    pub water: Option<Water>,
    /// The blight texture's path, with its extension.
    pub blight: Option<String>,
    /// The tileset's ground tiles (`[TileSets]`, after its name and
    /// blight), as World Editor's terrain palette offers them.
    pub tiles: Vec<String>,
    /// Its letter (`L`).
    pub letter: u8,
    /// What was missing.
    pub notes: Vec<String>,
}

impl Tileset {
    /// The tables of tileset `letter`.
    pub fn load(game: &Game, letter: char) -> Tileset {
        let mut notes = Vec::new();
        let mut table = |name: &str| {
            let slk = Slk::read(game, name);
            if slk.is_none() {
                notes.push(format!("No {name} in the game's files or the roots."));
            }
            slk
        };
        let terrain = table("TerrainArt\\Terrain.slk");
        let cliffs = table("TerrainArt\\CliffTypes.slk");
        let water = table("TerrainArt\\Water.slk").and_then(|w| water(&w, letter)).map(|mut w| {
            w.average = crate::textures::average(game, &with_extension(&format!("{}00", w.texture)));
            w
        });
        let fields: Vec<String> = game
            .read_patched("UI\\WorldEditData.txt")
            .and_then(|text| {
                let doc = bni_rs::parse(&text);
                let section = doc.section("TileSets")?;
                let item = section.items.iter().find(|i| i.key.as_ref().is_some_and(|k| k.text.trim() == letter.to_string()))?;
                Some(item.fields.iter().map(|f| f.text().trim().trim_matches('"').to_string()).collect())
            })
            .unwrap_or_default();
        let blight = fields.get(1).filter(|p| !p.is_empty()).map(|p| with_extension(p));
        let tiles = fields.iter().skip(2).filter(|t| t.len() == 4).cloned().collect();
        Tileset { terrain, cliffs, water, blight, tiles, letter: letter as u8, notes }
    }

    /// Ground tile `id`'s texture path (`dir\file.blp`).
    pub fn ground_path(&self, id: &str) -> Option<String> {
        let t = self.terrain.as_ref()?;
        Some(with_extension(&format!("{}\\{}", t.get(id, "dir")?, t.get(id, "file")?)))
    }

    /// The ground tile under cliff `id`.
    pub fn cliff_ground(&self, id: &str) -> Option<&str> {
        self.cliffs.as_ref()?.get(id, "groundTile")
    }
}

/// `path` with `.blp` when it has no extension.
pub fn with_extension(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    if file.contains('.') { path.to_string() } else { format!("{path}.blp") }
}

fn water(slk: &Slk, letter: char) -> Option<Water> {
    let id = format!("{letter}Sha");
    let n = |c: &str| slk.number(&id, c).unwrap_or(0.0);
    let color = |p: &str| ["R", "G", "B", "A"].map(|c| n(&format!("{p}_{c}")).clamp(0.0, 255.0) as u8);
    Some(Water {
        height: n("height"),
        texture: slk.get(&id, "texFile")?.to_string(),
        count: n("numTex") as u32,
        rate: n("texRate"),
        colors: [color("Smin"), color("Smax"), color("Dmin"), color("Dmax")],
        average: None,
    })
}

#[cfg(test)]
#[path = "tileset.test.rs"]
mod tests;
