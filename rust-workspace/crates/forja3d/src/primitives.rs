use alloc::{string::String, vec, vec::Vec};
use core::f32::consts::TAU;

use crate::{
    format::{
        fnv1a_32, ColliderShape, ColliderSpec, Material, MeshPart, Model, PhysicsSpec,
        RigidBodyKind, Vertex,
    },
    math::{sin_cos, Aabb, Mat4, Vec3},
    mesh_ops::{
        apply_shading, build_extruded_polygon, build_lathe_profile, build_terrain,
        deduplicate_vertices, default_extrude_profile, default_lathe_profile, ShadingMode,
        TerrainPreset, TerrainSpec,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Pyramid,
    Torus,
    Plane,
    Terrain,
    Extrude,
    Lathe,
    CustomMesh,
}

impl PrimitiveKind {
    pub const ALL: [Self; 10] = [
        Self::Cube,
        Self::Sphere,
        Self::Cylinder,
        Self::Cone,
        Self::Pyramid,
        Self::Torus,
        Self::Plane,
        Self::Terrain,
        Self::Extrude,
        Self::Lathe,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Cube => "Cubo",
            Self::Sphere => "Esfera",
            Self::Cylinder => "Cilindro",
            Self::Cone => "Cono",
            Self::Pyramid => "Pirámide",
            Self::Torus => "Toroide",
            Self::Plane => "Plano",
            Self::Terrain => "Terreno",
            Self::Extrude => "Extruir 2D",
            Self::Lathe => "Revolución",
            Self::CustomMesh => "Malla .f3d",
        }
    }

    pub const fn uses_profile(self) -> bool {
        matches!(self, Self::Extrude | Self::Lathe)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartSpec {
    pub name: String,
    pub kind: PrimitiveKind,
    pub position: [f32; 3],
    pub rotation_deg: [f32; 3],
    pub scale: [f32; 3],
    pub color: [u8; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub segments: u16,
    pub visible: bool,
    pub mirror: [bool; 3],
    pub group: Option<String>,
    pub shading: ShadingMode,
    pub deduplicate: bool,
    pub profile: Vec<[f32; 2]>,
    pub extrude_depth: f32,
    pub terrain: TerrainSpec,
    pub custom_mesh: Option<MeshPart>,
    pub collider: ColliderShape,
    pub collider_auto_fit: bool,
    pub collider_offset: [f32; 3],
}

impl PartSpec {
    pub fn new(name: impl Into<String>, kind: PrimitiveKind) -> Self {
        let profile = match kind {
            PrimitiveKind::Lathe => default_lathe_profile(),
            _ => default_extrude_profile(),
        };
        let is_terrain = kind == PrimitiveKind::Terrain;
        Self {
            name: name.into(),
            kind,
            position: if is_terrain { [0.0, 0.0, 0.0] } else { [0.0, 0.5, 0.0] },
            rotation_deg: [0.0, 0.0, 0.0],
            scale: if is_terrain { [4.5, 1.4, 4.5] } else { [1.0, 1.0, 1.0] },
            color: if is_terrain { [88, 146, 78, 255] } else { [224, 159, 62, 255] },
            metallic: if is_terrain { 0.05 } else { 0.2 },
            roughness: if is_terrain { 0.88 } else { 0.42 },
            segments: if is_terrain { 28 } else { 20 },
            visible: true,
            mirror: [false; 3],
            group: None,
            shading: ShadingMode::Original,
            deduplicate: true,
            profile,
            extrude_depth: 0.4,
            terrain: TerrainSpec::default(),
            custom_mesh: None,
            collider: ColliderShape::None,
            collider_auto_fit: true,
            collider_offset: [0.0; 3],
        }
    }

    pub fn recommended_collider_shape(&self) -> ColliderShape {
        match self.kind {
            PrimitiveKind::Sphere => ColliderShape::Sphere { radius: 0.5 },
            PrimitiveKind::Cylinder => ColliderShape::Capsule {
                radius: 0.5,
                half_height: 0.25,
            },
            _ => ColliderShape::Box {
                half_extents: [0.5, 0.5, 0.5],
            },
        }
    }

    pub fn fitted_collider_for_mesh(&self, baked: &MeshPart) -> Option<ColliderSpec> {
        let part_hash = fnv1a_32(baked.name.as_bytes());
        match self.collider {
            ColliderShape::None => None,
            _ if self.collider_auto_fit => {
                let (offset, shape) = match self.collider {
                    ColliderShape::Box { .. } => ColliderShape::fit_box(baked.bounds),
                    ColliderShape::Sphere { .. } => ColliderShape::fit_sphere(baked.bounds),
                    ColliderShape::Capsule { .. } => ColliderShape::fit_capsule(baked.bounds),
                    ColliderShape::None => unreachable!(),
                };
                Some(ColliderSpec {
                    part_hash,
                    offset,
                    shape,
                })
            }
            shape => {
                let b = baked.bounds;
                let center = if b.is_valid() {
                    [
                        (b.min[0] + b.max[0]) * 0.5,
                        (b.min[1] + b.max[1]) * 0.5,
                        (b.min[2] + b.max[2]) * 0.5,
                    ]
                } else {
                    self.position
                };
                Some(ColliderSpec {
                    part_hash,
                    offset: [
                        center[0] + self.collider_offset[0],
                        center[1] + self.collider_offset[1],
                        center[2] + self.collider_offset[2],
                    ],
                    shape,
                })
            }
        }
    }

    /// Wrap a decoded `MeshPart` (e.g., from a `.f3d` file) into an editable `PartSpec`
    /// centered at its bounding-box midpoint so gizmos & transforms act around its center.
    pub fn from_mesh_part(mesh: MeshPart) -> Self {
        let mut bounds = mesh.bounds;
        if !bounds.is_valid() {
            for v in &mesh.vertices {
                bounds.include_point(v.position);
            }
        }
        let center = if bounds.is_valid() {
            [
                (bounds.min[0] + bounds.max[0]) * 0.5,
                (bounds.min[1] + bounds.max[1]) * 0.5,
                (bounds.min[2] + bounds.max[2]) * 0.5,
            ]
        } else {
            [0.0, 0.0, 0.0]
        };

        let mut local_bounds = Aabb::EMPTY;
        let local_verts: Vec<Vertex> = mesh
            .vertices
            .iter()
            .map(|v| {
                let pos = [
                    v.position[0] - center[0],
                    v.position[1] - center[1],
                    v.position[2] - center[2],
                ];
                local_bounds.include_point(pos);
                Vertex {
                    position: pos,
                    ..*v
                }
            })
            .collect();

        let mat = mesh.material;
        let part_name = mesh.name.clone();
        let local_mesh = MeshPart {
            name: mesh.name,
            material: mat,
            vertices: local_verts,
            indices: mesh.indices,
            bounds: local_bounds,
        };

        Self {
            name: part_name,
            kind: PrimitiveKind::CustomMesh,
            position: center,
            rotation_deg: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            color: mat.base_color,
            metallic: mat.metallic,
            roughness: mat.roughness,
            segments: 20,
            visible: true,
            mirror: [false; 3],
            group: None,
            shading: ShadingMode::Original,
            deduplicate: true,
            profile: default_extrude_profile(),
            extrude_depth: 0.4,
            terrain: TerrainSpec::default(),
            custom_mesh: Some(local_mesh),
            collider: ColliderShape::None,
            collider_auto_fit: true,
            collider_offset: [0.0; 3],
        }
    }

    pub fn bake(&self) -> MeshPart {
        let (base_verts, base_indices) = match self.kind {
            PrimitiveKind::Extrude => build_extruded_polygon(&self.profile, self.extrude_depth, self.color),
            PrimitiveKind::Lathe => build_lathe_profile(&self.profile, self.segments.max(6), self.color),
            PrimitiveKind::Terrain => build_terrain(&self.terrain, self.segments.max(6), self.color),
            PrimitiveKind::CustomMesh => {
                if let Some(mesh) = &self.custom_mesh {
                    let recolor = self.color != mesh.material.base_color;
                    let verts = if recolor {
                        mesh.vertices
                            .iter()
                            .map(|v| Vertex {
                                color: self.color,
                                ..*v
                            })
                            .collect()
                    } else {
                        mesh.vertices.clone()
                    };
                    (verts, mesh.indices.clone())
                } else {
                    generate_primitive(PrimitiveKind::Cube, self.segments.max(6), self.color)
                }
            }
            _ => generate_primitive(self.kind, self.segments.max(6), self.color),
        };

        let (shaded_verts, shaded_indices) = apply_shading(base_verts, base_indices, self.shading);
        let (raw_verts, indices) = if self.deduplicate && self.shading != ShadingMode::Flat {
            deduplicate_vertices(shaded_verts, shaded_indices)
        } else {
            (shaded_verts, shaded_indices)
        };

        let mat = Mat4::from_trs(self.position, self.rotation_deg, self.scale);
        let mut bounds = Aabb::EMPTY;
        let vertices: Vec<Vertex> = raw_verts
            .into_iter()
            .map(|v| {
                let position = mat.transform_point(v.position);
                let normal = mat.transform_normal(v.normal, self.scale);
                bounds.include_point(position);
                Vertex {
                    position,
                    normal,
                    color: v.color,
                    uv: v.uv,
                }
            })
            .collect();

        MeshPart {
            name: self.name.clone(),
            material: Material {
                base_color: self.color,
                metallic: self.metallic,
                roughness: self.roughness,
            },
            vertices,
            indices,
            bounds,
        }
    }

    /// Bake this part along with any enabled mirror reflections (`[X, Y, Z]`).
    pub fn bake_all(&self) -> Vec<MeshPart> {
        let base = self.bake();
        let mut out = vec![base.clone()];

        let xs: &[f32] = if self.mirror[0] { &[1.0, -1.0] } else { &[1.0] };
        let ys: &[f32] = if self.mirror[1] { &[1.0, -1.0] } else { &[1.0] };
        let zs: &[f32] = if self.mirror[2] { &[1.0, -1.0] } else { &[1.0] };

        for &sx in xs {
            for &sy in ys {
                for &sz in zs {
                    if sx > 0.0 && sy > 0.0 && sz > 0.0 {
                        continue;
                    }
                    let mut suffix = String::new();
                    if sx < 0.0 {
                        suffix.push_str("_mx");
                    }
                    if sy < 0.0 {
                        suffix.push_str("_my");
                    }
                    if sz < 0.0 {
                        suffix.push_str("_mz");
                    }

                    let mut bounds = Aabb::EMPTY;
                    let vertices: Vec<Vertex> = base
                        .vertices
                        .iter()
                        .map(|v| {
                            let pos = [v.position[0] * sx, v.position[1] * sy, v.position[2] * sz];
                            let norm = [v.normal[0] * sx, v.normal[1] * sy, v.normal[2] * sz];
                            bounds.include_point(pos);
                            Vertex {
                                position: pos,
                                normal: norm,
                                color: v.color,
                                uv: v.uv,
                            }
                        })
                        .collect();

                    let flip_winding = (sx * sy * sz) < 0.0;
                    let mut indices = base.indices.clone();
                    if flip_winding {
                        for tri in indices.chunks_exact_mut(3) {
                            tri.swap(1, 2);
                        }
                    }

                    out.push(MeshPart {
                        name: alloc::format!("{}{}", base.name, suffix),
                        material: base.material,
                        vertices,
                        indices,
                        bounds,
                    });
                }
            }
        }

        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneDocument {
    pub model_name: String,
    pub parts: Vec<PartSpec>,
    pub physics_enabled: bool,
    pub physics: PhysicsSpec,
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            model_name: String::from("MiModelo"),
            parts: Vec::new(),
            physics_enabled: false,
            physics: PhysicsSpec::default(),
        }
    }
}

impl SceneDocument {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            ..Self::default()
        }
    }

    pub fn from_model(model: Model) -> Self {
        let phys_opt = model.physics;
        let mut parts: Vec<PartSpec> = model
            .parts
            .into_iter()
            .map(PartSpec::from_mesh_part)
            .collect();

        let (physics_enabled, physics) = if let Some(phys) = phys_opt {
            for part in &mut parts {
                let h = fnv1a_32(part.name.as_bytes());
                if let Some(c) = phys.colliders.iter().find(|c| c.part_hash == h) {
                    part.collider = c.shape;
                    part.collider_auto_fit = false;
                    part.collider_offset = [
                        c.offset[0] - part.position[0],
                        c.offset[1] - part.position[1],
                        c.offset[2] - part.position[2],
                    ];
                }
            }
            (
                true,
                PhysicsSpec {
                    colliders: Vec::new(),
                    ..phys
                },
            )
        } else {
            (false, PhysicsSpec::default())
        };

        Self {
            model_name: model.name,
            parts,
            physics_enabled,
            physics,
        }
    }

    /// Enable `.f3d` physics and assign auto-fitted colliders to any parts without one.
    pub fn auto_generate_colliders(&mut self) {
        self.physics_enabled = true;
        for part in &mut self.parts {
            if part.collider == ColliderShape::None {
                part.collider = part.recommended_collider_shape();
                part.collider_auto_fit = true;
                part.collider_offset = [0.0; 3];
            }
        }
    }

    pub fn bake(&self) -> Model {
        self.bake_with_part_map().0
    }

    /// Bakes all visible parts (including mirrors) and returns `(Model, source_part_index_per_mesh)`.
    pub fn bake_with_part_map(&self) -> (Model, Vec<usize>) {
        let mut model = Model::new(self.model_name.clone());
        let mut part_map = Vec::new();
        let mut colliders = Vec::new();

        for (idx, part) in self.parts.iter().enumerate() {
            if part.visible {
                for baked in part.bake_all() {
                    if self.physics_enabled {
                        if let Some(spec) = part.fitted_collider_for_mesh(&baked) {
                            colliders.push(spec);
                        }
                    }
                    model.parts.push(baked);
                    part_map.push(idx);
                }
            }
        }

        if self.physics_enabled {
            model.physics = Some(PhysicsSpec {
                colliders,
                ..self.physics.clone()
            });
        }

        (model, part_map)
    }

    pub fn group_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for part in &self.parts {
            if let Some(g) = &part.group {
                if !g.is_empty() && !names.iter().any(|existing| existing == g) {
                    names.push(g.clone());
                }
            }
        }
        names
    }

    pub fn preset_drone() -> Self {
        Self {
            model_name: String::from("ScoutDrone"),
            parts: vec![
                PartSpec {
                    position: [0.0, 0.75, 0.0],
                    rotation_deg: [0.0, 45.0, 0.0],
                    scale: [1.1, 0.35, 1.1],
                    color: [66, 75, 92, 255],
                    metallic: 0.75,
                    roughness: 0.32,
                    segments: 16,
                    group: Some(String::from("Cuerpo")),
                    collider: ColliderShape::Box { half_extents: [0.5; 3] },
                    ..PartSpec::new("chasis_central", PrimitiveKind::Cube)
                },
                PartSpec {
                    position: [0.0, 0.88, 0.42],
                    scale: [0.45, 0.32, 0.45],
                    color: [245, 158, 11, 255],
                    metallic: 0.25,
                    roughness: 0.18,
                    group: Some(String::from("Cuerpo")),
                    collider: ColliderShape::Sphere { radius: 0.5 },
                    ..PartSpec::new("nucleo_visor", PrimitiveKind::Sphere)
                },
                PartSpec {
                    position: [-1.05, 0.9, 0.0],
                    scale: [0.55, 0.35, 0.55],
                    color: [215, 222, 234, 255],
                    metallic: 0.8,
                    roughness: 0.28,
                    mirror: [true, false, false],
                    group: Some(String::from("Propulsion")),
                    collider: ColliderShape::Box { half_extents: [0.5; 3] },
                    ..PartSpec::new("rotor_lateral", PrimitiveKind::Torus)
                },
                PartSpec {
                    position: [0.0, 0.82, 0.0],
                    rotation_deg: [0.0, 0.0, 90.0],
                    scale: [0.12, 2.0, 0.12],
                    color: [110, 122, 142, 255],
                    metallic: 0.7,
                    roughness: 0.4,
                    segments: 14,
                    group: Some(String::from("Propulsion")),
                    ..PartSpec::new("brazo_transversal", PrimitiveKind::Cylinder)
                },
            ],
            physics_enabled: true,
            physics: PhysicsSpec {
                body_kind: RigidBodyKind::Dynamic,
                mass: 2.5,
                friction: 0.45,
                restitution: 0.35,
                linear_damping: 0.05,
                gravity_scale: 1.0,
                colliders: Vec::new(),
            },
        }
    }

    pub fn preset_obelisk() -> Self {
        Self {
            model_name: String::from("MonolitoForja"),
            parts: vec![
                PartSpec {
                    position: [0.0, 0.2, 0.0],
                    scale: [1.8, 0.4, 1.8],
                    color: [54, 62, 74, 255],
                    metallic: 0.2,
                    roughness: 0.75,
                    segments: 12,
                    group: Some(String::from("Estructura")),
                    collider: ColliderShape::Box { half_extents: [0.5; 3] },
                    ..PartSpec::new("base_pedestal", PrimitiveKind::Cube)
                },
                PartSpec {
                    position: [0.0, 1.15, 0.0],
                    scale: [0.75, 1.5, 0.75],
                    color: [178, 186, 198, 255],
                    metallic: 0.65,
                    roughness: 0.35,
                    segments: 8,
                    group: Some(String::from("Estructura")),
                    collider: ColliderShape::Capsule { radius: 0.4, half_height: 0.4 },
                    ..PartSpec::new("columna", PrimitiveKind::Cylinder)
                },
                PartSpec {
                    position: [0.0, 2.3, 0.0],
                    rotation_deg: [0.0, 45.0, 0.0],
                    scale: [0.82, 0.85, 0.82],
                    color: [245, 158, 11, 255],
                    metallic: 0.85,
                    roughness: 0.22,
                    segments: 12,
                    collider: ColliderShape::Box { half_extents: [0.5; 3] },
                    ..PartSpec::new("corona", PrimitiveKind::Pyramid)
                },
            ],
            physics_enabled: true,
            physics: PhysicsSpec {
                body_kind: RigidBodyKind::Static,
                mass: 10.0,
                friction: 0.6,
                restitution: 0.15,
                linear_damping: 0.04,
                gravity_scale: 1.0,
                colliders: Vec::new(),
            },
        }
    }

    pub fn preset_terrain() -> Self {
        Self {
            model_name: String::from("ValleForja"),
            parts: vec![
                PartSpec {
                    position: [0.0, 0.0, 0.0],
                    scale: [6.0, 1.8, 6.0],
                    color: [86, 144, 76, 255],
                    metallic: 0.04,
                    roughness: 0.9,
                    segments: 32,
                    terrain: TerrainSpec {
                        seed: 108,
                        amplitude: 0.55,
                        frequency: 2.8,
                        octaves: 4,
                        preset: TerrainPreset::Mountains,
                        biome_tint: true,
                        stamps: Vec::new(),
                    },
                    ..PartSpec::new("terreno_base", PrimitiveKind::Terrain)
                },
                PartSpec {
                    position: [0.0, 0.65, 0.0],
                    scale: [0.45, 0.9, 0.45],
                    color: [245, 158, 11, 255],
                    metallic: 0.72,
                    roughness: 0.25,
                    segments: 12,
                    ..PartSpec::new("faro_cumbre", PrimitiveKind::Pyramid)
                },
            ],
            physics_enabled: true,
            physics: PhysicsSpec {
                body_kind: RigidBodyKind::Static,
                mass: 50.0,
                friction: 0.75,
                restitution: 0.1,
                linear_damping: 0.05,
                gravity_scale: 1.0,
                colliders: Vec::new(),
            },
        }
    }
}

pub fn generate_primitive(kind: PrimitiveKind, segments: u16, color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    match kind {
        PrimitiveKind::Cube => build_cube(color),
        PrimitiveKind::Plane => build_plane(color),
        PrimitiveKind::Terrain => build_terrain(&TerrainSpec::default(), segments, color),
        PrimitiveKind::Pyramid => build_pyramid(color),
        PrimitiveKind::Sphere => build_sphere(segments, segments / 2 + 4, color),
        PrimitiveKind::Cylinder => build_cylinder(segments, color),
        PrimitiveKind::Cone => build_cone(segments, color),
        PrimitiveKind::Torus => build_torus(segments, (segments / 2).max(8), 0.34, color),
        PrimitiveKind::Extrude => build_extruded_polygon(&default_extrude_profile(), 0.4, color),
        PrimitiveKind::Lathe => build_lathe_profile(&default_lathe_profile(), segments, color),
        PrimitiveKind::CustomMesh => build_cube(color),
    }
}

fn push_quad(
    verts: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    normal: [f32; 3],
    color: [u8; 4],
) {
    let base = verts.len() as u32;
    verts.push(Vertex { position: p0, normal, color, uv: [0.0, 0.0] });
    verts.push(Vertex { position: p1, normal, color, uv: [1.0, 0.0] });
    verts.push(Vertex { position: p2, normal, color, uv: [1.0, 1.0] });
    verts.push(Vertex { position: p3, normal, color, uv: [0.0, 1.0] });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn build_cube(color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let mut v = Vec::with_capacity(24);
    let mut i = Vec::with_capacity(36);
    let h = 0.5;
    // +Z front
    push_quad(&mut v, &mut i, [-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h], [0.0, 0.0, 1.0], color);
    // -Z back
    push_quad(&mut v, &mut i, [h, -h, -h], [-h, -h, -h], [-h, h, -h], [h, h, -h], [0.0, 0.0, -1.0], color);
    // +X right
    push_quad(&mut v, &mut i, [h, -h, h], [h, -h, -h], [h, h, -h], [h, h, h], [1.0, 0.0, 0.0], color);
    // -X left
    push_quad(&mut v, &mut i, [-h, -h, -h], [-h, -h, h], [-h, h, h], [-h, h, -h], [-1.0, 0.0, 0.0], color);
    // +Y top
    push_quad(&mut v, &mut i, [-h, h, h], [h, h, h], [h, h, -h], [-h, h, -h], [0.0, 1.0, 0.0], color);
    // -Y bottom
    push_quad(&mut v, &mut i, [-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h], [0.0, -1.0, 0.0], color);
    (v, i)
}

fn build_plane(color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let mut v = Vec::with_capacity(4);
    let mut i = Vec::with_capacity(6);
    let h = 0.5;
    push_quad(&mut v, &mut i, [-h, 0.0, h], [h, 0.0, h], [h, 0.0, -h], [-h, 0.0, -h], [0.0, 1.0, 0.0], color);
    (v, i)
}

fn build_pyramid(color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let mut v = Vec::new();
    let mut i = Vec::new();
    let h = 0.5;
    let apex = [0.0, h, 0.0];
    let corners = [[-h, -h, h], [h, -h, h], [h, -h, -h], [-h, -h, -h]];

    for k in 0..4 {
        let a = corners[k];
        let b = corners[(k + 1) % 4];
        let ab = Vec3::new(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let ac = Vec3::new(apex[0] - a[0], apex[1] - a[1], apex[2] - a[2]);
        let n = ab.cross(ac).normalized().to_array();
        let base = v.len() as u32;
        v.push(Vertex { position: a, normal: n, color, uv: [0.0, 0.0] });
        v.push(Vertex { position: b, normal: n, color, uv: [1.0, 0.0] });
        v.push(Vertex { position: apex, normal: n, color, uv: [0.5, 1.0] });
        i.extend_from_slice(&[base, base + 1, base + 2]);
    }

    push_quad(&mut v, &mut i, corners[3], corners[2], corners[1], corners[0], [0.0, -1.0, 0.0], color);
    (v, i)
}

fn build_sphere(seg: u16, rings: u16, color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let seg = seg.max(6) as usize;
    let rings = rings.max(4) as usize;
    let mut v = Vec::with_capacity((seg + 1) * (rings + 1));
    let mut indices = Vec::with_capacity(seg * rings * 6);

    for r in 0..=rings {
        let v_frac = r as f32 / rings as f32;
        let phi = v_frac * core::f32::consts::PI;
        let (sin_p, cos_p) = sin_cos(phi);
        for s in 0..=seg {
            let u_frac = s as f32 / seg as f32;
            let theta = u_frac * TAU;
            let (sin_t, cos_t) = sin_cos(theta);
            let nx = sin_p * cos_t;
            let ny = cos_p;
            let nz = sin_p * sin_t;
            v.push(Vertex {
                position: [nx * 0.5, ny * 0.5, nz * 0.5],
                normal: [nx, ny, nz],
                color,
                uv: [u_frac, v_frac],
            });
        }
    }

    let stride = seg + 1;
    for r in 0..rings {
        for s in 0..seg {
            let a = (r * stride + s) as u32;
            let b = a + 1;
            let c = ((r + 1) * stride + s) as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (v, indices)
}

fn build_cylinder(seg: u16, color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let seg = seg.max(5) as usize;
    let mut v = Vec::new();
    let mut indices = Vec::new();
    let r = 0.5;
    let h = 0.5;

    for s in 0..=seg {
        let u = s as f32 / seg as f32;
        let a = u * TAU;
        let (sa, ca) = sin_cos(a);
        v.push(Vertex { position: [ca * r, -h, sa * r], normal: [ca, 0.0, sa], color, uv: [u, 0.0] });
        v.push(Vertex { position: [ca * r, h, sa * r], normal: [ca, 0.0, sa], color, uv: [u, 1.0] });
    }
    for s in 0..seg {
        let i0 = (s * 2) as u32;
        indices.extend_from_slice(&[i0, i0 + 1, i0 + 3, i0, i0 + 3, i0 + 2]);
    }

    // Top cap
    let top_center = v.len() as u32;
    v.push(Vertex { position: [0.0, h, 0.0], normal: [0.0, 1.0, 0.0], color, uv: [0.5, 0.5] });
    for s in 0..seg {
        let a0 = (s as f32 / seg as f32) * TAU;
        let a1 = ((s + 1) as f32 / seg as f32) * TAU;
        let (s0, c0) = sin_cos(a0);
        let (s1, c1) = sin_cos(a1);
        let b = v.len() as u32;
        v.push(Vertex { position: [c0 * r, h, s0 * r], normal: [0.0, 1.0, 0.0], color, uv: [0.5 + c0 * 0.5, 0.5 + s0 * 0.5] });
        v.push(Vertex { position: [c1 * r, h, s1 * r], normal: [0.0, 1.0, 0.0], color, uv: [0.5 + c1 * 0.5, 0.5 + s1 * 0.5] });
        indices.extend_from_slice(&[top_center, b + 1, b]);
    }

    // Bottom cap
    let bot_center = v.len() as u32;
    v.push(Vertex { position: [0.0, -h, 0.0], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
    for s in 0..seg {
        let a0 = (s as f32 / seg as f32) * TAU;
        let a1 = ((s + 1) as f32 / seg as f32) * TAU;
        let (s0, c0) = sin_cos(a0);
        let (s1, c1) = sin_cos(a1);
        let b = v.len() as u32;
        v.push(Vertex { position: [c0 * r, -h, s0 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5 + c0 * 0.5, 0.5 + s0 * 0.5] });
        v.push(Vertex { position: [c1 * r, -h, s1 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5 + c1 * 0.5, 0.5 + s1 * 0.5] });
        indices.extend_from_slice(&[bot_center, b, b + 1]);
    }

    (v, indices)
}

fn build_cone(seg: u16, color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let seg = seg.max(5) as usize;
    let mut v = Vec::new();
    let mut indices = Vec::new();
    let r = 0.5;
    let h = 0.5;
    let apex = [0.0, h, 0.0];

    for s in 0..seg {
        let a0 = (s as f32 / seg as f32) * TAU;
        let a1 = ((s + 1) as f32 / seg as f32) * TAU;
        let (s0, c0) = sin_cos(a0);
        let (s1, c1) = sin_cos(a1);
        let p0 = [c0 * r, -h, s0 * r];
        let p1 = [c1 * r, -h, s1 * r];
        let e1 = Vec3::new(p1[0] - p0[0], 0.0, p1[2] - p0[2]);
        let e2 = Vec3::new(apex[0] - p0[0], apex[1] - p0[1], apex[2] - p0[2]);
        let n = e2.cross(e1).normalized().to_array();
        let base = v.len() as u32;
        v.push(Vertex { position: p0, normal: n, color, uv: [0.0, 0.0] });
        v.push(Vertex { position: p1, normal: n, color, uv: [1.0, 0.0] });
        v.push(Vertex { position: apex, normal: n, color, uv: [0.5, 1.0] });
        indices.extend_from_slice(&[base, base + 2, base + 1]);
    }

    let bot_center = v.len() as u32;
    v.push(Vertex { position: [0.0, -h, 0.0], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
    for s in 0..seg {
        let a0 = (s as f32 / seg as f32) * TAU;
        let a1 = ((s + 1) as f32 / seg as f32) * TAU;
        let (s0, c0) = sin_cos(a0);
        let (s1, c1) = sin_cos(a1);
        let b = v.len() as u32;
        v.push(Vertex { position: [c0 * r, -h, s0 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
        v.push(Vertex { position: [c1 * r, -h, s1 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
        indices.extend_from_slice(&[bot_center, b, b + 1]);
    }
    (v, indices)
}

fn build_torus(major_seg: u16, minor_seg: u16, tube_r: f32, color: [u8; 4]) -> (Vec<Vertex>, Vec<u32>) {
    let major = major_seg.max(8) as usize;
    let minor = minor_seg.max(6) as usize;
    let main_r = 0.5;
    let mut v = Vec::with_capacity((major + 1) * (minor + 1));
    let mut indices = Vec::with_capacity(major * minor * 6);

    for i in 0..=major {
        let u = i as f32 / major as f32;
        let theta = u * TAU;
        let (st, ct) = sin_cos(theta);
        for j in 0..=minor {
            let v_frac = j as f32 / minor as f32;
            let phi = v_frac * TAU;
            let (sp, cp) = sin_cos(phi);

            let ring_r = main_r + tube_r * cp;
            let px = ring_r * ct;
            let py = tube_r * sp;
            let pz = ring_r * st;

            let nx = cp * ct;
            let ny = sp;
            let nz = cp * st;

            v.push(Vertex {
                position: [px, py, pz],
                normal: [nx, ny, nz],
                color,
                uv: [u, v_frac],
            });
        }
    }

    let stride = minor + 1;
    for i in 0..major {
        for j in 0..minor {
            let a = (i * stride + j) as u32;
            let b = ((i + 1) * stride + j) as u32;
            let c = b + 1;
            let d = a + 1;
            indices.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }

    (v, indices)
}
