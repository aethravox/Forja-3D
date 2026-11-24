#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod codegen;
pub mod format;
pub mod math;
pub mod mesh_ops;
pub mod primitives;

pub use codegen::RustExportTarget;
pub use format::{
    DecodeError, Material, MeshPart, Model, Vertex, FLAG_COMPACT, FORMAT_VERSION, MAGIC,
};
pub use math::{Aabb, Mat4, Vec3};
pub use mesh_ops::{
    apply_shading, build_extruded_polygon, build_lathe_profile, deduplicate_vertices,
    default_extrude_profile, default_lathe_profile, ShadingMode,
};
pub use primitives::{PartSpec, PrimitiveKind, SceneDocument};
