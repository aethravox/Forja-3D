use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::math::Aabb;

/// Binary file magic header for Forja3D (.f3d) models.
pub const MAGIC: [u8; 4] = *b"F3D1";
/// Current specification version.
pub const FORMAT_VERSION: u16 = 1;
/// Header flag indicating quantized compact vertex/index encoding (17 B/vertex + u16 indices).
pub const FLAG_COMPACT: u16 = 0x0001;
/// Header flag indicating embedded rigid-body and collider metadata (`PhysicsSpec`).
pub const FLAG_PHYSICS: u16 = 0x0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RigidBodyKind {
    Static = 0,
    #[default]
    Dynamic = 1,
    Kinematic = 2,
    Sensor = 3,
}

impl RigidBodyKind {
    pub const ALL: [Self; 4] = [Self::Dynamic, Self::Static, Self::Kinematic, Self::Sensor];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Static => "Estático",
            Self::Dynamic => "Dinámico",
            Self::Kinematic => "Cinemático",
            Self::Sensor => "Sensor",
        }
    }

    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Static),
            1 => Some(Self::Dynamic),
            2 => Some(Self::Kinematic),
            3 => Some(Self::Sensor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ColliderShape {
    #[default]
    None,
    Box {
        half_extents: [f32; 3],
    },
    Sphere {
        radius: f32,
    },
    Capsule {
        radius: f32,
        half_height: f32,
    },
}

impl ColliderShape {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "Ninguno",
            Self::Box { .. } => "Caja",
            Self::Sphere { .. } => "Esfera",
            Self::Capsule { .. } => "Cápsula",
        }
    }

    /// Fit an axis-aligned box collider to a bounding box, returning `(center_offset, shape)`.
    pub fn fit_box(bounds: Aabb) -> ([f32; 3], Self) {
        if !bounds.is_valid() {
            return ([0.0; 3], Self::Box { half_extents: [0.25; 3] });
        }
        let center = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
            (bounds.min[2] + bounds.max[2]) * 0.5,
        ];
        let half_extents = [
            ((bounds.max[0] - bounds.min[0]) * 0.5).max(0.02),
            ((bounds.max[1] - bounds.min[1]) * 0.5).max(0.02),
            ((bounds.max[2] - bounds.min[2]) * 0.5).max(0.02),
        ];
        (center, Self::Box { half_extents })
    }

    /// Fit a bounding sphere collider to an AABB, returning `(center_offset, shape)`.
    pub fn fit_sphere(bounds: Aabb) -> ([f32; 3], Self) {
        if !bounds.is_valid() {
            return ([0.0; 3], Self::Sphere { radius: 0.25 });
        }
        let center = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
            (bounds.min[2] + bounds.max[2]) * 0.5,
        ];
        let hx = (bounds.max[0] - bounds.min[0]) * 0.5;
        let hy = (bounds.max[1] - bounds.min[1]) * 0.5;
        let hz = (bounds.max[2] - bounds.min[2]) * 0.5;
        let radius = hx.max(hy).max(hz).max(0.02);
        (center, Self::Sphere { radius })
    }

    /// Fit a Y-axis capsule collider to an AABB, returning `(center_offset, shape)`.
    pub fn fit_capsule(bounds: Aabb) -> ([f32; 3], Self) {
        if !bounds.is_valid() {
            return ([0.0; 3], Self::Capsule { radius: 0.2, half_height: 0.25 });
        }
        let center = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
            (bounds.min[2] + bounds.max[2]) * 0.5,
        ];
        let hx = (bounds.max[0] - bounds.min[0]) * 0.5;
        let hy = (bounds.max[1] - bounds.min[1]) * 0.5;
        let hz = (bounds.max[2] - bounds.min[2]) * 0.5;
        let radius = hx.max(hz).max(0.02);
        let half_height = (hy - radius).max(0.01);
        (center, Self::Capsule { radius, half_height })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColliderSpec {
    pub part_hash: u32,
    pub offset: [f32; 3],
    pub shape: ColliderShape,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsSpec {
    pub body_kind: RigidBodyKind,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub gravity_scale: f32,
    pub colliders: Vec<ColliderSpec>,
}

impl Default for PhysicsSpec {
    fn default() -> Self {
        Self {
            body_kind: RigidBodyKind::Dynamic,
            mass: 1.0,
            friction: 0.45,
            restitution: 0.3,
            linear_damping: 0.04,
            gravity_scale: 1.0,
            colliders: Vec::new(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub base_color: [u8; 4],
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [220, 224, 232, 255],
            metallic: 0.15,
            roughness: 0.45,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshPart {
    pub name: String,
    pub material: Material,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds: Aabb,
}

impl MeshPart {
    pub fn recompute_bounds(&mut self) {
        let mut aabb = Aabb::EMPTY;
        for v in &self.vertices {
            aabb.include_point(v.position);
        }
        self.bounds = aabb;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub parts: Vec<MeshPart>,
    pub physics: Option<PhysicsSpec>,
}

impl Model {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parts: Vec::new(),
            physics: None,
        }
    }

    /// Generate automatic bounding-box colliders for every mesh part and attach a `PhysicsSpec`.
    pub fn with_auto_physics(mut self, body_kind: RigidBodyKind) -> Self {
        let mut colliders = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            let (offset, shape) = ColliderShape::fit_box(part.bounds);
            colliders.push(ColliderSpec {
                part_hash: fnv1a_32(part.name.as_bytes()),
                offset,
                shape,
            });
        }
        if colliders.is_empty() && self.bounds().is_valid() {
            let (offset, shape) = ColliderShape::fit_box(self.bounds());
            colliders.push(ColliderSpec {
                part_hash: fnv1a_32(self.name.as_bytes()),
                offset,
                shape,
            });
        }
        self.physics = Some(PhysicsSpec {
            body_kind,
            colliders,
            ..PhysicsSpec::default()
        });
        self
    }

    pub fn vertex_count(&self) -> usize {
        self.parts.iter().map(|p| p.vertices.len()).sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.indices.len() / 3).sum()
    }

    pub fn bounds(&self) -> Aabb {
        self.parts
            .iter()
            .fold(Aabb::EMPTY, |acc, part| acc.merge(part.bounds))
    }

    /// Encode this model into the binary `.f3d` (`F3D1`) format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);

        for part in &self.parts {
            write_str(&mut payload, &part.name);
            payload.extend_from_slice(&part.material.base_color);
            payload.extend_from_slice(&part.material.metallic.to_le_bytes());
            payload.extend_from_slice(&part.material.roughness.to_le_bytes());

            for f in part.bounds.min {
                payload.extend_from_slice(&f.to_le_bytes());
            }
            for f in part.bounds.max {
                payload.extend_from_slice(&f.to_le_bytes());
            }

            payload.extend_from_slice(&(part.vertices.len() as u32).to_le_bytes());
            for v in &part.vertices {
                for f in v.position {
                    payload.extend_from_slice(&f.to_le_bytes());
                }
                for f in v.normal {
                    payload.extend_from_slice(&f.to_le_bytes());
                }
                payload.extend_from_slice(&v.color);
                for f in v.uv {
                    payload.extend_from_slice(&f.to_le_bytes());
                }
            }

            payload.extend_from_slice(&(part.indices.len() as u32).to_le_bytes());
            for &idx in &part.indices {
                payload.extend_from_slice(&idx.to_le_bytes());
            }
        }

        let mut flags = 0u16;
        if let Some(physics) = &self.physics {
            flags |= FLAG_PHYSICS;
            encode_physics(&mut payload, physics);
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(16 + payload.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&(self.parts.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Encode this model into compact quantized `.f3d` (`F3D1` with `FLAG_COMPACT`),
    /// reducing vertex size from 36 B to 17 B and indices from 4 B to 2 B when <= 65,535 vertices.
    pub fn to_bytes_compact(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);

        for part in &self.parts {
            write_str(&mut payload, &part.name);
            payload.extend_from_slice(&part.material.base_color);
            payload.extend_from_slice(&part.material.metallic.to_le_bytes());
            payload.extend_from_slice(&part.material.roughness.to_le_bytes());

            let mut tight_bounds = Aabb::EMPTY;
            for v in &part.vertices {
                tight_bounds.include_point(v.position);
            }
            if !tight_bounds.is_valid() {
                tight_bounds = if part.bounds.is_valid() {
                    part.bounds
                } else {
                    Aabb {
                        min: [0.0; 3],
                        max: [0.0; 3],
                    }
                };
            }

            for f in tight_bounds.min {
                payload.extend_from_slice(&f.to_le_bytes());
            }
            for f in tight_bounds.max {
                payload.extend_from_slice(&f.to_le_bytes());
            }

            let extent = [
                (tight_bounds.max[0] - tight_bounds.min[0]).max(1e-6),
                (tight_bounds.max[1] - tight_bounds.min[1]).max(1e-6),
                (tight_bounds.max[2] - tight_bounds.min[2]).max(1e-6),
            ];

            let vcount = part.vertices.len();
            payload.extend_from_slice(&(vcount as u32).to_le_bytes());
            for v in &part.vertices {
                for axis in 0..3 {
                    let t = ((v.position[axis] - tight_bounds.min[axis]) / extent[axis]).clamp(0.0, 1.0);
                    let q = round_to_u16(t * 65535.0);
                    payload.extend_from_slice(&q.to_le_bytes());
                }
                for axis in 0..3 {
                    let q = round_to_i8(v.normal[axis].clamp(-1.0, 1.0) * 127.0);
                    payload.push(q as u8);
                }
                payload.extend_from_slice(&v.color);
                for axis in 0..2 {
                    // Map [-1.0, 2.0] UVs cleanly into u16
                    let t = ((v.uv[axis] + 1.0) / 3.0).clamp(0.0, 1.0);
                    let q = round_to_u16(t * 65535.0);
                    payload.extend_from_slice(&q.to_le_bytes());
                }
            }

            payload.extend_from_slice(&(part.indices.len() as u32).to_le_bytes());
            if vcount <= u16::MAX as usize {
                for &idx in &part.indices {
                    payload.extend_from_slice(&(idx as u16).to_le_bytes());
                }
            } else {
                for &idx in &part.indices {
                    payload.extend_from_slice(&idx.to_le_bytes());
                }
            }
        }

        let mut flags = FLAG_COMPACT;
        if let Some(physics) = &self.physics {
            flags |= FLAG_PHYSICS;
            encode_physics(&mut payload, physics);
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(16 + payload.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&(self.parts.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Decode and validate a binary `.f3d` (`F3D1`) byte slice (both standard and compact).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 16 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        let compact = (flags & FLAG_COMPACT) != 0;
        let has_physics = (flags & FLAG_PHYSICS) != 0;
        let part_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let expected_checksum = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        let payload = &bytes[16..];
        let actual_checksum = fnv1a_32(payload);
        if actual_checksum != expected_checksum {
            return Err(DecodeError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        let mut cursor = Cursor::new(payload);
        let name = cursor.read_str()?;
        let mut parts = Vec::with_capacity(part_count.min(4096));

        for _ in 0..part_count {
            let part_name = cursor.read_str()?;
            let base_color = cursor.read_4()?;
            let metallic = cursor.read_f32()?;
            let roughness = cursor.read_f32()?;
            let bounds = Aabb {
                min: [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?],
                max: [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?],
            };

            let vertex_count = cursor.read_u32()? as usize;
            let mut vertices = Vec::with_capacity(vertex_count.min(65536));
            if compact {
                let extent = [
                    (bounds.max[0] - bounds.min[0]).max(1e-6),
                    (bounds.max[1] - bounds.min[1]).max(1e-6),
                    (bounds.max[2] - bounds.min[2]).max(1e-6),
                ];
                for _ in 0..vertex_count {
                    let qx = cursor.read_u16()? as f32 / 65535.0;
                    let qy = cursor.read_u16()? as f32 / 65535.0;
                    let qz = cursor.read_u16()? as f32 / 65535.0;
                    let position = [
                        bounds.min[0] + qx * extent[0],
                        bounds.min[1] + qy * extent[1],
                        bounds.min[2] + qz * extent[2],
                    ];
                    let n_bytes = cursor.take(3)?;
                    let nx = (n_bytes[0] as i8) as f32 / 127.0;
                    let ny = (n_bytes[1] as i8) as f32 / 127.0;
                    let nz = (n_bytes[2] as i8) as f32 / 127.0;
                    let nlen = crate::math::sqrtf(nx * nx + ny * ny + nz * nz);
                    let normal = if nlen > 1e-5 {
                        [nx / nlen, ny / nlen, nz / nlen]
                    } else {
                        [0.0, 1.0, 0.0]
                    };
                    let color = cursor.read_4()?;
                    let qu = cursor.read_u16()? as f32 / 65535.0;
                    let qv = cursor.read_u16()? as f32 / 65535.0;
                    let uv = [qu * 3.0 - 1.0, qv * 3.0 - 1.0];
                    vertices.push(Vertex {
                        position,
                        normal,
                        color,
                        uv,
                    });
                }
            } else {
                for _ in 0..vertex_count {
                    let position = [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?];
                    let normal = [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?];
                    let color = cursor.read_4()?;
                    let uv = [cursor.read_f32()?, cursor.read_f32()?];
                    vertices.push(Vertex {
                        position,
                        normal,
                        color,
                        uv,
                    });
                }
            }

            let index_count = cursor.read_u32()? as usize;
            let mut indices = Vec::with_capacity(index_count.min(262144));
            let use_u16 = compact && vertex_count <= u16::MAX as usize;
            for _ in 0..index_count {
                let idx = if use_u16 {
                    cursor.read_u16()? as u32
                } else {
                    cursor.read_u32()?
                };
                if (idx as usize) >= vertex_count {
                    return Err(DecodeError::IndexOutOfBounds {
                        index: idx,
                        vertex_count,
                    });
                }
                indices.push(idx);
            }

            parts.push(MeshPart {
                name: part_name,
                material: Material {
                    base_color,
                    metallic,
                    roughness,
                },
                vertices,
                indices,
                bounds,
            });
        }

        let physics = if has_physics {
            Some(decode_physics(&mut cursor)?)
        } else {
            None
        };

        Ok(Self { name, parts, physics })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    UnexpectedEof,
    InvalidMagic,
    UnsupportedVersion(u16),
    ChecksumMismatch { expected: u32, actual: u32 },
    InvalidUtf8,
    IndexOutOfBounds { index: u32, vertex_count: usize },
    InvalidPhysicsData,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of .f3d stream"),
            Self::InvalidMagic => write!(f, "invalid magic header (expected F3D1)"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported .f3d version: {v}"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch (expected 0x{expected:08x}, got 0x{actual:08x})")
            }
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 string in .f3d stream"),
            Self::IndexOutOfBounds { index, vertex_count } => {
                write!(f, "triangle index {index} out of bounds for {vertex_count} vertices")
            }
            Self::InvalidPhysicsData => write!(f, "invalid physics block in .f3d stream"),
        }
    }
}

impl core::error::Error for DecodeError {}

fn encode_physics(payload: &mut Vec<u8>, phys: &PhysicsSpec) {
    payload.push(phys.body_kind as u8);
    payload.extend_from_slice(&phys.mass.to_le_bytes());
    payload.extend_from_slice(&phys.friction.to_le_bytes());
    payload.extend_from_slice(&phys.restitution.to_le_bytes());
    payload.extend_from_slice(&phys.linear_damping.to_le_bytes());
    payload.extend_from_slice(&phys.gravity_scale.to_le_bytes());
    let count = phys.colliders.len().min(u16::MAX as usize) as u16;
    payload.extend_from_slice(&count.to_le_bytes());
    for c in phys.colliders.iter().take(count as usize) {
        payload.extend_from_slice(&c.part_hash.to_le_bytes());
        for f in c.offset {
            payload.extend_from_slice(&f.to_le_bytes());
        }
        match c.shape {
            ColliderShape::None => payload.push(0),
            ColliderShape::Box { half_extents } => {
                payload.push(1);
                for f in half_extents {
                    payload.extend_from_slice(&f.to_le_bytes());
                }
            }
            ColliderShape::Sphere { radius } => {
                payload.push(2);
                payload.extend_from_slice(&radius.to_le_bytes());
            }
            ColliderShape::Capsule { radius, half_height } => {
                payload.push(3);
                payload.extend_from_slice(&radius.to_le_bytes());
                payload.extend_from_slice(&half_height.to_le_bytes());
            }
        }
    }
}

fn decode_physics(cursor: &mut Cursor<'_>) -> Result<PhysicsSpec, DecodeError> {
    let kind_raw = cursor.take(1)?[0];
    let body_kind = RigidBodyKind::from_u8(kind_raw).ok_or(DecodeError::InvalidPhysicsData)?;
    let mass = cursor.read_f32()?;
    let friction = cursor.read_f32()?;
    let restitution = cursor.read_f32()?;
    let linear_damping = cursor.read_f32()?;
    let gravity_scale = cursor.read_f32()?;
    let count = cursor.read_u16()? as usize;
    let mut colliders = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let part_hash = cursor.read_u32()?;
        let offset = [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?];
        let tag = cursor.take(1)?[0];
        let shape = match tag {
            0 => ColliderShape::None,
            1 => ColliderShape::Box {
                half_extents: [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?],
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
        colliders.push(ColliderSpec {
            part_hash,
            offset,
            shape,
        });
    }
    Ok(PhysicsSpec {
        body_kind,
        mass,
        friction,
        restitution,
        linear_damping,
        gravity_scale,
        colliders,
    })
}

pub fn fnv1a_32(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &b in data {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn round_to_u16(v: f32) -> u16 {
    ((v + 0.5) as i32).clamp(0, u16::MAX as i32) as u16
}

fn round_to_i8(v: f32) -> i8 {
    let r = if v >= 0.0 { v + 0.5 } else { v - 0.5 } as i32;
    r.clamp(-127, 127) as i8
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&bytes[..len as usize]);
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::UnexpectedEof)?;
        if end > self.data.len() {
            return Err(DecodeError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn read_4(&mut self) -> Result<[u8; 4], DecodeError> {
        let s = self.take(4)?;
        Ok([s[0], s[1], s[2], s[3]])
    }

    fn read_u16(&mut self) -> Result<u16, DecodeError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.read_4()?))
    }

    fn read_f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.read_4()?))
    }

    fn read_str(&mut self) -> Result<String, DecodeError> {
        let len = self.read_u16()? as usize;
        let bytes = self.take(len)?;
        let s = core::str::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)?;
        Ok(String::from(s))
    }
}
