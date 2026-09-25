//! Rigid FBX node animation. Geometry stays immutable while instance transforms change.
use super::{FbxModel, fbx};
use crate::{render::EngineResult, scene::MeshInstance};
use glam::{DMat4, Mat4};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub start: f64,
    pub duration: f64,
}

/// Owns the parsed FBX and reusable rest geometry. Does not implement skeletal deformation.
pub struct AnimatedFbx {
    source: ufbx::SceneRoot,
    model: FbxModel,
    clips: Vec<AnimationClip>,
    bindings: Vec<(usize, DMat4)>,
}

impl AnimatedFbx {
    pub fn load(path: impl AsRef<Path>) -> EngineResult<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| format!("FBX '{}': {e}", path.display()))?;
        Self::from_bytes(&bytes).map_err(|e| format!("FBX '{}': {e}", path.display()).into())
    }

    pub fn from_bytes(bytes: &[u8]) -> EngineResult<Self> {
        let source = fbx::parse(bytes)?;
        let mut model = fbx::import_scene(&source)?;
        model
            .warnings
            .retain(|w| !w.starts_with("Animation tracks"));
        let clips = source
            .anim_stacks
            .iter()
            .map(|stack| AnimationClip {
                name: stack.element.name.to_string(),
                start: stack.time_begin,
                duration: stack.time_end - stack.time_begin,
            })
            .collect::<Vec<_>>();
        if clips.is_empty() {
            return Err("FBX contains no animation clips".into());
        }
        if clips
            .iter()
            .any(|c| !c.start.is_finite() || !c.duration.is_finite() || c.duration < 0.0)
        {
            return Err("FBX animation has an invalid time range".into());
        }
        let bindings = source
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                fbx::visible(n) && n.mesh.as_ref().is_some_and(|m| m.num_triangles > 0)
            })
            .map(|(i, n)| (i, matrix(&n.geometry_to_world).inverse()))
            .collect();
        Ok(Self {
            source,
            model,
            clips,
            bindings,
        })
    }

    /// Rest-pose geometry and bounds. Bounds do not include the animation's motion.
    pub fn model(&self) -> &FbxModel {
        &self.model
    }
    pub fn clips(&self) -> &[AnimationClip] {
        &self.clips
    }

    /// Sample seconds relative to the clip start. Looping wraps; non-looping clamps.
    /// Returned instances share the same CPU/GPU geometry across every sample.
    /// Placement is applied after the authored animation, for moving the entire model.
    pub fn sample(
        &self,
        clip: usize,
        seconds: f64,
        looping: bool,
        placement: Mat4,
    ) -> EngineResult<Vec<MeshInstance>> {
        if !seconds.is_finite() {
            return Err("animation time must be finite".into());
        }
        let info = self
            .clips
            .get(clip)
            .ok_or("animation clip index is out of range")?;
        let offset = if looping && info.duration > 0.0 {
            seconds.rem_euclid(info.duration)
        } else {
            seconds.clamp(0.0, info.duration)
        };
        let evaluated = ufbx::evaluate_scene(
            &self.source,
            &self.source.anim_stacks[clip].anim,
            info.start + offset,
            Default::default(),
        )
        .map_err(|e| format!("FBX animation evaluation failed: {e:?}"))?;
        self.bindings
            .iter()
            .zip(&self.model.meshes)
            .map(|(&(index, inverse), rest)| {
                let mut instance = rest.clone();
                let delta = (matrix(&evaluated.nodes[index].geometry_to_world) * inverse).as_mat4();
                instance.set_transform(placement * delta).map_err(|e| {
                    format!(
                        "animated node '{}': {e}",
                        self.source.nodes[index].element.name
                    )
                })?;
                Ok(instance)
            })
            .collect()
    }
}

fn matrix(m: &ufbx::Matrix) -> DMat4 {
    DMat4::from_cols_array(&[
        m.m00, m.m10, m.m20, 0.0, m.m01, m.m11, m.m21, 0.0, m.m02, m.m12, m.m22, 0.0, m.m03, m.m13,
        m.m23, 1.0,
    ])
}
