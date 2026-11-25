//! A fresh folder of the temp folder for a test, removed when dropped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(name: &str) -> TestDir {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wc3-editor-test-{}-{name}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TestDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `bytes` to `rel` in it, its folders made.
    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small terrain: 3 by 2 points, Lordaeron Summer.
pub fn terrain() -> Vec<u8> {
    use w3e_rs::{Point, Terrain};
    let terrain = Terrain {
        version: 11,
        tileset: b'L',
        custom_tilesets: 0,
        ground: vec![parser_rs::bin::Rawcode(*b"Ldrt")],
        cliffs: vec![parser_rs::bin::Rawcode(*b"CLdi")],
        width: 3,
        height: 2,
        offset_x: -128.0,
        offset_y: -64.0,
        points: vec![Point { height: 0x2000, water: 0x1F00, flags: 0, variation: 0, layer: 2 }; 6],
    };
    terrain.to_bytes().unwrap()
}

/// A map's info: its name a string of the map's table.
pub fn info() -> Vec<u8> {
    let mut info = w3i_rs::Info::new(25);
    info.name = "TRIGSTR_001".into();
    info.author = "Someone".into();
    info.tileset = b'L';
    info.to_bytes().unwrap()
}
