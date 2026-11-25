use alloc::{collections::BTreeMap, vec, vec::Vec};
use core::f32::consts::TAU;

use crate::{
    format::Vertex,
    math::{sin_cos, Vec3},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingMode {
    /// Keep native primitive normals (sharp edges on cubes, smooth on spheres).
    Original,
    /// Split triangles and recompute flat per-face normals (low-poly faceted look).
    Flat,
    /// Weld shared positions and average area-weighted vertex normals.
    Smooth,
}

impl ShadingMode {
    pub const ALL: [Self; 3] = [Self::Original, Self::Flat, Self::Smooth];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Flat => "Facetado",
            Self::Smooth => "Suave",
        }
    }
}

pub fn default_extrude_profile() -> Vec<[f32; 2]> {
    vec![
        [-0.45, -0.45],
        [0.45, -0.45],
        [0.25, 0.0],
        [0.52, 0.35],
        [0.0, 0.55],
        [-0.52, 0.35],
        [-0.25, 0.0],
    ]
}

pub fn default_lathe_profile() -> Vec<[f32; 2]> {
    vec![
        [0.0, -0.5],
        [0.36, -0.5],
        [0.20, -0.12],
        [0.42, 0.22],
        [0.30, 0.5],
        [0.0, 0.5],
    ]
}

/// Apply flat or smooth normal shading to an indexed triangle mesh.
pub fn apply_shading(vertices: Vec<Vertex>, indices: Vec<u32>, mode: ShadingMode) -> (Vec<Vertex>, Vec<u32>) {
    match mode {
        ShadingMode::Original => (vertices, indices),
        ShadingMode::Flat => apply_flat_normals(&vertices, &indices),
        ShadingMode::Smooth => apply_smooth_normals(&vertices, &indices),
    }
}

fn apply_flat_normals(vertices: &[Vertex], indices: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    let mut out_verts = Vec::with_capacity(indices.len());
    let mut out_indices = Vec::with_capacity(indices.len());

    for tri in indices.chunks_exact(3) {
        let Some(&v0) = vertices.get(tri[0] as usize) else { continue };
        let Some(&v1) = vertices.get(tri[1] as usize) else { continue };
        let Some(&v2) = vertices.get(tri[2] as usize) else { continue };

        let p0 = Vec3::new(v0.position[0], v0.position[1], v0.position[2]);
        let p1 = Vec3::new(v1.position[0], v1.position[1], v1.position[2]);
        let p2 = Vec3::new(v2.position[0], v2.position[1], v2.position[2]);
        let normal = (p1 - p0).cross(p2 - p0).normalized().to_array();

        let base = out_verts.len() as u32;
        out_verts.push(Vertex { normal, ..v0 });
        out_verts.push(Vertex { normal, ..v1 });
        out_verts.push(Vertex { normal, ..v2 });
        out_indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    (out_verts, out_indices)
}

fn apply_smooth_normals(vertices: &[Vertex], indices: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    // Weld vertices sharing quantized (position, color) and accumulate area-weighted face normals
    let mut map: BTreeMap<([i32; 3], [u8; 4]), u32> = BTreeMap::new();
    let mut out_verts: Vec<Vertex> = Vec::new();
    let mut remap: Vec<u32> = vec![0; vertices.len()];

    for (old_idx, v) in vertices.iter().enumerate() {
        let key = (quant_vec3(v.position, 10_000.0), v.color);
        if let Some(&new_idx) = map.get(&key) {
            remap[old_idx] = new_idx;
        } else {
            let new_idx = out_verts.len() as u32;
            map.insert(key, new_idx);
            out_verts.push(Vertex {
                normal: [0.0, 0.0, 0.0],
                ..*v
            });
            remap[old_idx] = new_idx;
        }
    }

    let mut out_indices = Vec::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (
            remap.get(tri[0] as usize),
            remap.get(tri[1] as usize),
            remap.get(tri[2] as usize),
        ) else {
            continue;
        };
        if a == b || b == c || a == c {
            continue;
        }
        let pa = Vec3::new(
            out_verts[a as usize].position[0],
            out_verts[a as usize].position[1],
            out_verts[a as usize].position[2],
        );
        let pb = Vec3::new(
            out_verts[b as usize].position[0],
            out_verts[b as usize].position[1],
            out_verts[b as usize].position[2],
        );
        let pc = Vec3::new(
            out_verts[c as usize].position[0],
            out_verts[c as usize].position[1],
            out_verts[c as usize].position[2],
        );
        // Area-weighted cross product (not normalized yet)
        let weighted = (pb - pa).cross(pc - pa);
        for &idx in &[a, b, c] {
            let n = &mut out_verts[idx as usize].normal;
            n[0] += weighted.x;
            n[1] += weighted.y;
            n[2] += weighted.z;
        }
        out_indices.extend_from_slice(&[a, b, c]);
    }

    for v in &mut out_verts {
        v.normal = Vec3::new(v.normal[0], v.normal[1], v.normal[2])
            .normalized()
            .to_array();
    }

    (out_verts, out_indices)
}

/// Deduplicate identical vertices (quantized position, normal, color) and remap triangle indices.
pub fn deduplicate_vertices(vertices: Vec<Vertex>, indices: Vec<u32>) -> (Vec<Vertex>, Vec<u32>) {
    let mut map: BTreeMap<([i32; 3], [i32; 3], [u8; 4]), u32> = BTreeMap::new();
    let mut out_verts = Vec::new();
    let mut remap = vec![0u32; vertices.len()];

    for (old_idx, v) in vertices.iter().enumerate() {
        let key = (
            quant_vec3(v.position, 10_000.0),
            quant_vec3(v.normal, 1_000.0),
            v.color,
        );
        if let Some(&new_idx) = map.get(&key) {
            remap[old_idx] = new_idx;
        } else {
            let new_idx = out_verts.len() as u32;
            map.insert(key, new_idx);
            out_verts.push(*v);
            remap[old_idx] = new_idx;
        }
    }

    let mut out_indices = Vec::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (
            remap.get(tri[0] as usize),
            remap.get(tri[1] as usize),
            remap.get(tri[2] as usize),
        ) else {
            continue;
        };
        if a != b && b != c && a != c {
            out_indices.extend_from_slice(&[a, b, c]);
        }
    }

    (out_verts, out_indices)
}

#[inline]
fn quant_vec3(v: [f32; 3], scale: f32) -> [i32; 3] {
    [
        round_to_i32(v[0] * scale),
        round_to_i32(v[1] * scale),
        round_to_i32(v[2] * scale),
    ]
}

#[inline]
fn round_to_i32(v: f32) -> i32 {
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

/// Extrude a closed 2D polygon (supporting concave shapes via ear-clipping) along Z.
pub fn build_extruded_polygon(
    raw_profile: &[[f32; 2]],
    depth: f32,
    color: [u8; 4],
) -> (Vec<Vertex>, Vec<u32>) {
    let fallback = default_extrude_profile();
    let pts_src = if raw_profile.len() >= 3 { raw_profile } else { &fallback };

    let mut pts: Vec<[f32; 2]> = pts_src.to_vec();
    if signed_area_2d(&pts) < 0.0 {
        pts.reverse();
    }

    let tris = triangulate_polygon_2d(&pts);
    let half_z = depth.max(0.04) * 0.5;
    let n = pts.len();

    let mut verts = Vec::new();
    let mut indices = Vec::new();

    // Front cap (+Z)
    let front_base = verts.len() as u32;
    for &p in &pts {
        verts.push(Vertex {
            position: [p[0], p[1], half_z],
            normal: [0.0, 0.0, 1.0],
            color,
            uv: [p[0] + 0.5, p[1] + 0.5],
        });
    }
    for [a, b, c] in &tris {
        indices.extend_from_slice(&[
            front_base + *a as u32,
            front_base + *b as u32,
            front_base + *c as u32,
        ]);
    }

    // Back cap (-Z, flipped winding)
    let back_base = verts.len() as u32;
    for &p in &pts {
        verts.push(Vertex {
            position: [p[0], p[1], -half_z],
            normal: [0.0, 0.0, -1.0],
            color,
            uv: [p[0] + 0.5, p[1] + 0.5],
        });
    }
    for [a, b, c] in &tris {
        indices.extend_from_slice(&[
            back_base + *a as u32,
            back_base + *c as u32,
            back_base + *b as u32,
        ]);
    }

    // Side walls
    for i in 0..n {
        let p0 = pts[i];
        let p1 = pts[(i + 1) % n];
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let norm = Vec3::new(dy, -dx, 0.0).normalized().to_array();
        let base = verts.len() as u32;

        verts.push(Vertex { position: [p0[0], p0[1], half_z], normal: norm, color, uv: [0.0, 1.0] });
        verts.push(Vertex { position: [p0[0], p0[1], -half_z], normal: norm, color, uv: [0.0, 0.0] });
        verts.push(Vertex { position: [p1[0], p1[1], -half_z], normal: norm, color, uv: [1.0, 0.0] });
        verts.push(Vertex { position: [p1[0], p1[1], half_z], normal: norm, color, uv: [1.0, 1.0] });
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (verts, indices)
}

/// Revolve a 2D profile (x = radius >= 0, y = height) around the Y axis.
pub fn build_lathe_profile(
    raw_profile: &[[f32; 2]],
    segments: u16,
    color: [u8; 4],
) -> (Vec<Vertex>, Vec<u32>) {
    let fallback = default_lathe_profile();
    let prof_src = if raw_profile.len() >= 2 { raw_profile } else { &fallback };
    let seg = segments.max(6) as usize;
    let m = prof_src.len();

    // Precompute smooth 2D profile normals (nr, ny)
    let mut prof_norms = vec![[1.0_f32, 0.0_f32]; m];
    for i in 0..m {
        let prev = if i > 0 { prof_src[i - 1] } else { prof_src[i] };
        let next = if i + 1 < m { prof_src[i + 1] } else { prof_src[i] };
        let dr = next[0].max(0.0) - prev[0].max(0.0);
        let dy = next[1] - prev[1];
        let len = crate::math::sqrtf(dr * dr + dy * dy);
        if len > 1e-5 {
            prof_norms[i] = [dy / len, -dr / len];
        } else {
            prof_norms[i] = [1.0, 0.0];
        }
    }

    let mut verts = Vec::with_capacity((seg + 1) * m);
    let mut indices = Vec::with_capacity(seg * (m - 1) * 6);

    for s in 0..=seg {
        let u = s as f32 / seg as f32;
        let angle = u * TAU;
        let (sa, ca) = sin_cos(angle);
        for (j, &pt) in prof_src.iter().enumerate() {
            let r = pt[0].max(0.0);
            let y = pt[1];
            let n2 = prof_norms[j];
            let normal = Vec3::new(n2[0] * ca, n2[1], n2[0] * sa)
                .normalized()
                .to_array();
            let v_frac = j as f32 / (m - 1).max(1) as f32;
            verts.push(Vertex {
                position: [r * ca, y, r * sa],
                normal,
                color,
                uv: [u, v_frac],
            });
        }
    }

    for s in 0..seg {
        for j in 0..(m - 1) {
            let a = (s * m + j) as u32;
            let b = ((s + 1) * m + j) as u32;
            let c = b + 1;
            let d = a + 1;
            indices.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }

    // Optional bottom/top caps if profile endpoints are open (radius > 0.005)
    if let Some(&first) = prof_src.first() {
        let r = first[0].max(0.0);
        if r > 0.005 {
            let center = verts.len() as u32;
            verts.push(Vertex {
                position: [0.0, first[1], 0.0],
                normal: [0.0, -1.0, 0.0],
                color,
                uv: [0.5, 0.5],
            });
            for s in 0..seg {
                let a0 = (s as f32 / seg as f32) * TAU;
                let a1 = ((s + 1) as f32 / seg as f32) * TAU;
                let (s0, c0) = sin_cos(a0);
                let (s1, c1) = sin_cos(a1);
                let b = verts.len() as u32;
                verts.push(Vertex { position: [c0 * r, first[1], s0 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
                verts.push(Vertex { position: [c1 * r, first[1], s1 * r], normal: [0.0, -1.0, 0.0], color, uv: [0.5, 0.5] });
                indices.extend_from_slice(&[center, b, b + 1]);
            }
        }
    }
    if let Some(&last) = prof_src.last() {
        let r = last[0].max(0.0);
        if r > 0.005 {
            let center = verts.len() as u32;
            verts.push(Vertex {
                position: [0.0, last[1], 0.0],
                normal: [0.0, 1.0, 0.0],
                color,
                uv: [0.5, 0.5],
            });
            for s in 0..seg {
                let a0 = (s as f32 / seg as f32) * TAU;
                let a1 = ((s + 1) as f32 / seg as f32) * TAU;
                let (s0, c0) = sin_cos(a0);
                let (s1, c1) = sin_cos(a1);
                let b = verts.len() as u32;
                verts.push(Vertex { position: [c0 * r, last[1], s0 * r], normal: [0.0, 1.0, 0.0], color, uv: [0.5, 0.5] });
                verts.push(Vertex { position: [c1 * r, last[1], s1 * r], normal: [0.0, 1.0, 0.0], color, uv: [0.5, 0.5] });
                indices.extend_from_slice(&[center, b + 1, b]);
            }
        }
    }

    (verts, indices)
}

fn signed_area_2d(pts: &[[f32; 2]]) -> f32 {
    let mut area = 0.0;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area * 0.5
}

fn cross_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle_2d(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let c1 = cross_2d(a, b, p);
    let c2 = cross_2d(b, c, p);
    let c3 = cross_2d(c, a, p);
    c1 >= -1e-6 && c2 >= -1e-6 && c3 >= -1e-6
}

fn triangulate_polygon_2d(pts: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[0, 1, 2]];
    }

    let mut remaining: Vec<usize> = (0..n).collect();
    let mut tris = Vec::with_capacity(n - 2);
    let mut guard = 0usize;
    let max_iters = n * n + 8;

    let mut i = 0usize;
    while remaining.len() > 3 && guard < max_iters {
        guard += 1;
        let len = remaining.len();
        let prev = remaining[(i + len - 1) % len];
        let curr = remaining[i % len];
        let next = remaining[(i + 1) % len];

        let a = pts[prev];
        let b = pts[curr];
        let c = pts[next];

        if cross_2d(a, b, c) > 1e-6 {
            let mut contains_other = false;
            for &other in &remaining {
                if other == prev || other == curr || other == next {
                    continue;
                }
                if point_in_triangle_2d(pts[other], a, b, c) {
                    contains_other = true;
                    break;
                }
            }
            if !contains_other {
                tris.push([prev, curr, next]);
                remaining.remove(i % len);
                if i >= remaining.len() {
                    i = 0;
                }
                continue;
            }
        }
        i = (i + 1) % len;
    }

    if remaining.len() == 3 {
        tris.push([remaining[0], remaining[1], remaining[2]]);
    }
    tris
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainPreset {
    Hills,
    Mountains,
    Island,
    Canyon,
}

impl TerrainPreset {
    pub const ALL: [Self; 4] = [Self::Hills, Self::Mountains, Self::Island, Self::Canyon];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Hills => "Colinas",
            Self::Mountains => "Montañas",
            Self::Island => "Isla",
            Self::Canyon => "Cañón",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainStamp {
    /// Normalized XZ coordinates inside [-0.5, 0.5].
    pub x: f32,
    pub z: f32,
    /// Influence radius in local terrain units (typically 0.08..=0.45).
    pub radius: f32,
    /// Height delta applied at the stamp center (+ raises, - digs).
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainSpec {
    pub seed: u32,
    pub amplitude: f32,
    pub frequency: f32,
    pub octaves: u8,
    pub preset: TerrainPreset,
    pub biome_tint: bool,
    pub stamps: Vec<TerrainStamp>,
}

impl Default for TerrainSpec {
    fn default() -> Self {
        Self {
            seed: 42,
            amplitude: 0.42,
            frequency: 2.6,
            octaves: 4,
            preset: TerrainPreset::Hills,
            biome_tint: true,
            stamps: Vec::new(),
        }
    }
}

#[inline]
fn floor_i32_no_std(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) > x {
        i - 1
    } else {
        i
    }
}

#[inline]
fn hash_2d(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(374_761_393)
        ^ (iz as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 16_777_215.0
}

fn value_noise_2d(x: f32, z: f32, seed: u32) -> f32 {
    let ix = floor_i32_no_std(x);
    let iz = floor_i32_no_std(z);
    let fx = x - ix as f32;
    let fz = z - iz as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);

    let v00 = hash_2d(ix, iz, seed);
    let v10 = hash_2d(ix + 1, iz, seed);
    let v01 = hash_2d(ix, iz + 1, seed);
    let v11 = hash_2d(ix + 1, iz + 1, seed);

    let a = v00 + (v10 - v00) * ux;
    let b = v01 + (v11 - v01) * ux;
    a + (b - a) * uz
}

fn fbm_2d(x: f32, z: f32, seed: u32, octaves: u8) -> f32 {
    let octs = octaves.clamp(1, 6);
    let mut sum = 0.0_f32;
    let mut amp = 0.5_f32;
    let mut freq = 1.0_f32;
    let mut norm = 0.0_f32;
    for o in 0..octs {
        let s = seed.wrapping_add((o as u32).wrapping_mul(1013));
        sum += amp * (value_noise_2d(x * freq, z * freq, s) * 2.0 - 1.0);
        norm += amp;
        amp *= 0.5;
        freq *= 2.05;
    }
    if norm > 1e-6 {
        sum / norm
    } else {
        0.0
    }
}

/// Evaluate local terrain height at normalized `(nx, nz)` inside `[-0.5, 0.5]`.
pub fn terrain_height_at(spec: &TerrainSpec, nx: f32, nz: f32) -> f32 {
    let freq = spec.frequency.max(0.2);
    let base = fbm_2d((nx + 0.5) * freq, (nz + 0.5) * freq, spec.seed, spec.octaves);
    let mut h = match spec.preset {
        TerrainPreset::Hills => base * spec.amplitude,
        TerrainPreset::Mountains => {
            let ridge = 1.0 - base.abs();
            (ridge * ridge * 1.35 - 0.35) * spec.amplitude
        }
        TerrainPreset::Island => {
            let r2 = (nx * nx + nz * nz) * 4.0;
            let falloff = (1.0 - r2).clamp(0.0, 1.0);
            let falloff_smooth = falloff * falloff * (3.0 - 2.0 * falloff);
            ((base * 0.65 + 0.55) * falloff_smooth - 0.15) * spec.amplitude
        }
        TerrainPreset::Canyon => {
            let valley = (nx * 2.2).abs().clamp(0.0, 1.0);
            let wall = valley * valley;
            (wall * 0.95 + base * 0.45 - 0.35) * spec.amplitude
        }
    };

    for stamp in &spec.stamps {
        let r = stamp.radius.max(0.02);
        let dx = nx - stamp.x;
        let dz = nz - stamp.z;
        let d2 = dx * dx + dz * dz;
        let r2 = r * r;
        if d2 < r2 {
            let t = 1.0 - d2 / r2;
            h += stamp.height * t * t;
        }
    }

    h
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

fn tint_terrain_color(base: [u8; 4], height: f32, min_h: f32, max_h: f32) -> [u8; 4] {
    let span = (max_h - min_h).max(1e-4);
    let t = ((height - min_h) / span).clamp(0.0, 1.0);
    if t < 0.32 {
        let k = t / 0.32;
        [
            lerp_u8((base[0] as f32 * 0.52) as u8, base[0], k),
            lerp_u8((base[1] as f32 * 0.50) as u8, base[1], k),
            lerp_u8((base[2] as f32 * 0.45) as u8, base[2], k),
            base[3],
        ]
    } else if t > 0.72 {
        let k = (t - 0.72) / 0.28;
        [
            lerp_u8(base[0], 230, k * 0.65),
            lerp_u8(base[1], 226, k * 0.65),
            lerp_u8(base[2], 216, k * 0.65),
            base[3],
        ]
    } else {
        base
    }
}

/// Build a subdivided 3D heightfield terrain mesh spanning `[-0.5, 0.5]` in XZ.
pub fn build_terrain(
    spec: &TerrainSpec,
    segments: u16,
    color: [u8; 4],
) -> (Vec<Vertex>, Vec<u32>) {
    let seg = (segments as usize).clamp(4, 64);
    let stride = seg + 1;
    let mut heights = Vec::with_capacity(stride * stride);
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;

    for z in 0..=seg {
        let vz = z as f32 / seg as f32;
        let nz = vz - 0.5;
        for x in 0..=seg {
            let ux = x as f32 / seg as f32;
            let nx = ux - 0.5;
            let h = terrain_height_at(spec, nx, nz);
            if h < min_h {
                min_h = h;
            }
            if h > max_h {
                max_h = h;
            }
            heights.push(h);
        }
    }

    let step = 1.0 / seg as f32;
    let mut verts = Vec::with_capacity(stride * stride);
    for z in 0..=seg {
        let vz = z as f32 / seg as f32;
        let nz = vz - 0.5;
        for x in 0..=seg {
            let ux = x as f32 / seg as f32;
            let nx = ux - 0.5;
            let idx = z * stride + x;
            let h = heights[idx];

            let h_l = heights[z * stride + x.saturating_sub(1)];
            let h_r = heights[z * stride + (x + 1).min(seg)];
            let h_d = heights[z.saturating_sub(1) * stride + x];
            let h_u = heights[(z + 1).min(seg) * stride + x];

            let dx_span = if x == 0 || x == seg { step } else { step * 2.0 };
            let dz_span = if z == 0 || z == seg { step } else { step * 2.0 };
            let normal = Vec3::new((h_l - h_r) / dx_span, 1.0, (h_d - h_u) / dz_span)
                .normalized()
                .to_array();

            let v_color = if spec.biome_tint && (max_h - min_h) > 1e-4 {
                tint_terrain_color(color, h, min_h, max_h)
            } else {
                color
            };

            verts.push(Vertex {
                position: [nx, h, nz],
                normal,
                color: v_color,
                uv: [ux, vz],
            });
        }
    }

    let mut indices = Vec::with_capacity(seg * seg * 6);
    for z in 0..seg {
        for x in 0..seg {
            let a = (z * stride + x) as u32;
            let b = a + 1;
            let c = ((z + 1) * stride + x) as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    (verts, indices)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshDeformKind {
    Taper,
    Twist,
    Bend,
}

/// Subdivide each triangle into 4 smaller triangles using edge midpoints.
pub fn subdivide_triangles(vertices: &[Vertex], indices: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    let mut out_verts = vertices.to_vec();
    let mut out_indices = Vec::with_capacity(indices.len() * 4);
    let mut mid_cache: BTreeMap<(u32, u32), u32> = BTreeMap::new();

    let mut get_mid = |a: u32, b: u32, verts: &mut Vec<Vertex>| -> u32 {
        let key = if a < b { (a, b) } else { (b, a) };
        if let Some(&idx) = mid_cache.get(&key) {
            return idx;
        }
        let va = verts[a as usize];
        let vb = verts[b as usize];
        let pos = [
            (va.position[0] + vb.position[0]) * 0.5,
            (va.position[1] + vb.position[1]) * 0.5,
            (va.position[2] + vb.position[2]) * 0.5,
        ];
        let normal = Vec3::new(
            va.normal[0] + vb.normal[0],
            va.normal[1] + vb.normal[1],
            va.normal[2] + vb.normal[2],
        )
        .normalized()
        .to_array();
        let color = [
            ((va.color[0] as u16 + vb.color[0] as u16) / 2) as u8,
            ((va.color[1] as u16 + vb.color[1] as u16) / 2) as u8,
            ((va.color[2] as u16 + vb.color[2] as u16) / 2) as u8,
            ((va.color[3] as u16 + vb.color[3] as u16) / 2) as u8,
        ];
        let uv = [(va.uv[0] + vb.uv[0]) * 0.5, (va.uv[1] + vb.uv[1]) * 0.5];
        let idx = verts.len() as u32;
        verts.push(Vertex { position: pos, normal, color, uv });
        mid_cache.insert(key, idx);
        idx
    };

    for tri in indices.chunks_exact(3) {
        let a = tri[0];
        let b = tri[1];
        let c = tri[2];
        if (a as usize) >= out_verts.len() || (b as usize) >= out_verts.len() || (c as usize) >= out_verts.len() {
            continue;
        }
        let ab = get_mid(a, b, &mut out_verts);
        let bc = get_mid(b, c, &mut out_verts);
        let ca = get_mid(c, a, &mut out_verts);
        out_indices.extend_from_slice(&[a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
    }
    (out_verts, out_indices)
}

/// Apply a modeling deformation (Taper, Twist, or Bend) along the Y axis and recompute face normals.
pub fn deform_vertices(
    vertices: &[Vertex],
    indices: &[u32],
    kind: MeshDeformKind,
    amount: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    if vertices.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for v in vertices {
        if v.position[1] < min_y { min_y = v.position[1]; }
        if v.position[1] > max_y { max_y = v.position[1]; }
    }
    let span_y = (max_y - min_y).max(0.001);
    let mut out_verts: Vec<Vertex> = vertices
        .iter()
        .map(|v| {
            let t = ((v.position[1] - min_y) / span_y).clamp(0.0, 1.0);
            let mut p = v.position;
            match kind {
                MeshDeformKind::Taper => {
                    let s = (1.0 + amount * (t - 0.5)).max(0.05);
                    p[0] *= s;
                    p[2] *= s;
                }
                MeshDeformKind::Twist => {
                    let angle = amount * (t - 0.5);
                    let (sn, cs) = sin_cos(angle);
                    let x = p[0] * cs - p[2] * sn;
                    let z = p[0] * sn + p[2] * cs;
                    p[0] = x;
                    p[2] = z;
                }
                MeshDeformKind::Bend => {
                    let angle = amount * t;
                    let (sn, cs) = sin_cos(angle);
                    let dx = p[0];
                    let dy = p[1] - min_y;
                    p[0] = dx * cs + dy * sn;
                    p[1] = min_y + (-dx * sn + dy * cs);
                }
            }
            Vertex { position: p, ..*v }
        })
        .collect();
    // Recompute normals smoothly across shared positions
    let mut acc_normals = vec![Vec3::ZERO; out_verts.len()];
    for tri in indices.chunks_exact(3) {
        let (Some(va), Some(vb), Some(vc)) = (
            out_verts.get(tri[0] as usize),
            out_verts.get(tri[1] as usize),
            out_verts.get(tri[2] as usize),
        ) else { continue };
        let pa = Vec3::new(va.position[0], va.position[1], va.position[2]);
        let pb = Vec3::new(vb.position[0], vb.position[1], vb.position[2]);
        let pc = Vec3::new(vc.position[0], vc.position[1], vc.position[2]);
        let w = (pb - pa).cross(pc - pa);
        acc_normals[tri[0] as usize] = acc_normals[tri[0] as usize] + w;
        acc_normals[tri[1] as usize] = acc_normals[tri[1] as usize] + w;
        acc_normals[tri[2] as usize] = acc_normals[tri[2] as usize] + w;
    }
    for (i, v) in out_verts.iter_mut().enumerate() {
        let n = acc_normals[i];
        if n.dot(n) > 1e-8 {
            v.normal = n.normalized().to_array();
        }
    }
    (out_verts, indices.to_vec())
}
