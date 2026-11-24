use alloc::{string::String, vec::Vec};

use crate::format::{fnv1a_32, write_str, Cursor, DecodeError};
use crate::math::sqrtf;

/// Binary magic header for Forja3D animation files (.f3a).
pub const F3A_MAGIC: [u8; 4] = *b"F3A1";
pub const F3A_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AnimationProperty {
    #[default]
    Translation = 0,
    Rotation = 1,
    Scale = 2,
}

impl AnimationProperty {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Translation),
            1 => Some(Self::Rotation),
            2 => Some(Self::Scale),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Translation => "Posición",
            Self::Rotation => "Rotación",
            Self::Scale => "Escala",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum InterpolationMode {
    Step = 0,
    #[default]
    Linear = 1,
}

impl InterpolationMode {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Step),
            1 => Some(Self::Linear),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AnimationTargetKind {
    #[default]
    Node = 0,
    Bone = 1,
}

impl AnimationTargetKind {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Node),
            1 => Some(Self::Bone),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Node => "Nodo",
            Self::Bone => "Hueso",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub time: f32,
    pub value: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationTrack {
    pub target_name: String,
    pub target_hash: u32,
    pub target_kind: AnimationTargetKind,
    pub property: AnimationProperty,
    pub interpolation: InterpolationMode,
    pub keyframes: Vec<Keyframe>,
}

impl AnimationTrack {
    pub fn new(
        target_name: impl Into<String>,
        property: AnimationProperty,
        interpolation: InterpolationMode,
    ) -> Self {
        Self::new_with_kind(target_name, AnimationTargetKind::Node, property, interpolation)
    }

    pub fn new_bone(
        bone_name: impl Into<String>,
        property: AnimationProperty,
        interpolation: InterpolationMode,
    ) -> Self {
        Self::new_with_kind(bone_name, AnimationTargetKind::Bone, property, interpolation)
    }

    pub fn new_with_kind(
        target_name: impl Into<String>,
        target_kind: AnimationTargetKind,
        property: AnimationProperty,
        interpolation: InterpolationMode,
    ) -> Self {
        let target_name = target_name.into();
        let target_hash = fnv1a_32(target_name.as_bytes());
        Self {
            target_name,
            target_hash,
            target_kind,
            property,
            interpolation,
            keyframes: Vec::new(),
        }
    }

    pub fn sample(&self, time: f32) -> Option<[f32; 4]> {
        if self.keyframes.is_empty() {
            return None;
        }
        if self.keyframes.len() == 1 || time <= self.keyframes[0].time {
            return Some(self.keyframes[0].value);
        }
        let last = self.keyframes[self.keyframes.len() - 1];
        if time >= last.time {
            return Some(last.value);
        }

        for pair in self.keyframes.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if time >= a.time && time <= b.time {
                if self.interpolation == InterpolationMode::Step || (b.time - a.time).abs() < 1e-6 {
                    return Some(a.value);
                }
                let alpha = ((time - a.time) / (b.time - a.time)).clamp(0.0, 1.0);
                if self.property == AnimationProperty::Rotation {
                    return Some(nlerp_quat(a.value, b.value, alpha));
                }
                return Some([
                    a.value[0] + (b.value[0] - a.value[0]) * alpha,
                    a.value[1] + (b.value[1] - a.value[1]) * alpha,
                    a.value[2] + (b.value[2] - a.value[2]) * alpha,
                    a.value[3] + (b.value[3] - a.value[3]) * alpha,
                ]);
            }
        }
        Some(last.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub looping: bool,
    pub tracks: Vec<AnimationTrack>,
}

impl AnimationClip {
    pub fn new(name: impl Into<String>, duration: f32, looping: bool) -> Self {
        Self {
            name: name.into(),
            duration: duration.max(0.0),
            looping,
            tracks: Vec::new(),
        }
    }

    pub fn wrap_time(&self, time: f32) -> f32 {
        if self.duration <= 1e-6 {
            return 0.0;
        }
        if !self.looping {
            return time.clamp(0.0, self.duration);
        }
        let mut t = time;
        if t < 0.0 {
            let k = ((-t / self.duration) as i32) + 1;
            t += k as f32 * self.duration;
        }
        if t >= self.duration {
            let k = (t / self.duration) as i32;
            t -= k as f32 * self.duration;
        }
        t.clamp(0.0, self.duration)
    }
}

/// Compiled Forja3D animation bundle (.f3a / F3A1).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationSet {
    pub name: String,
    pub clips: Vec<AnimationClip>,
}

impl AnimationSet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
        }
    }

    pub fn total_keyframes(&self) -> usize {
        self.clips
            .iter()
            .flat_map(|c| c.tracks.iter())
            .map(|t| t.keyframes.len())
            .sum()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);
        let clip_count = self.clips.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&clip_count.to_le_bytes());

        for clip in self.clips.iter().take(clip_count as usize) {
            write_str(&mut payload, &clip.name);
            payload.extend_from_slice(&clip.duration.to_le_bytes());
            payload.push(if clip.looping { 1 } else { 0 });
            let track_count = clip.tracks.len().min(u16::MAX as usize) as u16;
            payload.extend_from_slice(&track_count.to_le_bytes());

            for track in clip.tracks.iter().take(track_count as usize) {
                write_str(&mut payload, &track.target_name);
                payload.extend_from_slice(&track.target_hash.to_le_bytes());
                payload.push(track.target_kind as u8);
                payload.push(track.property as u8);
                payload.push(track.interpolation as u8);
                let kf_count = track.keyframes.len().min(u32::MAX as usize) as u32;
                payload.extend_from_slice(&kf_count.to_le_bytes());

                for kf in track.keyframes.iter().take(kf_count as usize) {
                    payload.extend_from_slice(&kf.time.to_le_bytes());
                    for v in kf.value {
                        payload.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&F3A_MAGIC);
        out.extend_from_slice(&F3A_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != F3A_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != F3A_VERSION {
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
        let clip_count = cursor.read_u16()? as usize;
        let mut clips = Vec::with_capacity(clip_count.min(1024));

        for _ in 0..clip_count {
            let clip_name = cursor.read_str()?;
            let duration = cursor.read_f32()?;
            let looping = cursor.take(1)?[0] != 0;
            let track_count = cursor.read_u16()? as usize;
            let mut tracks = Vec::with_capacity(track_count.min(4096));

            for _ in 0..track_count {
                let target_name = cursor.read_str()?;
                let target_hash = cursor.read_u32()?;
                let kind_raw = cursor.take(1)?[0];
                let prop_raw = cursor.take(1)?[0];
                let interp_raw = cursor.take(1)?[0];
                let target_kind =
                    AnimationTargetKind::from_u8(kind_raw).ok_or(DecodeError::InvalidPhysicsData)?;
                let property =
                    AnimationProperty::from_u8(prop_raw).ok_or(DecodeError::InvalidPhysicsData)?;
                let interpolation =
                    InterpolationMode::from_u8(interp_raw).ok_or(DecodeError::InvalidPhysicsData)?;
                let kf_count = cursor.read_u32()? as usize;
                let mut keyframes = Vec::with_capacity(kf_count.min(65536));

                for _ in 0..kf_count {
                    let time = cursor.read_f32()?;
                    let value = [
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                    ];
                    keyframes.push(Keyframe { time, value });
                }

                tracks.push(AnimationTrack {
                    target_name,
                    target_hash,
                    target_kind,
                    property,
                    interpolation,
                    keyframes,
                });
            }

            clips.push(AnimationClip {
                name: clip_name,
                duration,
                looping,
                tracks,
            });
        }

        Ok(Self { name, clips })
    }
}

fn nlerp_quat(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    let q = [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ];
    let len = sqrtf(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    if len > 1e-6 {
        [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}
