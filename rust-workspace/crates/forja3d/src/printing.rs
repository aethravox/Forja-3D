use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use core::fmt::Write;

use crate::format::{MeshPart, Model};
use crate::math::Vec3;
use crate::primitives::SceneDocument;

#[derive(Debug, Clone, PartialEq)]
pub struct PartWatertightReport {
    pub name: String,
    pub total_vertices: usize,
    pub total_triangles: usize,
    pub boundary_edges: usize,
    pub non_manifold_edges: usize,
    pub inverted_edges: usize,
    pub degenerate_triangles: usize,
    pub signed_volume: f32,
    pub is_watertight: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshWatertightReport {
    pub total_parts: usize,
    pub total_vertices: usize,
    pub total_triangles: usize,
    pub boundary_edges: usize,
    pub non_manifold_edges: usize,
    pub inverted_edges: usize,
    pub degenerate_triangles: usize,
    pub signed_volume: f32,
    pub is_watertight: bool,
    pub parts: Vec<PartWatertightReport>,
}

#[inline]
fn quantize_pos(p: [f32; 3]) -> (i64, i64, i64) {
    let q = |v: f32| -> i64 {
        if v.is_finite() {
            let scaled = v * 100_000.0;
            if scaled >= 0.0 {
                (scaled + 0.5) as i64
            } else {
                (scaled - 0.5) as i64
            }
        } else {
            0
        }
    };
    (q(p[0]), q(p[1]), q(p[2]))
}

#[derive(Default)]
struct EdgeStats {
    forward: usize,
    backward: usize,
}

/// Validates topological watertightness of a single `MeshPart`, welding coincident vertices
/// across flat-shaded faces by quantized 3D position so cubes/cylinders evaluate accurately.
pub fn validate_part_watertight(part: &MeshPart) -> PartWatertightReport {
    let mut pos_to_id: BTreeMap<(i64, i64, i64), usize> = BTreeMap::new();
    let mut vert_ids: Vec<usize> = Vec::with_capacity(part.vertices.len());
    for v in &part.vertices {
        let key = quantize_pos(v.position);
        let next_id = pos_to_id.len();
        let id = *pos_to_id.entry(key).or_insert(next_id);
        vert_ids.push(id);
    }

    let mut edges: BTreeMap<(usize, usize), EdgeStats> = BTreeMap::new();
    let mut total_triangles = 0usize;
    let mut degenerate_triangles = 0usize;
    let mut signed_volume_6 = 0.0_f64;

    for tri in part.indices.chunks_exact(3) {
        total_triangles += 1;
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let (Some(&q0), Some(&q1), Some(&q2)) = (vert_ids.get(i0), vert_ids.get(i1), vert_ids.get(i2)) else {
            degenerate_triangles += 1;
            continue;
        };

        if q0 == q1 || q1 == q2 || q2 == q0 {
            degenerate_triangles += 1;
            continue;
        }

        let p0 = part.vertices[i0].position;
        let p1 = part.vertices[i1].position;
        let p2 = part.vertices[i2].position;
        let e1 = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
        let e2 = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
        let cross = e1.cross(e2);
        if cross.dot(cross) < 1e-14 {
            degenerate_triangles += 1;
        }

        let v0 = [p0[0] as f64, p0[1] as f64, p0[2] as f64];
        let v1 = [p1[0] as f64, p1[1] as f64, p1[2] as f64];
        let v2 = [p2[0] as f64, p2[1] as f64, p2[2] as f64];
        let cx = v1[1] * v2[2] - v1[2] * v2[1];
        let cy = v1[2] * v2[0] - v1[0] * v2[2];
        let cz = v1[0] * v2[1] - v1[1] * v2[0];
        signed_volume_6 += v0[0] * cx + v0[1] * cy + v0[2] * cz;

        for &(a, b) in &[(q0, q1), (q1, q2), (q2, q0)] {
            if a < b {
                edges.entry((a, b)).or_default().forward += 1;
            } else {
                edges.entry((b, a)).or_default().backward += 1;
            }
        }
    }

    let mut boundary_edges = 0usize;
    let mut non_manifold_edges = 0usize;
    let mut inverted_edges = 0usize;

    for stats in edges.values() {
        let total = stats.forward + stats.backward;
        if total == 1 {
            boundary_edges += 1;
        } else if total > 2 {
            non_manifold_edges += 1;
        } else if total == 2 && (stats.forward == 2 || stats.backward == 2) {
            inverted_edges += 1;
        }
    }

    let signed_volume = (signed_volume_6 / 6.0) as f32;
    let is_watertight = total_triangles > 0
        && boundary_edges == 0
        && non_manifold_edges == 0
        && inverted_edges == 0
        && degenerate_triangles == 0
        && signed_volume >= -1e-6;

    PartWatertightReport {
        name: part.name.clone(),
        total_vertices: part.vertices.len(),
        total_triangles,
        boundary_edges,
        non_manifold_edges,
        inverted_edges,
        degenerate_triangles,
        signed_volume,
        is_watertight,
    }
}

impl Model {
    /// Checks every mesh part for 3D-printing watertightness (holes, non-manifold edges, inverted winding).
    pub fn validate_watertight(&self) -> MeshWatertightReport {
        let mut parts = Vec::with_capacity(self.parts.len());
        let mut total_vertices = 0usize;
        let mut total_triangles = 0usize;
        let mut boundary_edges = 0usize;
        let mut non_manifold_edges = 0usize;
        let mut inverted_edges = 0usize;
        let mut degenerate_triangles = 0usize;
        let mut signed_volume = 0.0_f32;
        let mut all_watertight = !self.parts.is_empty();

        for part in &self.parts {
            let report = validate_part_watertight(part);
            total_vertices += report.total_vertices;
            total_triangles += report.total_triangles;
            boundary_edges += report.boundary_edges;
            non_manifold_edges += report.non_manifold_edges;
            inverted_edges += report.inverted_edges;
            degenerate_triangles += report.degenerate_triangles;
            signed_volume += report.signed_volume;
            all_watertight &= report.is_watertight;
            parts.push(report);
        }

        MeshWatertightReport {
            total_parts: self.parts.len(),
            total_vertices,
            total_triangles,
            boundary_edges,
            non_manifold_edges,
            inverted_edges,
            degenerate_triangles,
            signed_volume,
            is_watertight: all_watertight,
            parts,
        }
    }

    /// Exports all mesh parts into a standard Binary `.stl` byte buffer for 3D printing / CAD.
    pub fn to_stl_binary(&self) -> Vec<u8> {
        let tri_count: usize = self
            .parts
            .iter()
            .map(|p| p.indices.len() / 3)
            .sum();
        let mut out = Vec::with_capacity(84 + tri_count * 50);

        let mut header = [0u8; 80];
        let label = format!("Forja-3D Binary STL: {}", self.name);
        let bytes = label.as_bytes();
        let copy_len = bytes.len().min(80);
        header[..copy_len].copy_from_slice(&bytes[..copy_len]);
        out.extend_from_slice(&header);
        out.extend_from_slice(&(tri_count.min(u32::MAX as usize) as u32).to_le_bytes());

        for part in &self.parts {
            for tri in part.indices.chunks_exact(3) {
                let (Some(v0), Some(v1), Some(v2)) = (
                    part.vertices.get(tri[0] as usize),
                    part.vertices.get(tri[1] as usize),
                    part.vertices.get(tri[2] as usize),
                ) else {
                    continue;
                };
                let p0 = v0.position;
                let p1 = v1.position;
                let p2 = v2.position;
                let e1 = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
                let e2 = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
                let n = e1.cross(e2).normalized().to_array();
                for f in n {
                    out.extend_from_slice(&(if f.is_finite() { f } else { 0.0 }).to_le_bytes());
                }
                for p in [p0, p1, p2] {
                    for f in p {
                        out.extend_from_slice(&(if f.is_finite() { f } else { 0.0 }).to_le_bytes());
                    }
                }
                out.extend_from_slice(&0u16.to_le_bytes());
            }
        }
        out
    }

    /// Exports all mesh parts into an ASCII `.ply` file preserving normals and RGBA vertex colors.
    pub fn to_ply_ascii(&self) -> Vec<u8> {
        let total_verts: usize = self.parts.iter().map(|p| p.vertices.len()).sum();
        let total_faces: usize = self.parts.iter().map(|p| p.indices.len() / 3).sum();

        let mut text = String::with_capacity(256 + total_verts * 48 + total_faces * 20);
        let _ = writeln!(text, "ply");
        let _ = writeln!(text, "format ascii 1.0");
        let _ = writeln!(text, "comment Generated by Forja-3D ({})", self.name);
        let _ = writeln!(text, "element vertex {total_verts}");
        let _ = writeln!(text, "property float x");
        let _ = writeln!(text, "property float y");
        let _ = writeln!(text, "property float z");
        let _ = writeln!(text, "property float nx");
        let _ = writeln!(text, "property float ny");
        let _ = writeln!(text, "property float nz");
        let _ = writeln!(text, "property uchar red");
        let _ = writeln!(text, "property uchar green");
        let _ = writeln!(text, "property uchar blue");
        let _ = writeln!(text, "property uchar alpha");
        let _ = writeln!(text, "element face {total_faces}");
        let _ = writeln!(text, "property list uchar int vertex_indices");
        let _ = writeln!(text, "end_header");

        for part in &self.parts {
            for v in &part.vertices {
                let _ = writeln!(
                    text,
                    "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {} {} {} {}",
                    v.position[0],
                    v.position[1],
                    v.position[2],
                    v.normal[0],
                    v.normal[1],
                    v.normal[2],
                    v.color[0],
                    v.color[1],
                    v.color[2],
                    v.color[3],
                );
            }
        }

        let mut base_vertex = 0usize;
        for part in &self.parts {
            for tri in part.indices.chunks_exact(3) {
                let _ = writeln!(
                    text,
                    "3 {} {} {}",
                    base_vertex + tri[0] as usize,
                    base_vertex + tri[1] as usize,
                    base_vertex + tri[2] as usize,
                );
            }
            base_vertex += part.vertices.len();
        }

        text.into_bytes()
    }
}

impl SceneDocument {
    pub fn validate_watertight(&self) -> MeshWatertightReport {
        self.bake().validate_watertight()
    }

    pub fn to_stl_binary(&self) -> Vec<u8> {
        self.bake().to_stl_binary()
    }

    pub fn to_ply_ascii(&self) -> Vec<u8> {
        self.bake().to_ply_ascii()
    }
}
