use alloc::{string::String, vec::Vec};

/// A single skeletal joint/bone node within a model's bone hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    pub name: String,
    pub parent_index: Option<usize>,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 4], // Quaternion [x, y, z, w]
    pub local_scale: [f32; 3],
    pub inverse_bind_matrix: [f32; 16],
}

impl Default for Joint {
    fn default() -> Self {
        Self {
            name: String::new(),
            parent_index: None,
            local_position: [0.0; 3],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind_matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }
}

/// Up to 4 bone influences and normalized weights per vertex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VertexSkinning {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl Default for VertexSkinning {
    fn default() -> Self {
        Self {
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Skeletal rig containing all joints for skinned meshes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
}

impl Skeleton {
    pub fn new() -> Self {
        Self { joints: Vec::new() }
    }

    pub fn add_joint(&mut self, mut joint: Joint) -> usize {
        let index = self.joints.len();
        if joint.name.is_empty() {
            joint.name = alloc::format!("joint_{index}");
        }
        self.joints.push(joint);
        index
    }

    pub fn find_joint(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }
}
