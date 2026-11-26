use super::*;
use mdx_rs::{Bone, Chunk, Extent, Fixed, Geoset, Model, NodeTrack, Sequence};

fn node(id: u32, parent: u32, tracks: Vec<NodeTrack>) -> Node {
    Node { name: Fixed::new("n", 80), object_id: id, parent_id: parent, flags: 0x100, tracks }
}

fn key<T: Copy>(frame: i32, value: T) -> Key<T> {
    Key { frame, value, tangents: None }
}

/// A model of two bones (the second under the first), Stand 0..1000; a
/// geoset of two vertices, one bound to each.
fn model(first: Vec<NodeTrack>, second: Vec<NodeTrack>) -> Model {
    let stand = Sequence { name: Fixed::new("Stand", 80), start: 0, end: 1000, move_speed: 0.0, flags: 0, rarity: 0.0, sync_point: 0, extent: Extent::default() };
    let g = Geoset {
        vertices: vec![[0.0; 3]; 2],
        normals: vec![[0.0, 0.0, 1.0]; 2],
        vertex_groups: vec![0, 1],
        matrix_groups: vec![1, 2],
        matrix_indices: vec![0, 0, 1],
        uv_sets: vec![vec![[0.0; 2]; 2]],
        ..Default::default()
    };
    let bone = |n: Node| Bone { node: n, geoset_id: 0, geoset_animation_id: u32::MAX };
    Model {
        chunks: vec![
            Chunk::Version(800),
            Chunk::Sequences(vec![stand]),
            Chunk::Geosets(vec![g]),
            Chunk::Bones(vec![bone(node(0, u32::MAX, first)), bone(node(1, 0, second))]),
            Chunk::Pivots(vec![[0.0; 3], [10.0, 0.0, 0.0]]),
        ],
    }
}

/// Bone `slot`'s translation (the matrix's last column) at frame `f`.
fn moved(b: &Baked, f: usize, slot: usize) -> [f32; 3] {
    let row = b.bones as usize * BONE + b.geosets as usize * 4;
    let m = &b.matrices[f * row + slot * BONE..][..12];
    [m[3], m[7], m[11]]
}

#[test]
fn a_bone_moved_between_its_keys_its_child_with_it() {
    let up = NodeTrack::Translation(Track { interpolation: 1, global_sequence: -1, keys: vec![key(0, [0.0, 0.0, 0.0]), key(1000, [0.0, 0.0, 100.0])] });
    let m = model(vec![up], vec![]);
    let b = bake(&m).unwrap();
    // 1 second at 20 a second.
    assert_eq!((b.bones, b.frames), (2, 20));
    assert_eq!(moved(&b, 0, 0), [0.0, 0.0, 0.0]);
    let half = moved(&b, 10, 0);
    assert!((half[2] - 50.0).abs() < 1e-3, "{half:?}");
    // The child, no track of its own, moves with it.
    assert!((moved(&b, 10, 1)[2] - 50.0).abs() < 1e-3);
}

#[test]
fn a_turn_about_the_pivot() {
    // The child turned half round z, about its pivot (10, 0, 0): its
    // origin goes to (20, 0, 0).
    let half = NodeTrack::Rotation(Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, [0.0, 0.0, 1.0, 0.0])] });
    let m = model(vec![NodeTrack::Scaling(Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, [1.0; 3])] })], vec![half]);
    let b = bake(&m).unwrap();
    let t = moved(&b, 0, 1);
    assert!((t[0] - 20.0).abs() < 1e-4 && t[1].abs() < 1e-4, "{t:?}");
}

#[test]
fn nothing_moving_nothing_baked_and_the_skin() {
    assert!(bake(&model(vec![], vec![])).is_none());
    let m = model(vec![], vec![]);
    let sk = Skeleton::of(&m);
    let skin = skin(&m.geosets()[0], &sk.slots);
    // The first vertex: bone 0 alone; the second: bones 0 and 1, half each.
    assert_eq!(&skin[..8], [0, 0, 0, 0, 255, 0, 0, 0]);
    assert_eq!(&skin[8..], [0, 1, 0, 0, 128, 128, 0, 0]);
}

#[test]
fn past_the_last_key_the_game_s_way_round() {
    // Keys at 0 (0) and 500 (100), the sequence to 1000: at 750 between
    // the last and the first again, half way back.
    let t = Track { interpolation: 1, global_sequence: -1, keys: vec![key(0, [0.0f32; 3]), key(500, [100.0, 0.0, 0.0])] };
    let v = value(Some(&t), [0.0; 3], 750, 0, 1000, 750, &[]);
    assert!((v[0] - 50.0).abs() < 1e-3, "{v:?}");
}

#[test]
fn an_affine_inverse() {
    let m = local([0.0, 0.0, 0.38268343, 0.9238795], [5.0, -3.0, 2.0], [2.0, 2.0, 2.0], [1.0, 1.0, 0.0]);
    let i = mul(&inverse(&m), &m);
    for (k, v) in i.iter().enumerate() {
        assert!((v - identity()[k]).abs() < 1e-5, "{i:?}");
    }
}

#[test]
fn a_billboarded_bone_its_place_and_size_its_turn_left_to_the_shader() {
    // The first bone goes up 100 and doubles; the second faces the
    // camera (0x8).
    let up = NodeTrack::Translation(Track { interpolation: 1, global_sequence: -1, keys: vec![key(0, [0.0, 0.0, 0.0]), key(1000, [0.0, 0.0, 100.0])] });
    let big = NodeTrack::Scaling(Track { interpolation: 0, global_sequence: -1, keys: vec![key(0, [2.0; 3])] });
    let mut m = model(vec![up, big], vec![]);
    if let Some(Chunk::Bones(b)) = m.chunks.iter_mut().find(|c| matches!(c, Chunk::Bones(_))) {
        b[1].node.flags |= 0x8;
    }
    let b = bake(&m).unwrap();
    let row = b.bones as usize * BONE + b.geosets as usize * 4;
    let bone = |f: usize, slot: usize| b.matrices[f * row + slot * BONE..][..BONE].to_vec();
    // The first as before; the second: billboard 1, its pivot (10, 0, 0)
    // where its node puts it, (20, 0, 50) at frame 10; within it, the
    // vertex about the pivot, doubled.
    assert_eq!(bone(10, 0)[12], 0.0);
    let x = bone(10, 1);
    assert_eq!(x[12], 1.0);
    assert!((x[13] - 20.0).abs() < 1e-3 && (x[15] - 50.0).abs() < 1e-3, "{x:?}");
    assert!((x[0] - 2.0).abs() < 1e-4 && (x[3] + 20.0).abs() < 1e-3, "{x:?}");
    // Nothing moving, a billboard: baked all the same, one frame.
    let mut still = model(vec![], vec![]);
    if let Some(Chunk::Bones(b)) = still.chunks.iter_mut().find(|c| matches!(c, Chunk::Bones(_))) {
        b[0].node.flags |= 0x40;
    }
    let b = bake(&still).unwrap();
    assert_eq!(b.frames, 1);
    // Both under it: the child too.
    assert_eq!((b.matrices[12], b.matrices[BONE + 12]), (4.0, 4.0));
}

#[test]
fn a_geoset_hidden_halfway_its_colour_as_its_animation_says() {
    // KGAO: 1 at 0, 0 at 500 (no interpolation); KGAC a static blue,
    // green, red.
    let mut tracks = Vec::new();
    tracks.extend(b"KGAO");
    for v in [2u32, 0, u32::MAX] {
        tracks.extend(v.to_le_bytes());
    }
    for (f, a) in [(0i32, 1.0f32), (500, 0.0)] {
        tracks.extend(f.to_le_bytes());
        tracks.extend(a.to_le_bytes());
    }
    let mut m = model(vec![], vec![]);
    m.chunks.push(Chunk::GeosetAnimations(vec![mdx_rs::GeosetAnimation { alpha: 1.0, flags: 2, color: [0.25, 0.5, 1.0], geoset_id: 0, tracks }]));
    let b = bake(&m).unwrap();
    // Nothing moves, the alpha does: all of Stand, the geoset after the
    // two bones.
    assert_eq!((b.bones, b.geosets, b.frames), (2, 1, 20));
    let row = 2 * BONE + 4;
    let px = |f: usize| b.matrices[f * row + 2 * BONE..][..4].to_vec();
    assert_eq!(px(0), [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(px(15)[3], 0.0);
    // No animation at all: nothing baked.
    assert!(bake(&model(vec![], vec![])).is_none());
}
