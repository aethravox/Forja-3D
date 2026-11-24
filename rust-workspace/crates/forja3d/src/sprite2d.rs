use alloc::{string::String, vec::Vec};

use crate::format::{fnv1a_32, write_str, Cursor, DecodeError};

/// Binary magic header for Forja3D 2D vector/sprite documents (.f2d).
pub const F2D_MAGIC: [u8; 4] = *b"F2D1";
pub const F2D_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Path2D {
    pub name: String,
    pub closed: bool,
    pub fill_color: [u8; 4],
    pub stroke_color: [u8; 4],
    pub stroke_width: f32,
    pub points: Vec<[f32; 2]>,
}

impl Path2D {
    pub fn new(name: impl Into<String>, points: Vec<[f32; 2]>, closed: bool) -> Self {
        Self {
            name: name.into(),
            closed,
            fill_color: [245, 158, 11, 255],
            stroke_color: [255, 255, 255, 255],
            stroke_width: 2.0,
            points,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteQuad2D {
    pub name: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub rotation_deg: f32,
    pub uv_rect: [f32; 4],
    pub tint: [u8; 4],
}

impl Default for SpriteQuad2D {
    fn default() -> Self {
        Self {
            name: String::from("sprite"),
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            rotation_deg: 0.0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [255, 255, 255, 255],
        }
    }
}

/// Binary 2D vector profile & sprite layout asset (.f2d / F2D1).
#[derive(Debug, Clone, PartialEq)]
pub struct Document2D {
    pub name: String,
    pub canvas_size: [f32; 2],
    pub paths: Vec<Path2D>,
    pub sprites: Vec<SpriteQuad2D>,
}

impl Document2D {
    pub fn new(name: impl Into<String>, canvas_size: [f32; 2]) -> Self {
        Self {
            name: name.into(),
            canvas_size,
            paths: Vec::new(),
            sprites: Vec::new(),
        }
    }

    pub fn from_profile(name: impl Into<String>, points: Vec<[f32; 2]>, closed: bool) -> Self {
        let name = name.into();
        let mut doc = Self::new(name.clone(), [2.0, 2.0]);
        doc.paths.push(Path2D::new(name, points, closed));
        doc
    }

    pub fn primary_profile(&self) -> Option<&[[f32; 2]]> {
        self.paths.first().map(|p| p.points.as_slice())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);
        payload.extend_from_slice(&self.canvas_size[0].to_le_bytes());
        payload.extend_from_slice(&self.canvas_size[1].to_le_bytes());

        let path_count = self.paths.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&path_count.to_le_bytes());
        for p in self.paths.iter().take(path_count as usize) {
            write_str(&mut payload, &p.name);
            payload.push(if p.closed { 1 } else { 0 });
            payload.extend_from_slice(&p.fill_color);
            payload.extend_from_slice(&p.stroke_color);
            payload.extend_from_slice(&p.stroke_width.to_le_bytes());
            let pt_count = p.points.len().min(u32::MAX as usize) as u32;
            payload.extend_from_slice(&pt_count.to_le_bytes());
            for pt in p.points.iter().take(pt_count as usize) {
                payload.extend_from_slice(&pt[0].to_le_bytes());
                payload.extend_from_slice(&pt[1].to_le_bytes());
            }
        }

        let sprite_count = self.sprites.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&sprite_count.to_le_bytes());
        for s in self.sprites.iter().take(sprite_count as usize) {
            write_str(&mut payload, &s.name);
            for v in s.position {
                payload.extend_from_slice(&v.to_le_bytes());
            }
            for v in s.size {
                payload.extend_from_slice(&v.to_le_bytes());
            }
            payload.extend_from_slice(&s.rotation_deg.to_le_bytes());
            for v in s.uv_rect {
                payload.extend_from_slice(&v.to_le_bytes());
            }
            payload.extend_from_slice(&s.tint);
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&F2D_MAGIC);
        out.extend_from_slice(&F2D_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != F2D_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != F2D_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let expected_checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let payload = &bytes[12..];
        let actual_checksum = fnv1a_32(payload);
        if expected_checksum != actual_checksum {
            return Err(DecodeError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        let mut cursor = Cursor::new(payload);
        let name = cursor.read_str()?;
        let canvas_size = [cursor.read_f32()?, cursor.read_f32()?];

        let path_count = cursor.read_u16()? as usize;
        let mut paths = Vec::with_capacity(path_count.min(4096));
        for _ in 0..path_count {
            let p_name = cursor.read_str()?;
            let closed = cursor.take(1)?[0] != 0;
            let fill_color = cursor.read_4()?;
            let stroke_color = cursor.read_4()?;
            let stroke_width = cursor.read_f32()?;
            let pt_count = cursor.read_u32()? as usize;
            let mut points = Vec::with_capacity(pt_count.min(65536));
            for _ in 0..pt_count {
                points.push([cursor.read_f32()?, cursor.read_f32()?]);
            }
            paths.push(Path2D {
                name: p_name,
                closed,
                fill_color,
                stroke_color,
                stroke_width,
                points,
            });
        }

        let sprite_count = cursor.read_u16()? as usize;
        let mut sprites = Vec::with_capacity(sprite_count.min(4096));
        for _ in 0..sprite_count {
            let s_name = cursor.read_str()?;
            let position = [cursor.read_f32()?, cursor.read_f32()?];
            let size = [cursor.read_f32()?, cursor.read_f32()?];
            let rotation_deg = cursor.read_f32()?;
            let uv_rect = [
                cursor.read_f32()?,
                cursor.read_f32()?,
                cursor.read_f32()?,
                cursor.read_f32()?,
            ];
            let tint = cursor.read_4()?;
            sprites.push(SpriteQuad2D {
                name: s_name,
                position,
                size,
                rotation_deg,
                uv_rect,
                tint,
            });
        }

        Ok(Self {
            name,
            canvas_size,
            paths,
            sprites,
        })
    }
}
