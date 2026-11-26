use alloc::{string::String, vec::Vec};

use crate::format::{
    decode_physics, encode_physics, fnv1a_32, write_str, ColliderShape, Cursor, DecodeError,
    Material, MeshPart, RigidBodyKind, Vertex,
};
use crate::math::Aabb;
use crate::mesh_ops::{ShadingMode, TerrainPreset, TerrainSpec, TerrainStamp};
use crate::primitives::{PartSpec, PrimitiveKind, SceneDocument, SunLightSpec};
use crate::texture::Texture;
use crate::ui2d::UiDocument2D;

/// Binary magic header for complete Forja3D Studio project files (.forja / .f3s).
pub const F3S_MAGIC: [u8; 4] = *b"F3S1";
pub const F3S_VERSION: u16 = 2;
/// Header flag indicating zlib/deflate compressed payload via `miniz_oxide`.
pub const FLAG_PROJECT_ZLIB: u16 = 0x0001;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectCameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    pub ortho: bool,
}

impl Default for ProjectCameraState {
    fn default() -> Self {
        Self {
            yaw: 0.72,
            pitch: 0.45,
            distance: 5.0,
            target: [0.0, 0.75, 0.0],
            ortho: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectViewportState {
    pub textures_enabled: bool,
    pub wireframe: bool,
    pub show_grid: bool,
    pub show_colliders: bool,
}

impl Default for ProjectViewportState {
    fn default() -> Self {
        Self {
            textures_enabled: true,
            wireframe: false,
            show_grid: true,
            show_colliders: true,
        }
    }
}

/// Complete editable Forja3D Studio project (`.forja` / `F3S1`).
/// Preserves parametric `SceneDocument`, `SunLightSpec`s, imported `.f3t` textures,
/// optional 2D UI document (`.f2d`), camera framing, and viewport flags.
#[derive(Debug, Clone, PartialEq)]
pub struct ForjaProject {
    pub scene: SceneDocument,
    pub textures: Vec<Texture>,
    pub camera: ProjectCameraState,
    pub viewport: ProjectViewportState,
    pub doc2d: Option<UiDocument2D>,
}

impl Default for ForjaProject {
    fn default() -> Self {
        Self {
            scene: SceneDocument::default(),
            textures: Vec::new(),
            camera: ProjectCameraState::default(),
            viewport: ProjectViewportState::default(),
            doc2d: None,
        }
    }
}

impl ForjaProject {
    pub fn new(scene: SceneDocument) -> Self {
        Self {
            scene,
            ..Self::default()
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw = Vec::new();
        encode_scene(&mut raw, &self.scene);

        let tex_count = self.textures.len().min(u16::MAX as usize) as u16;
        raw.extend_from_slice(&tex_count.to_le_bytes());
        for tex in self.textures.iter().take(tex_count as usize) {
            let t_bytes = tex.to_bytes();
            let t_len = t_bytes.len().min(u32::MAX as usize) as u32;
            raw.extend_from_slice(&t_len.to_le_bytes());
            raw.extend_from_slice(&t_bytes);
        }

        // Camera state
        raw.extend_from_slice(&self.camera.yaw.to_le_bytes());
        raw.extend_from_slice(&self.camera.pitch.to_le_bytes());
        raw.extend_from_slice(&self.camera.distance.to_le_bytes());
        for f in self.camera.target {
            raw.extend_from_slice(&f.to_le_bytes());
        }
        raw.push(if self.camera.ortho { 1 } else { 0 });

        // Viewport flags
        let mut vp_bits = 0u8;
        if self.viewport.textures_enabled {
            vp_bits |= 1 << 0;
        }
        if self.viewport.wireframe {
            vp_bits |= 1 << 1;
        }
        if self.viewport.show_grid {
            vp_bits |= 1 << 2;
        }
        if self.viewport.show_colliders {
            vp_bits |= 1 << 3;
        }
        raw.push(vp_bits);

        // Optional 2D screen document
        if let Some(doc2d) = &self.doc2d {
            raw.push(1);
            let d_bytes = doc2d.to_bytes();
            let d_len = d_bytes.len().min(u32::MAX as usize) as u32;
            raw.extend_from_slice(&d_len.to_le_bytes());
            raw.extend_from_slice(&d_bytes);
        } else {
            raw.push(0);
        }

        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let (flags, payload) = if compressed.len() < raw.len() {
            (FLAG_PROJECT_ZLIB, compressed)
        } else {
            (0u16, raw)
        };

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&F3S_MAGIC);
        out.extend_from_slice(&F3S_VERSION.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != F3S_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != 1 && version != F3S_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        let expected_checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let stored_payload = &bytes[12..];
        let actual_checksum = fnv1a_32(stored_payload);
        if expected_checksum != actual_checksum {
            return Err(DecodeError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        let decompressed;
        let raw_payload: &[u8] = if (flags & FLAG_PROJECT_ZLIB) != 0 {
            decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(stored_payload)
                .map_err(|_| DecodeError::InvalidPhysicsData)?;
            &decompressed
        } else {
            stored_payload
        };

        let mut cursor = Cursor::new(raw_payload);
        let scene = decode_scene(&mut cursor, version)?;

        let tex_count = cursor.read_u16()? as usize;
        let mut textures = Vec::with_capacity(tex_count.min(512));
        for _ in 0..tex_count {
            let t_len = cursor.read_u32()? as usize;
            let t_slice = cursor.take(t_len)?;
            let tex = Texture::from_bytes(t_slice).map_err(|_| DecodeError::InvalidPhysicsData)?;
            textures.push(tex);
        }

        let camera = ProjectCameraState {
            yaw: cursor.read_f32()?,
            pitch: cursor.read_f32()?,
            distance: cursor.read_f32()?,
            target: [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?],
            ortho: read_bool(&mut cursor)?,
        };

        let vp_bits = cursor.take(1)?[0];
        let viewport = ProjectViewportState {
            textures_enabled: (vp_bits & (1 << 0)) != 0,
            wireframe: (vp_bits & (1 << 1)) != 0,
            show_grid: (vp_bits & (1 << 2)) != 0,
            show_colliders: (vp_bits & (1 << 3)) != 0,
        };

        let has_doc2d = read_bool(&mut cursor)?;
        let doc2d = if has_doc2d {
            let d_len = cursor.read_u32()? as usize;
            let d_slice = cursor.take(d_len)?;
            Some(UiDocument2D::from_bytes(d_slice)?)
        } else {
            None
        };

        Ok(Self {
            scene,
            textures,
            camera,
            viewport,
            doc2d,
        })
    }
}

fn read_bool(cursor: &mut Cursor<'_>) -> Result<bool, DecodeError> {
    Ok(cursor.take(1)?[0] != 0)
}

fn write_f32_3(buf: &mut Vec<u8>, v: [f32; 3]) {
    for f in v {
        buf.extend_from_slice(&f.to_le_bytes());
    }
}

fn read_f32_3(cursor: &mut Cursor<'_>) -> Result<[f32; 3], DecodeError> {
    Ok([cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?])
}

fn primitive_to_u8(kind: PrimitiveKind) -> u8 {
    match kind {
        PrimitiveKind::Cube => 0,
        PrimitiveKind::Sphere => 1,
        PrimitiveKind::Cylinder => 2,
        PrimitiveKind::Cone => 3,
        PrimitiveKind::Pyramid => 4,
        PrimitiveKind::Torus => 5,
        PrimitiveKind::Plane => 6,
        PrimitiveKind::Terrain => 7,
        PrimitiveKind::Extrude => 8,
        PrimitiveKind::Lathe => 9,
        PrimitiveKind::CustomMesh => 10,
    }
}

fn primitive_from_u8(v: u8) -> Result<PrimitiveKind, DecodeError> {
    match v {
        0 => Ok(PrimitiveKind::Cube),
        1 => Ok(PrimitiveKind::Sphere),
        2 => Ok(PrimitiveKind::Cylinder),
        3 => Ok(PrimitiveKind::Cone),
        4 => Ok(PrimitiveKind::Pyramid),
        5 => Ok(PrimitiveKind::Torus),
        6 => Ok(PrimitiveKind::Plane),
        7 => Ok(PrimitiveKind::Terrain),
        8 => Ok(PrimitiveKind::Extrude),
        9 => Ok(PrimitiveKind::Lathe),
        10 => Ok(PrimitiveKind::CustomMesh),
        _ => Err(DecodeError::InvalidPhysicsData),
    }
}

fn shading_to_u8(s: ShadingMode) -> u8 {
    match s {
        ShadingMode::Original => 0,
        ShadingMode::Flat => 1,
        ShadingMode::Smooth => 2,
    }
}

fn shading_from_u8(v: u8) -> Result<ShadingMode, DecodeError> {
    match v {
        0 => Ok(ShadingMode::Original),
        1 => Ok(ShadingMode::Flat),
        2 => Ok(ShadingMode::Smooth),
        _ => Err(DecodeError::InvalidPhysicsData),
    }
}

fn terrain_preset_to_u8(p: TerrainPreset) -> u8 {
    match p {
        TerrainPreset::Hills => 0,
        TerrainPreset::Mountains => 1,
        TerrainPreset::Island => 2,
        TerrainPreset::Canyon => 3,
    }
}

fn terrain_preset_from_u8(v: u8) -> Result<TerrainPreset, DecodeError> {
    match v {
        0 => Ok(TerrainPreset::Hills),
        1 => Ok(TerrainPreset::Mountains),
        2 => Ok(TerrainPreset::Island),
        3 => Ok(TerrainPreset::Canyon),
        _ => Err(DecodeError::InvalidPhysicsData),
    }
}

fn encode_scene(buf: &mut Vec<u8>, scene: &SceneDocument) {
    write_str(buf, &scene.model_name);

    let part_count = scene.parts.len().min(u32::MAX as usize) as u32;
    buf.extend_from_slice(&part_count.to_le_bytes());
    for part in scene.parts.iter().take(part_count as usize) {
        encode_part(buf, part);
    }

    let sun_count = scene.sun_lights.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&sun_count.to_le_bytes());
    for sun in scene.sun_lights.iter().take(sun_count as usize) {
        write_str(buf, &sun.name);
        write_f32_3(buf, sun.position);
        write_f32_3(buf, sun.direction);
        buf.extend_from_slice(&sun.color);
        buf.extend_from_slice(&sun.intensity.to_le_bytes());
        buf.extend_from_slice(&sun.ambient.to_le_bytes());
        buf.push(if sun.enabled { 1 } else { 0 });
        buf.push(if sun.visible { 1 } else { 0 });
        buf.push(if sun.aim_at_origin { 1 } else { 0 });
    }

    buf.push(if scene.physics_enabled { 1 } else { 0 });
    encode_physics(buf, &scene.physics);
}

fn decode_scene(cursor: &mut Cursor<'_>, version: u16) -> Result<SceneDocument, DecodeError> {
    let model_name = cursor.read_str()?;

    let part_count = cursor.read_u32()? as usize;
    let mut parts = Vec::with_capacity(part_count.min(4096));
    for _ in 0..part_count {
        parts.push(decode_part(cursor, version)?);
    }

    let sun_count = cursor.read_u16()? as usize;
    let mut sun_lights = Vec::with_capacity(sun_count.min(64));
    for _ in 0..sun_count {
        let name = cursor.read_str()?;
        let position = read_f32_3(cursor)?;
        let direction = read_f32_3(cursor)?;
        let color = cursor.read_4()?;
        let intensity = cursor.read_f32()?;
        let ambient = cursor.read_f32()?;
        let enabled = read_bool(cursor)?;
        let visible = read_bool(cursor)?;
        let aim_at_origin = read_bool(cursor)?;
        sun_lights.push(SunLightSpec {
            name,
            position,
            direction,
            color,
            intensity,
            ambient,
            enabled,
            visible,
            aim_at_origin,
        });
    }

    let physics_enabled = read_bool(cursor)?;
    let physics = decode_physics(cursor)?;

    Ok(SceneDocument {
        model_name,
        parts,
        sun_lights,
        physics_enabled,
        physics,
    })
}

fn encode_part(buf: &mut Vec<u8>, part: &PartSpec) {
    write_str(buf, &part.name);
    buf.push(primitive_to_u8(part.kind));
    write_f32_3(buf, part.position);
    write_f32_3(buf, part.rotation_deg);
    write_f32_3(buf, part.scale);
    buf.extend_from_slice(&part.color);
    buf.extend_from_slice(&part.metallic.to_le_bytes());
    buf.extend_from_slice(&part.roughness.to_le_bytes());
    buf.extend_from_slice(&part.segments.to_le_bytes());
    buf.push(if part.visible { 1 } else { 0 });
    for m in part.mirror {
        buf.push(if m { 1 } else { 0 });
    }
    if let Some(group) = &part.group {
        buf.push(1);
        write_str(buf, group);
    } else {
        buf.push(0);
    }
    buf.push(shading_to_u8(part.shading));
    buf.push(if part.deduplicate { 1 } else { 0 });

    let prof_count = part.profile.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&prof_count.to_le_bytes());
    for pt in part.profile.iter().take(prof_count as usize) {
        buf.extend_from_slice(&pt[0].to_le_bytes());
        buf.extend_from_slice(&pt[1].to_le_bytes());
    }
    buf.extend_from_slice(&part.extrude_depth.to_le_bytes());

    // TerrainSpec
    buf.extend_from_slice(&part.terrain.seed.to_le_bytes());
    buf.extend_from_slice(&part.terrain.amplitude.to_le_bytes());
    buf.extend_from_slice(&part.terrain.frequency.to_le_bytes());
    buf.push(part.terrain.octaves);
    buf.push(terrain_preset_to_u8(part.terrain.preset));
    buf.push(if part.terrain.biome_tint { 1 } else { 0 });
    let stamp_count = part.terrain.stamps.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&stamp_count.to_le_bytes());
    for s in part.terrain.stamps.iter().take(stamp_count as usize) {
        buf.extend_from_slice(&s.x.to_le_bytes());
        buf.extend_from_slice(&s.z.to_le_bytes());
        buf.extend_from_slice(&s.radius.to_le_bytes());
        buf.extend_from_slice(&s.height.to_le_bytes());
    }

    // Optional CustomMesh
    if let Some(mesh) = &part.custom_mesh {
        buf.push(1);
        write_str(buf, &mesh.name);
        buf.extend_from_slice(&mesh.material.base_color);
        buf.extend_from_slice(&mesh.material.metallic.to_le_bytes());
        buf.extend_from_slice(&mesh.material.roughness.to_le_bytes());
        write_f32_3(buf, mesh.bounds.min);
        write_f32_3(buf, mesh.bounds.max);
        let v_count = mesh.vertices.len().min(u32::MAX as usize) as u32;
        buf.extend_from_slice(&v_count.to_le_bytes());
        for v in mesh.vertices.iter().take(v_count as usize) {
            write_f32_3(buf, v.position);
            write_f32_3(buf, v.normal);
            buf.extend_from_slice(&v.color);
            buf.extend_from_slice(&v.uv[0].to_le_bytes());
            buf.extend_from_slice(&v.uv[1].to_le_bytes());
        }
        let i_count = mesh.indices.len().min(u32::MAX as usize) as u32;
        buf.extend_from_slice(&i_count.to_le_bytes());
        for &idx in mesh.indices.iter().take(i_count as usize) {
            buf.extend_from_slice(&idx.to_le_bytes());
        }
    } else {
        buf.push(0);
    }

    // Collider & per-part physics
    match part.collider {
        ColliderShape::None => buf.push(0),
        ColliderShape::Box { half_extents } => {
            buf.push(1);
            write_f32_3(buf, half_extents);
        }
        ColliderShape::Sphere { radius } => {
            buf.push(2);
            buf.extend_from_slice(&radius.to_le_bytes());
        }
        ColliderShape::Capsule { radius, half_height } => {
            buf.push(3);
            buf.extend_from_slice(&radius.to_le_bytes());
            buf.extend_from_slice(&half_height.to_le_bytes());
        }
    }
    buf.push(if part.collider_auto_fit { 1 } else { 0 });
    write_f32_3(buf, part.collider_offset);
    buf.push(part.body_kind as u8);
    buf.extend_from_slice(&part.mass.to_le_bytes());
    buf.extend_from_slice(&part.friction.to_le_bytes());
    buf.extend_from_slice(&part.restitution.to_le_bytes());
    buf.extend_from_slice(&part.linear_damping.to_le_bytes());
    buf.extend_from_slice(&part.gravity_scale.to_le_bytes());

    // Texture binding
    buf.push(if part.texture_enabled { 1 } else { 0 });
    if let Some(ti) = part.texture_index {
        buf.push(1);
        buf.extend_from_slice(&(ti.min(u32::MAX as usize) as u32).to_le_bytes());
    } else {
        buf.push(0);
    }

    // v2: UV transform & alpha cutoff
    buf.extend_from_slice(&part.uv_scale[0].to_le_bytes());
    buf.extend_from_slice(&part.uv_scale[1].to_le_bytes());
    buf.extend_from_slice(&part.uv_offset[0].to_le_bytes());
    buf.extend_from_slice(&part.uv_offset[1].to_le_bytes());
    buf.extend_from_slice(&part.alpha_cutoff.to_le_bytes());
}

fn decode_part(cursor: &mut Cursor<'_>, version: u16) -> Result<PartSpec, DecodeError> {
    let name = cursor.read_str()?;
    let kind = primitive_from_u8(cursor.take(1)?[0])?;
    let position = read_f32_3(cursor)?;
    let rotation_deg = read_f32_3(cursor)?;
    let scale = read_f32_3(cursor)?;
    let color = cursor.read_4()?;
    let metallic = cursor.read_f32()?;
    let roughness = cursor.read_f32()?;
    let segments = cursor.read_u16()?;
    let visible = read_bool(cursor)?;
    let mirror = [read_bool(cursor)?, read_bool(cursor)?, read_bool(cursor)?];
    let has_group = read_bool(cursor)?;
    let group = if has_group {
        Some(cursor.read_str()?)
    } else {
        None
    };
    let shading = shading_from_u8(cursor.take(1)?[0])?;
    let deduplicate = read_bool(cursor)?;

    let prof_count = cursor.read_u16()? as usize;
    let mut profile = Vec::with_capacity(prof_count.min(4096));
    for _ in 0..prof_count {
        profile.push([cursor.read_f32()?, cursor.read_f32()?]);
    }
    let extrude_depth = cursor.read_f32()?;

    let seed = cursor.read_u32()?;
    let amplitude = cursor.read_f32()?;
    let frequency = cursor.read_f32()?;
    let octaves = cursor.take(1)?[0];
    let preset = terrain_preset_from_u8(cursor.take(1)?[0])?;
    let biome_tint = read_bool(cursor)?;
    let stamp_count = cursor.read_u16()? as usize;
    let mut stamps = Vec::with_capacity(stamp_count.min(4096));
    for _ in 0..stamp_count {
        stamps.push(TerrainStamp {
            x: cursor.read_f32()?,
            z: cursor.read_f32()?,
            radius: cursor.read_f32()?,
            height: cursor.read_f32()?,
        });
    }
    let terrain = TerrainSpec {
        seed,
        amplitude,
        frequency,
        octaves,
        preset,
        biome_tint,
        stamps,
    };

    let has_custom = read_bool(cursor)?;
    let custom_mesh = if has_custom {
        let m_name = cursor.read_str()?;
        let base_color = cursor.read_4()?;
        let m_metallic = cursor.read_f32()?;
        let m_roughness = cursor.read_f32()?;
        let bounds = Aabb {
            min: read_f32_3(cursor)?,
            max: read_f32_3(cursor)?,
        };
        let v_count = cursor.read_u32()? as usize;
        let mut vertices = Vec::with_capacity(v_count.min(131072));
        for _ in 0..v_count {
            let v_pos = read_f32_3(cursor)?;
            let v_norm = read_f32_3(cursor)?;
            let v_col = cursor.read_4()?;
            let v_uv = [cursor.read_f32()?, cursor.read_f32()?];
            vertices.push(Vertex {
                position: v_pos,
                normal: v_norm,
                color: v_col,
                uv: v_uv,
            });
        }
        let i_count = cursor.read_u32()? as usize;
        let mut indices = Vec::with_capacity(i_count.min(262144));
        for _ in 0..i_count {
            let idx = cursor.read_u32()?;
            if (idx as usize) >= v_count {
                return Err(DecodeError::IndexOutOfBounds {
                    index: idx,
                    vertex_count: v_count,
                });
            }
            indices.push(idx);
        }
        Some(MeshPart {
            name: m_name,
            material: Material {
                base_color,
                metallic: m_metallic,
                roughness: m_roughness,
            },
            vertices,
            indices,
            bounds,
        })
    } else {
        None
    };

    let col_tag = cursor.take(1)?[0];
    let collider = match col_tag {
        0 => ColliderShape::None,
        1 => ColliderShape::Box {
            half_extents: read_f32_3(cursor)?,
        },
        2 => ColliderShape::Sphere {
            radius: cursor.read_f32()?,
        },
        3 => ColliderShape::Capsule {
            radius: cursor.read_f32()?,
            half_height: cursor.read_f32()?,
        },
        _ => return Err(DecodeError::InvalidPhysicsData),
    };
    let collider_auto_fit = read_bool(cursor)?;
    let collider_offset = read_f32_3(cursor)?;
    let body_kind =
        RigidBodyKind::from_u8(cursor.take(1)?[0]).ok_or(DecodeError::InvalidPhysicsData)?;
    let mass = cursor.read_f32()?;
    let friction = cursor.read_f32()?;
    let restitution = cursor.read_f32()?;
    let linear_damping = cursor.read_f32()?;
    let gravity_scale = cursor.read_f32()?;

    let texture_enabled = read_bool(cursor)?;
    let has_tex_idx = read_bool(cursor)?;
    let texture_index = if has_tex_idx {
        Some(cursor.read_u32()? as usize)
    } else {
        None
    };

    let (uv_scale, uv_offset, alpha_cutoff) = if version >= 2 {
        (
            [cursor.read_f32()?, cursor.read_f32()?],
            [cursor.read_f32()?, cursor.read_f32()?],
            cursor.read_f32()?,
        )
    } else {
        ([1.0, 1.0], [0.0, 0.0], 0.0)
    };

    Ok(PartSpec {
        name,
        kind,
        position,
        rotation_deg,
        scale,
        color,
        metallic,
        roughness,
        segments,
        visible,
        mirror,
        group,
        shading,
        deduplicate,
        profile,
        extrude_depth,
        terrain,
        custom_mesh,
        collider,
        collider_auto_fit,
        collider_offset,
        body_kind,
        mass,
        friction,
        restitution,
        linear_damping,
        gravity_scale,
        texture_enabled,
        texture_index,
        uv_scale,
        uv_offset,
        alpha_cutoff,
    })
}

pub fn new_default_project_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        String::from("proyecto")
    } else {
        String::from(trimmed)
    }
}
