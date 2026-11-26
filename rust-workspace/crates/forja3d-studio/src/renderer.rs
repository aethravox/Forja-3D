use eframe::glow;
use glow::HasContext;

use forja3d::{Aabb, ColliderShape, Model, Texture, TexturePixelFormat};

#[derive(Debug, Clone, Copy)]
pub struct RenderSunLight {
    pub dir_to_light: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub ambient: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderSunMarker {
    pub position: [f32; 3],
    pub ray_dir: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub enabled: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraViewPreset {
    Front,
    Back,
    Right,
    Left,
    Top,
    Iso,
}

#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    pub ortho: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.72,
            pitch: 0.45,
            distance: 5.0,
            target: [0.0, 0.75, 0.0],
            ortho: false,
        }
    }
}

impl OrbitCamera {
    pub fn set_view_preset(&mut self, preset: CameraViewPreset) {
        match preset {
            CameraViewPreset::Front => {
                self.yaw = core::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
            }
            CameraViewPreset::Back => {
                self.yaw = -core::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
            }
            CameraViewPreset::Right => {
                self.yaw = 0.0;
                self.pitch = 0.0;
            }
            CameraViewPreset::Left => {
                self.yaw = core::f32::consts::PI;
                self.pitch = 0.0;
            }
            CameraViewPreset::Top => {
                self.yaw = core::f32::consts::FRAC_PI_2;
                self.pitch = 1.555;
            }
            CameraViewPreset::Iso => {
                self.yaw = 0.72;
                self.pitch = 0.45;
            }
        }
    }

    pub fn eye(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        [
            self.target[0] + self.distance * cp * self.yaw.cos(),
            self.target[1] + self.distance * self.pitch.sin(),
            self.target[2] + self.distance * cp * self.yaw.sin(),
        ]
    }

    fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
        let eye = self.eye();
        let forward = normalize([
            self.target[0] - eye[0],
            self.target[1] - eye[1],
            self.target[2] - eye[2],
        ]);
        let world_up = if forward[1].abs() > 0.995 {
            [0.0, 0.0, if forward[1] < 0.0 { -1.0 } else { 1.0 }]
        } else {
            [0.0, 1.0, 0.0]
        };
        let right = normalize(cross(forward, world_up));
        let up = cross(right, forward);
        (eye, forward, right, up)
    }

    pub fn frame_bounds(&mut self, bounds: Aabb) {
        if !bounds.is_valid() {
            return;
        }
        let cx = (bounds.min[0] + bounds.max[0]) * 0.5;
        let cy = (bounds.min[1] + bounds.max[1]) * 0.5;
        let cz = (bounds.min[2] + bounds.max[2]) * 0.5;
        let dx = bounds.max[0] - bounds.min[0];
        let dy = bounds.max[1] - bounds.min[1];
        let dz = bounds.max[2] - bounds.min[2];
        let radius = ((dx * dx + dy * dy + dz * dz).sqrt() * 0.5).max(0.05);
        self.target = [cx, cy, cz];
        self.distance = (radius * 2.8).clamp(0.4, 5000.0);
    }

    pub fn view_matrix(&self) -> [f32; 16] {
        let (eye, forward, _, _) = self.basis();
        let world_up = if forward[1].abs() > 0.995 {
            [0.0, 0.0, if forward[1] < 0.0 { -1.0 } else { 1.0 }]
        } else {
            [0.0, 1.0, 0.0]
        };
        look_at(eye, self.target, world_up)
    }

    pub fn proj_matrix(&self, aspect: f32) -> [f32; 16] {
        let aspect = aspect.max(0.1);
        if self.ortho {
            let half_h = (self.distance * (45.0_f32.to_radians() * 0.5).tan()).max(0.05);
            let half_w = half_h * aspect;
            let span = (self.distance * 30.0).max(500.0);
            orthographic(-half_w, half_w, -half_h, half_h, -span, span)
        } else {
            let near = (self.distance * 0.004).clamp(0.005, 1.0);
            let far = (self.distance * 30.0).max(500.0);
            perspective(45.0_f32.to_radians(), aspect, near, far)
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (_, _, right, up) = self.basis();
        let speed = self.distance * 0.0022;
        for i in 0..3 {
            self.target[i] += (-right[i] * dx + up[i] * dy) * speed;
        }
    }

    pub fn screen_ray(&self, norm_x: f32, norm_y: f32, aspect: f32) -> ([f32; 3], [f32; 3]) {
        // norm_x, norm_y in [-1, 1] NDC
        let (eye, forward, right, up) = self.basis();
        let aspect = aspect.max(0.1);
        if self.ortho {
            let half_h = (self.distance * (45.0_f32.to_radians() * 0.5).tan()).max(0.05);
            let half_w = half_h * aspect;
            let origin = [
                eye[0] + right[0] * (norm_x * half_w) + up[0] * (norm_y * half_h),
                eye[1] + right[1] * (norm_x * half_w) + up[1] * (norm_y * half_h),
                eye[2] + right[2] * (norm_x * half_w) + up[2] * (norm_y * half_h),
            ];
            (origin, forward)
        } else {
            let half_h = (45.0_f32.to_radians() * 0.5).tan();
            let half_w = half_h * aspect;
            let dir = normalize([
                forward[0] + right[0] * (norm_x * half_w) + up[0] * (norm_y * half_h),
                forward[1] + right[1] * (norm_x * half_w) + up[1] * (norm_y * half_h),
                forward[2] + right[2] * (norm_x * half_w) + up[2] * (norm_y * half_h),
            ]);
            (eye, dir)
        }
    }

    pub fn project_to_screen(&self, world: [f32; 3], rect_min: [f32; 2], rect_size: [f32; 2]) -> Option<[f32; 2]> {
        let aspect = rect_size[0] / rect_size[1].max(1.0);
        let mvp = mat4_mul(self.proj_matrix(aspect), self.view_matrix());
        let cx = mvp[0] * world[0] + mvp[4] * world[1] + mvp[8] * world[2] + mvp[12];
        let cy = mvp[1] * world[0] + mvp[5] * world[1] + mvp[9] * world[2] + mvp[13];
        let cw = mvp[3] * world[0] + mvp[7] * world[1] + mvp[11] * world[2] + mvp[15];
        if cw <= 0.001 {
            return None;
        }
        let nx = cx / cw;
        let ny = cy / cw;
        Some([
            rect_min[0] + (nx * 0.5 + 0.5) * rect_size[0],
            rect_min[1] + (0.5 - ny * 0.5) * rect_size[1],
        ])
    }
}

pub fn ray_intersects_aabb(origin: [f32; 3], dir: [f32; 3], aabb: Aabb) -> Option<f32> {
    if !aabb.is_valid() {
        return None;
    }
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        if dir[i].abs() < 1e-6 {
            if origin[i] < aabb.min[i] || origin[i] > aabb.max[i] {
                return None;
            }
        } else {
            let inv = 1.0 / dir[i];
            let mut t1 = (aabb.min[i] - origin[i]) * inv;
            let mut t2 = (aabb.max[i] - origin[i]) * inv;
            if t1 > t2 {
                core::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmax >= 0.0 {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

struct GpuPart {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    wire_ebo: glow::Buffer,
    index_count: i32,
    wire_count: i32,
    metallic: f32,
    roughness: f32,
    center: [f32; 3],
    base_has_alpha: bool,
}

pub struct GlViewportRenderer {
    program: glow::Program,
    line_program: glow::Program,
    grid_vao: glow::VertexArray,
    grid_vbo: glow::Buffer,
    grid_vertex_count: i32,
    debug_vao: glow::VertexArray,
    debug_vbo: glow::Buffer,
    debug_ranges: Vec<(Option<usize>, i32, i32)>,
    sun_vao: glow::VertexArray,
    sun_vbo: glow::Buffer,
    gpu_textures: Vec<glow::Texture>,
    gpu_tex_has_alpha: Vec<bool>,
    fallback_texture: glow::Texture,
    uploaded_tex_gen: u64,
    parts: Vec<GpuPart>,
    uploaded_generation: u64,
    uploaded_debug_key: u64,
}

impl GlViewportRenderer {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let shader_header = if cfg!(any(target_arch = "wasm32", target_os = "android")) {
            "#version 300 es\nprecision mediump float;\n"
        } else {
            "#version 330 core\n"
        };

        let vs_mesh = format!(
            "{shader_header}
            layout(location = 0) in vec3 a_pos;
            layout(location = 1) in vec3 a_normal;
            layout(location = 2) in vec4 a_color;
            layout(location = 3) in vec2 a_uv;

            uniform mat4 u_mvp;
            uniform vec3 u_pivot;
            uniform vec4 u_quat;
            uniform vec3 u_offset;
            out vec3 v_world_pos;
            out vec3 v_normal;
            out vec4 v_color;
            out vec2 v_uv;

            vec3 rotate_quat(vec4 q, vec3 v) {{
                vec3 t = 2.0 * cross(q.xyz, v);
                return v + q.w * t + cross(q.xyz, t);
            }}

            void main() {{
                vec3 pos = rotate_quat(u_quat, a_pos - u_pivot) + u_pivot + u_offset;
                v_world_pos = pos;
                v_normal = rotate_quat(u_quat, a_normal);
                v_color = a_color;
                v_uv = a_uv;
                gl_Position = u_mvp * vec4(pos, 1.0);
            }}"
        );

        let fs_mesh = format!(
            "{shader_header}
            in vec3 v_world_pos;
            in vec3 v_normal;
            in vec4 v_color;
            in vec2 v_uv;

            uniform vec3 u_eye;
            uniform float u_metallic;
            uniform float u_roughness;
            uniform float u_selected;
            uniform sampler2D u_tex;
            uniform float u_use_tex;
            uniform float u_alpha_cutoff;
            uniform float u_use_scene_suns;
            uniform int u_sun_count;
            uniform float u_scene_ambient;
            uniform vec3 u_sun_dirs[4];
            uniform vec3 u_sun_colors[4];
            uniform float u_sun_intensities[4];
            out vec4 frag_color;

            const float PI = 3.14159265359;

            float distribution_ggx(vec3 n, vec3 h, float roughness) {{
                float a = roughness * roughness;
                float a2 = a * a;
                float n_dot_h = max(dot(n, h), 0.0);
                float n_dot_h2 = n_dot_h * n_dot_h;
                float denom = (n_dot_h2 * (a2 - 1.0) + 1.0);
                denom = PI * denom * denom;
                return a2 / max(denom, 0.0001);
            }}

            float geometry_schlick_ggx(float n_dot_v, float roughness) {{
                float r = roughness + 1.0;
                float k = (r * r) / 8.0;
                return n_dot_v / max(n_dot_v * (1.0 - k) + k, 0.0001);
            }}

            float geometry_smith(vec3 n, vec3 v, vec3 l, float roughness) {{
                float n_dot_v = max(dot(n, v), 0.0);
                float n_dot_l = max(dot(n, l), 0.0);
                return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
            }}

            vec3 fresnel_schlick(float cos_theta, vec3 f0) {{
                return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
            }}

            vec3 eval_pbr_light(
                vec3 n,
                vec3 v,
                vec3 l,
                vec3 radiance,
                vec3 albedo,
                float roughness,
                float metallic,
                vec3 f0
            ) {{
                vec3 h = normalize(v + l);
                float ndf = distribution_ggx(n, h, roughness);
                float g = geometry_smith(n, v, l, roughness);
                vec3 f = fresnel_schlick(max(dot(h, v), 0.0), f0);

                vec3 numerator = ndf * g * f;
                float denom = 4.0 * max(dot(n, v), 0.0) * max(dot(n, l), 0.0) + 0.0001;
                vec3 specular = numerator / denom;

                vec3 ks = f;
                vec3 kd = (vec3(1.0) - ks) * (1.0 - metallic);
                float n_dot_l = max(dot(n, l), 0.0);
                return (kd * albedo / PI + specular) * radiance * n_dot_l;
            }}

            vec3 aces_tonemap(vec3 x) {{
                return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
            }}

            void main() {{
                float n_len = length(v_normal);
                vec3 n = n_len > 0.0001 ? (v_normal / n_len) : vec3(0.0, 1.0, 0.0);
                vec3 v = normalize(u_eye - v_world_pos);
                vec4 tex_sample = texture(u_tex, v_uv);
                vec4 albedo_srgb = mix(v_color, vec4(v_color.rgb * tex_sample.rgb, v_color.a * tex_sample.a), u_use_tex);
                if (u_alpha_cutoff > 0.001 && albedo_srgb.a < u_alpha_cutoff) {{
                    discard;
                }}
                vec3 albedo = pow(max(albedo_srgb.rgb, vec3(0.0)), vec3(2.2));
                float roughness = clamp(u_roughness, 0.045, 1.0);
                float metallic = clamp(u_metallic, 0.0, 1.0);
                vec3 f0 = mix(vec3(0.04), albedo, metallic);

                vec3 lo = vec3(0.0);
                float amb_scale = 0.24;

                if (u_use_scene_suns > 0.5) {{
                    amb_scale = max(u_scene_ambient, 0.02);
                    for (int i = 0; i < 4; ++i) {{
                        if (i >= u_sun_count) {{
                            break;
                        }}
                        float inten = max(u_sun_intensities[i], 0.0) * 2.2;
                        vec3 l_dir = normalize(u_sun_dirs[i]);
                        vec3 sun_lin = pow(max(u_sun_colors[i], vec3(0.0)), vec3(2.2));
                        lo += eval_pbr_light(n, v, l_dir, sun_lin * inten, albedo, roughness, metallic, f0);
                    }}
                }} else {{
                    vec3 key_dir = normalize(vec3(0.55, 0.85, 0.45));
                    vec3 fill_dir = normalize(vec3(-0.7, 0.35, -0.5));
                    lo += eval_pbr_light(n, v, key_dir, vec3(2.4, 2.36, 2.28), albedo, roughness, metallic, f0);
                    lo += eval_pbr_light(n, v, fill_dir, vec3(0.65, 0.76, 0.92), albedo, roughness, metallic, f0);
                }}

                vec3 refl = reflect(-v, n);
                float hemi = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
                float refl_hemi = clamp(refl.y * 0.5 + 0.5, 0.0, 1.0);
                vec3 env_spec = mix(vec3(0.08, 0.10, 0.14), vec3(0.42, 0.52, 0.68), refl_hemi);
                vec3 f_amb = fresnel_schlick(max(dot(n, v), 0.0), f0);
                vec3 kd_amb = (vec3(1.0) - f_amb) * (1.0 - metallic);
                vec3 ambient = (kd_amb * albedo * (0.65 + 0.45 * hemi) + f_amb * env_spec * (1.0 - roughness * 0.65)) * amb_scale;

                vec3 hdr = ambient + lo;
                vec3 srgb = pow(aces_tonemap(hdr), vec3(1.0 / 2.2));

                float rim = pow(1.0 - max(dot(n, v), 0.0), 2.5);
                vec3 rim_col = mix(vec3(0.45, 0.55, 0.72), vec3(0.98, 0.65, 0.12), u_selected) * rim * mix(0.18, 0.85, u_selected);

                frag_color = vec4(srgb + rim_col, albedo_srgb.a);
            }}"
        );

        let vs_line = format!(
            "{shader_header}
            layout(location = 0) in vec3 a_pos;
            layout(location = 1) in vec3 a_color;

            uniform mat4 u_mvp;
            uniform vec3 u_pivot;
            uniform vec4 u_quat;
            uniform vec3 u_offset;
            uniform float u_use_override;
            uniform vec3 u_override_color;
            out vec3 v_col;

            vec3 rotate_quat(vec4 q, vec3 v) {{
                vec3 t = 2.0 * cross(q.xyz, v);
                return v + q.w * t + cross(q.xyz, t);
            }}

            void main() {{
                v_col = mix(a_color, u_override_color, u_use_override);
                vec3 pos = rotate_quat(u_quat, a_pos - u_pivot) + u_pivot + u_offset;
                gl_Position = u_mvp * vec4(pos, 1.0);
            }}"
        );

        let fs_line = format!(
            "{shader_header}
            in vec3 v_col;
            out vec4 frag_color;
            void main() {{
                frag_color = vec4(v_col, 1.0);
            }}"
        );

        let program = compile_program(gl, &vs_mesh, &fs_mesh)?;
        let line_program = compile_program(gl, &vs_line, &fs_line)?;

        let grid_data = build_grid_vertices(8, 0.5);
        let grid_vao = gl.create_vertex_array()?;
        let grid_vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(grid_vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(grid_vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck_slice(&grid_data), glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
        gl.bind_vertex_array(None);

        let debug_vao = gl.create_vertex_array()?;
        let debug_vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(debug_vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(debug_vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
        gl.bind_vertex_array(None);

        let sun_vao = gl.create_vertex_array()?;
        let sun_vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(sun_vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(sun_vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
        gl.bind_vertex_array(None);

        let fallback_rgba = build_default_checker_rgba();
        let fallback_texture = create_rgba_texture(gl, 16, 16, &fallback_rgba)?;

        Ok(Self {
            program,
            line_program,
            grid_vao,
            grid_vbo,
            grid_vertex_count: (grid_data.len() / 6) as i32,
            debug_vao,
            debug_vbo,
            debug_ranges: Vec::new(),
            sun_vao,
            sun_vbo,
            gpu_textures: Vec::new(),
            gpu_tex_has_alpha: Vec::new(),
            fallback_texture,
            uploaded_tex_gen: 0,
            parts: Vec::new(),
            uploaded_generation: 0,
            uploaded_debug_key: 0,
        })
    }

    pub unsafe fn sync_textures(&mut self, gl: &glow::Context, textures: &[Texture], tex_gen: u64) {
        if self.uploaded_tex_gen == tex_gen {
            return;
        }
        self.uploaded_tex_gen = tex_gen;
        for tex in self.gpu_textures.drain(..) {
            gl.delete_texture(tex);
        }
        self.gpu_tex_has_alpha.clear();
        for tex in textures {
            let rgba = decode_texture_rgba(tex);
            let has_alpha = rgba.chunks_exact(4).any(|px| px[3] < 255);
            if let Ok(gl_tex) = create_rgba_texture(gl, tex.width as i32, tex.height as i32, &rgba) {
                self.gpu_textures.push(gl_tex);
                self.gpu_tex_has_alpha.push(has_alpha);
            }
        }
    }

    pub unsafe fn sync_collider_debug(
        &mut self,
        gl: &glow::Context,
        model: &Model,
        generation: u64,
        show_colliders: bool,
    ) {
        let key = if show_colliders { generation } else { u64::MAX };
        if self.uploaded_debug_key == key {
            return;
        }
        self.uploaded_debug_key = key;
        self.debug_ranges.clear();

        let (line_data, ranges) = if show_colliders {
            build_collider_debug_vertices(model)
        } else {
            (Vec::new(), Vec::new())
        };
        self.debug_ranges = ranges;
        if !line_data.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.debug_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_slice(&line_data),
                glow::DYNAMIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
    }

    pub unsafe fn sync_model(&mut self, gl: &glow::Context, model: &Model, generation: u64) {
        if self.uploaded_generation == generation {
            return;
        }
        self.uploaded_generation = generation;

        for p in self.parts.drain(..) {
            gl.delete_vertex_array(p.vao);
            gl.delete_buffer(p.vbo);
            gl.delete_buffer(p.ebo);
            gl.delete_buffer(p.wire_ebo);
        }

        for part in &model.parts {
            let mut vdata: Vec<f32> = Vec::with_capacity(part.vertices.len() * 12);
            for v in &part.vertices {
                vdata.extend_from_slice(&v.position);
                vdata.extend_from_slice(&v.normal);
                vdata.push(v.color[0] as f32 / 255.0);
                vdata.push(v.color[1] as f32 / 255.0);
                vdata.push(v.color[2] as f32 / 255.0);
                vdata.push(v.color[3] as f32 / 255.0);
                vdata.extend_from_slice(&v.uv);
            }

            let mut wire_indices: Vec<u32> = Vec::with_capacity(part.indices.len() * 2);
            for tri in part.indices.chunks(3) {
                if tri.len() == 3 {
                    wire_indices.extend_from_slice(&[tri[0], tri[1], tri[1], tri[2], tri[2], tri[0]]);
                }
            }

            let Ok(vao) = gl.create_vertex_array() else { continue };
            let Ok(vbo) = gl.create_buffer() else { continue };
            let Ok(ebo) = gl.create_buffer() else { continue };
            let Ok(wire_ebo) = gl.create_buffer() else { continue };

            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck_slice(&vdata), glow::STATIC_DRAW);

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 48, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 48, 12);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 48, 24);
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 2, glow::FLOAT, false, 48, 40);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck_slice(&part.indices), glow::STATIC_DRAW);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wire_ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck_slice(&wire_indices), glow::STATIC_DRAW);

            gl.bind_vertex_array(None);

            let center = if part.bounds.is_valid() {
                [
                    (part.bounds.min[0] + part.bounds.max[0]) * 0.5,
                    (part.bounds.min[1] + part.bounds.max[1]) * 0.5,
                    (part.bounds.min[2] + part.bounds.max[2]) * 0.5,
                ]
            } else {
                [0.0; 3]
            };
            let base_has_alpha = part.material.has_alpha() || part.vertices.iter().any(|v| v.color[3] < 255);

            self.parts.push(GpuPart {
                vao,
                vbo,
                ebo,
                wire_ebo,
                index_count: part.indices.len() as i32,
                wire_count: wire_indices.len() as i32,
                metallic: part.material.metallic,
                roughness: part.material.roughness,
                center,
                base_has_alpha,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub unsafe fn paint(
        &self,
        gl: &glow::Context,
        aspect: f32,
        camera: &OrbitCamera,
        selected_mask: &[bool],
        show_grid: bool,
        wireframe: bool,
        textures_enabled: bool,
        part_use_tex: &[bool],
        part_tex_indices: &[Option<usize>],
        part_alpha_cutoffs: &[f32],
        has_scene_suns: bool,
        sun_lights: &[RenderSunLight],
        sun_markers: &[RenderSunMarker],
        part_transforms: &[([f32; 3], [f32; 4], [f32; 3])],
    ) {
        let view = camera.view_matrix();
        let proj = camera.proj_matrix(aspect);
        let mvp = mat4_mul(proj, view);
        let eye = camera.eye();
        let id_quat = [0.0_f32, 0.0, 0.0, 1.0];
        let zero_vec = [0.0_f32; 3];

        gl.enable(glow::DEPTH_TEST);
        gl.depth_mask(true);
        gl.depth_func(glow::LESS);
        gl.clear(glow::DEPTH_BUFFER_BIT);
        gl.disable(glow::CULL_FACE);

        if show_grid {
            gl.use_program(Some(self.line_program));
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_pivot") {
                gl.uniform_3_f32_slice(Some(&loc), &zero_vec);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_quat") {
                gl.uniform_4_f32_slice(Some(&loc), &id_quat);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                gl.uniform_3_f32_slice(Some(&loc), &zero_vec);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                gl.uniform_1_f32(Some(&loc), 0.0);
            }
            gl.bind_vertex_array(Some(self.grid_vao));
            gl.draw_arrays(glow::LINES, 0, self.grid_vertex_count);
            gl.bind_vertex_array(None);
        }

        gl.use_program(Some(self.program));
        if let Some(loc) = gl.get_uniform_location(self.program, "u_mvp") {
            gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
        }
        if let Some(loc) = gl.get_uniform_location(self.program, "u_eye") {
            gl.uniform_3_f32_slice(Some(&loc), &eye);
        }
        if let Some(loc) = gl.get_uniform_location(self.program, "u_tex") {
            gl.uniform_1_i32(Some(&loc), 0);
        }
        if let Some(loc) = gl.get_uniform_location(self.program, "u_use_scene_suns") {
            gl.uniform_1_f32(Some(&loc), if has_scene_suns { 1.0 } else { 0.0 });
        }
        let sun_count = sun_lights.len().min(4);
        if let Some(loc) = gl.get_uniform_location(self.program, "u_sun_count") {
            gl.uniform_1_i32(Some(&loc), sun_count as i32);
        }
        let scene_ambient = if sun_lights.is_empty() {
            0.08
        } else {
            sun_lights
                .iter()
                .map(|s| s.ambient)
                .fold(0.0_f32, f32::max)
        };
        if let Some(loc) = gl.get_uniform_location(self.program, "u_scene_ambient") {
            gl.uniform_1_f32(Some(&loc), scene_ambient);
        }
        for i in 0..4 {
            let (dir, col, inten) = if let Some(s) = sun_lights.get(i) {
                (s.dir_to_light, s.color, s.intensity)
            } else {
                ([0.0, 1.0, 0.0], [1.0, 1.0, 1.0], 0.0)
            };
            if let Some(loc) = gl.get_uniform_location(self.program, &format!("u_sun_dirs[{i}]")) {
                gl.uniform_3_f32_slice(Some(&loc), &dir);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, &format!("u_sun_colors[{i}]")) {
                gl.uniform_3_f32_slice(Some(&loc), &col);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, &format!("u_sun_intensities[{i}]")) {
                gl.uniform_1_f32(Some(&loc), inten);
            }
        }

        gl.active_texture(glow::TEXTURE0);

        let resolve_part_draw = |idx: usize, part: &GpuPart| {
            let part_tex_on = textures_enabled && part_use_tex.get(idx).copied().unwrap_or(true);
            let resolved_tex_slot = if part_tex_on {
                part_tex_indices
                    .get(idx)
                    .copied()
                    .flatten()
                    .filter(|&ti| ti < self.gpu_textures.len())
                    .or_else(|| (!self.gpu_textures.is_empty()).then_some(0))
            } else {
                None
            };
            let bound_tex = resolved_tex_slot
                .and_then(|ti| self.gpu_textures.get(ti).copied())
                .unwrap_or(self.fallback_texture);
            let tex_has_alpha = resolved_tex_slot
                .and_then(|ti| self.gpu_tex_has_alpha.get(ti).copied())
                .unwrap_or(false);
            let cutoff = part_alpha_cutoffs.get(idx).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let is_transparent = cutoff <= 0.001 && (part.base_has_alpha || tex_has_alpha);
            (part_tex_on, bound_tex, cutoff, is_transparent)
        };

        let mut opaque_indices = Vec::new();
        let mut transparent_indices: Vec<(usize, f32)> = Vec::new();
        for (idx, part) in self.parts.iter().enumerate() {
            let (_, _, _, is_transparent) = resolve_part_draw(idx, part);
            if is_transparent {
                let (_, _, offset) = part_transforms
                    .get(idx)
                    .copied()
                    .unwrap_or((zero_vec, id_quat, zero_vec));
                let cx = part.center[0] + offset[0] - eye[0];
                let cy = part.center[1] + offset[1] - eye[1];
                let cz = part.center[2] + offset[2] - eye[2];
                let dist_sq = cx * cx + cy * cy + cz * cz;
                transparent_indices.push((idx, dist_sq));
            } else {
                opaque_indices.push(idx);
            }
        }
        // Sort transparent parts back-to-front by distance from camera eye
        transparent_indices.sort_by(|a, b| b.1.total_cmp(&a.1));

        let draw_mesh_part = |idx: usize| {
            let part = &self.parts[idx];
            let is_sel = selected_mask.get(idx).copied().unwrap_or(false);
            let (pivot, quat, offset) = part_transforms
                .get(idx)
                .copied()
                .unwrap_or((zero_vec, id_quat, zero_vec));
            let (part_tex_on, bound_tex, cutoff, _) = resolve_part_draw(idx, part);

            gl.bind_texture(glow::TEXTURE_2D, Some(bound_tex));
            if let Some(loc) = gl.get_uniform_location(self.program, "u_use_tex") {
                gl.uniform_1_f32(Some(&loc), if part_tex_on { 1.0 } else { 0.0 });
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_alpha_cutoff") {
                gl.uniform_1_f32(Some(&loc), cutoff);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_pivot") {
                gl.uniform_3_f32_slice(Some(&loc), &pivot);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_quat") {
                gl.uniform_4_f32_slice(Some(&loc), &quat);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_offset") {
                gl.uniform_3_f32_slice(Some(&loc), &offset);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_metallic") {
                gl.uniform_1_f32(Some(&loc), part.metallic);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_roughness") {
                gl.uniform_1_f32(Some(&loc), part.roughness);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_selected") {
                gl.uniform_1_f32(Some(&loc), if is_sel { 1.0 } else { 0.0 });
            }

            if !wireframe {
                gl.bind_vertex_array(Some(part.vao));
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(part.ebo));
                gl.draw_elements(glow::TRIANGLES, part.index_count, glow::UNSIGNED_INT, 0);
            }
        };

        if !wireframe {
            gl.disable(glow::BLEND);
            gl.depth_mask(true);
            for idx in opaque_indices {
                draw_mesh_part(idx);
            }

            if !transparent_indices.is_empty() {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                for (idx, _) in transparent_indices {
                    draw_mesh_part(idx);
                }
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
        }

        // Draw wireframe & selection outlines cleanly with depth writes enabled
        gl.disable(glow::BLEND);
        gl.depth_mask(true);
        for (idx, part) in self.parts.iter().enumerate() {
            let is_sel = selected_mask.get(idx).copied().unwrap_or(false);
            if wireframe || is_sel {
                let (pivot, quat, offset) = part_transforms
                    .get(idx)
                    .copied()
                    .unwrap_or((zero_vec, id_quat, zero_vec));
                gl.use_program(Some(self.line_program));
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                    gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_pivot") {
                    gl.uniform_3_f32_slice(Some(&loc), &pivot);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_quat") {
                    gl.uniform_4_f32_slice(Some(&loc), &quat);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                    gl.uniform_3_f32_slice(Some(&loc), &offset);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                    gl.uniform_1_f32(Some(&loc), 1.0);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_override_color") {
                    let c = if is_sel { [0.96, 0.62, 0.04] } else { [0.82, 0.86, 0.92] };
                    gl.uniform_3_f32_slice(Some(&loc), &c);
                }
                gl.bind_vertex_array(Some(part.vao));
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(part.wire_ebo));
                gl.draw_elements(glow::LINES, part.wire_count, glow::UNSIGNED_INT, 0);
            }
        }
        gl.bind_texture(glow::TEXTURE_2D, None);

        if !self.debug_ranges.is_empty() {
            gl.use_program(Some(self.line_program));
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                gl.uniform_1_f32(Some(&loc), 0.0);
            }
            gl.bind_vertex_array(Some(self.debug_vao));
            for &(part_idx_opt, start, count) in &self.debug_ranges {
                let (pivot, quat, offset) = part_idx_opt
                    .and_then(|idx| part_transforms.get(idx).copied())
                    .unwrap_or((zero_vec, id_quat, zero_vec));
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_pivot") {
                    gl.uniform_3_f32_slice(Some(&loc), &pivot);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_quat") {
                    gl.uniform_4_f32_slice(Some(&loc), &quat);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                    gl.uniform_3_f32_slice(Some(&loc), &offset);
                }
                gl.draw_arrays(glow::LINES, start, count);
            }
        }

        if !sun_markers.is_empty() {
            let sun_verts = build_sun_marker_vertices(sun_markers);
            if !sun_verts.is_empty() {
                gl.use_program(Some(self.line_program));
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                    gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_pivot") {
                    gl.uniform_3_f32_slice(Some(&loc), &zero_vec);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_quat") {
                    gl.uniform_4_f32_slice(Some(&loc), &id_quat);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                    gl.uniform_3_f32_slice(Some(&loc), &zero_vec);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                    gl.uniform_1_f32(Some(&loc), 0.0);
                }
                gl.bind_vertex_array(Some(self.sun_vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.sun_vbo));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck_slice(&sun_verts),
                    glow::DYNAMIC_DRAW,
                );
                gl.draw_arrays(glow::LINES, 0, (sun_verts.len() / 6) as i32);
            }
        }

        gl.bind_vertex_array(None);
        gl.use_program(None);
        gl.disable(glow::DEPTH_TEST);
    }

    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        gl.delete_program(self.program);
        gl.delete_program(self.line_program);
        gl.delete_vertex_array(self.grid_vao);
        gl.delete_buffer(self.grid_vbo);
        gl.delete_vertex_array(self.debug_vao);
        gl.delete_buffer(self.debug_vbo);
        gl.delete_vertex_array(self.sun_vao);
        gl.delete_buffer(self.sun_vbo);
        gl.delete_texture(self.fallback_texture);
        for tex in self.gpu_textures.drain(..) {
            gl.delete_texture(tex);
        }
        for p in self.parts.drain(..) {
            gl.delete_vertex_array(p.vao);
            gl.delete_buffer(p.vbo);
            gl.delete_buffer(p.ebo);
            gl.delete_buffer(p.wire_ebo);
        }
    }
}

unsafe fn compile_program(gl: &glow::Context, vs_src: &str, fs_src: &str) -> Result<glow::Program, String> {
    let program = gl.create_program()?;
    let vs = gl.create_shader(glow::VERTEX_SHADER)?;
    gl.shader_source(vs, vs_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        return Err(format!("VS compile error: {}", gl.get_shader_info_log(vs)));
    }

    let fs = gl.create_shader(glow::FRAGMENT_SHADER)?;
    gl.shader_source(fs, fs_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        return Err(format!("FS compile error: {}", gl.get_shader_info_log(fs)));
    }

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(format!("Link error: {}", gl.get_program_info_log(program)));
    }
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    Ok(program)
}

fn build_grid_vertices(half_lines: i32, step: f32) -> Vec<f32> {
    let mut data = Vec::new();
    let max = half_lines as f32 * step;
    for i in -half_lines..=half_lines {
        let p = i as f32 * step;
        let col = if i == 0 { [0.28, 0.32, 0.40] } else { [0.16, 0.19, 0.24] };
        // Line parallel to Z
        data.extend_from_slice(&[p, 0.0, -max, col[0], col[1], col[2]]);
        data.extend_from_slice(&[p, 0.0, max, col[0], col[1], col[2]]);
        // Line parallel to X
        data.extend_from_slice(&[-max, 0.0, p, col[0], col[1], col[2]]);
        data.extend_from_slice(&[max, 0.0, p, col[0], col[1], col[2]]);
    }
    // Gizmo axes at origin
    data.extend_from_slice(&[0.0, 0.01, 0.0, 0.92, 0.28, 0.28, 1.2, 0.01, 0.0, 0.92, 0.28, 0.28]);
    data.extend_from_slice(&[0.0, 0.01, 0.0, 0.28, 0.85, 0.48, 0.0, 1.2, 0.0, 0.28, 0.85, 0.48]);
    data.extend_from_slice(&[0.0, 0.01, 0.0, 0.28, 0.56, 0.96, 0.0, 0.01, 1.2, 0.28, 0.56, 0.96]);
    data
}

fn bytemuck_slice<T>(slice: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(slice.as_ptr().cast::<u8>(), std::mem::size_of_val(slice))
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        s[0], u[0], -f[0], 0.0,
        s[1], u[1], -f[1], 0.0,
        s[2], u[2], -f[2], 0.0,
        -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0,
    ]
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) * nf, -1.0,
        0.0, 0.0, 2.0 * far * near * nf, 0.0,
    ]
}

fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    let rl = 1.0 / (right - left);
    let tb = 1.0 / (top - bottom);
    let fn_inv = 1.0 / (far - near);
    [
        2.0 * rl, 0.0, 0.0, 0.0,
        0.0, 2.0 * tb, 0.0, 0.0,
        0.0, 0.0, -2.0 * fn_inv, 0.0,
        -(right + left) * rl, -(top + bottom) * tb, -(far + near) * fn_inv, 1.0,
    ]
}

fn mat4_mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = a[row] * b[col * 4]
                + a[4 + row] * b[col * 4 + 1]
                + a[8 + row] * b[col * 4 + 2]
                + a[12 + row] * b[col * 4 + 3];
        }
    }
    out
}

fn push_line(out: &mut Vec<f32>, a: [f32; 3], b: [f32; 3], col: [f32; 3]) {
    out.extend_from_slice(&[a[0], a[1], a[2], col[0], col[1], col[2]]);
    out.extend_from_slice(&[b[0], b[1], b[2], col[0], col[1], col[2]]);
}

fn push_box_wire(out: &mut Vec<f32>, c: [f32; 3], h: [f32; 3], col: [f32; 3]) {
    let corners = [
        [c[0] - h[0], c[1] - h[1], c[2] - h[2]],
        [c[0] + h[0], c[1] - h[1], c[2] - h[2]],
        [c[0] + h[0], c[1] + h[1], c[2] - h[2]],
        [c[0] - h[0], c[1] + h[1], c[2] - h[2]],
        [c[0] - h[0], c[1] - h[1], c[2] + h[2]],
        [c[0] + h[0], c[1] - h[1], c[2] + h[2]],
        [c[0] + h[0], c[1] + h[1], c[2] + h[2]],
        [c[0] - h[0], c[1] + h[1], c[2] + h[2]],
    ];
    let edges = [
        (0, 1), (1, 2), (2, 3), (3, 0),
        (4, 5), (5, 6), (6, 7), (7, 4),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ];
    for (i, j) in edges {
        push_line(out, corners[i], corners[j], col);
    }
}

fn push_circle_wire(out: &mut Vec<f32>, c: [f32; 3], r: f32, plane: u8, col: [f32; 3]) {
    let segs = 20usize;
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * core::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * core::f32::consts::TAU;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let (p0, p1) = match plane {
            0 => ([c[0] + c0 * r, c[1], c[2] + s0 * r], [c[0] + c1 * r, c[1], c[2] + s1 * r]), // XZ
            1 => ([c[0] + c0 * r, c[1] + s0 * r, c[2]], [c[0] + c1 * r, c[1] + s1 * r, c[2]]), // XY
            _ => ([c[0], c[1] + s0 * r, c[2] + c0 * r], [c[0], c[1] + s1 * r, c[2] + c1 * r]), // YZ
        };
        push_line(out, p0, p1, col);
    }
}

fn build_collider_debug_vertices(model: &Model) -> (Vec<f32>, Vec<(Option<usize>, i32, i32)>) {
    let mut out = Vec::new();
    let mut ranges = Vec::new();
    let active_col = [0.20, 0.90, 0.58];
    let fallback_col = [0.95, 0.72, 0.22];

    if let Some(phys) = &model.physics {
        if !phys.colliders.is_empty() {
            for spec in &phys.colliders {
                let start = (out.len() / 6) as i32;
                let part_idx = model
                    .parts
                    .iter()
                    .position(|p| forja3d::fnv1a_32(p.name.as_bytes()) == spec.part_hash);
                match spec.shape {
                    ColliderShape::None => {}
                    ColliderShape::Box { half_extents } => {
                        push_box_wire(&mut out, spec.offset, half_extents, active_col);
                    }
                    ColliderShape::Sphere { radius } => {
                        push_circle_wire(&mut out, spec.offset, radius, 0, active_col);
                        push_circle_wire(&mut out, spec.offset, radius, 1, active_col);
                        push_circle_wire(&mut out, spec.offset, radius, 2, active_col);
                    }
                    ColliderShape::Capsule { radius, half_height } => {
                        let top = [spec.offset[0], spec.offset[1] + half_height, spec.offset[2]];
                        let bot = [spec.offset[0], spec.offset[1] - half_height, spec.offset[2]];
                        push_circle_wire(&mut out, top, radius, 0, active_col);
                        push_circle_wire(&mut out, bot, radius, 0, active_col);
                        push_circle_wire(&mut out, top, radius, 1, active_col);
                        push_circle_wire(&mut out, bot, radius, 2, active_col);
                        for [dx, dz] in [[radius, 0.0], [-radius, 0.0], [0.0, radius], [0.0, -radius]] {
                            push_line(
                                &mut out,
                                [bot[0] + dx, bot[1], bot[2] + dz],
                                [top[0] + dx, top[1], top[2] + dz],
                                active_col,
                            );
                        }
                    }
                }
                let end = (out.len() / 6) as i32;
                if end > start {
                    ranges.push((part_idx, start, end - start));
                }
            }
            return (out, ranges);
        }
    }

    let bounds = model.bounds();
    if bounds.is_valid() {
        let start = (out.len() / 6) as i32;
        let (center, shape) = ColliderShape::fit_box(bounds);
        if let ColliderShape::Box { half_extents } = shape {
            push_box_wire(&mut out, center, half_extents, fallback_col);
        }
        let end = (out.len() / 6) as i32;
        if end > start {
            ranges.push((None, start, end - start));
        }
    }
    (out, ranges)
}

unsafe fn create_rgba_texture(
    gl: &glow::Context,
    width: i32,
    height: i32,
    rgba: &[u8],
) -> Result<glow::Texture, String> {
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA as i32,
        width.max(1),
        height.max(1),
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(rgba)),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(tex)
}

fn build_default_checker_rgba() -> Vec<u8> {
    let size = 16usize;
    let mut out = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let checker = ((x / 4) + (y / 4)) % 2 == 0;
            let border = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let v = if border {
                235
            } else if checker {
                215
            } else {
                160
            };
            out.extend_from_slice(&[v, v, v, 255]);
        }
    }
    out
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

fn build_sun_marker_vertices(markers: &[RenderSunMarker]) -> Vec<f32> {
    let mut out = Vec::new();
    for m in markers {
        let col = if !m.enabled || m.intensity <= 0.001 {
            [0.45, 0.48, 0.54]
        } else if m.selected {
            [0.99, 0.78, 0.18]
        } else {
            m.color
        };
        let c = m.position;
        let r = 0.24_f32;
        push_circle_wire(&mut out, c, r, 0, col);
        push_circle_wire(&mut out, c, r, 1, col);
        push_circle_wire(&mut out, c, r, 2, col);
        if m.selected {
            push_circle_wire(&mut out, c, r * 1.45, 0, [0.98, 0.65, 0.12]);
            push_circle_wire(&mut out, c, r * 1.45, 1, [0.98, 0.65, 0.12]);
        }

        // Radial sunbeams
        let ray_dirs = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.707, 0.707, 0.0],
            [-0.707, 0.707, 0.0],
            [0.707, -0.707, 0.0],
            [-0.707, -0.707, 0.0],
        ];
        let inner = r * 1.15;
        let outer = r * (1.55 + (m.intensity * 0.15).clamp(0.0, 0.45));
        for d in ray_dirs {
            push_line(
                &mut out,
                [c[0] + d[0] * inner, c[1] + d[1] * inner, c[2] + d[2] * inner],
                [c[0] + d[0] * outer, c[1] + d[1] * outer, c[2] + d[2] * outer],
                col,
            );
        }

        // Light direction ray from sun toward scene
        let dir = normalize(m.ray_dir);
        let ray_len = 1.25;
        let tip = [
            c[0] + dir[0] * ray_len,
            c[1] + dir[1] * ray_len,
            c[2] + dir[2] * ray_len,
        ];
        let ray_col = if m.enabled && m.intensity > 0.001 {
            [1.0, 0.90, 0.45]
        } else {
            [0.42, 0.44, 0.50]
        };
        push_line(&mut out, c, tip, ray_col);
    }
    out
}

