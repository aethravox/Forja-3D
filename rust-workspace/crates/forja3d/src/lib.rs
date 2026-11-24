#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod animation;
pub mod codegen;
pub mod format;
pub mod math;
pub mod mesh_ops;
pub mod physics_file;
pub mod primitives;
pub mod skeleton;
pub mod sprite2d;
pub mod texture;

pub use animation::{
    AnimationClip, AnimationProperty, AnimationSet, AnimationTrack, InterpolationMode, Keyframe,
    F3A_MAGIC, F3A_VERSION,
};
pub use codegen::RustExportTarget;
pub use format::{
    ColliderShape, ColliderSpec, DecodeError, Material, MeshPart, Model, PhysicsSpec,
    RigidBodyKind, Vertex, FLAG_COMPACT, FLAG_PHYSICS, FORMAT_VERSION, MAGIC,
};
pub use math::{Aabb, Mat4, Vec3};
pub use mesh_ops::{
    apply_shading, build_extruded_polygon, build_lathe_profile, deduplicate_vertices,
    default_extrude_profile, default_lathe_profile, ShadingMode,
};
pub use physics_file::{PhysicsAsset, F3P_MAGIC, F3P_VERSION};
pub use primitives::{PartSpec, PrimitiveKind, SceneDocument};
pub use skeleton::{Joint, Skeleton, VertexSkinning};
pub use sprite2d::{Document2D, Path2D, SpriteQuad2D, F2D_MAGIC, F2D_VERSION};
pub use texture::{Texture, TextureError, TexturePixelFormat, F3T_MAGIC, F3T_VERSION};
