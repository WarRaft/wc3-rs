//! A model's particle emitters (`PRE2`), as the page draws them: what
//! each one's particles are (their speed, spread, life, look) and, at
//! every frame of the model's `Stand` (`anim::Timeline`), where the
//! emitter is (its node's matrix, about its pivot), whether it's shown
//! (`KP2V`) and how many it makes a second (`KP2E`). The page then works
//! out each particle from its number and the time alone (when it was
//! made, where it went), as HiveWE's `particle_emitter2_simulation` makes
//! them: a place in the emitter's width and length, a direction within
//! its latitude of straight up (in its plane alone for a line emitter),
//! its speed varied, gravity down; its colour, alpha and size between
//! its segments' and its texture's cell by its age (head only: tails
//! are next).

use mdx_rs::Model;

use crate::anim::{Skeleton, Timeline};

/// Floats an emitter's frame has: its matrix's three rows of four, its
/// visibility, its emission rate, two unused.
pub const FRAME: usize = 16;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Emitter {
    /// Speed, variation, latitude (radians), gravity, life span
    /// (seconds), length, width, time middle, rows, columns, filter mode,
    /// flags (the node's: 0x20000 line emitter, 0x100000 flat in xy, 0x8000
    /// unshaded), squirt, the most particles alive, two unused.
    pub params: [f32; 16],
    /// Segment colours (three RGB), alphas (three, 0-1), scaling (three).
    pub segments: [f32; 15],
    /// The head's cells: start, end, repeat, before and after the middle.
    pub head: [f32; 6],
    /// Its texture: an index into the textures (`placed::texture` for a
    /// replaceable one).
    pub texture: i32,
    pub fps: f32,
    /// [`FRAME`] floats a frame.
    pub frames: Vec<f32>,
}

/// The most particles an emitter keeps alive.
pub const MAX_ALIVE: f32 = 256.0;

/// The model's emitters; `texture(id)` the texture an emitter's
/// `TEXS` index (or replaceable id) is drawn with.
pub fn emitters(m: &Model, texture: &mut dyn FnMut(u32, u32) -> i32) -> Vec<Emitter> {
    if m.emitters2().is_empty() {
        return Vec::new();
    }
    let sk = Skeleton::of(m);
    let tl = Timeline::of(m);
    let frames = if sk.moves() { tl.frames } else { 1 };
    let worlds: Vec<_> = (0..frames).map(|f| sk.world(m, &tl, f)).collect();
    let pivots = m.pivots();
    m.emitters2()
        .iter()
        .map(|e| {
            let mut out = Vec::with_capacity(frames * FRAME);
            let mut most_rate = e.emission_rate;
            for (f, world) in worlds.iter().enumerate() {
                let node = world.get(&e.node.object_id).copied().unwrap_or_else(crate::anim::identity);
                // At its pivot: the node's matrix leaves it out at rest.
                let pivot = pivots.get(e.node.object_id as usize).copied().unwrap_or_default();
                let mut at = crate::anim::identity();
                at[12..15].copy_from_slice(&pivot);
                let m4 = crate::anim::mul(&node, &at);
                let mut frame = [0.0f32; FRAME];
                crate::anim::rows(&m4, &mut frame[..12]);
                frame[12] = crate::anim::track_at(m, &tl, f, e.track(b"KP2V"), 1.0);
                frame[13] = crate::anim::track_at(m, &tl, f, e.track(b"KP2E"), e.emission_rate).max(0.0);
                most_rate = most_rate.max(frame[13]);
                out.extend(frame);
            }
            let at0 = |tag: &[u8; 4], v: f32| crate::anim::track_at(m, &tl, 0, e.track(tag), v);
            let alive = (most_rate * e.life_span).ceil().clamp(0.0, MAX_ALIVE);
            let [c0, c1, c2] = e.segment_colors;
            Emitter {
                params: [
                    at0(b"KP2S", e.speed),
                    at0(b"KP2R", e.variation),
                    at0(b"KP2L", e.latitude).to_radians(),
                    at0(b"KP2G", e.gravity),
                    e.life_span,
                    at0(b"KP2N", e.length),
                    at0(b"KP2W", e.width),
                    e.time_middle,
                    e.rows.max(1) as f32,
                    e.columns.max(1) as f32,
                    e.filter_mode as f32,
                    e.node.flags as f32,
                    e.squirt as f32,
                    alive,
                    0.0,
                    0.0,
                ],
                segments: [
                    c0[0], c0[1], c0[2], c1[0], c1[1], c1[2], c2[0], c2[1], c2[2],
                    e.segment_alphas[0] as f32 / 255.0,
                    e.segment_alphas[1] as f32 / 255.0,
                    e.segment_alphas[2] as f32 / 255.0,
                    e.segment_scaling[0],
                    e.segment_scaling[1],
                    e.segment_scaling[2],
                ],
                head: [e.head_intervals, e.head_decay_intervals].concat().iter().map(|v| *v as f32).collect::<Vec<_>>().try_into().unwrap(),
                texture: texture(e.texture_id, e.replaceable_id),
                fps: tl.fps(),
                frames: out,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "particles.test.rs"]
mod tests;
