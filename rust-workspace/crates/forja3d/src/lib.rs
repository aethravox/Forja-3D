#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod codegen;
pub mod format;
pub mod math;
pub mod mesh_ops;
pub mod primitives;
pub mod skeleton;
pub mod texture;

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
pub use primitives::{PartSpec, PrimitiveKind, SceneDocument};
pub use skeleton::{Joint, Skeleton, VertexSkinning};
pub use texture::{Texture, TextureError, TexturePixelFormat, F3T_MAGIC, F3T_VERSION};
