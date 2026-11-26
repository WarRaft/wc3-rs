//! The map's light through the day: the game's day and night models
//! (`UI\WorldEditData.txt` `[TerrainLights]` for the ground and cliffs,
//! `[UnitLights]` for what's placed, by the letter of the tileset whose
//! light the map uses: `war3map.w3i`'s, else its terrain's), each a
//! model whose directional light (`LITE`) its `Stand` turns and colours
//! through a day, the sequence's start midnight, its end the next. Baked
//! here at [`SAMPLES`] times of the day: the sun's direction (the
//! light's node turned, (0, 0, -1) at rest), its colour times its
//! intensity, the ambient light's.

use mdx_rs::Model;
use wc3_server::game::Game;

use crate::anim::{Skeleton, Timeline};

/// Times of the day baked: every half an hour.
pub const SAMPLES: usize = 48;
/// Floats a time has: direction, colour, ambient (three each), unused.
pub const SAMPLE: usize = 10;

/// The light through the day, [`SAMPLE`] floats a time; none: the
/// view's own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DayLight {
    pub path: String,
    pub samples: Vec<f32>,
}

/// The ground's light and the things', and what was missing.
pub fn load(game: &Game, letter: char) -> (DayLight, DayLight, Vec<String>) {
    let mut notes = Vec::new();
    let data = game.read_patched("UI\\WorldEditData.txt").map(|t| bni_rs::parse(&t));
    let path = |section: &str| -> Option<String> {
        let doc = data.as_ref()?;
        let item = doc.section(section)?.items.iter().find(|i| i.key.as_ref().is_some_and(|k| k.text.trim() == letter.to_string()))?;
        Some(crate::placed::model_path(item.value_text().trim().trim_matches('"')))
    };
    let mut day = |section: &str| -> DayLight {
        let Some(p) = path(section) else {
            notes.push(format!("No light for tileset {letter} in UI\\WorldEditData.txt [{section}]: the view's own."));
            return DayLight::default();
        };
        match game.read(&p).map(|(b, _)| mdx_rs::parse(&b)) {
            Some(Ok(m)) => DayLight { samples: bake(&m), path: p },
            Some(Err(e)) => {
                notes.push(format!("{p}: {e}"));
                DayLight::default()
            }
            None => {
                notes.push(format!("{p}: not in the game's files or the roots"));
                DayLight::default()
            }
        }
    };
    (day("TerrainLights"), day("UnitLights"), notes)
}

/// A day and night model's directional light through its `Stand`.
pub fn bake(m: &Model) -> Vec<f32> {
    let Some(light) = m.lights().iter().find(|l| l.kind == 1).or(m.lights().first()) else { return Vec::new() };
    let sk = Skeleton::of(m);
    let tl = Timeline::of(m);
    let mut out = Vec::with_capacity(SAMPLES * SAMPLE);
    // The model's colours are blue, green, red.
    let rgb = |c: [f32; 3]| [c[2], c[1], c[0]];
    for s in 0..SAMPLES {
        let since = ((tl.end - tl.start) as f32 * s as f32 / SAMPLES as f32) as u32;
        let frame = tl.start + since;
        let at = Timeline { start: tl.start, end: tl.end, frames: 1, step: 0.0 };
        let world = sk.world_at(m, &at, frame, since);
        let node = world.get(&light.node.object_id).copied().unwrap_or_else(crate::anim::identity);
        // (0, 0, -1) turned by the node.
        let dir = [-node[8], -node[9], -node[10]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
        let value = |t: Option<&mdx_rs::Track<f32>>, d: f32| crate::anim::value_at(t, d, frame, tl.start, tl.end, since, m.global_sequences());
        let color = |t: Option<&mdx_rs::Track<[f32; 3]>>, d: [f32; 3]| crate::anim::value_at(t, d, frame, tl.start, tl.end, since, m.global_sequences());
        let c = rgb(color(light.color(b"KLAC"), light.color)).map(|v| v * value(light.number(b"KLAI"), light.intensity));
        let a = rgb(color(light.color(b"KLBC"), light.ambient_color)).map(|v| v * value(light.number(b"KLBI"), light.ambient_intensity));
        out.extend(dir.map(|v| v / len));
        out.extend(c);
        out.extend(a);
        out.push(0.0);
    }
    out
}

#[cfg(test)]
#[path = "light.test.rs"]
mod tests;
