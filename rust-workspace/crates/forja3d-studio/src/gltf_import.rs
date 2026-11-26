use forja3d::{
    apply_shading, Aabb, Material, MeshPart, PartSpec, SceneDocument, ShadingMode, Texture,
    Vertex,
};
use std::path::Path;

pub struct ImportedGltf {
    pub scene: SceneDocument,
    pub textures: Vec<Texture>,
}

struct DecodedImage {
    name: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    average: [u8; 4],
}

impl DecodedImage {
    fn sample(&self, uv: [f32; 2]) -> [u8; 4] {
        if self.width == 0 || self.height == 0 || self.rgba.len() < 4 {
            return [255, 255, 255, 255];
        }
        let u = if uv[0].is_finite() { uv[0].rem_euclid(1.0) } else { 0.0 };
        let v = if uv[1].is_finite() { uv[1].rem_euclid(1.0) } else { 0.0 };
        let x = ((u * (self.width.saturating_sub(1)) as f32) + 0.5) as usize;
        let y = ((v * (self.height.saturating_sub(1)) as f32) + 0.5) as usize;
        let x = x.min(self.width as usize - 1);
        let y = y.min(self.height as usize - 1);
        let idx = (y * self.width as usize + x) * 4;
        [
            self.rgba[idx],
            self.rgba[idx + 1],
            self.rgba[idx + 2],
            self.rgba[idx + 3],
        ]
    }
}

#[allow(dead_code)]
pub fn import_gltf_or_glb(
    filename: Option<&str>,
    bytes: &[u8],
    base_dir: Option<&Path>,
) -> Result<SceneDocument, String> {
    import_gltf_or_glb_full(filename, bytes, base_dir).map(|r| r.scene)
}

pub fn import_gltf_or_glb_full(
    filename: Option<&str>,
    bytes: &[u8],
    base_dir: Option<&Path>,
) -> Result<ImportedGltf, String> {
    let gltf = gltf::Gltf::from_slice(bytes)
        .map_err(|e| format!("Error leyendo GLB/GLTF: {e}"))?;

    let buffers = resolve_buffers(&gltf.document, gltf.blob.as_deref(), base_dir)?;
    let images = resolve_images(&gltf.document, &buffers, base_dir);
    let mut image_to_tex_idx: Vec<Option<usize>> = vec![None; images.len()];
    let mut textures: Vec<Texture> = Vec::new();
    for (img_idx, img_opt) in images.iter().enumerate() {
        if let Some(img) = img_opt {
            if let (Ok(w), Ok(h)) = (u16::try_from(img.width), u16::try_from(img.height)) {
                if w > 0 && h > 0 {
                    let tex_idx = textures.len();
                    textures.push(Texture::new_rgba(img.name.clone(), w, h, img.rgba.clone()));
                    image_to_tex_idx[img_idx] = Some(tex_idx);
                }
            }
        }
    }

    let mut parts: Vec<PartSpec> = Vec::new();
    let saw_draco = gltf
        .document
        .extensions_required()
        .any(|ext| ext == "KHR_draco_mesh_compression")
        || gltf
            .document
            .extensions_used()
            .any(|ext| ext == "KHR_draco_mesh_compression");

    if let Some(scene) = gltf
        .document
        .default_scene()
        .or_else(|| gltf.document.scenes().next())
    {
        for node in scene.nodes() {
            collect_node(
                &node,
                IDENTITY,
                &buffers,
                &images,
                &image_to_tex_idx,
                &mut parts,
            );
        }
    } else {
        for node in gltf.document.nodes() {
            collect_node(
                &node,
                IDENTITY,
                &buffers,
                &images,
                &image_to_tex_idx,
                &mut parts,
            );
        }
    }

    if parts.is_empty() {
        if saw_draco {
            return Err(String::from(
                "Este archivo GLB/GLTF usa compresión Draco (KHR_draco_mesh_compression), aún no soportada. Expórtalo sin Draco.",
            ));
        }
        return Err(String::from(
            "El archivo GLB/GLTF no contiene mallas triangulares editables.",
        ));
    }

    let model_name = gltf
        .document
        .default_scene()
        .or_else(|| gltf.document.scenes().next())
        .and_then(|s| s.name())
        .map(String::from)
        .or_else(|| {
            filename.map(|f| {
                f.trim_end_matches(".glb")
                    .trim_end_matches(".GLB")
                    .trim_end_matches(".gltf")
                    .trim_end_matches(".GLTF")
                    .to_string()
            })
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from("ModeloGLTF"));

    Ok(ImportedGltf {
        scene: SceneDocument {
            model_name,
            parts,
            ..SceneDocument::default()
        },
        textures,
    })
}

fn collect_node(
    node: &gltf::Node<'_>,
    parent_world: [[f32; 4]; 4],
    buffers: &[Vec<u8>],
    images: &[Option<DecodedImage>],
    image_to_tex_idx: &[Option<usize>],
    out_parts: &mut Vec<PartSpec>,
) {
    let local = node.transform().matrix();
    let world = mat4_mul(parent_world, local);
    let norm_mat = normal_matrix_3x3(world);

    if let Some(mesh) = node.mesh() {
        let base_name = node
            .name()
            .or_else(|| mesh.name())
            .map(String::from)
            .unwrap_or_else(|| format!("pieza_{}", out_parts.len() + 1));

        let prim_count = mesh.primitives().count();
        for (prim_idx, primitive) in mesh.primitives().enumerate() {
            if let Some((mesh_part, tex_idx)) = convert_primitive(
                &primitive,
                &base_name,
                prim_idx,
                prim_count,
                world,
                norm_mat,
                buffers,
                images,
                image_to_tex_idx,
            ) {
                let mut part = PartSpec::from_mesh_part(mesh_part);
                part.texture_index = tex_idx;
                part.texture_enabled = tex_idx.is_some();
                if prim_count > 1 {
                    part.group = Some(base_name.clone());
                }
                out_parts.push(part);
            }
        }
    }

    for child in node.children() {
        collect_node(&child, world, buffers, images, image_to_tex_idx, out_parts);
    }
}

#[allow(clippy::too_many_arguments)]
fn convert_primitive(
    primitive: &gltf::Primitive<'_>,
    base_name: &str,
    prim_idx: usize,
    prim_count: usize,
    world: [[f32; 4]; 4],
    norm_mat: [[f32; 3]; 3],
    buffers: &[Vec<u8>],
    images: &[Option<DecodedImage>],
    image_to_tex_idx: &[Option<usize>],
) -> Option<(MeshPart, Option<usize>)> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    if positions.is_empty() {
        return None;
    }

    let raw_indices: Vec<u32> = match reader.read_indices() {
        Some(iter) => iter.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };

    let mut indices = triangulate_indices(primitive.mode(), &raw_indices)?;
    let v_count = positions.len() as u32;
    let mut valid_indices = Vec::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        if tri[0] < v_count && tri[1] < v_count && tri[2] < v_count {
            valid_indices.extend_from_slice(tri);
        }
    }
    indices = valid_indices;
    if indices.is_empty() {
        return None;
    }

    let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|it| it.collect());
    let uvs: Option<Vec<[f32; 2]>> = reader.read_tex_coords(0).map(|tc| tc.into_f32().collect());
    let colors: Option<Vec<[u8; 4]>> = reader.read_colors(0).map(|c| c.into_rgba_u8().collect());

    let pbr = primitive.material().pbr_metallic_roughness();
    let cf = pbr.base_color_factor();
    let base_factor = [
        float_to_u8(cf[0]),
        float_to_u8(cf[1]),
        float_to_u8(cf[2]),
        float_to_u8(cf[3]),
    ];
    let tex_source_idx = pbr
        .base_color_texture()
        .map(|info| info.texture().source().index());
    let tex_img = tex_source_idx
        .and_then(|idx| images.get(idx))
        .and_then(|opt| opt.as_ref());
    let mapped_tex_idx = tex_source_idx
        .and_then(|idx| image_to_tex_idx.get(idx).copied().flatten());
    let has_uvs = uvs.is_some();
    let uses_gpu_tex = mapped_tex_idx.is_some() && has_uvs;

    let mut pos_min = [f32::INFINITY; 3];
    let mut pos_max = [f32::NEG_INFINITY; 3];
    if !has_uvs {
        for &p in &positions {
            for k in 0..3 {
                let v = if p[k].is_finite() { p[k] } else { 0.0 };
                if v < pos_min[k] {
                    pos_min[k] = v;
                }
                if v > pos_max[k] {
                    pos_max[k] = v;
                }
            }
        }
    }
    let pos_span = [
        (pos_max[0] - pos_min[0]).max(1e-4),
        (pos_max[1] - pos_min[1]).max(1e-4),
        (pos_max[2] - pos_min[2]).max(1e-4),
    ];

    let metallic = pbr.metallic_factor().clamp(0.0, 1.0);
    let roughness = pbr.roughness_factor().clamp(0.05, 1.0);

    let has_normals = normals.is_some();
    let mut vertices = Vec::with_capacity(positions.len());

    let mut sum_rgba = [0u64; 4];
    for (i, &raw_pos) in positions.iter().enumerate() {
        let pos = [
            if raw_pos[0].is_finite() { raw_pos[0] } else { 0.0 },
            if raw_pos[1].is_finite() { raw_pos[1] } else { 0.0 },
            if raw_pos[2].is_finite() { raw_pos[2] } else { 0.0 },
        ];
        let world_pos = transform_point(world, pos);
        let world_norm = match &normals {
            Some(ns) => transform_normal(norm_mat, ns.get(i).copied().unwrap_or([0.0, 1.0, 0.0])),
            None => [0.0, 1.0, 0.0],
        };
        let uv = match uvs.as_ref().and_then(|u| u.get(i).copied()) {
            Some(u) => u,
            None => {
                let nx = (pos[0] - pos_min[0]) / pos_span[0];
                let ny = (pos[1] - pos_min[1]) / pos_span[1];
                let nz = (pos[2] - pos_min[2]) / pos_span[2];
                let an = [world_norm[0].abs(), world_norm[1].abs(), world_norm[2].abs()];
                if an[0] >= an[1] && an[0] >= an[2] {
                    [nz, ny]
                } else if an[1] >= an[0] && an[1] >= an[2] {
                    [nx, nz]
                } else {
                    [nx, ny]
                }
            }
        };

        let mut color = base_factor;
        if let Some(img) = tex_img {
            if !uses_gpu_tex {
                let tc = if has_uvs {
                    img.sample(uv)
                } else {
                    img.average
                };
                color = mul_rgba(color, tc);
            }
        }
        if let Some(vc) = colors.as_ref().and_then(|c| c.get(i).copied()) {
            color = mul_rgba(color, vc);
        }

        for k in 0..4 {
            sum_rgba[k] += color[k] as u64;
        }

        vertices.push(Vertex {
            position: world_pos,
            normal: world_norm,
            color,
            uv,
        });
    }

    let n_verts = vertices.len().max(1) as u64;
    let avg_color = [
        (sum_rgba[0] / n_verts) as u8,
        (sum_rgba[1] / n_verts) as u8,
        (sum_rgba[2] / n_verts) as u8,
        (sum_rgba[3] / n_verts) as u8,
    ];
    let display_base_color = if (!uses_gpu_tex && tex_img.is_some()) || colors.is_some() {
        avg_color
    } else {
        base_factor
    };

    let (final_verts, final_indices) = if has_normals {
        (vertices, indices)
    } else {
        apply_shading(vertices, indices, ShadingMode::Flat)
    };

    let name = if prim_count > 1 {
        format!("{base_name}_{}", prim_idx + 1)
    } else {
        base_name.to_string()
    };

    let mut mesh_part = MeshPart {
        name,
        material: Material {
            base_color: display_base_color,
            metallic,
            roughness,
        },
        vertices: final_verts,
        indices: final_indices,
        bounds: Aabb::EMPTY,
    };
    mesh_part.recompute_bounds();
    Some((mesh_part, if uses_gpu_tex { mapped_tex_idx } else { None }))
}

#[inline]
fn mul_rgba(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    [
        ((a[0] as u16 * b[0] as u16) / 255) as u8,
        ((a[1] as u16 * b[1] as u16) / 255) as u8,
        ((a[2] as u16 * b[2] as u16) / 255) as u8,
        ((a[3] as u16 * b[3] as u16) / 255) as u8,
    ]
}

fn resolve_images(
    document: &gltf::Document,
    buffers: &[Vec<u8>],
    base_dir: Option<&Path>,
) -> Vec<Option<DecodedImage>> {
    document
        .images()
        .enumerate()
        .map(|(idx, img)| {
            let name = img
                .name()
                .map(String::from)
                .unwrap_or_else(|| format!("tex_{}", idx + 1));
            let raw_bytes: Vec<u8> = match img.source() {
                gltf::image::Source::View { view, .. } => {
                    let buf = buffers.get(view.buffer().index())?;
                    let start = view.offset();
                    let end = start.checked_add(view.length())?;
                    buf.get(start..end)?.to_vec()
                }
                gltf::image::Source::Uri { uri, .. } => {
                    if uri.starts_with("data:") {
                        let (_, b64) = uri.split_once(";base64,")?;
                        decode_base64(b64).ok()?
                    } else if let Some(dir) = base_dir {
                        let clean = uri.replace("%20", " ");
                        std::fs::read(dir.join(&clean))
                            .or_else(|_| std::fs::read(dir.join(uri)))
                            .ok()?
                    } else {
                        return None;
                    }
                }
            };

            let decoded = image::load_from_memory(&raw_bytes).ok()?.to_rgba8();
            let (width, height) = decoded.dimensions();
            let rgba = decoded.into_raw();
            let pixel_count = (width as u64 * height as u64).max(1);
            let mut sum = [0u64; 4];
            for px in rgba.chunks_exact(4) {
                sum[0] += px[0] as u64;
                sum[1] += px[1] as u64;
                sum[2] += px[2] as u64;
                sum[3] += px[3] as u64;
            }
            let average = [
                (sum[0] / pixel_count) as u8,
                (sum[1] / pixel_count) as u8,
                (sum[2] / pixel_count) as u8,
                (sum[3] / pixel_count) as u8,
            ];
            Some(DecodedImage {
                name,
                width,
                height,
                rgba,
                average,
            })
        })
        .collect()
}

fn triangulate_indices(mode: gltf::mesh::Mode, raw: &[u32]) -> Option<Vec<u32>> {
    match mode {
        gltf::mesh::Mode::Triangles => {
            let count = (raw.len() / 3) * 3;
            Some(raw[..count].to_vec())
        }
        gltf::mesh::Mode::TriangleStrip => {
            if raw.len() < 3 {
                return None;
            }
            let mut out = Vec::with_capacity((raw.len() - 2) * 3);
            for i in 0..(raw.len() - 2) {
                let (a, b, c) = (raw[i], raw[i + 1], raw[i + 2]);
                if a == b || b == c || a == c {
                    continue;
                }
                if i % 2 == 0 {
                    out.extend_from_slice(&[a, b, c]);
                } else {
                    out.extend_from_slice(&[b, a, c]);
                }
            }
            Some(out)
        }
        gltf::mesh::Mode::TriangleFan => {
            if raw.len() < 3 {
                return None;
            }
            let mut out = Vec::with_capacity((raw.len() - 2) * 3);
            let root = raw[0];
            for i in 1..(raw.len() - 1) {
                out.extend_from_slice(&[root, raw[i], raw[i + 1]]);
            }
            Some(out)
        }
        _ => None,
    }
}

fn resolve_buffers(
    document: &gltf::Document,
    blob: Option<&[u8]>,
    base_dir: Option<&Path>,
) -> Result<Vec<Vec<u8>>, String> {
    let mut buffers = Vec::new();
    for buffer in document.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                let bin = blob.ok_or_else(|| {
                    String::from("El archivo GLB no contiene el bloque binario esperado.")
                })?;
                buffers.push(bin.to_vec());
            }
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with("data:") {
                    let (_, b64) = uri.split_once(";base64,").ok_or_else(|| {
                        String::from("URI data: en .gltf sin codificación base64.")
                    })?;
                    let decoded = decode_base64(b64)?;
                    buffers.push(decoded);
                } else if let Some(dir) = base_dir {
                    let clean = uri.replace("%20", " ");
                    let data = std::fs::read(dir.join(&clean))
                        .or_else(|_| std::fs::read(dir.join(uri)))
                        .map_err(|e| format!("No se pudo leer el buffer externo '{uri}': {e}"))?;
                    buffers.push(data);
                } else {
                    return Err(format!(
                        "Este .gltf referencia '{uri}' externo. Expórtalo como .glb o .gltf embebido."
                    ));
                }
            }
        }
    }
    Ok(buffers)
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity((input.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;

    for b in input.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err(String::from("Base64 inválido en buffer embebido de .gltf")),
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0_f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
        }
    }
    out
}

fn transform_point(m: [[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

fn normal_matrix_3x3(m: [[f32; 4]; 4]) -> [[f32; 3]; 3] {
    let c00 = m[1][1] * m[2][2] - m[2][1] * m[1][2];
    let c01 = m[2][0] * m[1][2] - m[1][0] * m[2][2];
    let c02 = m[1][0] * m[2][1] - m[2][0] * m[1][1];

    let c10 = m[2][1] * m[0][2] - m[0][1] * m[2][2];
    let c11 = m[0][0] * m[2][2] - m[2][0] * m[0][2];
    let c12 = m[2][0] * m[0][1] - m[0][0] * m[2][1];

    let c20 = m[0][1] * m[1][2] - m[1][1] * m[0][2];
    let c21 = m[1][0] * m[0][2] - m[0][0] * m[1][2];
    let c22 = m[0][0] * m[1][1] - m[1][0] * m[0][1];

    let det = m[0][0] * c00 + m[1][0] * c10 + m[2][0] * c20;
    let sign = if det < 0.0 { -1.0 } else { 1.0 };

    [
        [c00 * sign, c01 * sign, c02 * sign],
        [c10 * sign, c11 * sign, c12 * sign],
        [c20 * sign, c21 * sign, c22 * sign],
    ]
}

fn transform_normal(nm: [[f32; 3]; 3], n: [f32; 3]) -> [f32; 3] {
    let x = nm[0][0] * n[0] + nm[1][0] * n[1] + nm[2][0] * n[2];
    let y = nm[0][1] * n[0] + nm[1][1] * n[1] + nm[2][1] * n[2];
    let z = nm[0][2] * n[0] + nm[1][2] * n[1] + nm[2][2] * n[2];
    let len = (x * x + y * y + z * z).sqrt();
    if len > 1e-6 {
        [x / len, y / len, z / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn float_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use forja3d::PrimitiveKind;
    use std::io::Cursor;

    fn encode_base64_test(data: &[u8]) -> String {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[((n >> 18) & 63) as usize] as char);
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
            out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
        }
        out
    }

    fn sample_triangle_bin() -> Vec<u8> {
        let mut bin = Vec::new();
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        for p in positions {
            for f in p {
                bin.extend_from_slice(&f.to_le_bytes());
            }
        }
        let indices: [u16; 3] = [0, 1, 2];
        for i in indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        bin.extend_from_slice(&[0u8; 2]); // 4-byte align
        bin
    }

    fn wrap_glb(mut json: Vec<u8>, mut bin: Vec<u8>) -> Vec<u8> {
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let total_len = 12 + 8 + json.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total_len);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_len as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin);
        glb
    }

    #[test]
    fn imports_embedded_gltf_with_node_translation_and_pbr_material() {
        let bin = sample_triangle_bin();
        let b64 = encode_base64_test(&bin);
        let json = format!(
            r#"{{
  "asset": {{ "version": "2.0" }},
  "scene": 0,
  "scenes": [{{ "name": "AlaEscena", "nodes": [0] }}],
  "nodes": [{{ "name": "Ala", "mesh": 0, "translation": [2.0, 1.0, 0.0] }}],
  "materials": [{{ "pbrMetallicRoughness": {{ "baseColorFactor": [1.0, 0.5, 0.0, 1.0], "metallicFactor": 0.8, "roughnessFactor": 0.25 }} }}],
  "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }}, "indices": 1, "material": 0 }}] }}],
  "buffers": [{{ "byteLength": {}, "uri": "data:application/octet-stream;base64,{}" }}],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": 36, "byteLength": 6 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] }},
    {{ "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR" }}
  ]
}}"#,
            bin.len(),
            b64
        );

        let scene = import_gltf_or_glb(Some("ala.gltf"), json.as_bytes(), None)
            .expect("embedded .gltf should parse");
        assert_eq!(scene.model_name, "AlaEscena");
        assert_eq!(scene.parts.len(), 1);
        assert_eq!(scene.parts[0].kind, PrimitiveKind::CustomMesh);
        assert_eq!(scene.parts[0].name, "Ala");
        assert!((scene.parts[0].metallic - 0.8).abs() < 1e-4);
        assert!((scene.parts[0].roughness - 0.25).abs() < 1e-4);

        let baked = scene.bake();
        assert_eq!(baked.triangle_count(), 1);
        assert!((baked.parts[0].vertices[0].position[0] - 2.0).abs() < 1e-4);
        assert!((baked.parts[0].vertices[0].position[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn imports_binary_glb_container() {
        let bin = sample_triangle_bin();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"CuboTri","mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            bin.len()
        )
        .into_bytes();
        let glb = wrap_glb(json, bin);

        let scene = import_gltf_or_glb(Some("triangulo.glb"), &glb, None)
            .expect("binary .glb should parse");
        assert_eq!(scene.model_name, "triangulo");
        assert_eq!(scene.parts.len(), 1);
        assert_eq!(scene.parts[0].name, "CuboTri");
    }

    #[test]
    fn samples_embedded_png_texture_inside_glb() {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 180, 90, 255]));
        let mut png_bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .expect("png encode");

        let mut bin = Vec::new();
        for p in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for f in p {
                bin.extend_from_slice(&f.to_le_bytes());
            }
        }
        for uv in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for f in uv {
                bin.extend_from_slice(&f.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        bin.extend_from_slice(&[0u8; 2]); // 4-byte align before image view
        let img_offset = bin.len();
        bin.extend_from_slice(&png_bytes);

        let json = format!(
            r#"{{
  "asset": {{ "version": "2.0" }},
  "scene": 0,
  "scenes": [{{ "nodes": [0] }}],
  "nodes": [{{ "name": "Texturado", "mesh": 0 }}],
  "images": [{{ "name": "Verde", "bufferView": 3, "mimeType": "image/png" }}],
  "textures": [{{ "source": 0 }}],
  "materials": [{{ "pbrMetallicRoughness": {{ "baseColorTexture": {{ "index": 0 }} }} }}],
  "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0, "TEXCOORD_0": 1 }}, "indices": 2, "material": 0 }}] }}],
  "buffers": [{{ "byteLength": {} }}],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": 36, "byteLength": 24 }},
    {{ "buffer": 0, "byteOffset": 60, "byteLength": 6 }},
    {{ "buffer": 0, "byteOffset": {}, "byteLength": {} }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [1,1,0] }},
    {{ "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2" }},
    {{ "bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR" }}
  ]
}}"#,
            bin.len(),
            img_offset,
            png_bytes.len()
        )
        .into_bytes();

        let glb = wrap_glb(json, bin);
        let imported = import_gltf_or_glb_full(Some("tex.glb"), &glb, None)
            .expect("textured .glb should import");
        assert_eq!(imported.textures.len(), 1);
        assert_eq!(imported.scene.parts[0].texture_index, Some(0));
        assert!(imported.scene.parts[0].texture_enabled);
        assert_eq!(imported.scene.parts[0].color, [255, 255, 255, 255]);
        assert_eq!(&imported.textures[0].data[0..4], &[20, 180, 90, 255]);
    }

    #[test]
    fn roundtrip_forja3d_scene_to_glb_and_back_with_textures_alpha_and_lights() {
        use forja3d::{MaterialPreset, SunLightSpec};

        let mut scene = SceneDocument::new("NaveForja");
        let mut hull = PartSpec::new("casco", PrimitiveKind::Cube);
        hull.apply_material_preset(
            *MaterialPreset::ALL
                .iter()
                .find(|p| p.id == "gold")
                .unwrap(),
        );
        hull.uv_scale = [2.5, 1.5];
        hull.texture_index = Some(0);
        hull.texture_enabled = true;

        let mut canopy = PartSpec::new("cabina_vidrio", PrimitiveKind::Sphere);
        canopy.apply_material_preset(
            *MaterialPreset::ALL
                .iter()
                .find(|p| p.id == "glass")
                .unwrap(),
        );
        canopy.position = [0.0, 0.8, 0.0];
        canopy.texture_enabled = false;

        scene.parts.push(hull);
        scene.parts.push(canopy);
        scene.sun_lights.push(SunLightSpec::new("SolOrbital"));

        let tex = Texture::new_rgba("panel_albedo", 2, 2, [220u8, 180, 90, 255].repeat(4));
        let glb_bytes = scene.to_glb(&[tex]);

        let parsed = gltf::Gltf::from_slice(&glb_bytes).expect("gltf crate should parse exported .glb");
        assert_eq!(parsed.document.meshes().count(), 2);
        assert_eq!(parsed.document.materials().count(), 2);
        assert_eq!(parsed.document.images().count(), 1);
        assert!(parsed
            .document
            .extensions_used()
            .any(|ext| ext == "KHR_lights_punctual"));
        let mat_modes: Vec<_> = parsed.document.materials().map(|m| m.alpha_mode()).collect();
        assert_eq!(mat_modes[0], gltf::material::AlphaMode::Opaque);
        assert_eq!(mat_modes[1], gltf::material::AlphaMode::Blend);

        let reimported = import_gltf_or_glb_full(Some("naveforja.glb"), &glb_bytes, None)
            .expect("re-importing exported .glb should succeed");
        assert_eq!(reimported.textures.len(), 1);
        assert_eq!(reimported.scene.parts.len(), 2);
        assert!(reimported.scene.parts[1].color[3] < 255);
    }
}
