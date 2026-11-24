use alloc::{string::String, vec::Vec};

use crate::format::{
    decode_physics, encode_physics, fnv1a_32, write_str, Cursor, DecodeError, Model, PhysicsSpec,
};

/// Binary magic header for standalone Forja3D physics assets (.f3p).
pub const F3P_MAGIC: [u8; 4] = *b"F3P1";
pub const F3P_VERSION: u16 = 1;

/// Standalone rigid-body & collider asset (.f3p / F3P1).
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsAsset {
    pub name: String,
    pub gravity: [f32; 3],
    pub spec: PhysicsSpec,
}

impl Default for PhysicsAsset {
    fn default() -> Self {
        Self {
            name: String::from("Fisicas"),
            gravity: [0.0, -9.81, 0.0],
            spec: PhysicsSpec::default(),
        }
    }
}

impl PhysicsAsset {
    pub fn new(name: impl Into<String>, spec: PhysicsSpec) -> Self {
        Self {
            name: name.into(),
            gravity: [0.0, -9.81, 0.0],
            spec,
        }
    }

    pub fn from_model(model: &Model) -> Self {
        let spec = match &model.physics {
            Some(p) => p.clone(),
            None => model
                .clone()
                .with_auto_physics(crate::format::RigidBodyKind::Dynamic)
                .physics
                .unwrap_or_default(),
        };
        Self::new(model.name.clone(), spec)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);
        for g in self.gravity {
            payload.extend_from_slice(&g.to_le_bytes());
        }
        encode_physics(&mut payload, &self.spec);

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
        let gravity = [cursor.read_f32()?, cursor.read_f32()?, cursor.read_f32()?];
        let spec = decode_physics(&mut cursor)?;
        Ok(Self {
            name,
            gravity,
            spec,
        })
    }
}
