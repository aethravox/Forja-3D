use core::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0, z: 1.0 };

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[inline]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    #[inline]
    pub fn length(self) -> f32 {
        sqrtf(self.dot(self))
    }

    #[inline]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len > 1e-6 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            Self::new(0.0, 1.0, 0.0)
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

/// Column-major 4x4 affine transform matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn from_trs(position: [f32; 3], rotation_deg: [f32; 3], scale: [f32; 3]) -> Self {
        let rx = rotation_deg[0].to_radians();
        let ry = rotation_deg[1].to_radians();
        let rz = rotation_deg[2].to_radians();

        let (sx, cx) = sin_cos(rx);
        let (sy, cy) = sin_cos(ry);
        let (sz, cz) = sin_cos(rz);

        // R = Rz * Ry * Rx
        let r00 = cz * cy;
        let r01 = cz * sy * sx - sz * cx;
        let r02 = cz * sy * cx + sz * sx;

        let r10 = sz * cy;
        let r11 = sz * sy * sx + cz * cx;
        let r12 = sz * sy * cx - cz * sx;

        let r20 = -sy;
        let r21 = cy * sx;
        let r22 = cy * cx;

        Self {
            cols: [
                [r00 * scale[0], r10 * scale[0], r20 * scale[0], 0.0],
                [r01 * scale[1], r11 * scale[1], r21 * scale[1], 0.0],
                [r02 * scale[2], r12 * scale[2], r22 * scale[2], 0.0],
                [position[0], position[1], position[2], 1.0],
            ],
        }
    }

    #[inline]
    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        let x = self.cols[0][0] * p[0] + self.cols[1][0] * p[1] + self.cols[2][0] * p[2] + self.cols[3][0];
        let y = self.cols[0][1] * p[0] + self.cols[1][1] * p[1] + self.cols[2][1] * p[2] + self.cols[3][1];
        let z = self.cols[0][2] * p[0] + self.cols[1][2] * p[1] + self.cols[2][2] * p[2] + self.cols[3][2];
        [x, y, z]
    }

    pub fn transform_normal(&self, n: [f32; 3], scale: [f32; 3]) -> [f32; 3] {
        // Undo scale before applying rotation part to keep normals orthogonal under non-uniform scale
        let sx = if scale[0].abs() > 1e-6 { 1.0 / (scale[0] * scale[0]) } else { 1.0 };
        let sy = if scale[1].abs() > 1e-6 { 1.0 / (scale[1] * scale[1]) } else { 1.0 };
        let sz = if scale[2].abs() > 1e-6 { 1.0 / (scale[2] * scale[2]) } else { 1.0 };

        let x = self.cols[0][0] * n[0] * sx + self.cols[1][0] * n[1] * sy + self.cols[2][0] * n[2] * sz;
        let y = self.cols[0][1] * n[0] * sx + self.cols[1][1] * n[1] * sy + self.cols[2][1] * n[2] * sz;
        let z = self.cols[0][2] * n[0] * sx + self.cols[1][2] * n[1] * sy + self.cols[2][2] * n[2] * sz;
        Vec3::new(x, y, z).normalized().to_array()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    pub fn include_point(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            if p[i] < self.min[i] {
                self.min[i] = p[i];
            }
            if p[i] > self.max[i] {
                self.max[i] = p[i];
            }
        }
    }

    pub fn merge(self, other: Self) -> Self {
        if !self.is_valid() {
            return other;
        }
        if !other.is_valid() {
            return self;
        }
        Self {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    pub fn is_valid(&self) -> bool {
        self.min[0] <= self.max[0] && self.min[1] <= self.max[1] && self.min[2] <= self.max[2]
    }
}

#[cfg(feature = "std")]
#[inline]
pub fn sin_cos(rad: f32) -> (f32, f32) {
    rad.sin_cos()
}

#[cfg(not(feature = "std"))]
#[inline]
pub fn sin_cos(rad: f32) -> (f32, f32) {
    // Pure no_std range-reduced Taylor approximation accurate to ~1e-6
    let tau = core::f32::consts::TAU;
    let mut x = rad % tau;
    if x > core::f32::consts::PI {
        x -= tau;
    } else if x < -core::f32::consts::PI {
        x += tau;
    }
    let s = sin_poly(x);
    let c = sin_poly(x + core::f32::consts::FRAC_PI_2);
    (s, c)
}

#[cfg(not(feature = "std"))]
fn sin_poly(mut x: f32) -> f32 {
    if x > core::f32::consts::PI {
        x -= core::f32::consts::TAU;
    }
    let x2 = x * x;
    x * (1.0 - x2 * (1.0 / 6.0 - x2 * (1.0 / 120.0 - x2 * (1.0 / 5040.0 - x2 / 362880.0))))
}

#[cfg(feature = "std")]
#[inline]
pub fn sqrtf(v: f32) -> f32 {
    v.sqrt()
}

#[cfg(not(feature = "std"))]
#[inline]
pub fn sqrtf(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let mut x = v;
    for _ in 0..8 {
        x = 0.5 * (x + v / x);
    }
    x
}
