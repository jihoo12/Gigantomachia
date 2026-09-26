//! Isosurface reconstruction from the current particle positions, not a preauthored water sheet.
use super::Fluid;
use crate::{mesh::Vertex, render::EngineResult};
use glam::Vec3;

#[derive(Clone, Debug, Default)]
pub struct FluidSurface {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
}
impl FluidSurface {
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
}

impl Fluid {
    /// Reconstruct a smooth density isosurface with marching tetrahedra.
    /// This bounded CPU prototype rejects domains requiring more than 600,000 samples.
    pub fn surface(&self) -> EngineResult<FluidSurface> {
        let cell = self.spacing * 0.65;
        let radius = self.spacing * 1.8;
        let minimum = self
            .positions
            .iter()
            .copied()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min)
            - Vec3::splat(radius + cell);
        let maximum = self
            .positions
            .iter()
            .copied()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max)
            + Vec3::splat(radius + cell);
        let origin = (minimum / cell).floor() * cell;
        let size = ((maximum - origin) / cell).ceil().as_uvec3() + glam::UVec3::ONE;
        let total = u64::from(size.x) * u64::from(size.y) * u64::from(size.z);
        if total > 600_000 {
            return Err("fluid surface domain exceeds 600,000 grid samples; reduce spread or increase particle spacing".into());
        }
        let index = |x: u32, y: u32, z: u32| ((z * size.y + y) * size.x + x) as usize;
        let mut values = vec![0.0_f32; total as usize];
        let mut gradients = vec![Vec3::ZERO; total as usize];
        for &p in &self.positions {
            let a = ((p - Vec3::splat(radius) - origin) / cell)
                .floor()
                .max(Vec3::ZERO)
                .as_uvec3();
            let b = ((p + Vec3::splat(radius) - origin) / cell)
                .ceil()
                .as_uvec3()
                .min(size - glam::UVec3::ONE);
            for z in a.z..=b.z {
                for y in a.y..=b.y {
                    for x in a.x..=b.x {
                        let delta = origin + Vec3::new(x as f32, y as f32, z as f32) * cell - p;
                        let w = (1.0 - delta.length_squared() / (radius * radius)).max(0.0);
                        let i = index(x, y, z);
                        values[i] += w * w * w;
                        gradients[i] += delta * (6.0 * w * w / (radius * radius));
                    }
                }
            }
        }
        let mut out = FluidSurface::default();
        const CORNERS: [[u32; 3]; 8] = [
            [0, 0, 0],
            [1, 0, 0],
            [1, 1, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 0, 1],
            [1, 1, 1],
            [0, 1, 1],
        ];
        const TETS: [[usize; 4]; 6] = [
            [0, 5, 1, 6],
            [0, 1, 2, 6],
            [0, 2, 3, 6],
            [0, 3, 7, 6],
            [0, 7, 4, 6],
            [0, 4, 5, 6],
        ];
        const EDGES: [[usize; 2]; 6] = [[0, 1], [1, 2], [2, 0], [0, 3], [1, 3], [2, 3]];
        const TABLE: [&[usize]; 16] = [
            &[],
            &[0, 2, 3],
            &[0, 1, 4],
            &[1, 2, 3, 1, 3, 4],
            &[1, 2, 5],
            &[0, 1, 5, 0, 5, 3],
            &[0, 2, 5, 0, 5, 4],
            &[3, 4, 5],
            &[3, 4, 5],
            &[0, 2, 5, 0, 5, 4],
            &[0, 1, 5, 0, 5, 3],
            &[1, 2, 5],
            &[1, 2, 3, 1, 3, 4],
            &[0, 1, 4],
            &[0, 2, 3],
            &[],
        ];
        for z in 0..size.z - 1 {
            for y in 0..size.y - 1 {
                for x in 0..size.x - 1 {
                    let ids = CORNERS.map(|c| index(x + c[0], y + c[1], z + c[2]));
                    let field = ids.map(|i| values[i]);
                    if field.iter().all(|&v| v < 0.6) || field.iter().all(|&v| v >= 0.6) {
                        continue;
                    }
                    let points = CORNERS.map(|c| {
                        origin
                            + Vec3::new((x + c[0]) as f32, (y + c[1]) as f32, (z + c[2]) as f32)
                                * cell
                    });
                    for tet in TETS {
                        let mask =
                            (0..4).fold(0, |m, i| m | ((field[tet[i]] >= 0.6) as usize) << i);
                        for tri in TABLE[mask].as_chunks::<3>().0 {
                            let mut positions = [Vec3::ZERO; 3];
                            let mut normals = [Vec3::ZERO; 3];
                            for k in 0..3 {
                                let [a, b] = EDGES[tri[k]].map(|i| tet[i]);
                                let t = ((0.6 - field[a]) / (field[b] - field[a])).clamp(0.0, 1.0);
                                positions[k] = points[a].lerp(points[b], t);
                                normals[k] = gradients[ids[a]]
                                    .lerp(gradients[ids[b]], t)
                                    .normalize_or_zero();
                            }
                            let geometric =
                                (positions[1] - positions[0]).cross(positions[2] - positions[0]);
                            if geometric.length_squared() < 1e-14 {
                                continue;
                            }
                            if geometric.dot(normals[0] + normals[1] + normals[2]) < 0.0 {
                                positions.swap(1, 2);
                                normals.swap(1, 2);
                            }
                            if out.vertices.len() > 2_000_000 {
                                return Err("fluid surface exceeds two million vertices".into());
                            }
                            let base = out.vertices.len() as u32;
                            for k in 0..3 {
                                out.vertices.push(Vertex {
                                    position: positions[k].to_array(),
                                    normal: normals[k].to_array(),
                                    color: [0.015, 0.16, 0.23],
                                });
                            }
                            out.indices.extend([base, base + 1, base + 2]);
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}
