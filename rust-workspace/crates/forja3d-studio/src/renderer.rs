use eframe::glow;
use glow::HasContext;

use forja3d::{Aabb, ColliderShape, Model};

#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.72,
            pitch: 0.45,
            distance: 5.0,
            target: [0.0, 0.75, 0.0],
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        [
            self.target[0] + self.distance * cp * self.yaw.cos(),
            self.target[1] + self.distance * self.pitch.sin(),
            self.target[2] + self.distance * cp * self.yaw.sin(),
        ]
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
        look_at(self.eye(), self.target, [0.0, 1.0, 0.0])
    }

    pub fn proj_matrix(&self, aspect: f32) -> [f32; 16] {
        let near = (self.distance * 0.004).clamp(0.005, 1.0);
        let far = (self.distance * 30.0).max(500.0);
        perspective(45.0_f32.to_radians(), aspect.max(0.1), near, far)
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let forward = normalize([
            self.target[0] - self.eye()[0],
            self.target[1] - self.eye()[1],
            self.target[2] - self.eye()[2],
        ]);
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        let speed = self.distance * 0.0022;
        for i in 0..3 {
            self.target[i] += (-right[i] * dx + up[i] * dy) * speed;
        }
    }

    pub fn screen_ray(&self, norm_x: f32, norm_y: f32, aspect: f32) -> ([f32; 3], [f32; 3]) {
        // norm_x, norm_y in [-1, 1] NDC
        let eye = self.eye();
        let forward = normalize([
            self.target[0] - eye[0],
            self.target[1] - eye[1],
            self.target[2] - eye[2],
        ]);
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        let half_h = (45.0_f32.to_radians() * 0.5).tan();
        let half_w = half_h * aspect.max(0.1);
        let dir = normalize([
            forward[0] + right[0] * (norm_x * half_w) + up[0] * (norm_y * half_h),
            forward[1] + right[1] * (norm_x * half_w) + up[1] * (norm_y * half_h),
            forward[2] + right[2] * (norm_x * half_w) + up[2] * (norm_y * half_h),
        ]);
        (eye, dir)
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
}

pub struct GlViewportRenderer {
    program: glow::Program,
    line_program: glow::Program,
    grid_vao: glow::VertexArray,
    grid_vbo: glow::Buffer,
    grid_vertex_count: i32,
    debug_vao: glow::VertexArray,
    debug_vbo: glow::Buffer,
    debug_vertex_count: i32,
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

            uniform mat4 u_mvp;
            uniform vec3 u_offset;
            out vec3 v_world_pos;
            out vec3 v_normal;
            out vec4 v_color;

            void main() {{
                vec3 pos = a_pos + u_offset;
                v_world_pos = pos;
                v_normal = a_normal;
                v_color = a_color;
                gl_Position = u_mvp * vec4(pos, 1.0);
            }}"
        );

        let fs_mesh = format!(
            "{shader_header}
            in vec3 v_world_pos;
            in vec3 v_normal;
            in vec4 v_color;

            uniform vec3 u_eye;
            uniform float u_metallic;
            uniform float u_roughness;
            uniform float u_selected;
            out vec4 frag_color;

            void main() {{
                float n_len = length(v_normal);
                vec3 n = n_len > 0.0001 ? (v_normal / n_len) : vec3(0.0, 1.0, 0.0);
                vec3 v = normalize(u_eye - v_world_pos);
                vec3 key_dir = normalize(vec3(0.55, 0.85, 0.45));
                vec3 fill_dir = normalize(vec3(-0.7, 0.35, -0.5));

                float key_diff = max(dot(n, key_dir), 0.0);
                float fill_diff = max(dot(n, fill_dir), 0.0);
                vec3 half_vec = normalize(key_dir + v);
                float shininess = mix(96.0, 8.0, clamp(u_roughness, 0.05, 1.0));
                float spec = pow(max(dot(n, half_vec), 0.0), shininess) * mix(0.25, 1.15, u_metallic);

                float rim = pow(1.0 - max(dot(n, v), 0.0), 2.5);
                vec3 ambient = v_color.rgb * 0.24;
                vec3 diffuse = v_color.rgb * (key_diff * 0.85 + fill_diff * 0.28);
                vec3 spec_col = mix(vec3(1.0), v_color.rgb, u_metallic) * spec;
                vec3 rim_col = mix(vec3(0.45, 0.55, 0.72), vec3(0.98, 0.65, 0.12), u_selected) * rim * mix(0.22, 0.85, u_selected);

                vec3 col = ambient + diffuse + spec_col + rim_col;
                frag_color = vec4(col, v_color.a);
            }}"
        );

        let vs_line = format!(
            "{shader_header}
            layout(location = 0) in vec3 a_pos;
            layout(location = 1) in vec3 a_color;

            uniform mat4 u_mvp;
            uniform vec3 u_offset;
            uniform float u_use_override;
            uniform vec3 u_override_color;
            out vec3 v_col;

            void main() {{
                v_col = mix(a_color, u_override_color, u_use_override);
                gl_Position = u_mvp * vec4(a_pos + u_offset, 1.0);
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

        Ok(Self {
            program,
            line_program,
            grid_vao,
            grid_vbo,
            grid_vertex_count: (grid_data.len() / 6) as i32,
            debug_vao,
            debug_vbo,
            debug_vertex_count: 0,
            parts: Vec::new(),
            uploaded_generation: 0,
            uploaded_debug_key: 0,
        })
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

        let line_data = if show_colliders {
            build_collider_debug_vertices(model)
        } else {
            Vec::new()
        };
        self.debug_vertex_count = (line_data.len() / 6) as i32;
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
            let mut vdata: Vec<f32> = Vec::with_capacity(part.vertices.len() * 10);
            for v in &part.vertices {
                vdata.extend_from_slice(&v.position);
                vdata.extend_from_slice(&v.normal);
                vdata.push(v.color[0] as f32 / 255.0);
                vdata.push(v.color[1] as f32 / 255.0);
                vdata.push(v.color[2] as f32 / 255.0);
                vdata.push(v.color[3] as f32 / 255.0);
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
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 40, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 40, 12);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 40, 24);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck_slice(&part.indices), glow::STATIC_DRAW);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wire_ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck_slice(&wire_indices), glow::STATIC_DRAW);

            gl.bind_vertex_array(None);

            self.parts.push(GpuPart {
                vao,
                vbo,
                ebo,
                wire_ebo,
                index_count: part.indices.len() as i32,
                wire_count: wire_indices.len() as i32,
                metallic: part.material.metallic,
                roughness: part.material.roughness,
            });
        }
    }

    pub unsafe fn paint(
        &self,
        gl: &glow::Context,
        aspect: f32,
        camera: &OrbitCamera,
        selected_mask: &[bool],
        show_grid: bool,
        wireframe: bool,
        model_offset: [f32; 3],
    ) {
        let view = camera.view_matrix();
        let proj = camera.proj_matrix(aspect);
        let mvp = mat4_mul(proj, view);
        let eye = camera.eye();

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
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                gl.uniform_3_f32_slice(Some(&loc), &[0.0, 0.0, 0.0]);
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
        if let Some(loc) = gl.get_uniform_location(self.program, "u_offset") {
            gl.uniform_3_f32_slice(Some(&loc), &model_offset);
        }
        if let Some(loc) = gl.get_uniform_location(self.program, "u_eye") {
            gl.uniform_3_f32_slice(Some(&loc), &eye);
        }

        for (idx, part) in self.parts.iter().enumerate() {
            let is_sel = selected_mask.get(idx).copied().unwrap_or(false);
            if let Some(loc) = gl.get_uniform_location(self.program, "u_metallic") {
                gl.uniform_1_f32(Some(&loc), part.metallic);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_roughness") {
                gl.uniform_1_f32(Some(&loc), part.roughness);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "u_selected") {
                gl.uniform_1_f32(Some(&loc), if is_sel { 1.0 } else { 0.0 });
            }

            gl.bind_vertex_array(Some(part.vao));
            if !wireframe {
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(part.ebo));
                gl.draw_elements(glow::TRIANGLES, part.index_count, glow::UNSIGNED_INT, 0);
            }
            if wireframe || is_sel {
                gl.use_program(Some(self.line_program));
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                    gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                    gl.uniform_3_f32_slice(Some(&loc), &model_offset);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                    gl.uniform_1_f32(Some(&loc), 1.0);
                }
                if let Some(loc) = gl.get_uniform_location(self.line_program, "u_override_color") {
                    let c = if is_sel { [0.96, 0.62, 0.04] } else { [0.82, 0.86, 0.92] };
                    gl.uniform_3_f32_slice(Some(&loc), &c);
                }
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(part.wire_ebo));
                gl.draw_elements(glow::LINES, part.wire_count, glow::UNSIGNED_INT, 0);
                gl.use_program(Some(self.program));
            }
        }

        if self.debug_vertex_count > 0 {
            gl.use_program(Some(self.line_program));
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_offset") {
                gl.uniform_3_f32_slice(Some(&loc), &model_offset);
            }
            if let Some(loc) = gl.get_uniform_location(self.line_program, "u_use_override") {
                gl.uniform_1_f32(Some(&loc), 0.0);
            }
            gl.bind_vertex_array(Some(self.debug_vao));
            gl.draw_arrays(glow::LINES, 0, self.debug_vertex_count);
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

fn build_collider_debug_vertices(model: &Model) -> Vec<f32> {
    let mut out = Vec::new();
    let active_col = [0.20, 0.90, 0.58];
    let fallback_col = [0.95, 0.72, 0.22];

    if let Some(phys) = &model.physics {
        if !phys.colliders.is_empty() {
            for spec in &phys.colliders {
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
            }
            return out;
        }
    }

    let bounds = model.bounds();
    if bounds.is_valid() {
        let (center, shape) = ColliderShape::fit_box(bounds);
        if let ColliderShape::Box { half_extents } = shape {
            push_box_wire(&mut out, center, half_extents, fallback_col);
        }
    }
    out
}

