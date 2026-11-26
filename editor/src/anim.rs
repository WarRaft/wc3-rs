//! A model's animation, worked out here once and sent as it is: the
//! matrices of its bones at every frame of its `Stand` (20 a second, 120
//! at most), and each vertex's bones and weights, for the page to move
//! the vertices with on the GPU and do nothing else a frame (HiveWE's
//! `skeleton.ixx`, the game's way):
//!
//! - a node's matrix: moved by its translation, turned and scaled about
//!   its pivot (`PIVT`), under its parent's;
//! - a track's value at a frame: between the keys about it in the
//!   sequence (or its global sequence's loop), as the game interpolates
//!   (none, linear, Hermite, Bezier; quaternions between, normalized),
//!   with the game's own way past the last key; none in it, the default;
//! - a vertex: its matrix group's bones (`MTGC`, `MATS`), four at most,
//!   each the same weight.
//!
//! Billboarded nodes face the camera in the game: not here yet.

use std::collections::HashMap;

use mdx_rs::{Key, Model, Node, Track};

/// Frames a second the animation is sampled at, and the most frames.
pub const FPS: f32 = 20.0;
pub const MAX_FRAMES: usize = 120;

/// A model's bones at each frame: `frames` of `bones` matrices, each
/// three rows of four (`x' = row0 · (x, y, z, 1)` ...).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Baked {
    pub bones: u32,
    pub frames: u32,
    pub fps: f32,
    pub matrices: Vec<f32>,
}

/// A value tracks key, in a way interpolation can use.
trait Lerp: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
    fn hermite(a: Self, out: Self, inn: Self, b: Self, t: f32) -> Self;
    fn bezier(a: Self, out: Self, inn: Self, b: Self, t: f32) -> Self;
}

impl Lerp for [f32; 3] {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t)
    }
    fn hermite(a: Self, out: Self, inn: Self, b: Self, t: f32) -> Self {
        let t2 = t * t;
        let f1 = t2 * (2.0 * t - 3.0) + 1.0;
        let f2 = t2 * (t - 2.0) + t;
        let f3 = t2 * (t - 1.0);
        let f4 = t2 * (3.0 - 2.0 * t);
        [0, 1, 2].map(|k| a[k] * f1 + out[k] * f2 + inn[k] * f3 + b[k] * f4)
    }
    fn bezier(a: Self, out: Self, inn: Self, b: Self, t: f32) -> Self {
        let it = 1.0 - t;
        let (f1, f2, f3, f4) = (it * it * it, 3.0 * t * it * it, 3.0 * t * t * it, t * t * t);
        [0, 1, 2].map(|k| a[k] * f1 + out[k] * f2 + inn[k] * f3 + b[k] * f4)
    }
}

impl Lerp for [f32; 4] {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        // The shorter way round, normalized.
        let dot: f32 = (0..4).map(|k| a[k] * b[k]).sum();
        let b = if dot < 0.0 { b.map(|v| -v) } else { b };
        let q = [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t);
        let len = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
        q.map(|v| v / len)
    }
    fn hermite(a: Self, _: Self, _: Self, b: Self, t: f32) -> Self {
        Self::lerp(a, b, t)
    }
    fn bezier(a: Self, _: Self, _: Self, b: Self, t: f32) -> Self {
        Self::lerp(a, b, t)
    }
}

/// `track`'s value at `frame` of the sequence `start..=end` (a global
/// sequence's track at `since` the sequence started, its own loop).
fn value<T: Lerp + mdx_rs::Value>(track: Option<&Track<T>>, default: T, frame: u32, start: u32, end: u32, since: u32, globals: &[u32]) -> T {
    let Some(track) = track else { return default };
    let (now, start, end) = match usize::try_from(track.global_sequence).ok().and_then(|g| globals.get(g)) {
        Some(&len) if len > 0 => ((since % len) as i64, 0i64, len as i64),
        Some(_) => (0, 0, 0),
        None => (frame as i64, start as i64, end as i64),
    };
    let keys: Vec<&Key<T>> = track.keys.iter().filter(|k| (start..=end).contains(&(k.frame as i64))).collect();
    match keys.len() {
        0 => return default,
        1 => return keys[0].value,
        _ => {}
    }
    // The key at or before now, and the one after (past the last, the
    // first: the loop round).
    let left = keys.iter().rposition(|k| k.frame as i64 <= now).unwrap_or(keys.len() - 1);
    let right = if left + 1 < keys.len() && keys[0].frame as i64 <= now { left + 1 } else { 0 };
    let (l, r) = (keys[left], keys[right]);
    // The game's (and World Editor's) way of the time between them.
    let mut floor = l.frame as i64;
    let mut between = r.frame as i64 - floor;
    if between < 0 {
        between += end - start;
        if now < floor {
            floor = r.frame as i64;
        }
    }
    let t = if between == 0 { 0.0 } else { ((now - floor) as f32 / between as f32).clamp(0.0, 1.0) };
    let tan = |k: &Key<T>, out: bool| k.tangents.map_or(k.value, |(i, o)| if out { o } else { i });
    match track.interpolation {
        0 => l.value,
        1 => T::lerp(l.value, r.value, t),
        2 => T::hermite(l.value, tan(l, true), tan(r, false), r.value, t),
        _ => T::bezier(l.value, tan(l, true), tan(r, false), r.value, t),
    }
}

/// A 4 by 4 matrix, column by column.
type Mat = [f32; 16];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

/// Moved by `v`, turned by `q` and scaled by `s` about `pivot`.
fn local(q: [f32; 4], v: [f32; 3], s: [f32; 3], pivot: [f32; 3]) -> Mat {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz, yy, yz, zz) = (x * x2, x * y2, x * z2, y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let r = [
        [(1.0 - (yy + zz)) * s[0], (xy + wz) * s[0], (xz - wy) * s[0]],
        [(xy - wz) * s[1], (1.0 - (xx + zz)) * s[1], (yz + wx) * s[1]],
        [(xz + wy) * s[2], (yz - wx) * s[2], (1.0 - (xx + yy)) * s[2]],
    ];
    let mut m = [0.0; 16];
    for c in 0..3 {
        m[c * 4..c * 4 + 3].copy_from_slice(&r[c]);
    }
    // Translation: v + pivot - R·S·pivot.
    for k in 0..3 {
        m[12 + k] = v[k] + pivot[k] - (r[0][k] * pivot[0] + r[1][k] * pivot[1] + r[2][k] * pivot[2]);
    }
    m[15] = 1.0;
    m
}

/// The model's skeleton: its nodes (bones and helpers), parents first,
/// and the slot each node bound to a vertex has among the bones baked.
pub struct Skeleton<'a> {
    nodes: Vec<&'a Node>,
    pub slots: HashMap<u32, usize>,
}

impl<'a> Skeleton<'a> {
    pub fn of(m: &'a Model) -> Skeleton<'a> {
        let all: HashMap<u32, &Node> = m.bones().iter().map(|b| &b.node).chain(m.helpers()).map(|n| (n.object_id, n)).collect();
        // Parents first.
        let mut order: Vec<&Node> = Vec::new();
        let mut placed = std::collections::HashSet::new();
        fn visit<'b>(id: u32, all: &HashMap<u32, &'b Node>, placed: &mut std::collections::HashSet<u32>, order: &mut Vec<&'b Node>, depth: u32) {
            if placed.contains(&id) || depth > 64 {
                return;
            }
            let Some(n) = all.get(&id) else { return };
            if let Some(p) = n.parent() {
                visit(p, all, placed, order, depth + 1);
            }
            if placed.insert(id) {
                order.push(n);
            }
        }
        let mut ids: Vec<u32> = all.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            visit(id, &all, &mut placed, &mut order, 0);
        }
        let mut slots = HashMap::new();
        for g in m.geosets() {
            for id in &g.matrix_indices {
                let next = slots.len();
                slots.entry(*id).or_insert(next);
            }
        }
        Skeleton { nodes: order, slots }
    }
}

/// The model's `Stand` (or first sequence) baked; `None` when nothing
/// moves (no node has a track in it) or it has too many bones.
pub fn bake(m: &Model) -> Option<Baked> {
    let sk = Skeleton::of(m);
    if sk.slots.is_empty() || sk.slots.len() > 255 || sk.nodes.iter().all(|n| n.tracks.is_empty()) {
        return None;
    }
    let seqs = m.sequences();
    let seq = seqs.iter().find(|s| s.name.text().to_ascii_lowercase().starts_with("stand")).or(seqs.first());
    let (start, end) = seq.map_or((0, 0), |s| (s.start, s.end.max(s.start)));
    let length = (end - start) as f32;
    let frames = ((length / 1000.0 * FPS).ceil() as usize).clamp(1, MAX_FRAMES);
    let step = if frames > 1 { length / frames as f32 } else { 0.0 };
    let fps = if step > 0.0 { 1000.0 / step } else { FPS };
    let pivots = m.pivots();
    let globals = m.global_sequences();
    let mut matrices = vec![0.0f32; frames * sk.slots.len() * 12];
    let mut world: HashMap<u32, Mat> = HashMap::new();
    for f in 0..frames {
        let since = (f as f32 * step) as u32;
        let frame = start + since;
        world.clear();
        for n in &sk.nodes {
            let t = value(n.translation(), [0.0; 3], frame, start, end, since, globals);
            let r = value(n.rotation(), [0.0, 0.0, 0.0, 1.0], frame, start, end, since, globals);
            let s = value(n.scaling(), [1.0; 3], frame, start, end, since, globals);
            let pivot = pivots.get(n.object_id as usize).copied().unwrap_or_default();
            let mut m = local(r, t, s, pivot);
            if let Some(parent) = n.parent().and_then(|p| world.get(&p)) {
                m = mul(parent, &m);
            }
            world.insert(n.object_id, m);
        }
        for (id, slot) in &sk.slots {
            let m = world.get(id).copied().unwrap_or_else(identity);
            let at = (f * sk.slots.len() + slot) * 12;
            for row in 0..3 {
                for c in 0..4 {
                    matrices[at + row * 4 + c] = m[c * 4 + row];
                }
            }
        }
    }
    Some(Baked { bones: sk.slots.len() as u32, frames: frames as u32, fps, matrices })
}

fn identity() -> Mat {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

/// A geoset's vertices' bones (slots) and weights, eight bytes each: four
/// slots, four weights out of 255.
pub fn skin(g: &mdx_rs::Geoset, slots: &HashMap<u32, usize>) -> Vec<u8> {
    let mut starts = Vec::with_capacity(g.matrix_groups.len());
    let mut at = 0usize;
    for size in &g.matrix_groups {
        starts.push((at, *size as usize));
        at += *size as usize;
    }
    let mut out = Vec::with_capacity(g.vertices.len() * 8);
    for v in 0..g.vertices.len() {
        let group = g.vertex_groups.get(v).copied().unwrap_or(0) as usize;
        let (from, size) = starts.get(group).copied().unwrap_or((0, 0));
        let bones: Vec<u8> = g.matrix_indices.get(from..from + size).unwrap_or(&[]).iter().take(4).filter_map(|id| slots.get(id)).map(|s| *s as u8).collect();
        let n = bones.len();
        for k in 0..4 {
            out.push(bones.get(k).copied().unwrap_or(0));
        }
        for k in 0..4 {
            out.push(if k < n { (255.0 / n as f32).round() as u8 } else { 0 });
        }
    }
    out
}

#[cfg(test)]
#[path = "anim.test.rs"]
mod tests;
