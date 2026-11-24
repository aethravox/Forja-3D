use alloc::{string::String, vec::Vec};

use crate::animation::AnimationSet;
use crate::format::{fnv1a_32, write_str, Cursor, DecodeError, Model};
use crate::texture::Texture;

/// Binary magic header for Forja3D complete pack files (.f3p).
pub const F3P_MAGIC: [u8; 4] = *b"F3P1";
pub const F3P_VERSION: u16 = 1;

/// Forja 3D Pack (.f3p / F3P1) - Contenedor todo-en-uno que reúne
/// geometría/modelo (.f3d), texturas (.f3t) y animaciones (.f3a) en un solo archivo.
#[derive(Debug, Clone, PartialEq)]
pub struct ForjaPack {
    pub name: String,
    pub model: Option<Model>,
    pub textures: Vec<Texture>,
    pub animations: Vec<AnimationSet>,
}

pub type Pack = ForjaPack;

impl Default for ForjaPack {
    fn default() -> Self {
        Self {
            name: String::from("Paquete"),
            model: None,
            textures: Vec::new(),
            animations: Vec::new(),
        }
    }
}

impl ForjaPack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model: None,
            textures: Vec::new(),
            animations: Vec::new(),
        }
    }

    pub fn from_model(model: Model) -> Self {
        let name = model.name.clone();
        Self {
            name,
            model: Some(model),
            textures: Vec::new(),
            animations: Vec::new(),
        }
    }

    pub fn total_mesh_triangles(&self) -> usize {
        self.model.as_ref().map_or(0, |m| m.triangle_count())
    }

    pub fn total_animation_clips(&self) -> usize {
        self.animations.iter().map(|a| a.clips.len()).sum()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);

        if let Some(model) = &self.model {
            payload.push(1);
            let m_bytes = model.to_bytes();
            let m_len = m_bytes.len().min(u32::MAX as usize) as u32;
            payload.extend_from_slice(&m_len.to_le_bytes());
            payload.extend_from_slice(&m_bytes);
        } else {
            payload.push(0);
        }

        let tex_count = self.textures.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&tex_count.to_le_bytes());
        for tex in self.textures.iter().take(tex_count as usize) {
            let t_bytes = tex.to_bytes();
            let t_len = t_bytes.len().min(u32::MAX as usize) as u32;
            payload.extend_from_slice(&t_len.to_le_bytes());
            payload.extend_from_slice(&t_bytes);
        }

        let anim_count = self.animations.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&anim_count.to_le_bytes());
        for anim in self.animations.iter().take(anim_count as usize) {
            let a_bytes = anim.to_bytes();
            let a_len = a_bytes.len().min(u32::MAX as usize) as u32;
            payload.extend_from_slice(&a_len.to_le_bytes());
            payload.extend_from_slice(&a_bytes);
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&F3P_MAGIC);
        out.extend_from_slice(&F3P_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != F3P_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != F3P_VERSION {
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

        let has_model = cursor.take(1)?[0] != 0;
        let model = if has_model {
            let m_len = cursor.read_u32()? as usize;
            let m_slice = cursor.take(m_len)?;
            Some(Model::from_bytes(m_slice)?)
        } else {
            None
        };

        let tex_count = cursor.read_u16()? as usize;
        let mut textures = Vec::with_capacity(tex_count.min(1024));
        for _ in 0..tex_count {
            let t_len = cursor.read_u32()? as usize;
            let t_slice = cursor.take(t_len)?;
            let tex = Texture::from_bytes(t_slice)
                .map_err(|_| DecodeError::InvalidPhysicsData)?;
            textures.push(tex);
        }

        let anim_count = cursor.read_u16()? as usize;
        let mut animations = Vec::with_capacity(anim_count.min(1024));
        for _ in 0..anim_count {
            let a_len = cursor.read_u32()? as usize;
            let a_slice = cursor.take(a_len)?;
            let anim = AnimationSet::from_bytes(a_slice)?;
            animations.push(anim);
        }

        Ok(Self {
            name,
            model,
            textures,
            animations,
        })
    }
}
