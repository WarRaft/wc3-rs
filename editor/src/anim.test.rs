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
    let m = &b.matrices[(f * b.bones as usize + slot) * 12..][..12];
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
