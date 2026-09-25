//! Immutable CPU geometry, shared between scenes and uploaded by the renderer.

use crate::render::EngineResult;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::sync::atomic::{AtomicU64, Ordering};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Linear RGB, in the range 0..1.
    pub color: [f32; 3],
}

#[derive(Debug)]
pub struct Mesh {
    id: u64,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Mesh {
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u32>) -> EngineResult<Self> {
        if vertices.is_empty()
            || indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || indices.len() > u32::MAX as usize
            || indices.iter().any(|&i| i as usize >= vertices.len())
        {
            return Err("mesh must contain indexed triangles within its vertex bounds".into());
        }
        for vertex in &vertices {
            if !vertex.position.iter().all(|v| v.is_finite())
                || !vertex.normal.iter().all(|v| v.is_finite())
                || Vec3::from_array(vertex.normal).length_squared() < 1e-8
                || !vertex
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                return Err("mesh vertices require finite positions, nonzero normals, and linear RGB in 0..1".into());
            }
        }
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            vertices,
            indices,
        })
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
}

/// CCW triangles for a regular XZ grid, viewed from +Y.
pub(crate) fn grid_indices(cells: u32) -> Vec<u32> {
    let mut indices = Vec::with_capacity((cells * cells * 6) as usize);
    for z in 0..cells {
        for x in 0..cells {
            let a = z * (cells + 1) + x;
            let b = a + cells + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_geometry_is_rejected_before_upload() {
        let v = Vertex {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            color: [0.5; 3],
        };
        assert!(Mesh::new(vec![v], vec![0, 1, 0]).is_err());
        assert!(Mesh::new(vec![v], vec![0, 0]).is_err());
        assert!(
            Mesh::new(
                vec![Vertex {
                    normal: [0.0; 3],
                    ..v
                }],
                vec![0; 3]
            )
            .is_err()
        );
    }
}
