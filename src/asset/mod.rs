//! CPU asset import. Loaded meshes use the ordinary scene and rendering paths.

mod fbx;

pub use fbx::{FbxModel, load_fbx, load_fbx_bytes};
