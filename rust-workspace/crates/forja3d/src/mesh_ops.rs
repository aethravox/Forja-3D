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
