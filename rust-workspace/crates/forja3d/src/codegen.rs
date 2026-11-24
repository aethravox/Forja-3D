use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;

use crate::format::{MeshPart, Model, Vertex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustExportTarget {
    /// Emits a builder returning `forja3d::Model`
    Forja3dModel,
    /// Emits pure zero-dependency `const` arrays (`VERTICES`, `NORMALS`, `COLORS`, `INDICES`)
    ZeroDepConst,
    /// Emits `#[repr(C)]` `Vertex` structs ready for `wgpu` + `bytemuck`
    WgpuBytemuck,
}

impl RustExportTarget {
    pub const ALL: [Self; 3] = [
        Self::ZeroDepConst,
        Self::Forja3dModel,
        Self::WgpuBytemuck,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::ZeroDepConst => "Zero-dep const",
            Self::Forja3dModel => "forja3d::Model",
            Self::WgpuBytemuck => "wgpu / bytemuck",
        }
    }
}

impl Model {
    /// Generate ready-to-compile Rust source code (`.rs`) for this model.
    pub fn to_rust_source(&self, mod_name: &str, target: RustExportTarget) -> String {
        let mut out = String::new();
        let safe_name = sanitize_ident(mod_name);
        let _ = writeln!(
            out,
            "// Generado por Forja3D — Modelo: {} ({} vértices, {} triángulos)",
            self.name,
            self.vertex_count(),
            self.triangle_count()
        );
        let _ = writeln!(out, "#![allow(dead_code, clippy::excessive_precision)]\n");

        match target {
            RustExportTarget::ZeroDepConst => emit_zero_dep(&mut out, self),
            RustExportTarget::Forja3dModel => emit_forja3d_model(&mut out, &safe_name, self),
            RustExportTarget::WgpuBytemuck => emit_wgpu_bytemuck(&mut out, self),
        }
        out
    }

    /// Export this model as a standard Wavefront `.obj` string (compatible with Blender, Godot, Unity).
    pub fn to_obj(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# Exportado por Forja3D — Modelo: {} ({} vértices, {} triángulos)",
            self.name,
            self.vertex_count(),
            self.triangle_count()
        );

        let mut v_offset = 1usize;
        for part in &self.parts {
            let safe_part = sanitize_ident(&part.name);
            let _ = writeln!(out, "\no {safe_part}");
            for v in &part.vertices {
                let _ = writeln!(
                    out,
                    "v {:.5} {:.5} {:.5} {:.3} {:.3} {:.3}",
                    v.position[0],
                    v.position[1],
                    v.position[2],
                    v.color[0] as f32 / 255.0,
                    v.color[1] as f32 / 255.0,
                    v.color[2] as f32 / 255.0
                );
            }
            for v in &part.vertices {
                let _ = writeln!(out, "vt {:.4} {:.4}", v.uv[0], v.uv[1]);
            }
            for v in &part.vertices {
                let _ = writeln!(
                    out,
                    "vn {:.5} {:.5} {:.5}",
                    v.normal[0], v.normal[1], v.normal[2]
                );
            }
            for tri in part.indices.chunks_exact(3) {
                let a = v_offset + tri[0] as usize;
                let b = v_offset + tri[1] as usize;
                let c = v_offset + tri[2] as usize;
                let _ = writeln!(out, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
            }
            v_offset += part.vertices.len();
        }
        out
    }
}

fn merge_parts(model: &Model) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::with_capacity(model.vertex_count());
    let mut indices = Vec::with_capacity(model.triangle_count() * 3);
    for part in &model.parts {
        let offset = verts.len() as u32;
        verts.extend_from_slice(&part.vertices);
        for &idx in &part.indices {
            indices.push(offset + idx);
        }
    }
    (verts, indices)
}

fn emit_zero_dep(out: &mut String, model: &Model) {
    let (verts, indices) = merge_parts(model);
    let _ = writeln!(out, "pub const VERTEX_COUNT: usize = {};", verts.len());
    let _ = writeln!(out, "pub const TRIANGLE_COUNT: usize = {};\n", indices.len() / 3);

    let _ = writeln!(out, "pub const POSITIONS: &[[f32; 3]] = &[");
    for v in &verts {
        let _ = writeln!(out, "    [{:.5}, {:.5}, {:.5}],", v.position[0], v.position[1], v.position[2]);
    }
    let _ = writeln!(out, "];\n");

    let _ = writeln!(out, "pub const NORMALS: &[[f32; 3]] = &[");
    for v in &verts {
        let _ = writeln!(out, "    [{:.5}, {:.5}, {:.5}],", v.normal[0], v.normal[1], v.normal[2]);
    }
    let _ = writeln!(out, "];\n");

    let _ = writeln!(out, "pub const COLORS: &[[u8; 4]] = &[");
    for v in &verts {
        let _ = writeln!(out, "    [{}, {}, {}, {}],", v.color[0], v.color[1], v.color[2], v.color[3]);
    }
    let _ = writeln!(out, "];\n");

    let _ = writeln!(out, "pub const INDICES: &[u32] = &[");
    for tri in indices.chunks(3) {
        if tri.len() == 3 {
            let _ = writeln!(out, "    {}, {}, {},", tri[0], tri[1], tri[2]);
        }
    }
    let _ = writeln!(out, "];");
}

fn emit_wgpu_bytemuck(out: &mut String, model: &Model) {
    let (verts, indices) = merge_parts(model);
    let _ = writeln!(out, "#[repr(C)]");
    let _ = writeln!(out, "#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]");
    let _ = writeln!(out, "pub struct Vertex {{");
    let _ = writeln!(out, "    pub position: [f32; 3],");
    let _ = writeln!(out, "    pub normal: [f32; 3],");
    let _ = writeln!(out, "    pub color: [f32; 4],");
    let _ = writeln!(out, "    pub uv: [f32; 2],");
    let _ = writeln!(out, "}}\n");

    let _ = writeln!(out, "pub const VERTICES: &[Vertex] = &[");
    for v in &verts {
        let c = [
            v.color[0] as f32 / 255.0,
            v.color[1] as f32 / 255.0,
            v.color[2] as f32 / 255.0,
            v.color[3] as f32 / 255.0,
        ];
        let _ = writeln!(
            out,
            "    Vertex {{ position: [{:.5}, {:.5}, {:.5}], normal: [{:.5}, {:.5}, {:.5}], color: [{:.4}, {:.4}, {:.4}, {:.4}], uv: [{:.4}, {:.4}] }},",
            v.position[0], v.position[1], v.position[2],
            v.normal[0], v.normal[1], v.normal[2],
            c[0], c[1], c[2], c[3],
            v.uv[0], v.uv[1],
        );
    }
    let _ = writeln!(out, "];\n");

    let _ = writeln!(out, "pub const INDICES: &[u32] = &[");
    for tri in indices.chunks(3) {
        if tri.len() == 3 {
            let _ = writeln!(out, "    {}, {}, {},", tri[0], tri[1], tri[2]);
        }
    }
    let _ = writeln!(out, "];");
}

fn emit_forja3d_model(out: &mut String, fn_name: &str, model: &Model) {
    let _ = writeln!(out, "use forja3d::{{Aabb, Material, MeshPart, Model, Vertex}};\n");
    let _ = writeln!(out, "pub fn build_{fn_name}() -> Model {{");
    let _ = writeln!(out, "    Model {{");
    let _ = writeln!(out, "        name: String::from({:?}),", model.name);
    let _ = writeln!(out, "        parts: vec![");
    for part in &model.parts {
        emit_part(out, part);
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
}

fn emit_part(out: &mut String, part: &MeshPart) {
    let _ = writeln!(out, "            MeshPart {{");
    let _ = writeln!(out, "                name: String::from({:?}),", part.name);
    let _ = writeln!(
        out,
        "                material: Material {{ base_color: {:?}, metallic: {:.4}, roughness: {:.4} }},",
        part.material.base_color, part.material.metallic, part.material.roughness
    );
    let _ = writeln!(
        out,
        "                bounds: Aabb {{ min: [{:.5}, {:.5}, {:.5}], max: [{:.5}, {:.5}, {:.5}] }},",
        part.bounds.min[0], part.bounds.min[1], part.bounds.min[2],
        part.bounds.max[0], part.bounds.max[1], part.bounds.max[2],
    );
    let _ = writeln!(out, "                vertices: vec![");
    for v in &part.vertices {
        let _ = writeln!(
            out,
            "                    Vertex {{ position: [{:.5}, {:.5}, {:.5}], normal: [{:.5}, {:.5}, {:.5}], color: {:?}, uv: [{:.4}, {:.4}] }},",
            v.position[0], v.position[1], v.position[2],
            v.normal[0], v.normal[1], v.normal[2],
            v.color,
            v.uv[0], v.uv[1]
        );
    }
    let _ = writeln!(out, "                ],");
    let idx_list: Vec<String> = part.indices.iter().map(|i| format!("{i}")).collect();
    let _ = writeln!(out, "                indices: vec![{}],", idx_list.join(", "));
    let _ = writeln!(out, "            }},");
}

fn sanitize_ident(input: &str) -> String {
    let mut s = String::new();
    for (idx, ch) in input.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_';
        if idx == 0 && ch.is_ascii_digit() {
            s.push('_');
        }
        s.push(if valid { ch.to_ascii_lowercase() } else { '_' });
    }
    if s.is_empty() {
        String::from("model")
    } else {
        s
    }
}
