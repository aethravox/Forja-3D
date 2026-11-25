use alloc::{string::String, vec::Vec};
use core::fmt;

/// Magic header bytes for compiled Forja3D textures (.f3t).
pub const F3T_MAGIC: [u8; 4] = *b"F3T1";
pub const F3T_VERSION: u16 = 1;

/// Pixel format and compression encoding of the .f3t texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TexturePixelFormat {
    #[default]
    Rgba8 = 0,
    Rgb8 = 1,
    R8 = 2,
    RleRgba8 = 3,
}

impl TexturePixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 | Self::RleRgba8 => 4,
            Self::Rgb8 => 3,
            Self::R8 => 1,
        }
    }

    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Rgba8),
            1 => Some(Self::Rgb8),
            2 => Some(Self::R8),
            3 => Some(Self::RleRgba8),
            _ => None,
        }
    }
}

/// In-memory representation of a compiled Forja3D texture (.f3t).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub format: TexturePixelFormat,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureError {
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidFormat(u8),
    BufferTooShort,
    ChecksumMismatch { expected: u32, computed: u32 },
    DecompressionError,
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "encabezado F3T1 no coincide"),
            Self::UnsupportedVersion(v) => write!(f, "versión {} no soportada", v),
            Self::InvalidFormat(fmt) => write!(f, "formato de pixel {} desconocido", fmt),
            Self::BufferTooShort => write!(f, "búfer truncado"),
            Self::ChecksumMismatch { expected, computed } => {
                write!(f, "checksum erróneo (esperado {expected:#x}, calculado {computed:#x})")
            }
            Self::DecompressionError => write!(f, "error al descomprimir datos zlib/deflate"),
        }
    }
}

impl Texture {
    pub fn new_rgba(name: impl Into<String>, width: u16, height: u16, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            format: TexturePixelFormat::Rgba8,
            data,
        }
    }

    /// Encode texture into binary .f3t format with zlib/deflate compression.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        let name_len = self.name.len().min(u16::MAX as usize);
        let name_bytes = &self.name.as_bytes()[..name_len];
        payload.extend_from_slice(&(name_len as u16).to_le_bytes());
        payload.extend_from_slice(name_bytes);
        payload.extend_from_slice(&self.width.to_le_bytes());
        payload.extend_from_slice(&self.height.to_le_bytes());
        payload.push(self.format as u8);

        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&self.data, 6);
        payload.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        payload.extend_from_slice(&compressed);

        let checksum = crate::format::fnv1a_32(&payload);

        let mut out = Vec::with_capacity(16 + payload.len());
        out.extend_from_slice(&F3T_MAGIC);
        out.extend_from_slice(&F3T_VERSION.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // flags: bit 0 = zlib/deflate compressed
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Decode texture from binary .f3t format (supporting both compressed and legacy uncompressed).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        if bytes.len() < 12 {
            return Err(TextureError::BufferTooShort);
        }
        if &bytes[0..4] != &F3T_MAGIC {
            return Err(TextureError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != F3T_VERSION {
            return Err(TextureError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        let is_compressed = (flags & 1) != 0;

        let expected_checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let payload = &bytes[12..];
        let computed_checksum = crate::format::fnv1a_32(payload);
        if expected_checksum != computed_checksum {
            return Err(TextureError::ChecksumMismatch {
                expected: expected_checksum,
                computed: computed_checksum,
            });
        }

        let mut cur = 0;
        if payload.len() < 2 {
            return Err(TextureError::BufferTooShort);
        }
        let name_len = u16::from_le_bytes([payload[cur], payload[cur + 1]]) as usize;
        cur += 2;
        if payload.len() < cur + name_len {
            return Err(TextureError::BufferTooShort);
        }
        let name = alloc::string::String::from_utf8_lossy(&payload[cur..cur + name_len]).into_owned();
        cur += name_len;

        if payload.len() < cur + 9 {
            return Err(TextureError::BufferTooShort);
        }
        let width = u16::from_le_bytes([payload[cur], payload[cur + 1]]);
        cur += 2;
        let height = u16::from_le_bytes([payload[cur], payload[cur + 1]]);
        cur += 2;
        let format = TexturePixelFormat::from_u8(payload[cur])
            .ok_or(TextureError::InvalidFormat(payload[cur]))?;
        cur += 1;
        let data_len = u32::from_le_bytes([payload[cur], payload[cur + 1], payload[cur + 2], payload[cur + 3]]) as usize;
        cur += 4;

        if payload.len() < cur + data_len {
            return Err(TextureError::BufferTooShort);
        }
        let raw_data = &payload[cur..cur + data_len];
        let data = if is_compressed {
            miniz_oxide::inflate::decompress_to_vec_zlib(raw_data)
                .map_err(|_| TextureError::DecompressionError)?
        } else {
            raw_data.to_vec()
        };

        Ok(Self {
            name,
            width,
            height,
            format,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_texture() {
        let tex = Texture::new_rgba("albedo_metal", 2, 2, alloc::vec![
            255, 0, 0, 255,
            0, 255, 0, 255,
            0, 0, 255, 255,
            255, 255, 0, 255,
        ]);
        let bytes = tex.to_bytes();
        let decoded = Texture::from_bytes(&bytes).expect("decodifica textura f3t");
        assert_eq!(decoded.name, "albedo_metal");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.data.len(), 16);
    }

    #[test]
    fn roundtrip_other_pixel_formats() {
        for (format, data) in [
            (TexturePixelFormat::Rgb8, alloc::vec![255, 128, 64, 10, 20, 30]),
            (TexturePixelFormat::R8, alloc::vec![0, 255]),
            (TexturePixelFormat::RleRgba8, alloc::vec![2, 200, 100, 50, 255]),
        ] {
            let tex = Texture {
                name: String::from("fmt_test"),
                width: 2,
                height: 1,
                format,
                data,
            };
            let decoded = Texture::from_bytes(&tex.to_bytes()).expect("decodifica formato f3t");
            assert_eq!(decoded, tex);
        }
    }
}
