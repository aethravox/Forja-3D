use alloc::{format, string::String, vec, vec::Vec};
use core::fmt::Write;

use crate::format::Model;
use crate::math::{sqrtf, Vec3};
use crate::primitives::SunLightSpec;
use crate::texture::{Texture, TexturePixelFormat};

pub const GLB_MAGIC: [u8; 4] = *b"glTF";
pub const GLB_VERSION: u32 = 2;
const GLB_CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const GLB_CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Options controlling `.glb` (binary glTF 2.0) export.
#[derive(Debug, Clone, Copy, Default)]
pub struct GlbExportOptions<'a> {
    pub textures: &'a [Texture],
    pub part_texture_indices: &'a [Option<usize>],
    pub part_alpha_cutoffs: &'a [f32],
    pub sun_lights: &'a [SunLightSpec],
}

impl Model {
    /// Export this model to a self-contained binary `.glb` (glTF 2.0) container.
    pub fn to_glb(&self) -> Vec<u8> {
        self.to_glb_with_options(&GlbExportOptions::default())
    }

    /// Export this model to `.glb` preserving meshes, PBR materials, embedded PNG textures,
    /// alpha blending/cutout modes, and directional `KHR_lights_punctual` sun lights.
    pub fn to_glb_with_options(&self, opts: &GlbExportOptions<'_>) -> Vec<u8> {
        let mut bin: Vec<u8> = Vec::new();
        let mut buffer_views: Vec<(usize, usize, Option<u16>)> = Vec::new(); // (offset, length, target)
        let mut accessors: Vec<AccessorDef> = Vec::new();
        let mut images: Vec<(String, usize)> = Vec::new(); // (name, buffer_view_idx)
        let mut tex_alpha_flags: Vec<bool> = Vec::with_capacity(opts.textures.len());

        // 1. Embed textures as PNG image bufferViews
        for (ti, tex) in opts.textures.iter().enumerate() {
            let rgba = decode_texture_rgba(tex);
            let has_alpha = rgba.chunks_exact(4).any(|px| px[3] < 255);
            tex_alpha_flags.push(has_alpha);

            let png_bytes = encode_rgba_png(tex.width.max(1) as u32, tex.height.max(1) as u32, &rgba);
            align4(&mut bin);
            let offset = bin.len();
            bin.extend_from_slice(&png_bytes);
            let bv_idx = buffer_views.len();
            buffer_views.push((offset, png_bytes.len(), None));

            let tname = if tex.name.trim().is_empty() {
                format!("tex_{}", ti + 1)
            } else {
                tex.name.clone()
            };
            images.push((tname, bv_idx));
        }

        // 2. Encode each non-empty MeshPart into BIN + material + mesh + node
        struct MeshNodeEntry {
            name: String,
            pos_acc: usize,
            norm_acc: usize,
            uv_acc: usize,
            col_acc: Option<usize>,
            idx_acc: usize,
            mat_idx: usize,
        }

        struct MaterialEntry {
            name: String,
            base_color: [f32; 4],
            metallic: f32,
            roughness: f32,
            tex_idx: Option<usize>,
            alpha_mode: &'static str,
            alpha_cutoff: Option<f32>,
        }

        let mut mesh_entries: Vec<MeshNodeEntry> = Vec::new();
        let mut materials: Vec<MaterialEntry> = Vec::new();

        for (part_idx, part) in self.parts.iter().enumerate() {
            if part.vertices.is_empty() || part.indices.len() < 3 {
                continue;
            }

            let mat_col = part.material.base_color;
            let mut has_vertex_color_variation = false;
            let mut min_vertex_alpha = mat_col[3];
            for v in &part.vertices {
                if v.color != mat_col {
                    has_vertex_color_variation = true;
                }
                if v.color[3] < min_vertex_alpha {
                    min_vertex_alpha = v.color[3];
                }
            }

            // POSITION
            align4(&mut bin);
            let pos_offset = bin.len();
            let mut min_p = [f32::INFINITY; 3];
            let mut max_p = [f32::NEG_INFINITY; 3];
            for v in &part.vertices {
                let p = sanitize_vec3(v.position);
                for k in 0..3 {
                    if p[k] < min_p[k] {
                        min_p[k] = p[k];
                    }
                    if p[k] > max_p[k] {
                        max_p[k] = p[k];
                    }
                    bin.extend_from_slice(&p[k].to_le_bytes());
                }
            }
            let pos_bv = buffer_views.len();
            buffer_views.push((pos_offset, part.vertices.len() * 12, Some(34962)));
            let pos_acc = accessors.len();
            accessors.push(AccessorDef {
                buffer_view: pos_bv,
                component_type: 5126,
                count: part.vertices.len(),
                acc_type: "VEC3",
                normalized: false,
                min: Some(min_p),
                max: Some(max_p),
            });

            // NORMAL
            align4(&mut bin);
            let norm_offset = bin.len();
            for v in &part.vertices {
                let n = sanitize_normal(v.normal);
                for k in 0..3 {
                    bin.extend_from_slice(&n[k].to_le_bytes());
                }
            }
            let norm_bv = buffer_views.len();
            buffer_views.push((norm_offset, part.vertices.len() * 12, Some(34962)));
            let norm_acc = accessors.len();
            accessors.push(AccessorDef {
                buffer_view: norm_bv,
                component_type: 5126,
                count: part.vertices.len(),
                acc_type: "VEC3",
                normalized: false,
                min: None,
                max: None,
            });

            // TEXCOORD_0
            align4(&mut bin);
            let uv_offset = bin.len();
            for v in &part.vertices {
                let u = sanitize_f32(v.uv[0]);
                let vv = sanitize_f32(v.uv[1]);
                bin.extend_from_slice(&u.to_le_bytes());
                bin.extend_from_slice(&vv.to_le_bytes());
            }
            let uv_bv = buffer_views.len();
            buffer_views.push((uv_offset, part.vertices.len() * 8, Some(34962)));
            let uv_acc = accessors.len();
            accessors.push(AccessorDef {
                buffer_view: uv_bv,
                component_type: 5126,
                count: part.vertices.len(),
                acc_type: "VEC2",
                normalized: false,
                min: None,
                max: None,
            });

            // COLOR_0 (only emitted when vertices diverge from material.base_color, e.g. biome tint)
            let col_acc = if has_vertex_color_variation {
                align4(&mut bin);
                let col_offset = bin.len();
                for v in &part.vertices {
                    bin.extend_from_slice(&v.color);
                }
                let col_bv = buffer_views.len();
                buffer_views.push((col_offset, part.vertices.len() * 4, Some(34962)));
                let acc = accessors.len();
                accessors.push(AccessorDef {
                    buffer_view: col_bv,
                    component_type: 5121,
                    count: part.vertices.len(),
                    acc_type: "VEC4",
                    normalized: true,
                    min: None,
                    max: None,
                });
                Some(acc)
            } else {
                None
            };

            // INDICES
            align4(&mut bin);
            let idx_offset = bin.len();
            let valid_tri_count = part.indices.len() / 3;
            let idx_count = valid_tri_count * 3;
            let use_u16 = part.vertices.len() <= u16::MAX as usize;
            let idx_component_type = if use_u16 { 5123 } else { 5125 };
            let v_len = part.vertices.len() as u32;
            for &raw_idx in part.indices.iter().take(idx_count) {
                let idx = if raw_idx < v_len { raw_idx } else { 0 };
                if use_u16 {
                    bin.extend_from_slice(&(idx as u16).to_le_bytes());
                } else {
                    bin.extend_from_slice(&idx.to_le_bytes());
                }
            }
            let idx_byte_len = bin.len() - idx_offset;
            let idx_bv = buffer_views.len();
            buffer_views.push((idx_offset, idx_byte_len, Some(34963)));
            let idx_acc = accessors.len();
            accessors.push(AccessorDef {
                buffer_view: idx_bv,
                component_type: idx_component_type,
                count: idx_count,
                acc_type: "SCALAR",
                normalized: false,
                min: None,
                max: None,
            });

            // Material resolution
            let assigned_tex = opts
                .part_texture_indices
                .get(part_idx)
                .copied()
                .flatten()
                .filter(|&ti| ti < images.len());
            let tex_has_alpha = assigned_tex
                .and_then(|ti| tex_alpha_flags.get(ti).copied())
                .unwrap_or(false);
            let cutoff = opts
                .part_alpha_cutoffs
                .get(part_idx)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);

            let (alpha_mode, alpha_cutoff) = if cutoff > 0.001 {
                ("MASK", Some(cutoff))
            } else if min_vertex_alpha < 255 || tex_has_alpha {
                ("BLEND", None)
            } else {
                ("OPAQUE", None)
            };

            let base_color = if has_vertex_color_variation {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [
                    mat_col[0] as f32 / 255.0,
                    mat_col[1] as f32 / 255.0,
                    mat_col[2] as f32 / 255.0,
                    mat_col[3] as f32 / 255.0,
                ]
            };

            let mat_idx = materials.len();
            materials.push(MaterialEntry {
                name: format!("{}_mat", part.name),
                base_color,
                metallic: sanitize_f32(part.material.metallic).clamp(0.0, 1.0),
                roughness: sanitize_f32(part.material.roughness).clamp(0.04, 1.0),
                tex_idx: assigned_tex,
                alpha_mode,
                alpha_cutoff,
            });

            mesh_entries.push(MeshNodeEntry {
                name: part.name.clone(),
                pos_acc,
                norm_acc,
                uv_acc,
                col_acc,
                idx_acc,
                mat_idx,
            });
        }

        // 3. Enabled directional Sun lights (KHR_lights_punctual)
        let active_suns: Vec<&SunLightSpec> = opts
            .sun_lights
            .iter()
            .filter(|s| s.enabled && s.intensity > 0.0001)
            .collect();

        // 4. Build glTF 2.0 JSON
        let mut json = String::new();
        json.push('{');
        json.push_str(r#""asset":{"version":"2.0","generator":"Forja3D"}"#);

        if !active_suns.is_empty() {
            json.push_str(r#","extensionsUsed":["KHR_lights_punctual"]"#);
            json.push_str(r#","extensions":{"KHR_lights_punctual":{"lights":["#);
            for (li, sun) in active_suns.iter().enumerate() {
                if li > 0 {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    r#"{{"name":"{}","type":"directional","color":[{:.4},{:.4},{:.4}],"intensity":{:.4}}}"#,
                    escape_json(&sun.name),
                    sun.color[0] as f32 / 255.0,
                    sun.color[1] as f32 / 255.0,
                    sun.color[2] as f32 / 255.0,
                    sanitize_f32(sun.intensity).max(0.0),
                );
            }
            json.push_str("]}}");
        }

        let total_nodes = mesh_entries.len() + active_suns.len();
        json.push_str(r#","scene":0,"scenes":[{"name":""#);
        json.push_str(&escape_json(&self.name));
        json.push_str(r#"","nodes":["#);
        for ni in 0..total_nodes {
            if ni > 0 {
                json.push(',');
            }
            let _ = write!(json, "{ni}");
        }
        json.push_str("]}]");

        if total_nodes > 0 {
            json.push_str(r#","nodes":["#);
            let mut first = true;
            for (mi, m) in mesh_entries.iter().enumerate() {
                if !first {
                    json.push(',');
                }
                first = false;
                let _ = write!(json, r#"{{"name":"{}","mesh":{mi}}}"#, escape_json(&m.name));
            }
            for (li, sun) in active_suns.iter().enumerate() {
                if !first {
                    json.push(',');
                }
                first = false;
                let pos = sanitize_vec3(sun.position);
                let q = quat_from_neg_z_to(sun.ray_direction());
                let _ = write!(
                    json,
                    r#"{{"name":"{}","translation":[{:.6},{:.6},{:.6}],"rotation":[{:.6},{:.6},{:.6},{:.6}],"extensions":{{"KHR_lights_punctual":{{"light":{li}}}}}}}"#,
                    escape_json(&sun.name),
                    pos[0], pos[1], pos[2],
                    q[0], q[1], q[2], q[3],
                );
            }
            json.push(']');
        }

        if !mesh_entries.is_empty() {
            json.push_str(r#","meshes":["#);
            for (mi, m) in mesh_entries.iter().enumerate() {
                if mi > 0 {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    r#"{{"name":"{}","primitives":[{{"attributes":{{"POSITION":{},"NORMAL":{},"TEXCOORD_0":{}"#,
                    escape_json(&m.name),
                    m.pos_acc,
                    m.norm_acc,
                    m.uv_acc,
                );
                if let Some(ca) = m.col_acc {
                    let _ = write!(json, r#","COLOR_0":{ca}"#);
                }
                let _ = write!(
                    json,
                    r#"}},"indices":{},"material":{}}}]}}"#,
                    m.idx_acc, m.mat_idx
                );
            }
            json.push(']');
        }

        if !materials.is_empty() {
            json.push_str(r#","materials":["#);
            for (mi, mat) in materials.iter().enumerate() {
                if mi > 0 {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    r#"{{"name":"{}","doubleSided":true,"alphaMode":"{}""#,
                    escape_json(&mat.name),
                    mat.alpha_mode,
                );
                if let Some(cut) = mat.alpha_cutoff {
                    let _ = write!(json, r#","alphaCutoff":{cut:.4}"#);
                }
                let _ = write!(
                    json,
                    r#","pbrMetallicRoughness":{{"baseColorFactor":[{:.5},{:.5},{:.5},{:.5}],"metallicFactor":{:.4},"roughnessFactor":{:.4}"#,
                    mat.base_color[0],
                    mat.base_color[1],
                    mat.base_color[2],
                    mat.base_color[3],
                    mat.metallic,
                    mat.roughness,
                );
                if let Some(ti) = mat.tex_idx {
                    let _ = write!(json, r#","baseColorTexture":{{"index":{ti}}}"#);
                }
                json.push_str("}}");
            }
            json.push(']');
        }

        if !images.is_empty() {
            json.push_str(r#","samplers":[{"magFilter":9729,"minFilter":9987,"wrapS":10497,"wrapT":10497}]"#);
            json.push_str(r#","textures":["#);
            for ti in 0..images.len() {
                if ti > 0 {
                    json.push(',');
                }
                let _ = write!(json, r#"{{"sampler":0,"source":{ti}}}"#);
            }
            json.push(']');
            json.push_str(r#","images":["#);
            for (ii, (name, bv_idx)) in images.iter().enumerate() {
                if ii > 0 {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    r#"{{"name":"{}","bufferView":{bv_idx},"mimeType":"image/png"}}"#,
                    escape_json(name)
                );
            }
            json.push(']');
        }

        if !accessors.is_empty() {
            json.push_str(r#","accessors":["#);
            for (ai, acc) in accessors.iter().enumerate() {
                if ai > 0 {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    r#"{{"bufferView":{},"componentType":{},"count":{},"type":"{}""#,
                    acc.buffer_view, acc.component_type, acc.count, acc.acc_type
                );
                if acc.normalized {
                    json.push_str(r#","normalized":true"#);
                }
                if let (Some(mn), Some(mx)) = (acc.min, acc.max) {
                    let _ = write!(
                        json,
                        r#","min":[{:.6},{:.6},{:.6}],"max":[{:.6},{:.6},{:.6}]"#,
                        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
                    );
                }
                json.push('}');
            }
            json.push(']');
        }

        if !buffer_views.is_empty() {
            json.push_str(r#","bufferViews":["#);
            for (bi, &(offset, len, target)) in buffer_views.iter().enumerate() {
                if bi > 0 {
                    json.push(',');
                }
                let _ = write!(json, r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{len}"#);
                if let Some(t) = target {
                    let _ = write!(json, r#","target":{t}"#);
                }
                json.push('}');
            }
            json.push(']');
        }

        align4(&mut bin);
        if !bin.is_empty() {
            let _ = write!(json, r#","buffers":[{{"byteLength":{}}}]"#, bin.len());
        }
        json.push('}');

        // 5. Pack GLB v2 binary container (12-byte header + JSON chunk + optional BIN chunk)
        let mut json_bytes = json.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let has_bin = !bin.is_empty();
        let total_len = 12 + 8 + json_bytes.len() + if has_bin { 8 + bin.len() } else { 0 };
        let mut glb = Vec::with_capacity(total_len);
        glb.extend_from_slice(&GLB_MAGIC);
        glb.extend_from_slice(&GLB_VERSION.to_le_bytes());
        glb.extend_from_slice(&(total_len as u32).to_le_bytes());

        // JSON chunk
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&GLB_CHUNK_JSON.to_le_bytes());
        glb.extend_from_slice(&json_bytes);

        // BIN chunk
        if has_bin {
            glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            glb.extend_from_slice(&GLB_CHUNK_BIN.to_le_bytes());
            glb.extend_from_slice(&bin);
        }

        glb
    }
}

struct AccessorDef {
    buffer_view: usize,
    component_type: u16,
    count: usize,
    acc_type: &'static str,
    normalized: bool,
    min: Option<[f32; 3]>,
    max: Option<[f32; 3]>,
}

/// Encode a `Texture` into valid `#![no_std]` PNG bytes (RGBA8 + zlib IDAT).
pub fn encode_texture_to_png(tex: &Texture) -> Vec<u8> {
    let rgba = decode_texture_rgba(tex);
    encode_rgba_png(tex.width.max(1) as u32, tex.height.max(1) as u32, &rgba)
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let row_bytes = w * 4;

    let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * h);
    for y in 0..h {
        raw_scanlines.push(0); // filter type 0 (None)
        let start = y * row_bytes;
        let end = start + row_bytes;
        if end <= rgba.len() {
            raw_scanlines.extend_from_slice(&rgba[start..end]);
        } else {
            for _ in 0..w {
                raw_scanlines.extend_from_slice(&[255, 255, 255, 255]);
            }
        }
    }

    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw_scanlines, 6);

    let mut out = Vec::with_capacity(64 + compressed.len());
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(width.max(1)).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(height.max(1)).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type: RGBA
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    push_png_chunk(&mut out, *b"IHDR", &ihdr);
    push_png_chunk(&mut out, *b"IDAT", &compressed);
    push_png_chunk(&mut out, *b"IEND", &[]);
    out
}

fn push_png_chunk(out: &mut Vec<u8>, chunk_type: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&chunk_type);
    out.extend_from_slice(data);
    let mut crc = crc32_update(0xFFFF_FFFF, &chunk_type);
    crc = crc32_update(crc, data);
    out.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    crc
}

fn decode_texture_rgba(tex: &Texture) -> Vec<u8> {
    let w = (tex.width as usize).max(1);
    let h = (tex.height as usize).max(1);
    let expected_pixels = w * h;
    let mut out = vec![255u8; expected_pixels * 4];
    match tex.format {
        TexturePixelFormat::Rgba8 => {
            let copy_len = tex.data.len().min(out.len());
            out[..copy_len].copy_from_slice(&tex.data[..copy_len]);
        }
        TexturePixelFormat::Rgb8 => {
            for i in 0..expected_pixels {
                let src = i * 3;
                if src + 2 < tex.data.len() {
                    let dst = i * 4;
                    out[dst] = tex.data[src];
                    out[dst + 1] = tex.data[src + 1];
                    out[dst + 2] = tex.data[src + 2];
                    out[dst + 3] = 255;
                }
            }
        }
        TexturePixelFormat::R8 => {
            for i in 0..expected_pixels {
                if let Some(&g) = tex.data.get(i) {
                    let dst = i * 4;
                    out[dst] = g;
                    out[dst + 1] = g;
                    out[dst + 2] = g;
                    out[dst + 3] = 255;
                }
            }
        }
        TexturePixelFormat::RleRgba8 => {
            let mut dst = 0usize;
            for chunk in tex.data.chunks_exact(5) {
                let count = chunk[0].max(1) as usize;
                for _ in 0..count {
                    if dst + 3 >= out.len() {
                        break;
                    }
                    out[dst] = chunk[1];
                    out[dst + 1] = chunk[2];
                    out[dst + 2] = chunk[3];
                    out[dst + 3] = chunk[4];
                    dst += 4;
                }
            }
        }
    }
    out
}

fn align4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn sanitize_f32(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn sanitize_vec3(v: [f32; 3]) -> [f32; 3] {
    [sanitize_f32(v[0]), sanitize_f32(v[1]), sanitize_f32(v[2])]
}

fn sanitize_normal(n: [f32; 3]) -> [f32; 3] {
    let s = sanitize_vec3(n);
    let len = sqrtf(s[0] * s[0] + s[1] * s[1] + s[2] * s[2]);
    if len > 1e-6 {
        [s[0] / len, s[1] / len, s[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn quat_from_neg_z_to(dir: [f32; 3]) -> [f32; 4] {
    let b = Vec3::new(sanitize_f32(dir[0]), sanitize_f32(dir[1]), sanitize_f32(dir[2])).normalized();
    let a = Vec3::new(0.0, 0.0, -1.0);
    let dot = a.dot(b);
    if dot > 0.999_99 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if dot < -0.999_99 {
        return [0.0, 1.0, 0.0, 0.0];
    }
    let c = a.cross(b);
    let w = 1.0 + dot;
    let len = sqrtf(c.x * c.x + c.y * c.y + c.z * c.z + w * w);
    if len > 1e-6 {
        [c.x / len, c.y / len, c.z / len, w / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r#"\\"#),
            '\n' => out.push_str(r#"\n"#),
            '\r' => out.push_str(r#"\r"#),
            '\t' => out.push_str(r#"\t"#),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, r#"\u{:04x}"#, c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
