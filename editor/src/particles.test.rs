use super::*;
use mdx_rs::{Chunk, Emitter2, Extent, Fixed, Key, Node, Sequence, Track};

fn emitter(tracks: Vec<([u8; 4], Track<f32>)>) -> Emitter2 {
    Emitter2 {
        node: Node { name: Fixed::new("Fire", 80), object_id: 0, parent_id: u32::MAX, flags: 0x20000, tracks: vec![] },
        speed: 100.0,
        variation: 0.5,
        latitude: 30.0,
        gravity: 10.0,
        life_span: 2.0,
        emission_rate: 10.0,
        length: 20.0,
        width: 10.0,
        filter_mode: 1,
        rows: 2,
        columns: 4,
        head_or_tail: 0,
        tail_length: 1.0,
        time_middle: 0.5,
        segment_colors: [[1.0, 0.5, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]],
        segment_alphas: [255, 128, 0],
        segment_scaling: [10.0, 20.0, 30.0],
        head_intervals: [0, 3, 1],
        head_decay_intervals: [4, 7, 1],
        tail_intervals: [0; 3],
        tail_decay_intervals: [0; 3],
        texture_id: 0,
        squirt: 0,
        priority_plane: 0,
        replaceable_id: 0,
        tracks,
    }
}

fn model(e: Emitter2) -> Model {
    let stand = Sequence { name: Fixed::new("Stand", 80), start: 0, end: 1000, move_speed: 0.0, flags: 0, rarity: 0.0, sync_point: 0, extent: Extent::default() };
    Model { chunks: vec![Chunk::Version(800), Chunk::Sequences(vec![stand]), Chunk::Emitters2(vec![e]), Chunk::Pivots(vec![[5.0, 0.0, 0.0]])] }
}

#[test]
fn an_emitter_at_its_pivot_its_particles_described() {
    let m = model(emitter(vec![]));
    let list = emitters(&m, &mut |id, _| id as i32 + 7);
    let e = &list[0];
    // Nothing moves: one frame, the emitter at its pivot, shown, 10 a second.
    assert_eq!(e.frames.len(), FRAME);
    assert_eq!([e.frames[3], e.frames[7], e.frames[11]], [5.0, 0.0, 0.0]);
    assert_eq!((e.frames[12], e.frames[13]), (1.0, 10.0));
    // 30° in radians; 10 a second for 2 seconds: 20 alive at most.
    assert!((e.params[2] - 30f32.to_radians()).abs() < 1e-6);
    assert_eq!((e.params[11], e.params[13]), (0x20000 as f32, 20.0));
    assert_eq!(&e.segments[9..12], [1.0, 128.0 / 255.0, 0.0]);
    assert_eq!(e.head, [0.0, 3.0, 1.0, 4.0, 7.0, 1.0]);
    assert_eq!(e.texture, 7);
}

#[test]
fn a_tail_its_length_and_cells() {
    let mut x = emitter(vec![]);
    x.head_or_tail = 2;
    x.tail_length = 0.5;
    x.tail_intervals = [8, 11, 1];
    x.tail_decay_intervals = [12, 15, 2];
    let e = &emitters(&model(x), &mut |_, _| 0)[0];
    assert_eq!((e.params[14], e.params[15]), (2.0, 0.5));
    assert_eq!(e.tail, [8.0, 11.0, 1.0, 12.0, 15.0, 2.0]);
}

#[test]
fn a_ribbon_at_its_pivot_with_its_materials_texture() {
    let key = |frame, value| Key { frame, value, tangents: None };
    let ribbon = mdx_rs::Ribbon {
        node: Node { name: Fixed::new("Trail", 80), object_id: 0, parent_id: u32::MAX, flags: 0x4000, tracks: vec![] },
        height_above: 25.0,
        height_below: 10.0,
        alpha: 1.0,
        color: [1.0, 0.5, 0.1],
        life_span: 0.4,
        texture_slot: 0,
        emission_rate: 25,
        rows: 1,
        columns: 2,
        material_id: 0,
        gravity: 0.0,
        tracks: vec![
            (*b"KRAL", mdx_rs::RibbonTrack::Number(Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, 0.5)] })),
            (*b"KRCO", mdx_rs::RibbonTrack::Color(Track { interpolation: 0, global_sequence: -1, keys: vec![Key { frame: 0, value: [0.0, 1.0, 0.0], tangents: None }] })),
            (*b"KRTX", mdx_rs::RibbonTrack::Slot(Track { interpolation: 0, global_sequence: -1, keys: vec![Key { frame: 0, value: 1u32, tangents: None }] })),
        ],
    };
    let layer = mdx_rs::Layer { filter_mode: 3, shading: 0, texture_id: 1, texture_animation_id: u32::MAX, coord_id: 0, alpha: 1.0, rest: vec![] };
    let stand = Sequence { name: Fixed::new("Stand", 80), start: 0, end: 1000, move_speed: 0.0, flags: 0, rarity: 0.0, sync_point: 0, extent: Extent::default() };
    let m = Model {
        chunks: vec![
            Chunk::Version(800),
            Chunk::Sequences(vec![stand]),
            Chunk::Materials(vec![mdx_rs::Material { priority_plane: 0, flags: 0, shader: None, layers: vec![layer] }]),
            Chunk::Ribbons(vec![ribbon]),
            Chunk::Pivots(vec![[0.0, 0.0, 50.0]]),
        ],
    };
    let list = ribbons(&m, &mut |id| id as i32 + 3);
    let r = &list[0];
    // 25 a second for 0.4 seconds: 11 segments; additive; its texture
    // the layer's.
    assert_eq!(r.params, [0.4, 25.0, 1.0, 2.0, 0.0, 0.0, 1.0, 11.0]);
    assert_eq!((r.texture, r.color), (4, [1.0, 0.5, 0.1]));
    assert_eq!(r.frames.len(), RIBBON_FRAME);
    assert_eq!(r.frames[11], 50.0);
    assert_eq!(&r.frames[12..16], [1.0, 25.0, 10.0, 0.5]);
    // Its colour and cell as their tracks say.
    assert_eq!(&r.frames[16..20], [0.0, 1.0, 0.0, 1.0]);
}

#[test]
fn shown_and_made_as_its_tracks_say() {
    let key = |frame, value| Key { frame, value, tangents: None };
    let hidden = Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, 1.0), key(500, 0.0)] };
    let faster = Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, 50.0)] };
    let mut e = emitter(vec![(*b"KP2V", hidden), (*b"KP2E", faster)]);
    // A track on its node: it moves, frames all through Stand.
    e.node.tracks.push(mdx_rs::NodeTrack::Translation(Track { interpolation: 1, global_sequence: -1, keys: vec![Key { frame: 0, value: [0.0; 3], tangents: None }, Key { frame: 1000, value: [0.0, 0.0, 100.0], tangents: None }] }));
    let list = emitters(&model(e), &mut |_, _| 0);
    let e = &list[0];
    assert_eq!(e.frames.len(), 20 * FRAME);
    let frame = |f: usize| &e.frames[f * FRAME..][..FRAME];
    assert_eq!((frame(0)[12], frame(15)[12]), (1.0, 0.0));
    assert!((frame(10)[11] - 50.0).abs() < 1e-3, "the emitter up with its node");
    // 50 a second at most: 100 alive.
    assert_eq!(e.params[13], 100.0);
}
