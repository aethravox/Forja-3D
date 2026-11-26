#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod animation;
pub mod codegen;
pub mod format;
pub mod glb;
pub mod math;
pub mod mesh_ops;
pub mod pack;
pub mod primitives;
pub mod project;
pub mod skeleton;
pub mod texture;
pub mod ui2d;

pub use animation::{
    AnimationClip, AnimationProperty, AnimationSet, AnimationTargetKind, AnimationTrack,
    InterpolationMode, Keyframe, F3A_MAGIC, F3A_VERSION,
};
pub use codegen::RustExportTarget;
pub use format::{
    fnv1a_32, ColliderShape, ColliderSpec, DecodeError, Material, MaterialPreset, MeshPart, Model,
    PhysicsSpec, RigidBodyKind, Vertex, FLAG_COMPACT, FLAG_PHYSICS, FORMAT_VERSION, MAGIC,
};
pub use glb::{encode_texture_to_png, GlbExportOptions, GLB_MAGIC, GLB_VERSION};
pub use math::{Aabb, Mat4, Vec3};
pub use mesh_ops::{
    apply_shading, build_extruded_polygon, build_lathe_profile, build_terrain,
    deduplicate_vertices, default_extrude_profile, default_lathe_profile, terrain_height_at,
    deform_vertices, subdivide_triangles, MeshDeformKind, ShadingMode, TerrainPreset,
    TerrainSpec, TerrainStamp,
};
pub use pack::{ForjaPack, Pack, F3P_MAGIC, F3P_VERSION};
pub use primitives::{PartSpec, PrimitiveKind, SceneDocument, SunLightSpec};
pub use project::{
    ForjaProject, ProjectCameraState, ProjectViewportState, F3S_MAGIC, F3S_VERSION,
    FLAG_PROJECT_ZLIB,
};
pub use skeleton::{Joint, Skeleton, VertexSkinning};
pub use texture::{Texture, TextureError, TexturePixelFormat, F3T_MAGIC, F3T_VERSION};
pub use ui2d::{Anchor2D, Document2D, UiDocument2D, Widget2D, WidgetKind2D, F2D_MAGIC, F2D_VERSION};
