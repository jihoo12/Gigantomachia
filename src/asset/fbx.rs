//! Static ASCII/binary FBX import through ufbx. No Autodesk SDK or external file loading.

use crate::{
    mesh::{Mesh, Vertex},
    render::EngineResult,
    scene::MeshInstance,
};
use glam::{DMat3, DVec3, Vec3};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::Arc,
};

/// Imported visible polygon meshes in right-handed, Y-up meters.
/// Authored hierarchy and geometric transforms are baked into vertices; instances start at identity.
#[derive(Clone, Debug)]
pub struct FbxModel {
    pub meshes: Vec<MeshInstance>,
    /// One source node name for each mesh instance.
    pub names: Vec<String>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub warnings: Vec<String>,
}

/// Read a static FBX model. Paths need not be UTF-8, and referenced files are never opened.
pub fn load_fbx(path: impl AsRef<Path>) -> EngineResult<FbxModel> {
    let path = path.as_ref();
    let bytes =
        std::fs::read(path).map_err(|error| format!("FBX '{}': {error}", path.display()))?;
    load_fbx_bytes(&bytes).map_err(|error| format!("FBX '{}': {error}", path.display()).into())
}

/// Parse an FBX from memory, independent of the GPU or filesystem.
pub fn load_fbx_bytes(bytes: &[u8]) -> EngineResult<FbxModel> {
    let source = ufbx::load_memory(
        bytes,
        ufbx::LoadOpts {
            file_format: ufbx::FileFormat::Fbx,
            target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
            target_unit_meters: 1.0,
            handedness_conversion_axis: ufbx::MirrorAxis::X,
            space_conversion: ufbx::SpaceConversion::ModifyGeometry,
            generate_missing_normals: true,
            normalize_normals: true,
            load_external_files: false,
            ignore_embedded: true,
            ..Default::default()
        },
    )
    .map_err(|error| format!("could not parse FBX: {error:?}"))?;
    let mut warnings = BTreeSet::new();
    if !source.textures.is_empty() {
        warnings.insert(
            "Textures are not loaded; only vertex and base/diffuse colors are rendered.".to_owned(),
        );
    }
    if !source.anim_stacks.is_empty() {
        warnings.insert(
            "Animation tracks are not evaluated; authored static node transforms are imported."
                .to_owned(),
        );
    }
    if !source.lights.is_empty() || !source.cameras.is_empty() {
        warnings.insert(
            "FBX cameras and lights are ignored; the engine scene controls lighting and view."
                .to_owned(),
        );
    }
    if !source.nurbs_surfaces.is_empty()
        || !source.line_curves.is_empty()
        || !source.nurbs_curves.is_empty()
    {
        warnings.insert(
            "Non-polygon geometry is ignored; convert curves and surfaces to meshes before export."
                .to_owned(),
        );
    }
    let mut model = FbxModel {
        meshes: Vec::new(),
        names: Vec::new(),
        warnings: Vec::new(),
        bounds_min: Vec3::splat(f32::INFINITY),
        bounds_max: Vec3::splat(f32::NEG_INFINITY),
    };
    for node in &source.nodes {
        let Some(mesh) = &node.mesh else {
            continue;
        };
        if !visible(node) {
            continue;
        }
        if !mesh.skin_deformers.is_empty()
            || !mesh.blend_deformers.is_empty()
            || !mesh.cache_deformers.is_empty()
        {
            return Err(format!("node '{}': skinning, blend shapes, and geometry caches are not supported; export a baked static mesh", node.element.name).into());
        }
        if mesh.num_triangles == 0 {
            warnings.insert(format!(
                "Node '{}' has no polygon triangles and was skipped.",
                node.element.name
            ));
            continue;
        }
        let converted = convert_mesh(node, mesh, &mut warnings)
            .map_err(|error| format!("node '{}': {error}", node.element.name))?;
        for vertex in converted.vertices() {
            let p = Vec3::from_array(vertex.position);
            model.bounds_min = model.bounds_min.min(p);
            model.bounds_max = model.bounds_max.max(p);
        }
        model.meshes.push(MeshInstance::new(Arc::new(converted)));
        model.names.push(node.element.name.to_string());
    }
    if model.meshes.is_empty() {
        return Err("FBX contains no visible polygon meshes".into());
    }
    model.warnings = warnings.into_iter().collect();
    Ok(model)
}

fn visible(mut node: &ufbx::Node) -> bool {
    loop {
        if !node.visible {
            return false;
        }
        match &node.parent {
            Some(parent) => node = parent,
            None => return true,
        }
    }
}

fn vector(v: ufbx::Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

fn base_color(node: &ufbx::Node, mesh: &ufbx::Mesh, index: u32) -> Vec3 {
    let materials = if node.materials.is_empty() {
        &mesh.materials
    } else {
        &node.materials
    };
    let Some(material) = materials.get(index as usize) else {
        return Vec3::splat(0.65);
    };
    let (color, factor) = if material.pbr.base_color.has_value {
        (&material.pbr.base_color, &material.pbr.base_factor)
    } else {
        (&material.fbx.diffuse_color, &material.fbx.diffuse_factor)
    };
    if !color.has_value {
        return Vec3::splat(0.65);
    }
    let value = color.value_vec4;
    Vec3::new(value.x as f32, value.y as f32, value.z as f32)
        * if factor.has_value {
            factor.value_vec4.x as f32
        } else {
            1.0
        }
}

fn convert_mesh(
    node: &ufbx::Node,
    mesh: &ufbx::Mesh,
    warnings: &mut BTreeSet<String>,
) -> EngineResult<Mesh> {
    let m = &node.geometry_to_world;
    let linear = DMat3::from_cols(
        DVec3::new(m.m00, m.m10, m.m20),
        DVec3::new(m.m01, m.m11, m.m21),
        DVec3::new(m.m02, m.m12, m.m22),
    );
    let translation = DVec3::new(m.m03, m.m13, m.m23);
    let determinant = linear.determinant();
    let normal_matrix = linear.inverse().transpose();
    if !linear.is_finite()
        || !translation.is_finite()
        || determinant == 0.0
        || !normal_matrix.is_finite()
    {
        return Err("non-finite or singular geometry transform".into());
    }
    if mesh.vertex_uv.exists {
        warnings.insert(
            "UV coordinates are not retained by the current vertex-colored renderer.".to_owned(),
        );
    }
    let size = mesh
        .max_face_triangles
        .checked_mul(3)
        .ok_or("FBX face is too large")?;
    let mut corners = vec![0; size];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut remap = HashMap::new();
    for (face_index, &face) in mesh.faces.iter().enumerate() {
        if face.num_indices < 3 {
            continue;
        }
        if mesh.face_hole.get(face_index).copied().unwrap_or(false) {
            return Err(
                "polygon holes are not supported; triangulate the mesh before exporting".into(),
            );
        }
        let count = mesh.triangulate_face(&mut corners, face) as usize * 3;
        let material = mesh
            .face_material
            .get(face_index)
            .copied()
            .unwrap_or(u32::MAX);
        let color = base_color(node, mesh, material);
        for triangle in corners[..count].as_chunks::<3>().0 {
            let mut triangle = *triangle;
            // ufbx already corrects winding for coordinate conversion. Only compensate the node's remaining mirror.
            if determinant < 0.0 {
                triangle.swap(1, 2);
            }
            for corner in triangle {
                let key = (corner, material);
                let index = if let Some(&index) = remap.get(&key) {
                    index
                } else {
                    let position = (linear * vector(mesh.vertex_position[corner as usize])
                        + translation)
                        .as_vec3();
                    let normal = (normal_matrix * vector(mesh.vertex_normal[corner as usize]))
                        .try_normalize()
                        .ok_or("a vertex has an invalid or zero-length normal")?
                        .as_vec3();
                    let mut color = color;
                    if mesh.vertex_color.exists {
                        let v = mesh.vertex_color[corner as usize];
                        color *= Vec3::new(v.x as f32, v.y as f32, v.z as f32);
                        if v.w < 0.999 {
                            warnings.insert(
                                "Vertex alpha is ignored; imported meshes are opaque.".to_owned(),
                            );
                        }
                    }
                    if !position.is_finite() || !normal.is_finite() || !color.is_finite() {
                        return Err(
                            "vertex attributes are non-finite or outside the engine's float range"
                                .into(),
                        );
                    }
                    let index = u32::try_from(vertices.len())
                        .map_err(|_| "FBX mesh exceeds 32-bit vertex indexing")?;
                    vertices.push(Vertex {
                        position: position.to_array(),
                        normal: normal.to_array(),
                        color: color.clamp(Vec3::ZERO, Vec3::ONE).to_array(),
                    });
                    remap.insert(key, index);
                    index
                };
                indices.push(index);
            }
        }
    }
    Mesh::new(vertices, indices)
}
