mod editor_2d;
mod gltf_import;
mod icons;
mod renderer;

use std::sync::{Arc, Mutex};

use eframe::egui;
use forja3d::{
    default_extrude_profile, default_lathe_profile, deform_vertices, fnv1a_32,
    subdivide_triangles, terrain_height_at, AnimationClip, AnimationProperty, AnimationSet,
    AnimationTrack, ColliderShape, F2D_MAGIC, F3A_MAGIC, F3P_MAGIC, F3S_MAGIC, F3T_MAGIC,
    FLAG_COMPACT, FLAG_PHYSICS, ForjaPack, ForjaProject, InterpolationMode, Keyframe, Mat4,
    Material, MaterialPreset, MeshDeformKind, MeshPart, Model, PartSpec, PrimitiveKind,
    ProjectCameraState, ProjectViewportState, RigidBodyKind, RustExportTarget, SceneDocument,
    ShadingMode, SunLightSpec, TerrainPreset, TerrainStamp, Texture, UiDocument2D, Vec3, Vertex,
};
use forja3d_physics::{BodyHandle, PhysicsWorld, RigidBody};

use editor_2d::{
    default_ui_document_2d, render_inspector_2d, render_scene_panel_2d, render_viewport_2d,
    ActiveWidgetDrag2D,
};
use icons::{
    icon_btn, icon_btn_enabled, icon_text_btn, icon_text_btn_enabled, SvgIcon,
};
use renderer::{
    ray_intersects_aabb, CameraViewPreset, GlViewportRenderer, OrbitCamera, RenderSunLight,
    RenderSunMarker,
};

const MAX_UNDO_HISTORY: usize = 32;

const QUICK_PALETTE: [(&str, [u8; 4]); 8] = [
    ("Ambar", [224, 159, 62, 255]),
    ("Rojo", [220, 68, 68, 255]),
    ("Bosque", [84, 148, 76, 255]),
    ("Cielo", [56, 189, 248, 255]),
    ("Tierra", [142, 98, 64, 255]),
    ("Arena", [214, 192, 144, 255]),
    ("Piedra", [112, 118, 128, 255]),
    ("Blanco", [240, 244, 248, 255]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Mode3D,
    Mode2D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportTab {
    ProjectForja,
    GlbBinary,
    F3dBinary,
    AssetFormats,
    RustSource,
    WavefrontObj,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobileTab {
    Viewport,
    Scene,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GizmoMode {
    Translate,
    Rotate,
    Scale,
    Vertices,
}

impl GizmoMode {
    const fn label(self) -> &'static str {
        match self {
            Self::Translate => "Mover",
            Self::Rotate => "Rotar",
            Self::Scale => "Escalar",
            Self::Vertices => "Vértices",
        }
    }

    const fn icon(self) -> SvgIcon {
        match self {
            Self::Translate => SvgIcon::Move,
            Self::Rotate => SvgIcon::Rotate,
            Self::Scale => SvgIcon::Scale,
            Self::Vertices => SvgIcon::Vertices,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GizmoAxis {
    X = 0,
    Y = 1,
    Z = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    Part(usize),
    Group(String),
    Sun(usize),
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    scene: SceneDocument,
    selection: Option<Selection>,
}

struct ActiveGizmoDrag {
    axis: GizmoAxis,
    mode: GizmoMode,
    start_parts: Vec<PartSpec>,
    start_suns: Vec<SunLightSpec>,
    accum_pixels: f32,
}

struct ActiveVertexDrag {
    part_idx: usize,
    local_anchor: [f32; 3],
    world_anchor: [f32; 3],
    start_vertices: Vec<Vertex>,
    start_pointer_px: egui::Pos2,
}

pub struct StudioApp {
    view_mode: ViewMode,
    show_file_modal: bool,
    scene: SceneDocument,
    baked_model: Model,
    part_map: Vec<usize>,
    generation: u64,
    selection: Option<Selection>,
    camera: OrbitCamera,
    wireframe: bool,
    show_grid: bool,
    show_colliders: bool,
    snap_enabled: bool,
    sim_world: Option<PhysicsWorld>,
    sim_part_bodies: Vec<(usize, BodyHandle, [f32; 3])>,
    sim_part_transforms: Vec<([f32; 3], [f32; 4], [f32; 3])>,
    sim_last_time: Option<f64>,
    gizmo_mode: GizmoMode,
    active_gizmo_drag: Option<ActiveGizmoDrag>,
    selected_vertex_idx: Option<usize>,
    active_vertex_drag: Option<ActiveVertexDrag>,
    deform_amount: f32,
    array_count: usize,
    array_step: [f32; 3],
    array_radial: bool,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    continuous_edit_snapshot: Option<HistoryEntry>,
    selected_profile_pt: Option<usize>,
    dragging_profile_pt: Option<usize>,
    copied_color: Option<[u8; 4]>,
    copied_material: Option<([u8; 4], f32, f32, f32)>,
    terrain_sculpt_raise: bool,
    terrain_brush_radius: f32,
    terrain_brush_strength: f32,
    terrain_sculpt_last: Option<[f32; 2]>,
    imported_textures: Vec<Texture>,
    textures_shared: Arc<Vec<Texture>>,
    texture_generation: u64,
    textures_enabled: bool,
    import_tex_for_selection_only: bool,
    new_group_draft: String,
    export_tab: ExportTab,
    mobile_tab: MobileTab,
    rust_target: RustExportTarget,
    f3d_compact: bool,
    cached_f3d_bytes: Vec<u8>,
    cached_f3d_compact_bytes: Vec<u8>,
    cached_rust_src: String,
    cached_obj_src: String,
    doc2d: UiDocument2D,
    selected_widget_2d: Option<usize>,
    dragging_widget_2d: Option<ActiveWidgetDrag2D>,
    pending_import: Arc<Mutex<Option<(String, Vec<u8>, Option<std::path::PathBuf>)>>>,
    pending_status: Arc<Mutex<Option<String>>>,
    android_picking: bool,
    safe_insets_dp: [f32; 4],
    last_inset_poll: f64,
    was_focused: bool,
    immersive_applied: bool,
    status_note: String,
    pub lang_table: langes::LangTable,
    gl_renderer: Arc<Mutex<Option<GlViewportRenderer>>>,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Self::configure_style(&cc.egui_ctx);

        let gl_renderer = if let Some(gl) = &cc.gl {
            let initialized = unsafe { GlViewportRenderer::new(gl) }.ok();
            Arc::new(Mutex::new(initialized))
        } else {
            Arc::new(Mutex::new(None))
        };

        let scene = SceneDocument::preset_drone();
        let (baked_model, part_map) = scene.bake_with_part_map();
        let cached_f3d_bytes = baked_model.to_bytes();
        let cached_f3d_compact_bytes = baked_model.to_bytes_compact();
        let rust_target = RustExportTarget::ZeroDepConst;
        let cached_rust_src = baked_model.to_rust_source(&scene.model_name, rust_target);
        let cached_obj_src = baked_model.to_obj();
        let doc2d = default_ui_document_2d(&scene.model_name);

        Self {
            view_mode: ViewMode::Mode3D,
            show_file_modal: false,
            scene,
            baked_model,
            part_map,
            generation: 1,
            selection: Some(Selection::Part(0)),
            camera: OrbitCamera::default(),
            wireframe: false,
            show_grid: true,
            show_colliders: true,
            snap_enabled: false,
            sim_world: None,
            sim_part_bodies: Vec::new(),
            sim_part_transforms: Vec::new(),
            sim_last_time: None,
            gizmo_mode: GizmoMode::Translate,
            active_gizmo_drag: None,
            selected_vertex_idx: None,
            active_vertex_drag: None,
            deform_amount: 0.45,
            array_count: 3,
            array_step: [1.2, 0.0, 0.0],
            array_radial: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            continuous_edit_snapshot: None,
            selected_profile_pt: Some(0),
            dragging_profile_pt: None,
            copied_color: None,
            copied_material: None,
            terrain_sculpt_raise: true,
            terrain_brush_radius: 0.22,
            terrain_brush_strength: 0.22,
            terrain_sculpt_last: None,
            imported_textures: Vec::new(),
            textures_shared: Arc::new(Vec::new()),
            texture_generation: 1,
            textures_enabled: false,
            import_tex_for_selection_only: false,
            new_group_draft: String::from("Grupo"),
            export_tab: ExportTab::ProjectForja,
            mobile_tab: MobileTab::Viewport,
            rust_target,
            f3d_compact: false,
            cached_f3d_bytes,
            cached_f3d_compact_bytes,
            cached_rust_src,
            cached_obj_src,
            doc2d,
            selected_widget_2d: Some(0),
            dragging_widget_2d: None,
            pending_import: Arc::new(Mutex::new(None)),
            pending_status: Arc::new(Mutex::new(None)),
            android_picking: false,
            safe_insets_dp: [0.0; 4],
            last_inset_poll: -1.0,
            was_focused: false,
            immersive_applied: false,
            status_note: String::from("Arrastra el gizmo X/Y/Z para editar"),
            lang_table: langes::LangTable::parse_text(
                "[es]\ntitle = \"Forja3D Studio\"\nexport = \"Exportar\"\nphysics = \"Física\"\n[en]\ntitle = \"Forja3D Studio\"\nexport = \"Export\"\nphysics = \"Physics\"",
                "es",
            ),
            gl_renderer,
        }
    }

    fn configure_style(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(16, 19, 25);
        visuals.window_fill = egui::Color32::from_rgb(20, 24, 32);
        visuals.extreme_bg_color = egui::Color32::from_rgb(11, 13, 18);
        visuals.selection.bg_fill = egui::Color32::from_rgb(217, 119, 6);
        visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(251, 191, 36));
        ctx.set_visuals(visuals);
    }

    fn snapshot(&self) -> HistoryEntry {
        HistoryEntry {
            scene: self.scene.clone(),
            selection: self.selection.clone(),
        }
    }

    fn push_undo_entry(&mut self, entry: HistoryEntry) {
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(entry);
        self.redo_stack.clear();
    }

    fn push_undo(&mut self) {
        let entry = self.snapshot();
        self.push_undo_entry(entry);
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            let current = self.snapshot();
            self.redo_stack.push(current);
            self.scene = prev.scene;
            self.selection = prev.selection;
            self.selected_vertex_idx = None;
            self.rebuild_outputs();
            self.status_note = String::from("Deshacer aplicado");
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            let current = self.snapshot();
            self.undo_stack.push(current);
            self.scene = next.scene;
            self.selection = next.selection;
            self.selected_vertex_idx = None;
            self.rebuild_outputs();
            self.status_note = String::from("Rehacer aplicado");
        }
    }

    fn stop_physics_sim(&mut self) {
        self.sim_world = None;
        self.sim_part_bodies.clear();
        self.sim_part_transforms.clear();
        self.sim_last_time = None;
    }

    fn start_physics_sim(&mut self) {
        self.stop_physics_sim();
        let mut world = PhysicsWorld::new();
        let part_count = self.baked_model.parts.len();
        self.sim_part_transforms = vec![([0.0; 3], [0.0, 0.0, 0.0, 1.0], [0.0; 3]); part_count];

        let mut dyn_count = 0usize;
        for (mesh_idx, mesh_part) in self.baked_model.parts.iter().enumerate() {
            let src_idx = self.part_map.get(mesh_idx).copied().unwrap_or(0);
            let Some(part_spec) = self.scene.parts.get(src_idx) else {
                continue;
            };
            let pivot = if mesh_part.bounds.is_valid() {
                [
                    (mesh_part.bounds.min[0] + mesh_part.bounds.max[0]) * 0.5,
                    (mesh_part.bounds.min[1] + mesh_part.bounds.max[1]) * 0.5,
                    (mesh_part.bounds.min[2] + mesh_part.bounds.max[2]) * 0.5,
                ]
            } else {
                part_spec.position
            };

            let mut part_phys = forja3d::PhysicsSpec {
                body_kind: part_spec.body_kind,
                mass: part_spec.mass.max(0.05),
                friction: part_spec.friction,
                restitution: part_spec.restitution,
                linear_damping: part_spec.linear_damping,
                gravity_scale: part_spec.gravity_scale,
                colliders: Vec::new(),
            };

            let local_bounds = forja3d::Aabb {
                min: [
                    mesh_part.bounds.min[0] - pivot[0],
                    mesh_part.bounds.min[1] - pivot[1],
                    mesh_part.bounds.min[2] - pivot[2],
                ],
                max: [
                    mesh_part.bounds.max[0] - pivot[0],
                    mesh_part.bounds.max[1] - pivot[1],
                    mesh_part.bounds.max[2] - pivot[2],
                ],
            };
            let is_rotated = part_spec.rotation_deg.iter().any(|r| r.abs() > 0.01);
            let is_mirrored = part_spec.mirror.iter().any(|&m| m);
            let sx = part_spec.scale[0].abs().max(0.05);
            let sy = part_spec.scale[1].abs().max(0.05);
            let sz = part_spec.scale[2].abs().max(0.05);
            let s_max = sx.max(sy).max(sz);
            let is_nonuniform_sphere = part_spec.kind == PrimitiveKind::Sphere
                && ((sx - sy).abs() > s_max * 0.03
                    || (sy - sz).abs() > s_max * 0.03
                    || (sx - sz).abs() > s_max * 0.03
                    || is_mirrored);

            let use_exact_mesh = (matches!(
                part_spec.kind,
                PrimitiveKind::Pyramid
                    | PrimitiveKind::Cone
                    | PrimitiveKind::Cylinder
                    | PrimitiveKind::Plane
                    | PrimitiveKind::Torus
                    | PrimitiveKind::Terrain
                    | PrimitiveKind::Extrude
                    | PrimitiveKind::Lathe
                    | PrimitiveKind::CustomMesh
            ) || (part_spec.kind == PrimitiveKind::Cube && (is_rotated || is_mirrored))
                || is_nonuniform_sphere)
                && (mesh_part.indices.len() / 3) <= 12288;

            let mut body = if use_exact_mesh {
                let mut tris = Vec::with_capacity(mesh_part.indices.len() / 3);
                for tri in mesh_part.indices.chunks_exact(3) {
                    if let (Some(a), Some(b), Some(c)) = (
                        mesh_part.vertices.get(tri[0] as usize),
                        mesh_part.vertices.get(tri[1] as usize),
                        mesh_part.vertices.get(tri[2] as usize),
                    ) {
                        tris.push([
                            [a.position[0] - pivot[0], a.position[1] - pivot[1], a.position[2] - pivot[2]],
                            [b.position[0] - pivot[0], b.position[1] - pivot[1], b.position[2] - pivot[2]],
                            [c.position[0] - pivot[0], c.position[1] - pivot[1], c.position[2] - pivot[2]],
                        ]);
                    }
                }
                let mut rb = RigidBody::from_physics_spec(
                    mesh_part.name.clone(),
                    pivot,
                    &part_phys,
                    local_bounds,
                );
                // Keep exact triangle mesh collision as primary without an oversized fallback AABB
                rb.colliders.clear();
                rb.with_mesh_triangles(tris)
            } else {
                let mut shape_spec = if part_spec.collider != ColliderShape::None {
                    part_spec.fitted_collider_for_mesh(mesh_part)
                } else {
                    None
                };
                if shape_spec.is_none() {
                    let dx = (local_bounds.max[0] - local_bounds.min[0]).max(0.05);
                    let dy = (local_bounds.max[1] - local_bounds.min[1]).max(0.05);
                    let dz = (local_bounds.max[2] - local_bounds.min[2]).max(0.05);
                    let shape = match part_spec.kind {
                        PrimitiveKind::Sphere => ColliderShape::Sphere {
                            radius: (dx.max(dy).max(dz).max(s_max) * 0.5).max(0.04),
                        },
                        _ => ColliderShape::Box {
                            half_extents: [dx * 0.5, dy * 0.5, dz * 0.5],
                        },
                    };
                    shape_spec = Some(forja3d::ColliderSpec {
                        part_hash: fnv1a_32(mesh_part.name.as_bytes()),
                        offset: pivot,
                        shape,
                    });
                }
                if let Some(mut spec) = shape_spec {
                    spec.offset[0] -= pivot[0];
                    spec.offset[1] -= pivot[1];
                    spec.offset[2] -= pivot[2];
                    part_phys.colliders.push(spec);
                }
                RigidBody::from_physics_spec(mesh_part.name.clone(), pivot, &part_phys, local_bounds)
            };

            body.body_kind = part_spec.body_kind;
            body.inv_mass = match body.body_kind {
                RigidBodyKind::Dynamic => 1.0 / body.mass.max(0.01),
                RigidBodyKind::Static | RigidBodyKind::Kinematic | RigidBodyKind::Sensor => 0.0,
            };
            if body.body_kind == RigidBodyKind::Dynamic {
                dyn_count += 1;
            }
            let handle = world.add_body(body);
            self.sim_part_bodies.push((mesh_idx, handle, pivot));
            self.sim_part_transforms[mesh_idx] = (pivot, [0.0, 0.0, 0.0, 1.0], [0.0; 3]);
        }

        self.sim_world = Some(world);
        self.sim_last_time = None;
        self.status_note = if dyn_count > 0 {
            format!("Física activa ({} cuerpos independientes)", self.sim_part_bodies.len())
        } else {
            String::from("Escena estática: cambia una pieza a Dinámico para caer")
        };
    }

    fn step_physics_sim(&mut self, ctx: &egui::Context) {
        let Some(world) = self.sim_world.as_mut() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let dt = match self.sim_last_time {
            Some(prev) => (now - prev) as f32,
            None => 1.0 / 60.0,
        };
        self.sim_last_time = Some(now);
        let dt = dt.clamp(0.001, 0.05);
        world.step(dt);
        for &(mesh_idx, handle, initial_pivot) in &self.sim_part_bodies {
            if let Some(body) = world.body(handle) {
                if let Some(slot) = self.sim_part_transforms.get_mut(mesh_idx) {
                    *slot = (
                        initial_pivot,
                        body.orientation,
                        [
                            body.position[0] - initial_pivot[0],
                            body.position[1] - initial_pivot[1],
                            body.position[2] - initial_pivot[2],
                        ],
                    );
                }
            }
        }
        ctx.request_repaint();
    }

    fn rebuild_outputs(&mut self) {
        self.stop_physics_sim();
        let (baked, part_map) = self.scene.bake_with_part_map();
        self.baked_model = baked;
        self.part_map = part_map;
        self.generation += 1;
        self.cached_f3d_bytes = self.baked_model.to_bytes();
        self.cached_f3d_compact_bytes = self.baked_model.to_bytes_compact();
        self.cached_rust_src = self
            .baked_model
            .to_rust_source(&self.scene.model_name, self.rust_target);
        self.cached_obj_src = self.baked_model.to_obj();
    }

    fn sync_textures_cache(&mut self) {
        self.texture_generation = self.texture_generation.wrapping_add(1).max(1);
        self.textures_shared = Arc::new(self.imported_textures.clone());
    }

    fn assign_texture_index_to_all(&mut self, tex_idx: usize) {
        for p in &mut self.scene.parts {
            p.texture_index = Some(tex_idx);
            p.texture_enabled = true;
        }
    }

    fn assign_texture_index_to_selection_or_all(&mut self, tex_idx: usize) {
        let selected_indices = self.selected_part_indices();
        if selected_indices.is_empty() {
            self.assign_texture_index_to_all(tex_idx);
        } else {
            for idx in selected_indices {
                if let Some(p) = self.scene.parts.get_mut(idx) {
                    p.texture_index = Some(tex_idx);
                    p.texture_enabled = true;
                }
            }
        }
    }

    fn export_texture_asset(&self) -> Texture {
        if let Some(Selection::Part(idx)) = self.selection {
            if let Some(tex) = self
                .scene
                .parts
                .get(idx)
                .and_then(|p| p.texture_index)
                .and_then(|ti| self.imported_textures.get(ti))
            {
                return tex.clone();
            }
        }
        if let Some(tex) = self.imported_textures.first() {
            return tex.clone();
        }
        self.build_texture_asset()
    }

    fn import_model_bytes(
        &mut self,
        filename: Option<&str>,
        bytes: &[u8],
        base_dir: Option<&std::path::Path>,
    ) {
        let selection_only = std::mem::take(&mut self.import_tex_for_selection_only);
        let lower = filename.unwrap_or("").to_ascii_lowercase();
        let is_f3d_magic = bytes.len() >= 4 && bytes[0..4] == forja3d::MAGIC;
        let is_f3s_magic = bytes.len() >= 4 && bytes[0..4] == F3S_MAGIC;
        let is_f3p_magic = bytes.len() >= 4 && bytes[0..4] == F3P_MAGIC;
        let is_f3a_magic = bytes.len() >= 4 && bytes[0..4] == F3A_MAGIC;
        let is_f2d_magic = bytes.len() >= 4 && bytes[0..4] == F2D_MAGIC;
        let is_f3t_magic = bytes.len() >= 4 && bytes[0..4] == F3T_MAGIC;
        let is_image_ext = lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".webp");
        let is_glb_magic = bytes.len() >= 4 && &bytes[0..4] == b"glTF";
        let is_gltf = lower.ends_with(".glb") || lower.ends_with(".gltf") || is_glb_magic;

        if is_f3s_magic || lower.ends_with(".forja") || lower.ends_with(".f3s") {
            match ForjaProject::from_bytes(bytes) {
                Ok(proj) => {
                    self.push_undo();
                    self.scene = proj.scene;
                    self.imported_textures = proj.textures;
                    self.sync_textures_cache();
                    self.textures_enabled = proj.viewport.textures_enabled;
                    self.wireframe = proj.viewport.wireframe;
                    self.show_grid = proj.viewport.show_grid;
                    self.show_colliders = proj.viewport.show_colliders;
                    self.camera = OrbitCamera {
                        yaw: proj.camera.yaw,
                        pitch: proj.camera.pitch,
                        distance: proj.camera.distance,
                        target: proj.camera.target,
                        ortho: proj.camera.ortho,
                    };
                    if let Some(doc2d) = proj.doc2d {
                        self.selected_widget_2d = if doc2d.widgets.is_empty() { None } else { Some(0) };
                        self.doc2d = doc2d;
                    }
                    self.selection = if !self.scene.parts.is_empty() {
                        Some(Selection::Part(0))
                    } else if !self.scene.sun_lights.is_empty() {
                        Some(Selection::Sun(0))
                    } else {
                        None
                    };
                    self.selected_vertex_idx = None;
                    self.rebuild_outputs();
                    let src = filename.unwrap_or(&self.scene.model_name);
                    self.status_note = format!(
                        "Proyecto '{src}' abierto ({} piezas, {} soles, {} tex)",
                        self.scene.parts.len(),
                        self.scene.sun_lights.len(),
                        self.imported_textures.len()
                    );
                    self.view_mode = ViewMode::Mode3D;
                }
                Err(err) => {
                    self.status_note = format!("Error al abrir .forja: {err}");
                }
            }
        } else if is_f3d_magic || (lower.ends_with(".f3d") && !is_gltf) {
            match Model::from_bytes(bytes) {
                Ok(decoded) => {
                    self.push_undo();
                    let flags = if bytes.len() >= 8 {
                        u16::from_le_bytes([bytes[6], bytes[7]])
                    } else {
                        0
                    };
                    let was_compact = (flags & FLAG_COMPACT) != 0;
                    let has_physics = (flags & FLAG_PHYSICS) != 0;
                    self.scene = SceneDocument::from_model(decoded);
                    self.imported_textures.clear();
                    self.textures_enabled = false;
                    self.sync_textures_cache();
                    self.selection = if self.scene.parts.is_empty() {
                        None
                    } else {
                        Some(Selection::Part(0))
                    };
                    self.selected_vertex_idx = None;
                    self.rebuild_outputs();
                    self.camera.frame_bounds(self.baked_model.bounds());
                    let src = filename.unwrap_or(&self.scene.model_name);
                    let tag = if was_compact { "compacto" } else { "estándar" };
                    let phys_tag = if has_physics { " + físicas" } else { "" };
                    self.status_note = format!(
                        "Importado '{src}' ({tag}{phys_tag}, {} piezas, {} tri)",
                        self.scene.parts.len(),
                        self.baked_model.triangle_count()
                    );
                    self.view_mode = ViewMode::Mode3D;
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f3d: {err}");
                }
            }
        } else if is_f3p_magic || lower.ends_with(".f3p") {
            match ForjaPack::from_bytes(bytes) {
                Ok(pack) => {
                    self.push_undo();
                    let mut summary = Vec::new();
                    self.imported_textures = pack.textures.clone();
                    self.textures_enabled = !self.imported_textures.is_empty();
                    self.sync_textures_cache();
                    if let Some(model) = pack.model {
                        let tri = model.triangle_count();
                        self.scene = SceneDocument::from_model(model);
                        if !self.imported_textures.is_empty() {
                            self.assign_texture_index_to_all(0);
                        }
                        self.selection = if self.scene.parts.is_empty() {
                            None
                        } else {
                            Some(Selection::Part(0))
                        };
                        self.selected_vertex_idx = None;
                        self.rebuild_outputs();
                        self.camera.frame_bounds(self.baked_model.bounds());
                        summary.push(format!("malla {} tri", tri));
                    } else if !self.imported_textures.is_empty() {
                        self.assign_texture_index_to_all(0);
                        self.view_mode = ViewMode::Mode3D;
                    }
                    if !pack.textures.is_empty() {
                        summary.push(format!("{} texturas", pack.textures.len()));
                    }
                    if !pack.animations.is_empty() {
                        summary.push(format!("{} animaciones", pack.animations.len()));
                    }
                    let src = filename.unwrap_or(&pack.name);
                    let info = if summary.is_empty() {
                        String::from("vacío")
                    } else {
                        summary.join(", ")
                    };
                    self.status_note = format!("Paquete .f3p '{src}' importado ({info})");
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f3p: {err}");
                }
            }
        } else if is_f3a_magic || lower.ends_with(".f3a") {
            match AnimationSet::from_bytes(bytes) {
                Ok(anim) => {
                    let tracks: usize = anim.clips.iter().map(|c| c.tracks.len()).sum();
                    let kf = anim.total_keyframes();
                    let src = filename.unwrap_or(&anim.name);
                    self.status_note = format!(
                        "Animación .f3a '{src}' válida ({} clips, {tracks} pistas, {kf} keyframes)",
                        anim.clips.len()
                    );
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f3a: {err}");
                }
            }
        } else if is_f2d_magic || lower.ends_with(".f2d") {
            match UiDocument2D::from_bytes(bytes) {
                Ok(doc) => {
                    let widget_count = doc.widgets.len();
                    let action_count = doc.actions.len();
                    let src = filename.unwrap_or(&doc.name).to_string();
                    self.doc2d = doc;
                    self.selected_widget_2d = if widget_count > 0 { Some(0) } else { None };
                    self.view_mode = ViewMode::Mode2D;
                    self.status_note = format!(
                        "Pantalla 2D .f2d '{src}' cargada ({widget_count} controles, {action_count} acciones .rs)"
                    );
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f2d: {err}");
                }
            }
        } else if is_f3t_magic || lower.ends_with(".f3t") {
            match Texture::from_bytes(bytes) {
                Ok(mut tex) => {
                    self.push_undo();
                    if tex.name.trim().is_empty() {
                        if let Some(fname) = filename {
                            tex.name = fname.to_string();
                        }
                    }
                    let src = filename.unwrap_or(&tex.name).to_string();
                    let w = tex.width;
                    let h = tex.height;
                    let tex_idx = self.imported_textures.len();
                    self.imported_textures.push(tex);
                    self.sync_textures_cache();
                    self.textures_enabled = true;
                    if selection_only {
                        self.assign_texture_index_to_selection_or_all(tex_idx);
                    } else {
                        self.assign_texture_index_to_all(tex_idx);
                    }
                    self.view_mode = ViewMode::Mode3D;
                    self.status_note = format!("Textura .f3t '{src}' aplicada ({w}x{h} px)");
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f3t: {err}");
                }
            }
        } else if is_image_ext {
            match image::load_from_memory(bytes) {
                Ok(mut dyn_img) => {
                    self.push_undo();
                    if dyn_img.width() > 2048 || dyn_img.height() > 2048 {
                        dyn_img = dyn_img.resize(2048, 2048, image::imageops::FilterType::Triangle);
                    }
                    let rgba_img = dyn_img.to_rgba8();
                    let (w, h) = rgba_img.dimensions();
                    let tex_name = filename.unwrap_or("textura_imagen").to_string();
                    let tex = Texture::new_rgba(
                        tex_name.clone(),
                        w.clamp(1, u16::MAX as u32) as u16,
                        h.clamp(1, u16::MAX as u32) as u16,
                        rgba_img.into_raw(),
                    );
                    let tex_idx = self.imported_textures.len();
                    self.imported_textures.push(tex);
                    self.sync_textures_cache();
                    self.textures_enabled = true;
                    if selection_only {
                        self.assign_texture_index_to_selection_or_all(tex_idx);
                    } else {
                        self.assign_texture_index_to_all(tex_idx);
                    }
                    self.view_mode = ViewMode::Mode3D;
                    self.status_note = format!("Imagen '{tex_name}' cargada ({w}x{h} px)");
                }
                Err(err) => {
                    self.status_note = format!("Error al leer imagen: {err}");
                }
            }
        } else {
            match gltf_import::import_gltf_or_glb_full(filename, bytes, base_dir) {
                Ok(imported) => {
                    self.push_undo();
                    self.scene = imported.scene;
                    let tex_count = imported.textures.len();
                    self.imported_textures = imported.textures;
                    self.textures_enabled = tex_count > 0;
                    self.sync_textures_cache();
                    self.selection = if self.scene.parts.is_empty() {
                        None
                    } else {
                        Some(Selection::Part(0))
                    };
                    self.selected_vertex_idx = None;
                    self.rebuild_outputs();
                    self.camera.frame_bounds(self.baked_model.bounds());
                    let src = filename.unwrap_or(&self.scene.model_name);
                    let tex_tag = if tex_count > 0 {
                        format!(", {tex_count} tex")
                    } else {
                        String::new()
                    };
                    self.status_note = format!(
                        "GLB/GLTF '{src}' ({} piezas, {} tri{tex_tag})",
                        self.scene.parts.len(),
                        self.baked_model.triangle_count()
                    );
                    self.view_mode = ViewMode::Mode3D;
                }
                Err(err) => {
                    self.status_note = err;
                }
            }
        }
    }

    fn build_project_asset(&self) -> ForjaProject {
        ForjaProject {
            scene: self.scene.clone(),
            textures: self.imported_textures.clone(),
            camera: ProjectCameraState {
                yaw: self.camera.yaw,
                pitch: self.camera.pitch,
                distance: self.camera.distance,
                target: self.camera.target,
                ortho: self.camera.ortho,
            },
            viewport: ProjectViewportState {
                textures_enabled: self.textures_enabled,
                wireframe: self.wireframe,
                show_grid: self.show_grid,
                show_colliders: self.show_colliders,
            },
            doc2d: Some(self.doc2d.clone()),
        }
    }

    fn build_pack_asset(&self) -> ForjaPack {
        let mut pack = ForjaPack::new(self.scene.model_name.clone());
        pack.model = Some(self.baked_model.clone());
        if self.imported_textures.is_empty() {
            pack.textures.push(self.build_texture_asset());
        } else {
            pack.textures.extend(self.imported_textures.iter().cloned());
        }
        pack.animations.push(self.build_animation_asset());
        pack
    }

    fn build_animation_asset(&self) -> AnimationSet {
        let mut set = AnimationSet::new(self.scene.model_name.clone());
        let mut clip = AnimationClip::new("Idle", 2.0, true);
        let mut added = 0usize;
        for part in self.scene.parts.iter().filter(|p| p.visible).take(6) {
            let mut pos_track = AnimationTrack::new(
                part.name.clone(),
                AnimationProperty::Translation,
                InterpolationMode::Linear,
            );
            let p = part.position;
            pos_track.keyframes.push(Keyframe {
                time: 0.0,
                value: [p[0], p[1], p[2], 0.0],
            });
            pos_track.keyframes.push(Keyframe {
                time: 1.0,
                value: [p[0], p[1] + 0.15, p[2], 0.0],
            });
            pos_track.keyframes.push(Keyframe {
                time: 2.0,
                value: [p[0], p[1], p[2], 0.0],
            });
            clip.tracks.push(pos_track);

            let mut rot_track = AnimationTrack::new(
                part.name.clone(),
                AnimationProperty::Rotation,
                InterpolationMode::Linear,
            );
            rot_track.keyframes.push(Keyframe {
                time: 0.0,
                value: [0.0, 0.0, 0.0, 1.0],
            });
            rot_track.keyframes.push(Keyframe {
                time: 1.0,
                value: [0.0, 0.38268343, 0.0, 0.9238795],
            });
            rot_track.keyframes.push(Keyframe {
                time: 2.0,
                value: [0.0, 0.0, 0.0, 1.0],
            });
            clip.tracks.push(rot_track);
            added += 1;
        }
        if added == 0 {
            let mut root = AnimationTrack::new(
                self.scene.model_name.clone(),
                AnimationProperty::Translation,
                InterpolationMode::Linear,
            );
            root.keyframes.push(Keyframe {
                time: 0.0,
                value: [0.0, 0.0, 0.0, 0.0],
            });
            root.keyframes.push(Keyframe {
                time: 2.0,
                value: [0.0, 0.0, 0.0, 0.0],
            });
            clip.tracks.push(root);
        }
        set.clips.push(clip);
        set
    }

    fn build_texture_asset(&self) -> Texture {
        const SIZE: u16 = 16;
        let palette: Vec<[u8; 4]> = if self.scene.parts.is_empty() {
            vec![[224, 159, 62, 255], [56, 189, 248, 255]]
        } else {
            self.scene.parts.iter().map(|p| p.color).collect()
        };
        let mut rgba = Vec::with_capacity((SIZE as usize) * (SIZE as usize) * 4);
        for y in 0..SIZE as usize {
            for x in 0..SIZE as usize {
                let idx = ((x / 4) + (y / 4) * 4) % palette.len();
                let mut c = palette[idx];
                if (x + y) % 2 == 0 {
                    c[0] = c[0].saturating_add(28);
                    c[1] = c[1].saturating_add(28);
                    c[2] = c[2].saturating_add(28);
                } else {
                    c[0] = c[0].saturating_sub(24);
                    c[1] = c[1].saturating_sub(24);
                    c[2] = c[2].saturating_sub(24);
                }
                rgba.extend_from_slice(&c);
            }
        }
        Texture::new_rgba(
            format!("{}_albedo", self.scene.model_name),
            SIZE,
            SIZE,
            rgba,
        )
    }

    fn load_preset(&mut self, scene: SceneDocument) {
        self.push_undo();
        self.scene = scene;
        self.imported_textures.clear();
        self.textures_enabled = false;
        self.sync_textures_cache();
        self.selection = if self.scene.parts.is_empty() {
            None
        } else {
            Some(Selection::Part(0))
        };
        self.selected_vertex_idx = None;
        self.rebuild_outputs();
        self.camera.frame_bounds(self.baked_model.bounds());
        self.status_note = format!("Escena: {}", self.scene.model_name);
    }

    fn add_sun_light(&mut self) {
        self.push_undo();
        let idx = self.scene.sun_lights.len();
        let mut sun = SunLightSpec::new(format!("Sol_{}", idx + 1));
        if idx > 0 {
            let angle = (idx as f32) * 1.4;
            sun.position = [3.6 * angle.cos(), 4.5 + (idx as f32) * 0.4, 3.6 * angle.sin()];
        }
        self.scene.sun_lights.push(sun);
        self.selection = Some(Selection::Sun(idx));
        self.selected_vertex_idx = None;
        self.status_note = format!("Sol_{} añadido (intensidad 1.25x)", idx + 1);
    }

    fn add_primitive(&mut self, kind: PrimitiveKind) {
        self.push_undo();
        let idx = self.scene.parts.len() + 1;
        let mut part = PartSpec::new(format!("{}_{}", kind.label().to_lowercase(), idx), kind);
        if kind != PrimitiveKind::Terrain {
            part.position = [((idx % 3) as f32 - 1.0) * 0.4, 0.5, 0.0];
        }
        if let Some(Selection::Group(g)) = &self.selection {
            part.group = Some(g.clone());
        }
        self.scene.parts.push(part);
        self.selection = Some(Selection::Part(self.scene.parts.len() - 1));
        self.selected_vertex_idx = None;
        self.rebuild_outputs();
    }

    fn stack_selected_on_below(&mut self) {
        let Some(Selection::Part(sel_idx)) = self.selection else {
            self.status_note = String::from("Selecciona una pieza para apilar");
            return;
        };
        if sel_idx >= self.scene.parts.len() {
            return;
        }
        let sel_bounds = self.scene.parts[sel_idx].bake().bounds;
        if !sel_bounds.is_valid() {
            return;
        }
        // Prefer the visible part immediately preceding sel_idx, or the highest other visible part
        let target_bounds = if sel_idx > 0 && self.scene.parts[sel_idx - 1].visible {
            Some(self.scene.parts[sel_idx - 1].bake().bounds)
        } else {
            self.scene
                .parts
                .iter()
                .enumerate()
                .filter(|(i, p)| *i != sel_idx && p.visible)
                .map(|(_, p)| p.bake().bounds)
                .filter(|b| b.is_valid())
                .max_by(|a, b| a.max[1].total_cmp(&b.max[1]))
        };

        let Some(tb) = target_bounds else {
            self.push_undo();
            self.scene.parts[sel_idx].position[1] -= sel_bounds.min[1];
            self.rebuild_outputs();
            self.status_note = String::from("Pieza apoyada en el suelo Y=0");
            return;
        };

        self.push_undo();
        let tc_x = (tb.min[0] + tb.max[0]) * 0.5;
        let tc_z = (tb.min[2] + tb.max[2]) * 0.5;
        let sc_x = (sel_bounds.min[0] + sel_bounds.max[0]) * 0.5;
        let sc_z = (sel_bounds.min[2] + sel_bounds.max[2]) * 0.5;
        let p = &mut self.scene.parts[sel_idx];
        p.position[0] += tc_x - sc_x;
        p.position[2] += tc_z - sc_z;
        p.position[1] += tb.max[1] - sel_bounds.min[1] + 0.02;
        self.rebuild_outputs();
        self.status_note = String::from("Pieza apilada encima");
    }

    fn merge_visible_parts(&mut self) {
        let visible_indices: Vec<usize> = self
            .scene
            .parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.visible.then_some(i))
            .collect();
        if visible_indices.len() < 2 {
            self.status_note = String::from("Se necesitan al menos 2 piezas visibles para combinar");
            return;
        }
        self.push_undo();
        let mut combined_verts = Vec::new();
        let mut combined_indices = Vec::new();
        let mut combined_bounds = forja3d::Aabb::EMPTY;
        let mut mat = Material::default();
        if let Some(first) = self.scene.parts.get(visible_indices[0]) {
            mat.base_color = first.color;
            mat.metallic = first.metallic;
            mat.roughness = first.roughness;
        }

        for &idx in &visible_indices {
            for baked in self.scene.parts[idx].bake_all() {
                let base_v = combined_verts.len() as u32;
                for v in baked.vertices {
                    combined_bounds.include_point(v.position);
                    combined_verts.push(v);
                }
                for i in baked.indices {
                    combined_indices.push(base_v + i);
                }
            }
        }

        let center = if combined_bounds.is_valid() {
            [
                (combined_bounds.min[0] + combined_bounds.max[0]) * 0.5,
                (combined_bounds.min[1] + combined_bounds.max[1]) * 0.5,
                (combined_bounds.min[2] + combined_bounds.max[2]) * 0.5,
            ]
        } else {
            [0.0; 3]
        };
        let mut local_bounds = forja3d::Aabb::EMPTY;
        for v in &mut combined_verts {
            v.position[0] -= center[0];
            v.position[1] -= center[1];
            v.position[2] -= center[2];
            local_bounds.include_point(v.position);
        }

        let merged_mesh = MeshPart {
            name: format!("{}_combinada", self.scene.model_name),
            material: mat,
            vertices: combined_verts,
            indices: combined_indices,
            bounds: local_bounds,
        };
        let mut merged_part = PartSpec::from_mesh_part(merged_mesh);
        merged_part.position = center;

        self.scene.parts.retain(|p| !p.visible);
        self.scene.parts.push(merged_part);
        self.selection = Some(Selection::Part(self.scene.parts.len() - 1));
        self.selected_vertex_idx = None;
        self.rebuild_outputs();
        self.status_note = String::from("Piezas visibles combinadas en Malla Editable");
    }

    fn convert_selected_to_mesh(&mut self) -> Option<usize> {
        let Some(Selection::Part(idx)) = self.selection else {
            return None;
        };
        if idx >= self.scene.parts.len() {
            return None;
        }
        if self.scene.parts[idx].kind != PrimitiveKind::CustomMesh {
            self.push_undo();
            self.scene.parts[idx].convert_to_custom_mesh();
            self.selected_vertex_idx = Some(0);
            self.rebuild_outputs();
            self.status_note = String::from("Convertida a Malla Editable");
        }
        Some(idx)
    }

    fn subdivide_selected(&mut self) {
        let Some(Selection::Part(idx)) = self.selection else {
            self.status_note = String::from("Selecciona una pieza para subdividir");
            return;
        };
        if idx >= self.scene.parts.len() {
            return;
        }
        self.push_undo();
        if self.scene.parts[idx].kind != PrimitiveKind::CustomMesh {
            self.scene.parts[idx].convert_to_custom_mesh();
        }
        if let Some(mesh) = self.scene.parts[idx].custom_mesh.as_mut() {
            if mesh.indices.len() / 3 > 16_000 {
                self.status_note = String::from("Malla ya es muy densa (>16k tri)");
                return;
            }
            let (v, i) = subdivide_triangles(&mesh.vertices, &mesh.indices);
            mesh.vertices = v;
            mesh.indices = i;
            recompute_mesh_normals(mesh);
            self.status_note = format!("Subdividida ({} tri)", mesh.indices.len() / 3);
        }
        self.rebuild_outputs();
    }

    fn deform_selected(&mut self, kind: MeshDeformKind) {
        let Some(Selection::Part(idx)) = self.selection else {
            self.status_note = String::from("Selecciona una pieza para deformar");
            return;
        };
        if idx >= self.scene.parts.len() {
            return;
        }
        self.push_undo();
        if self.scene.parts[idx].kind != PrimitiveKind::CustomMesh {
            self.scene.parts[idx].convert_to_custom_mesh();
        }
        let amount = self.deform_amount;
        if let Some(mesh) = self.scene.parts[idx].custom_mesh.as_mut() {
            let (v, i) = deform_vertices(&mesh.vertices, &mesh.indices, kind, amount);
            mesh.vertices = v;
            mesh.indices = i;
            recompute_mesh_normals(mesh);
        }
        let label = match kind {
            MeshDeformKind::Taper => "Afilado (Taper)",
            MeshDeformKind::Twist => "Torsión (Twist)",
            MeshDeformKind::Bend => "Doblado (Bend)",
        };
        self.status_note = format!("{label} aplicado");
        self.rebuild_outputs();
    }

    fn array_clone_selected(&mut self) {
        let Some(Selection::Part(idx)) = self.selection else {
            self.status_note = String::from("Selecciona una pieza para clonar en matriz");
            return;
        };
        if idx >= self.scene.parts.len() {
            return;
        }
        let count = self.array_count.clamp(1, 24);
        self.push_undo();
        let template = self.scene.parts[idx].clone();
        if self.array_radial {
            let total = count + 1;
            let radius = template.position[0].hypot(template.position[2]).max(1.0);
            let base_angle = template.position[2].atan2(template.position[0]);
            for k in 1..=count {
                let mut c = template.clone();
                c.name = format!("{}_r{k}", template.name);
                let ang = base_angle + (k as f32 / total as f32) * core::f32::consts::TAU;
                c.position[0] = radius * ang.cos();
                c.position[2] = radius * ang.sin();
                c.rotation_deg[1] += (k as f32 / total as f32) * 360.0;
                self.scene.parts.push(c);
            }
        } else {
            for k in 1..=count {
                let mut c = template.clone();
                c.name = format!("{}_m{k}", template.name);
                c.position[0] += self.array_step[0] * k as f32;
                c.position[1] += self.array_step[1] * k as f32;
                c.position[2] += self.array_step[2] * k as f32;
                self.scene.parts.push(c);
            }
        }
        self.rebuild_outputs();
        self.status_note = format!("Matriz creada ({count} copias)");
    }

    fn selected_part_indices(&self) -> Vec<usize> {
        match &self.selection {
            Some(Selection::Part(idx)) if *idx < self.scene.parts.len() => vec![*idx],
            Some(Selection::Group(group_name)) => self
                .scene
                .parts
                .iter()
                .enumerate()
                .filter_map(|(i, p)| (p.group.as_deref() == Some(group_name.as_str())).then_some(i))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn selected_mesh_mask(&self) -> Vec<bool> {
        let active_parts = self.selected_part_indices();
        self.part_map
            .iter()
            .map(|src_idx| active_parts.contains(src_idx))
            .collect()
    }

    fn selection_pivot(&self) -> Option<[f32; 3]> {
        if let Some(Selection::Sun(sun_idx)) = self.selection {
            return self.scene.sun_lights.get(sun_idx).map(|s| s.position);
        }
        let indices = self.selected_part_indices();
        if indices.is_empty() {
            return None;
        }
        let mut sum = [0.0_f32; 3];
        for &idx in &indices {
            let p = self.scene.parts[idx].position;
            sum[0] += p[0];
            sum[1] += p[1];
            sum[2] += p[2];
        }
        let n = indices.len() as f32;
        Some([sum[0] / n, sum[1] / n, sum[2] / n])
    }

    fn snap_step(val: f32, step: f32) -> f32 {
        (val / step).round() * step
    }

    fn begin_continuous_edit(&mut self, ui: &egui::Ui) {
        if self.continuous_edit_snapshot.is_none() {
            self.continuous_edit_snapshot = Some(self.snapshot());
        }
        let _ = ui;
    }

    fn flush_continuous_edit(&mut self, ui: &egui::Ui) {
        if !ui.ctx().input(|i| i.pointer.any_down()) {
            if let Some(entry) = self.continuous_edit_snapshot.take() {
                if entry.scene != self.scene {
                    self.push_undo_entry(entry);
                }
            }
        }
    }

    fn render_scene_panel(&mut self, ui: &mut egui::Ui, dirty: &mut bool, compact: bool) {
        ui.label(egui::RichText::new("Nombre del modelo").strong());
        if ui.text_edit_singleline(&mut self.scene.model_name).changed() {
            *dirty = true;
        }

        let btn_h = if compact { 32.0 } else { 24.0 };
        ui.add_space(6.0);

        egui::CollapsingHeader::new(egui::RichText::new("Escenas base").strong())
            .default_open(!compact)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.add_sized([74.0, btn_h], egui::Button::new("Dron")).clicked() {
                        self.load_preset(SceneDocument::preset_drone());
                    }
                    if ui.add_sized([82.0, btn_h], egui::Button::new("Monolito")).clicked() {
                        self.load_preset(SceneDocument::preset_obelisk());
                    }
                    if ui.add_sized([78.0, btn_h], egui::Button::new("Terreno")).clicked() {
                        self.load_preset(SceneDocument::preset_terrain());
                    }
                    if ui.add_sized([70.0, btn_h], egui::Button::new("Vacío")).clicked() {
                        self.load_preset(SceneDocument::new("MiModelo"));
                    }
                });
            });

        egui::CollapsingHeader::new(egui::RichText::new("Iluminación y texturas").strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if icon_text_btn(ui, SvgIcon::Sun, 14.0, "Añadir Sol", [110.0, btn_h], false)
                        .on_hover_text("Crea un objeto Sol direccional con intensidad modificable")
                        .clicked()
                    {
                        self.add_sun_light();
                    }
                    if icon_text_btn(
                        ui,
                        SvgIcon::Texture,
                        14.0,
                        "Texturas",
                        [98.0, btn_h],
                        self.textures_enabled,
                    )
                    .on_hover_text("Activar o desactivar texturas en el modelo")
                    .clicked()
                    {
                        self.textures_enabled = !self.textures_enabled;
                        self.status_note = if self.textures_enabled {
                            String::from("Texturas del modelo activadas")
                        } else {
                            String::from("Texturas del modelo desactivadas")
                        };
                    }
                });
                if !self.scene.sun_lights.is_empty() {
                    ui.add_space(4.0);
                    let mut delete_sun: Option<usize> = None;
                    let mut dup_sun: Option<usize> = None;
                    let mut toggle_sun_vis: Option<usize> = None;
                    let mut toggle_sun_en: Option<usize> = None;
                    for (idx, sun) in self.scene.sun_lights.iter().enumerate() {
                        let is_sel = self.selection == Some(Selection::Sun(idx));
                        ui.horizontal(|ui| {
                            let eye_sz = if compact { [30.0, 28.0] } else { [24.0, 22.0] };
                            if icon_btn(ui, SvgIcon::Eye, 13.0, eye_sz, sun.visible)
                                .on_hover_text("Mostrar/ocultar marcador del Sol")
                                .clicked()
                            {
                                toggle_sun_vis = Some(idx);
                            }
                            if icon_btn(ui, SvgIcon::Sun, 13.0, eye_sz, sun.enabled)
                                .on_hover_text("Encender/apagar iluminación del Sol")
                                .clicked()
                            {
                                toggle_sun_en = Some(idx);
                            }
                            let label = format!("{} ({:.2}x)", sun.name, sun.intensity);
                            if ui.selectable_label(is_sel, label).clicked() {
                                self.selection = Some(Selection::Sun(idx));
                                self.selected_vertex_idx = None;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let act_sz = if compact { [30.0, 28.0] } else { [22.0, 22.0] };
                                if icon_btn(ui, SvgIcon::Trash, 12.0, act_sz, false)
                                    .on_hover_text("Eliminar Sol")
                                    .clicked()
                                {
                                    delete_sun = Some(idx);
                                }
                                if icon_btn(ui, SvgIcon::Duplicate, 12.0, act_sz, false)
                                    .on_hover_text("Duplicar Sol")
                                    .clicked()
                                {
                                    dup_sun = Some(idx);
                                }
                            });
                        });
                    }
                    if let Some(idx) = toggle_sun_vis {
                        self.push_undo();
                        self.scene.sun_lights[idx].visible = !self.scene.sun_lights[idx].visible;
                    }
                    if let Some(idx) = toggle_sun_en {
                        self.push_undo();
                        self.scene.sun_lights[idx].enabled = !self.scene.sun_lights[idx].enabled;
                    }
                    if let Some(idx) = dup_sun {
                        self.push_undo();
                        let mut c = self.scene.sun_lights[idx].clone();
                        c.name = format!("{}_copia", c.name);
                        c.position[0] += 1.2;
                        self.scene.sun_lights.push(c);
                        self.selection = Some(Selection::Sun(self.scene.sun_lights.len() - 1));
                    }
                    if let Some(idx) = delete_sun {
                        self.push_undo();
                        self.scene.sun_lights.remove(idx);
                        self.selection = if !self.scene.sun_lights.is_empty() {
                            Some(Selection::Sun(idx.min(self.scene.sun_lights.len() - 1)))
                        } else if !self.scene.parts.is_empty() {
                            Some(Selection::Part(0))
                        } else {
                            None
                        };
                    }
                }
            });

        egui::CollapsingHeader::new(egui::RichText::new("Añadir primitiva").strong())
            .default_open(true)
            .show(ui, |ui| {
                let cols = if compact { 3 } else { 2 };
                let prim_w = if compact { 102.0 } else { 112.0 };
                let prim_h = if compact { 32.0 } else { 25.0 };
                egui::Grid::new("add_primitives_grid")
                    .num_columns(cols)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        for (i, kind) in PrimitiveKind::ALL.iter().enumerate() {
                            if icon_text_btn(ui, SvgIcon::Plus, 13.0, kind.label(), [prim_w, prim_h], false)
                                .clicked()
                            {
                                self.add_primitive(*kind);
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });

        egui::CollapsingHeader::new(egui::RichText::new("Herramientas de modelado").strong())
            .default_open(false)
            .show(ui, |ui| {
                let has_sel = matches!(self.selection, Some(Selection::Part(_)));
                ui.horizontal_wrapped(|ui| {
                    if icon_text_btn_enabled(ui, has_sel, SvgIcon::Stack, 14.0, "Apilar encima", [118.0, btn_h]).clicked() {
                        self.stack_selected_on_below();
                    }
                    if icon_text_btn(ui, SvgIcon::Merge, 14.0, "Combinar", [100.0, btn_h], false).clicked() {
                        self.merge_visible_parts();
                    }
                    if icon_text_btn_enabled(ui, has_sel, SvgIcon::Vertices, 14.0, "A Malla", [92.0, btn_h]).clicked() {
                        self.convert_selected_to_mesh();
                    }
                    if icon_text_btn_enabled(ui, has_sel, SvgIcon::Subdivide, 14.0, "Subdividir", [104.0, btn_h]).clicked() {
                        self.subdivide_selected();
                    }
                });
                ui.add_space(4.0);
                ui.add(egui::Slider::new(&mut self.deform_amount, -1.5..=1.5).text("Deformación"));
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(has_sel, egui::Button::new("Afilar").min_size(egui::vec2(68.0, btn_h))).clicked() {
                        self.deform_selected(MeshDeformKind::Taper);
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("Torcer").min_size(egui::vec2(68.0, btn_h))).clicked() {
                        self.deform_selected(MeshDeformKind::Twist);
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("Doblar").min_size(egui::vec2(68.0, btn_h))).clicked() {
                        self.deform_selected(MeshDeformKind::Bend);
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Matriz 3D / Clonar").size(11.5).strong());
                ui.horizontal(|ui| {
                    ui.label("Copias:");
                    ui.add(egui::DragValue::new(&mut self.array_count).range(1..=24));
                    ui.checkbox(&mut self.array_radial, "Radial Y");
                });
                if !self.array_radial {
                    ui.horizontal(|ui| {
                        ui.label("Paso:");
                        for axis in 0..3 {
                            ui.add_sized(
                                [46.0, 22.0],
                                egui::DragValue::new(&mut self.array_step[axis])
                                    .speed(0.05)
                                    .fixed_decimals(2),
                            );
                        }
                    });
                }
                if icon_text_btn_enabled(ui, has_sel, SvgIcon::Duplicate, 14.0, "Generar matriz", [124.0, btn_h]).clicked() {
                    self.array_clone_selected();
                }
            });

        // Groups Section
        let groups = self.scene.group_names();
        ui.add_space(6.0);
        egui::CollapsingHeader::new(egui::RichText::new(format!("Grupos ({})", groups.len())).strong())
            .default_open(!groups.is_empty())
            .show(ui, |ui| {
                if groups.is_empty() {
                    ui.label(
                        egui::RichText::new("Sin grupos · asigna uno en Editar")
                            .size(11.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                } else {
                    let mut toggle_group_vis: Option<String> = None;
                    let mut dup_group: Option<String> = None;
                    for g in &groups {
                        let count = self
                            .scene
                            .parts
                            .iter()
                            .filter(|p| p.group.as_deref() == Some(g.as_str()))
                            .count();
                        let all_vis = self
                            .scene
                            .parts
                            .iter()
                            .filter(|p| p.group.as_deref() == Some(g.as_str()))
                            .all(|p| p.visible);
                        let is_sel = self.selection.as_ref() == Some(&Selection::Group(g.clone()));
                        ui.horizontal(|ui| {
                            let eye_sz = if compact { [30.0, 28.0] } else { [24.0, 22.0] };
                            if icon_btn(ui, SvgIcon::Eye, 13.0, eye_sz, all_vis)
                                .on_hover_text("Mostrar/ocultar grupo")
                                .clicked()
                            {
                                toggle_group_vis = Some(g.clone());
                            }
                            if ui.selectable_label(is_sel, format!("{g} ({count})")).clicked() {
                                self.selection = Some(Selection::Group(g.clone()));
                                self.status_note = format!("Grupo '{g}' seleccionado");
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let dup_sz = if compact { [34.0, 28.0] } else { [26.0, 22.0] };
                                if icon_btn(ui, SvgIcon::Duplicate, 13.0, dup_sz, false)
                                    .on_hover_text("Duplicar grupo")
                                    .clicked()
                                {
                                    dup_group = Some(g.clone());
                                }
                            });
                        });
                    }
                    if let Some(g_name) = toggle_group_vis {
                        self.push_undo();
                        let target_vis = !self
                            .scene
                            .parts
                            .iter()
                            .filter(|p| p.group.as_deref() == Some(g_name.as_str()))
                            .all(|p| p.visible);
                        for p in &mut self.scene.parts {
                            if p.group.as_deref() == Some(g_name.as_str()) {
                                p.visible = target_vis;
                            }
                        }
                        *dirty = true;
                    }
                    if let Some(g_name) = dup_group {
                        self.push_undo();
                        let new_group_name = format!("{g_name}_copia");
                        let clones: Vec<PartSpec> = self
                            .scene
                            .parts
                            .iter()
                            .filter(|p| p.group.as_deref() == Some(g_name.as_str()))
                            .map(|p| {
                                let mut c = p.clone();
                                c.name = format!("{}_copia", c.name);
                                c.position[0] += 0.6;
                                c.group = Some(new_group_name.clone());
                                c
                            })
                            .collect();
                        self.scene.parts.extend(clones);
                        self.selection = Some(Selection::Group(new_group_name));
                        *dirty = true;
                    }
                }
            });

        // Parts list WITHOUT nested ScrollArea so it never gets crushed to 2 rows
        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new(format!("Piezas ({})", self.scene.parts.len())).strong());

        let mut to_delete: Option<usize> = None;
        let mut to_duplicate: Option<usize> = None;
        let mut vis_toggled = false;

        for (idx, part) in self.scene.parts.iter_mut().enumerate() {
            let is_sel = self.selection == Some(Selection::Part(idx));
            ui.horizontal(|ui| {
                let eye_sz = if compact { [30.0, 28.0] } else { [24.0, 22.0] };
                if icon_btn(ui, SvgIcon::Eye, 13.0, eye_sz, part.visible)
                    .on_hover_text("Mostrar/ocultar pieza")
                    .clicked()
                {
                    part.visible = !part.visible;
                    vis_toggled = true;
                }
                let mirror_tag = if part.mirror.iter().any(|&m| m) { " [M]" } else { "" };
                let phys_tag = match part.body_kind {
                    RigidBodyKind::Static => String::from(" [Fijo]"),
                    RigidBodyKind::Kinematic => String::from(" [Cinem.]"),
                    RigidBodyKind::Sensor => String::from(" [Sensor]"),
                    RigidBodyKind::Dynamic => {
                        if (part.mass - 1.0).abs() > 0.05 {
                            format!(" [{:.1}kg]", part.mass)
                        } else {
                            String::new()
                        }
                    }
                };
                let label = format!("{}{}{}", part.name, mirror_tag, phys_tag);
                if ui.selectable_label(is_sel, label).clicked() {
                    self.selection = Some(Selection::Part(idx));
                    self.selected_vertex_idx = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let act_sz = if compact { [32.0, 28.0] } else { [24.0, 22.0] };
                    if icon_btn(ui, SvgIcon::Trash, 13.0, act_sz, false)
                        .on_hover_text("Eliminar")
                        .clicked()
                    {
                        to_delete = Some(idx);
                    }
                    if icon_btn(ui, SvgIcon::Duplicate, 13.0, act_sz, false)
                        .on_hover_text("Duplicar")
                        .clicked()
                    {
                        to_duplicate = Some(idx);
                    }
                });
            });
        }

        if vis_toggled {
            *dirty = true;
        }
        if let Some(idx) = to_duplicate {
            self.push_undo();
            let mut cloned = self.scene.parts[idx].clone();
            cloned.name = format!("{}_copia", cloned.name);
            cloned.position[0] += 0.45;
            self.scene.parts.push(cloned);
            self.selection = Some(Selection::Part(self.scene.parts.len() - 1));
            *dirty = true;
        }
        if let Some(idx) = to_delete {
            self.push_undo();
            self.scene.parts.remove(idx);
            self.selection = if self.scene.parts.is_empty() {
                None
            } else {
                Some(Selection::Part(idx.min(self.scene.parts.len() - 1)))
            };
            self.selected_vertex_idx = None;
            *dirty = true;
        }
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui, dirty: &mut bool, compact: bool) {
        let btn_h = if compact { 32.0 } else { 24.0 };
        let selected_part = match &self.selection {
            Some(Selection::Part(i)) if *i < self.scene.parts.len() => Some(*i),
            _ => None,
        };

        if let Some(Selection::Sun(sun_idx)) = self.selection {
            if sun_idx < self.scene.sun_lights.len() {
                let mut changed_continuous = false;
                let mut sun_action: Option<&'static str> = None;
                {
                    let sun = &mut self.scene.sun_lights[sun_idx];
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Inspector de Sol").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_btn(ui, SvgIcon::Trash, 14.0, [30.0, btn_h], false)
                                .on_hover_text("Eliminar Sol")
                                .clicked()
                            {
                                sun_action = Some("delete");
                            }
                            if icon_btn(ui, SvgIcon::Duplicate, 14.0, [30.0, btn_h], false)
                                .on_hover_text("Duplicar Sol")
                                .clicked()
                            {
                                sun_action = Some("duplicate");
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Nombre:");
                        if ui.text_edit_singleline(&mut sun.name).changed() {
                            changed_continuous = true;
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        if ui.checkbox(&mut sun.enabled, "Sol encendido").changed() {
                            changed_continuous = true;
                        }
                        if ui.checkbox(&mut sun.visible, "Mostrar 3D").changed() {
                            changed_continuous = true;
                        }
                    });

                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Slider::new(&mut sun.intensity, 0.0..=5.0)
                                .fixed_decimals(2)
                                .text("Intensidad"),
                        )
                        .changed()
                    {
                        changed_continuous = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (lbl, val) in [
                            ("0.5x", 0.5_f32),
                            ("1.0x", 1.0),
                            ("1.5x", 1.5),
                            ("2.5x", 2.5),
                            ("4.0x", 4.0),
                        ] {
                            let active = (sun.intensity - val).abs() < 0.05;
                            if ui
                                .add_sized([46.0, btn_h], egui::Button::selectable(active, lbl))
                                .clicked()
                            {
                                sun.intensity = val;
                                changed_continuous = true;
                            }
                        }
                    });

                    if ui
                        .add(
                            egui::Slider::new(&mut sun.ambient, 0.0..=0.8)
                                .fixed_decimals(2)
                                .text("Luz ambiental"),
                        )
                        .changed()
                    {
                        changed_continuous = true;
                    }

                    ui.horizontal_wrapped(|ui| {
                        ui.label("Color luz:");
                        let mut rgba = sun.color;
                        if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                            sun.color = rgba;
                            changed_continuous = true;
                        }
                        for (lbl, col) in [
                            ("Mediodía", [255, 248, 232, 255]),
                            ("Cálido", [255, 224, 170, 255]),
                            ("Ocaso", [255, 160, 96, 255]),
                            ("Frío", [185, 220, 255, 255]),
                        ] {
                            if ui
                                .add_sized([68.0, btn_h], egui::Button::new(lbl))
                                .clicked()
                            {
                                sun.color = col;
                                changed_continuous = true;
                            }
                        }
                    });

                    ui.add_space(4.0);
                    let drag_w = if compact { 72.0 } else { 64.0 };
                    egui::Grid::new("sun_transform_grid")
                        .num_columns(4)
                        .spacing([6.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Pos");
                            for axis in 0..3 {
                                if ui
                                    .add_sized(
                                        [drag_w, if compact { 32.0 } else { 22.0 }],
                                        egui::DragValue::new(&mut sun.position[axis])
                                            .speed(0.05)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    changed_continuous = true;
                                }
                            }
                            ui.end_row();
                        });

                    if ui
                        .checkbox(&mut sun.aim_at_origin, "Apuntar siempre al origen (0,0,0)")
                        .changed()
                    {
                        changed_continuous = true;
                    }

                    if !sun.aim_at_origin {
                        egui::Grid::new("sun_dir_grid")
                            .num_columns(4)
                            .spacing([6.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Dir");
                                for axis in 0..3 {
                                    if ui
                                        .add_sized(
                                            [drag_w, if compact { 32.0 } else { 22.0 }],
                                            egui::DragValue::new(&mut sun.direction[axis])
                                                .speed(0.02)
                                                .fixed_decimals(2),
                                        )
                                        .changed()
                                    {
                                        changed_continuous = true;
                                    }
                                }
                                ui.end_row();
                            });
                        if ui
                            .add_sized([140.0, btn_h], egui::Button::new("Apuntar al centro"))
                            .clicked()
                        {
                            sun.aim_at_origin = true;
                            changed_continuous = true;
                        }
                    }
                }

                if changed_continuous {
                    self.begin_continuous_edit(ui);
                }
                self.flush_continuous_edit(ui);

                if let Some(act) = sun_action {
                    match act {
                        "duplicate" => {
                            self.push_undo();
                            let mut cloned = self.scene.sun_lights[sun_idx].clone();
                            cloned.name = format!("{}_copia", cloned.name);
                            cloned.position[0] += 1.2;
                            self.scene.sun_lights.push(cloned);
                            self.selection = Some(Selection::Sun(self.scene.sun_lights.len() - 1));
                        }
                        "delete" => {
                            self.push_undo();
                            self.scene.sun_lights.remove(sun_idx);
                            self.selection = if !self.scene.sun_lights.is_empty() {
                                Some(Selection::Sun(sun_idx.min(self.scene.sun_lights.len() - 1)))
                            } else if !self.scene.parts.is_empty() {
                                Some(Selection::Part(0))
                            } else {
                                None
                            };
                        }
                        _ => {}
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                self.render_model_physics_section(ui, dirty, compact);
                return;
            }
        }

        if let Some(Selection::Group(g_name)) = self.selection.clone() {
            let group_parts: Vec<usize> = self
                .scene
                .parts
                .iter()
                .enumerate()
                .filter_map(|(i, p)| (p.group.as_deref() == Some(g_name.as_str())).then_some(i))
                .collect();
            ui.label(egui::RichText::new(format!("Grupo: {g_name} ({} piezas)", group_parts.len())).strong());
            ui.label(
                egui::RichText::new("Usa el gizmo 3D para mover, rotar o escalar todo el grupo.")
                    .size(11.5)
                    .color(egui::Color32::from_gray(165)),
            );
            ui.horizontal_wrapped(|ui| {
                for &(name, col) in &QUICK_PALETTE {
                    let c32 = egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], col[3]);
                    let sz = if compact { 24.0 } else { 20.0 };
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::click());
                    ui.painter().rect_filled(rect, 3.0, c32);
                    if resp.on_hover_text(format!("Teñir grupo: {name}")).clicked() {
                        self.push_undo();
                        for &idx in &group_parts {
                            self.scene.parts[idx].color = col;
                        }
                        *dirty = true;
                    }
                }
            });
            ui.add_space(6.0);
            if icon_text_btn(ui, SvgIcon::Trash, 14.0, "Disolver grupo", [132.0, btn_h], false).clicked() {
                self.push_undo();
                for &idx in &group_parts {
                    self.scene.parts[idx].group = None;
                }
                self.selection = group_parts.first().copied().map(Selection::Part);
                *dirty = true;
            }
            ui.add_space(8.0);
            ui.separator();
            self.render_model_physics_section(ui, dirty, compact);
            return;
        }

        match selected_part {
            Some(sel_idx) => {
                let existing_groups = self.scene.group_names();
                let baked_part = self.scene.parts[sel_idx].bake();
                let part_verts = baked_part.vertices.len();
                let part_tris = baked_part.indices.len() / 3;
                let part_bounds = baked_part.bounds;
                let snap = self.snap_enabled;
                let mut changed_continuous = false;
                let mut discrete_action: Option<&'static str> = None;
                let mut discrete_shading_change: Option<ShadingMode> = None;
                let mut discrete_dedup_toggle: Option<bool> = None;
                let mut discrete_mirror_toggle: Option<usize> = None;
                let mut new_group_assign: Option<Option<String>> = None;
                let mut discrete_profile_replace: Option<(Vec<[f32; 2]>, Option<usize>, &'static str)> = None;
                let mut discrete_collider_change: Option<ColliderShape> = None;
                let mut discrete_autofit_toggle: Option<bool> = None;
                let mut fill_color_group: Option<[u8; 4]> = None;
                let mut fill_color_all: Option<[u8; 4]> = None;
                let mut copy_color_req: Option<[u8; 4]> = None;
                let mut copy_mat_req: Option<([u8; 4], f32, f32, f32)> = None;
                let mut discrete_mat_preset: Option<MaterialPreset> = None;
                let copied_col_snapshot = self.copied_color;
                let copied_mat_snapshot = self.copied_material;
                let mut selected_pt = self.selected_profile_pt;
                let mut dragging_pt = self.dragging_profile_pt;
                let mut sculpt_raise = self.terrain_sculpt_raise;
                let mut brush_radius = self.terrain_brush_radius;
                let mut brush_strength = self.terrain_brush_strength;
                let mut sculpt_last = self.terrain_sculpt_last;
                let mut sel_vert_idx = self.selected_vertex_idx;
                let mut global_tex_on = self.textures_enabled;
                let mut tex_action: Option<&'static str> = None;
                let mut discrete_part_tex_toggle: Option<bool> = None;
                let mut discrete_part_tex_idx: Option<Option<usize>> = None;
                let tex_choices: Vec<(usize, String, u16, u16)> = self
                    .imported_textures
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (i, t.name.clone(), t.width, t.height))
                    .collect();

                {
                    let part = &mut self.scene.parts[sel_idx];
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Inspector de pieza").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_btn(ui, SvgIcon::Trash, 14.0, [30.0, btn_h], false)
                                .on_hover_text("Eliminar pieza")
                                .clicked()
                            {
                                discrete_action = Some("delete");
                            }
                            if icon_btn(ui, SvgIcon::Duplicate, 14.0, [30.0, btn_h], false)
                                .on_hover_text("Duplicar pieza")
                                .clicked()
                            {
                                discrete_action = Some("duplicate");
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Nombre:");
                        if ui.text_edit_singleline(&mut part.name).changed() {
                            *dirty = true;
                        }
                    });

                    // Per-part RigidBodyKind selector
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Física:");
                        for kind in RigidBodyKind::ALL {
                            if ui
                                .add_sized([74.0, btn_h], egui::Button::selectable(part.body_kind == kind, kind.label()))
                                .clicked()
                                && part.body_kind != kind
                            {
                                part.body_kind = kind;
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }
                    });

                    ui.add_space(4.0);
                    let drag_w = if compact { 72.0 } else { 64.0 };
                    egui::Grid::new("transform_grid").num_columns(4).spacing([6.0, 6.0]).show(ui, |ui| {
                        ui.label("Pos");
                        for axis in 0..3 {
                            let resp = ui.add_sized(
                                [drag_w, if compact { 32.0 } else { 22.0 }],
                                egui::DragValue::new(&mut part.position[axis]).speed(0.03).fixed_decimals(2),
                            );
                            if resp.changed() {
                                if snap {
                                    part.position[axis] = Self::snap_step(part.position[axis], 0.25);
                                }
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }
                        ui.end_row();

                        ui.label("Rot");
                        for axis in 0..3 {
                            let resp = ui.add_sized(
                                [drag_w, if compact { 32.0 } else { 22.0 }],
                                egui::DragValue::new(&mut part.rotation_deg[axis]).speed(1.0).suffix("°"),
                            );
                            if resp.changed() {
                                if snap {
                                    part.rotation_deg[axis] = Self::snap_step(part.rotation_deg[axis], 15.0);
                                }
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }
                        ui.end_row();

                        ui.label("Esc");
                        for axis in 0..3 {
                            let resp = ui.add_sized(
                                [drag_w, if compact { 32.0 } else { 22.0 }],
                                egui::DragValue::new(&mut part.scale[axis])
                                    .speed(0.02)
                                    .range(0.02..=50.0)
                                    .fixed_decimals(2),
                            );
                            if resp.changed() {
                                if snap {
                                    part.scale[axis] = Self::snap_step(part.scale[axis], 0.25).max(0.25);
                                }
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }
                        ui.end_row();
                    });

                    // Vertex editing controls when CustomMesh
                    if part.kind == PrimitiveKind::CustomMesh {
                        if let Some(mesh) = part.custom_mesh.as_mut() {
                            if !mesh.vertices.is_empty() {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(format!("Vértices editables ({})", mesh.vertices.len())).strong());
                                let max_v = mesh.vertices.len().saturating_sub(1);
                                let mut v_idx = sel_vert_idx.unwrap_or(0).min(max_v);
                                ui.horizontal(|ui| {
                                    ui.label("Índice:");
                                    if ui.add(egui::DragValue::new(&mut v_idx).range(0..=max_v)).changed() {
                                        sel_vert_idx = Some(v_idx);
                                    }
                                });
                                let anchor = mesh.vertices[v_idx].position;
                                let mut edited = anchor;
                                ui.horizontal(|ui| {
                                    ui.label("V:");
                                    for axis in 0..3 {
                                        if ui
                                            .add_sized(
                                                [58.0, 22.0],
                                                egui::DragValue::new(&mut edited[axis])
                                                    .speed(0.02)
                                                    .fixed_decimals(2),
                                            )
                                            .changed()
                                        {
                                            let dx = edited[0] - anchor[0];
                                            let dy = edited[1] - anchor[1];
                                            let dz = edited[2] - anchor[2];
                                            for v in &mut mesh.vertices {
                                                if (v.position[0] - anchor[0]).abs() < 1e-4
                                                    && (v.position[1] - anchor[1]).abs() < 1e-4
                                                    && (v.position[2] - anchor[2]).abs() < 1e-4
                                                {
                                                    v.position[0] += dx;
                                                    v.position[1] += dy;
                                                    v.position[2] += dz;
                                                }
                                            }
                                            recompute_mesh_normals(mesh);
                                            changed_continuous = true;
                                            *dirty = true;
                                        }
                                    }
                                });
                                sel_vert_idx = Some(v_idx);
                            }
                        }
                    }

                    let mut uni_scale = (part.scale[0] + part.scale[1] + part.scale[2]) / 3.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Escala U:");
                        let prev_uni = uni_scale;
                        if ui
                            .add_sized(
                                [62.0, if compact { 30.0 } else { 22.0 }],
                                egui::DragValue::new(&mut uni_scale)
                                    .speed(0.02)
                                    .range(0.02..=50.0)
                                    .fixed_decimals(2),
                            )
                            .changed()
                            && prev_uni > 1e-4
                        {
                            let ratio = uni_scale / prev_uni;
                            for axis in 0..3 {
                                part.scale[axis] = (part.scale[axis] * ratio).clamp(0.02, 50.0);
                            }
                            changed_continuous = true;
                            *dirty = true;
                        }
                        if ui.add_sized([84.0, btn_h], egui::Button::new("Al suelo Y=0")).clicked() {
                            discrete_action = Some("ground");
                        }
                        if icon_text_btn(ui, SvgIcon::Stack, 13.0, "Apilar", [74.0, btn_h], false).clicked() {
                            discrete_action = Some("stack");
                        }
                        if ui.add_sized([74.0, btn_h], egui::Button::new("Centrar XZ")).clicked() {
                            discrete_action = Some("center_xz");
                        }
                        if ui.add_sized([84.0, btn_h], egui::Button::new("Ajustar 2m")).clicked() {
                            discrete_action = Some("normalize_2m");
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Color:");
                        let mut rgba = [part.color[0], part.color[1], part.color[2], part.color[3]];
                        if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                            part.color = rgba;
                            changed_continuous = true;
                            *dirty = true;
                        }
                        if icon_text_btn(ui, SvgIcon::Copy, 13.0, "Copiar", [74.0, btn_h], false).clicked() {
                            copy_color_req = Some(part.color);
                            copy_mat_req = Some((part.color, part.metallic, part.roughness, part.alpha_cutoff));
                        }
                        if ui
                            .add_enabled(
                                copied_col_snapshot.is_some(),
                                egui::Button::new("Pegar").min_size(egui::vec2(58.0, btn_h)),
                            )
                            .clicked()
                        {
                            discrete_action = Some("paste_color");
                        }
                        if ui
                            .add_enabled(
                                copied_mat_snapshot.is_some(),
                                egui::Button::new("Pegar Mat").min_size(egui::vec2(76.0, btn_h)),
                            )
                            .clicked()
                        {
                            discrete_action = Some("paste_material");
                        }
                    });

                    let sz = if compact { 22.0 } else { 18.0 };
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for &(name, col) in &QUICK_PALETTE {
                            let c32 = egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], col[3]);
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::click());
                            ui.painter().rect_filled(rect, 3.0, c32);
                            ui.painter().rect_stroke(
                                rect,
                                3.0,
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 78, 92)),
                                egui::StrokeKind::Inside,
                            );
                            if resp.on_hover_text(name).clicked() {
                                part.color = col;
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        if part.group.is_some()
                            && ui.add_sized([108.0, btn_h], egui::Button::new("Rellenar grupo")).clicked()
                        {
                            fill_color_group = Some(part.color);
                        }
                        if ui.add_sized([104.0, btn_h], egui::Button::new("Rellenar todo")).clicked() {
                            fill_color_all = Some(part.color);
                        }
                        if part.kind == PrimitiveKind::CustomMesh {
                            if let Some(orig) = part.custom_mesh.as_ref().map(|m| m.material.base_color) {
                                if orig != part.color
                                    && ui.add_sized([108.0, btn_h], egui::Button::new("Color original")).clicked()
                                {
                                    discrete_action = Some("restore_original_color");
                                }
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Seg:");
                        let max_seg = if part.kind == PrimitiveKind::Terrain { 64 } else { 48 };
                        if ui.add(egui::DragValue::new(&mut part.segments).range(6..=max_seg)).changed() {
                            changed_continuous = true;
                            *dirty = true;
                        }
                    });

                    if ui.add(egui::Slider::new(&mut part.metallic, 0.0..=1.0).text("Metálico")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    if ui.add(egui::Slider::new(&mut part.roughness, 0.05..=1.0).text("Rugosidad")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    let mut alpha_pct = part.color[3] as f32 / 255.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut alpha_pct, 0.05..=1.0)
                                .fixed_decimals(2)
                                .text("Opacidad"),
                        )
                        .changed()
                    {
                        part.color[3] = (alpha_pct * 255.0 + 0.5).clamp(12.0, 255.0) as u8;
                        changed_continuous = true;
                        *dirty = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        let mut cutout_on = part.alpha_cutoff > 0.001;
                        if ui
                            .checkbox(&mut cutout_on, "Alpha Cutout")
                            .on_hover_text("Recorta píxeles transparentes sin ordenamiento Alpha Blend")
                            .changed()
                        {
                            part.alpha_cutoff = if cutout_on { 0.5 } else { 0.0 };
                            changed_continuous = true;
                            *dirty = true;
                        }
                        if part.alpha_cutoff > 0.001
                            && ui
                                .add(
                                    egui::Slider::new(&mut part.alpha_cutoff, 0.05..=0.95)
                                        .fixed_decimals(2)
                                        .text("Umbral"),
                                )
                                .changed()
                        {
                            changed_continuous = true;
                            *dirty = true;
                        }
                    });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Biblioteca de materiales PBR")
                            .size(11.5)
                            .color(egui::Color32::from_gray(190)),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for preset in MaterialPreset::ALL {
                            let is_active = part.color == preset.material.base_color
                                && (part.metallic - preset.material.metallic).abs() < 0.03
                                && (part.roughness - preset.material.roughness).abs() < 0.03;
                            let btn_w = if compact { 96.0 } else { 88.0 };
                            let resp = ui.add_sized(
                                [btn_w, btn_h],
                                egui::Button::selectable(is_active, preset.label),
                            );
                            if resp
                                .on_hover_text(format!(
                                    "{} · Metálico {:.2} · Rugosidad {:.2} · Alpha {}",
                                    preset.label,
                                    preset.material.metallic,
                                    preset.material.roughness,
                                    preset.material.base_color[3]
                                ))
                                .clicked()
                            {
                                discrete_mat_preset = Some(preset);
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Textura del modelo y pieza").strong());
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut global_tex_on, "Activar texturas (modelo)");
                        let mut part_tex = part.texture_enabled;
                        if ui.checkbox(&mut part_tex, "En esta pieza").changed() {
                            discrete_part_tex_toggle = Some(part_tex);
                        }
                    });
                    let cur_tex_label = match part.texture_index.and_then(|ti| tex_choices.get(ti)) {
                        Some((_, name, w, h)) => format!("{name} ({w}x{h})"),
                        None if !tex_choices.is_empty() => format!("Auto ({})", tex_choices[0].1),
                        None => String::from("Por defecto (Damero 16x16)"),
                    };
                    ui.horizontal(|ui| {
                        ui.label("Mapa:");
                        egui::ComboBox::from_id_salt("part_texture_combo")
                            .selected_text(cur_tex_label)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(part.texture_index.is_none(), "Por defecto / Auto")
                                    .clicked()
                                {
                                    discrete_part_tex_idx = Some(None);
                                }
                                for (ti, name, w, h) in &tex_choices {
                                    let active = part.texture_index == Some(*ti);
                                    if ui
                                        .selectable_label(active, format!("{name} ({w}x{h})"))
                                        .clicked()
                                    {
                                        discrete_part_tex_idx = Some(Some(*ti));
                                    }
                                }
                            });
                    });
                    ui.horizontal_wrapped(|ui| {
                        if icon_text_btn(ui, SvgIcon::Texture, 13.0, "Generar textura", [118.0, btn_h], false)
                            .clicked()
                        {
                            tex_action = Some("generate_tex");
                        }
                        if icon_text_btn(ui, SvgIcon::Upload, 13.0, "Cargar imagen", [112.0, btn_h], false)
                            .on_hover_text("Importa una textura .png, .jpg o .f3t")
                            .clicked()
                        {
                            tex_action = Some("load_tex");
                        }
                        if ui
                            .add_sized([114.0, btn_h], egui::Button::new("Aplicar a todas"))
                            .clicked()
                        {
                            tex_action = Some("apply_tex_all");
                        }
                    });

                    ui.add_space(4.0);
                    egui::Grid::new("uv_transform_grid")
                        .num_columns(4)
                        .spacing([6.0, 4.0])
                        .show(ui, |ui| {
                            let uv_w = if compact { 66.0 } else { 58.0 };
                            let uv_h = if compact { 28.0 } else { 22.0 };
                            ui.label("Tiling UV");
                            for axis in 0..2 {
                                if ui
                                    .add_sized(
                                        [uv_w, uv_h],
                                        egui::DragValue::new(&mut part.uv_scale[axis])
                                            .speed(0.05)
                                            .range(0.05..=32.0)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            }
                            if ui.add_sized([58.0, uv_h], egui::Button::new("Reset")).clicked() {
                                part.uv_scale = [1.0, 1.0];
                                part.uv_offset = [0.0, 0.0];
                                changed_continuous = true;
                                *dirty = true;
                            }
                            ui.end_row();

                            ui.label("Offset UV");
                            for axis in 0..2 {
                                if ui
                                    .add_sized(
                                        [uv_w, uv_h],
                                        egui::DragValue::new(&mut part.uv_offset[axis])
                                            .speed(0.02)
                                            .range(-16.0..=16.0)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            }
                            ui.label("");
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Sombreado:");
                        for mode in ShadingMode::ALL {
                            if ui
                                .add_sized([68.0, btn_h], egui::Button::selectable(part.shading == mode, mode.label()))
                                .clicked()
                                && part.shading != mode
                            {
                                discrete_shading_change = Some(mode);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut dedup = part.deduplicate;
                        let dedup_enabled = part.shading != ShadingMode::Flat;
                        if ui
                            .add_enabled(dedup_enabled, egui::Checkbox::new(&mut dedup, "Deduplicar vértices"))
                            .changed()
                        {
                            discrete_dedup_toggle = Some(dedup);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{part_verts} v · {part_tris} tri"))
                                    .size(11.0)
                                    .monospace()
                                    .color(egui::Color32::from_gray(165)),
                            );
                        });
                    });

                    if part.kind == PrimitiveKind::Terrain {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(egui::RichText::new("Editor de Terreno 3D").strong());
                        ui.horizontal_wrapped(|ui| {
                            for preset in TerrainPreset::ALL {
                                if ui
                                    .add_sized(
                                        [78.0, btn_h],
                                        egui::Button::selectable(part.terrain.preset == preset, preset.label()),
                                    )
                                    .clicked()
                                    && part.terrain.preset != preset
                                {
                                    part.terrain.preset = preset;
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            }
                        });
                        if ui.add(egui::Slider::new(&mut part.terrain.amplitude, 0.0..=2.5).text("Relieve")).changed() {
                            changed_continuous = true;
                            *dirty = true;
                        }
                        if ui.add(egui::Slider::new(&mut part.terrain.frequency, 0.5..=8.0).text("Frecuencia")).changed() {
                            changed_continuous = true;
                            *dirty = true;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Octavas:");
                            if ui.add(egui::DragValue::new(&mut part.terrain.octaves).range(1..=6)).changed() {
                                changed_continuous = true;
                                *dirty = true;
                            }
                            ui.label("Semilla:");
                            if ui.add(egui::DragValue::new(&mut part.terrain.seed).speed(1.0)).changed() {
                                changed_continuous = true;
                                *dirty = true;
                            }
                            if ui.add_sized([84.0, btn_h], egui::Button::new("Aleatorizar")).clicked() {
                                part.terrain.seed = part.terrain.seed.wrapping_mul(1664525).wrapping_add(1013904223);
                                changed_continuous = true;
                                *dirty = true;
                            }
                        });
                        if ui.checkbox(&mut part.terrain.biome_tint, "Colorear por elevación (bioma)").changed() {
                            changed_continuous = true;
                            *dirty = true;
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if icon_text_btn(ui, SvgIcon::Plus, 13.0, "Elevar", [78.0, btn_h], sculpt_raise).clicked() {
                                sculpt_raise = true;
                            }
                            if icon_text_btn(ui, SvgIcon::Minus, 13.0, "Hundir", [78.0, btn_h], !sculpt_raise).clicked() {
                                sculpt_raise = false;
                            }
                            if ui
                                .add_enabled(
                                    !part.terrain.stamps.is_empty(),
                                    egui::Button::new("Deshacer trazo").min_size(egui::vec2(96.0, btn_h)),
                                )
                                .clicked()
                            {
                                part.terrain.stamps.pop();
                                changed_continuous = true;
                                *dirty = true;
                            }
                            if ui
                                .add_enabled(
                                    !part.terrain.stamps.is_empty(),
                                    egui::Button::new("Limpiar").min_size(egui::vec2(64.0, btn_h)),
                                )
                                .clicked()
                            {
                                part.terrain.stamps.clear();
                                changed_continuous = true;
                                *dirty = true;
                            }
                        });
                        ui.add(egui::Slider::new(&mut brush_radius, 0.06..=0.45).text("Radio"));
                        ui.add(egui::Slider::new(&mut brush_strength, 0.05..=0.80).text("Fuerza"));

                        let pad_w = ui.available_width().clamp(180.0, 260.0);
                        let pad_h = (pad_w * 0.65).clamp(115.0, 170.0);
                        let (pad_resp, painter) =
                            ui.allocate_painter(egui::vec2(pad_w, pad_h), egui::Sense::click_and_drag());
                        let prect = pad_resp.rect;
                        let grid_n = 20usize;
                        let cw = prect.width() / grid_n as f32;
                        let ch = prect.height() / grid_n as f32;
                        let mut hs = Vec::with_capacity(grid_n * grid_n);
                        let mut min_h = f32::INFINITY;
                        let mut max_h = f32::NEG_INFINITY;
                        for gz in 0..grid_n {
                            let nz = (gz as f32 + 0.5) / grid_n as f32 - 0.5;
                            for gx in 0..grid_n {
                                let nx = (gx as f32 + 0.5) / grid_n as f32 - 0.5;
                                let h = terrain_height_at(&part.terrain, nx, nz);
                                min_h = min_h.min(h);
                                max_h = max_h.max(h);
                                hs.push(h);
                            }
                        }
                        let span = (max_h - min_h).max(1e-4);
                        for gz in 0..grid_n {
                            for gx in 0..grid_n {
                                let t = ((hs[gz * grid_n + gx] - min_h) / span).clamp(0.0, 1.0);
                                let r = (36.0 + t * 175.0) as u8;
                                let g = (62.0 + t * 155.0) as u8;
                                let b = (48.0 + t * 120.0) as u8;
                                let cell = egui::Rect::from_min_size(
                                    egui::pos2(prect.left() + gx as f32 * cw, prect.top() + gz as f32 * ch),
                                    egui::vec2(cw + 0.6, ch + 0.6),
                                );
                                painter.rect_filled(cell, 0.0, egui::Color32::from_rgb(r, g, b));
                            }
                        }
                        painter.rect_stroke(
                            prect,
                            4.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(72, 82, 98)),
                            egui::StrokeKind::Inside,
                        );
                        if (pad_resp.clicked() || pad_resp.dragged()) && pad_resp.interact_pointer_pos().is_some() {
                            if let Some(ptr) = pad_resp.interact_pointer_pos() {
                                let nx = ((ptr.x - prect.left()) / prect.width()).clamp(0.0, 1.0) - 0.5;
                                let nz = ((ptr.y - prect.top()) / prect.height()).clamp(0.0, 1.0) - 0.5;
                                let should_stamp = if pad_resp.clicked() {
                                    true
                                } else if let Some(prev) = sculpt_last {
                                    let dx = nx - prev[0];
                                    let dz = nz - prev[1];
                                    (dx * dx + dz * dz).sqrt() >= (brush_radius * 0.28)
                                } else {
                                    true
                                };
                                if should_stamp {
                                    let signed_h = if sculpt_raise { brush_strength } else { -brush_strength };
                                    part.terrain.stamps.push(TerrainStamp {
                                        x: nx,
                                        z: nz,
                                        radius: brush_radius,
                                        height: signed_h,
                                    });
                                    sculpt_last = Some([nx, nz]);
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            }
                        }
                        if pad_resp.drag_stopped() {
                            sculpt_last = None;
                        }
                    }

                    if part.kind.uses_profile() {
                        ui.add_space(6.0);
                        ui.separator();
                        let title = if part.kind == PrimitiveKind::Extrude {
                            "Editor de Perfil 2D (Extrusión)"
                        } else {
                            "Editor de Perfil 2D (Revolución Y)"
                        };
                        ui.label(egui::RichText::new(title).strong());

                        if part.kind == PrimitiveKind::Extrude {
                            let resp = ui.add(egui::Slider::new(&mut part.extrude_depth, 0.05..=3.0).text("Profundidad Z"));
                            if resp.changed() {
                                if snap {
                                    part.extrude_depth = Self::snap_step(part.extrude_depth, 0.25).max(0.25);
                                }
                                changed_continuous = true;
                                *dirty = true;
                            }
                        }

                        if part.profile.is_empty() {
                            part.profile = if part.kind == PrimitiveKind::Lathe {
                                default_lathe_profile()
                            } else {
                                default_extrude_profile()
                            };
                        }
                        selected_pt = Some(selected_pt.unwrap_or(0).min(part.profile.len().saturating_sub(1)));

                        let avail_w = ui.available_width().max(220.0);
                        let canvas_h = if compact { 190.0 } else { 166.0 };
                        let (canvas_resp, painter) =
                            ui.allocate_painter(egui::vec2(avail_w, canvas_h), egui::Sense::click_and_drag());
                        let crect = canvas_resp.rect;
                        let is_lathe = part.kind == PrimitiveKind::Lathe;
                        let scale = if is_lathe {
                            (crect.height() * 0.43).min(crect.width() * 0.64 / 0.95).max(36.0)
                        } else {
                            (crect.width().min(crect.height()) * 0.44).max(36.0)
                        };
                        let origin = if is_lathe {
                            egui::pos2(crect.left() + crect.width() * 0.28, crect.center().y)
                        } else {
                            crect.center()
                        };
                        let to_px = |p: [f32; 2]| -> egui::Pos2 {
                            egui::pos2(origin.x + p[0] * scale, origin.y - p[1] * scale)
                        };
                        let to_2d = |px: egui::Pos2| -> [f32; 2] {
                            let mut x = (px.x - origin.x) / scale;
                            let mut y = (origin.y - px.y) / scale;
                            x = if is_lathe { x.clamp(0.0, 0.95) } else { x.clamp(-0.95, 0.95) };
                            y = y.clamp(-0.95, 0.95);
                            if snap {
                                x = Self::snap_step(x, 0.05);
                                y = Self::snap_step(y, 0.05);
                                if is_lathe {
                                    x = x.max(0.0);
                                }
                            }
                            [x, y]
                        };
                        painter.rect_filled(crect, 6.0, egui::Color32::from_rgb(11, 14, 20));
                        painter.rect_stroke(
                            crect,
                            6.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 48, 60)),
                            egui::StrokeKind::Inside,
                        );
                        let hit_r = if compact { 20.0 } else { 13.0 };
                        if let Some(pointer) = canvas_resp.interact_pointer_pos() {
                            if canvas_resp.drag_started() || canvas_resp.clicked() {
                                let mut best = None;
                                let mut best_d = hit_r;
                                for (i, &pt) in part.profile.iter().enumerate() {
                                    let d = pointer.distance(to_px(pt));
                                    if d < best_d {
                                        best_d = d;
                                        best = Some(i);
                                    }
                                }
                                if let Some(hit_idx) = best {
                                    selected_pt = Some(hit_idx);
                                    if canvas_resp.drag_started() {
                                        dragging_pt = Some(hit_idx);
                                    }
                                }
                            }
                        }
                        if canvas_resp.dragged() {
                            if let (Some(idx), Some(pointer)) = (dragging_pt, canvas_resp.interact_pointer_pos()) {
                                if idx < part.profile.len() {
                                    part.profile[idx] = to_2d(pointer);
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            }
                        }
                        if canvas_resp.drag_stopped() {
                            dragging_pt = None;
                        }
                        let screen_pts: Vec<egui::Pos2> = part.profile.iter().map(|&p| to_px(p)).collect();
                        let edge_col = egui::Color32::from_rgb(56, 189, 248);
                        if is_lathe {
                            for w in screen_pts.windows(2) {
                                painter.line_segment([w[0], w[1]], egui::Stroke::new(2.0, edge_col));
                            }
                        } else if screen_pts.len() >= 2 {
                            for i in 0..screen_pts.len() {
                                painter.line_segment(
                                    [screen_pts[i], screen_pts[(i + 1) % screen_pts.len()]],
                                    egui::Stroke::new(2.0, edge_col),
                                );
                            }
                        }
                        let pt_r = if compact { 6.5 } else { 4.8 };
                        for (i, &pos_px) in screen_pts.iter().enumerate() {
                            let fill = if selected_pt == Some(i) {
                                egui::Color32::from_rgb(251, 191, 36)
                            } else {
                                egui::Color32::from_rgb(226, 232, 240)
                            };
                            painter.circle_filled(pos_px, pt_r, fill);
                        }

                        let min_pts = if is_lathe { 2 } else { 3 };
                        ui.horizontal_wrapped(|ui| {
                            if icon_text_btn(ui, SvgIcon::Plus, 13.0, "Punto", [74.0, btn_h], false).clicked() {
                                let mut next_prof = part.profile.clone();
                                let cur = selected_pt.unwrap_or(0).min(next_prof.len() - 1);
                                let next_i = if is_lathe {
                                    (cur + 1).min(next_prof.len() - 1)
                                } else {
                                    (cur + 1) % next_prof.len()
                                };
                                let mid = [
                                    (next_prof[cur][0] + next_prof[next_i][0]) * 0.5,
                                    (next_prof[cur][1] + next_prof[next_i][1]) * 0.5,
                                ];
                                let insert_at = cur + 1;
                                next_prof.insert(insert_at, mid);
                                discrete_profile_replace = Some((next_prof, Some(insert_at), "Punto añadido al perfil"));
                            }
                            if icon_text_btn_enabled(ui, part.profile.len() > min_pts, SvgIcon::Minus, 13.0, "Punto", [74.0, btn_h]).clicked() {
                                let mut next_prof = part.profile.clone();
                                let cur = selected_pt.unwrap_or(0).min(next_prof.len() - 1);
                                next_prof.remove(cur);
                                let new_sel = cur.min(next_prof.len() - 1);
                                discrete_profile_replace = Some((next_prof, Some(new_sel), "Punto eliminado del perfil"));
                            }
                        });
                    }

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Simetría:");
                        for (axis, lbl) in [(0, "Espejo X"), (1, "Espejo Y"), (2, "Espejo Z")] {
                            if ui.add_sized([74.0, btn_h], egui::Button::selectable(part.mirror[axis], lbl)).clicked() {
                                discrete_mirror_toggle = Some(axis);
                            }
                        }
                    });

                    ui.add_space(4.0);
                    let current_group_label = part.group.clone().unwrap_or_else(|| String::from("Sin grupo"));
                    ui.horizontal(|ui| {
                        ui.label("Grupo:");
                        egui::ComboBox::from_id_salt("part_group_combo")
                            .selected_text(current_group_label)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(part.group.is_none(), "Sin grupo").clicked() {
                                    new_group_assign = Some(None);
                                }
                                for g in &existing_groups {
                                    let active = part.group.as_deref() == Some(g.as_str());
                                    if ui.selectable_label(active, g).clicked() {
                                        new_group_assign = Some(Some(g.clone()));
                                    }
                                }
                            });
                    });

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Colisionador de pieza").strong());
                    ui.horizontal_wrapped(|ui| {
                        let is_none = matches!(part.collider, ColliderShape::None);
                        let is_box = matches!(part.collider, ColliderShape::Box { .. });
                        let is_sphere = matches!(part.collider, ColliderShape::Sphere { .. });
                        let is_capsule = matches!(part.collider, ColliderShape::Capsule { .. });
                        if ui.add_sized([68.0, btn_h], egui::Button::selectable(is_none, "Ninguno")).clicked() && !is_none {
                            discrete_collider_change = Some(ColliderShape::None);
                        }
                        if ui.add_sized([54.0, btn_h], egui::Button::selectable(is_box, "Caja")).clicked() && !is_box {
                            discrete_collider_change = Some(ColliderShape::Box { half_extents: [0.5; 3] });
                        }
                        if ui.add_sized([60.0, btn_h], egui::Button::selectable(is_sphere, "Esfera")).clicked() && !is_sphere {
                            discrete_collider_change = Some(ColliderShape::Sphere { radius: 0.5 });
                        }
                        if ui.add_sized([68.0, btn_h], egui::Button::selectable(is_capsule, "Cápsula")).clicked() && !is_capsule {
                            discrete_collider_change = Some(ColliderShape::Capsule { radius: 0.35, half_height: 0.4 });
                        }
                    });
                    if !matches!(part.collider, ColliderShape::None) {
                        let mut auto_fit = part.collider_auto_fit;
                        if ui.checkbox(&mut auto_fit, "Auto-ajustar a geometría").changed() {
                            discrete_autofit_toggle = Some(auto_fit);
                        }
                    }

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Propiedades físicas de esta pieza").strong());
                    if ui.add(egui::Slider::new(&mut part.mass, 0.05..=200.0).logarithmic(true).text("Masa (kg)")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    if ui.add(egui::Slider::new(&mut part.friction, 0.0..=1.5).text("Fricción")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    if ui.add(egui::Slider::new(&mut part.restitution, 0.0..=1.0).text("Rebote")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    if ui.add(egui::Slider::new(&mut part.gravity_scale, -2.0..=3.0).text("Escala gravedad")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                    if ui.add(egui::Slider::new(&mut part.linear_damping, 0.0..=2.0).text("Amortiguación")).changed() {
                        changed_continuous = true;
                        *dirty = true;
                    }
                }

                self.selected_profile_pt = selected_pt;
                self.dragging_profile_pt = dragging_pt;
                self.terrain_sculpt_raise = sculpt_raise;
                self.terrain_brush_radius = brush_radius;
                self.terrain_brush_strength = brush_strength;
                self.terrain_sculpt_last = sculpt_last;
                self.selected_vertex_idx = sel_vert_idx;

                if let Some(c) = copy_color_req {
                    self.copied_color = Some(c);
                    self.status_note = format!("Color copiado ({}, {}, {})", c[0], c[1], c[2]);
                }
                if let Some(m) = copy_mat_req {
                    self.copied_material = Some(m);
                }
                if changed_continuous {
                    self.begin_continuous_edit(ui);
                }
                self.flush_continuous_edit(ui);

                if let Some(col) = fill_color_group {
                    if let Some(g_name) = self.scene.parts[sel_idx].group.clone() {
                        self.push_undo();
                        for p in &mut self.scene.parts {
                            if p.group.as_deref() == Some(g_name.as_str()) {
                                p.color = col;
                            }
                        }
                        *dirty = true;
                    }
                }
                if let Some(col) = fill_color_all {
                    self.push_undo();
                    for p in &mut self.scene.parts {
                        p.color = col;
                    }
                    *dirty = true;
                }
                if let Some(act) = discrete_action {
                    match act {
                        "paste_color" => {
                            if let Some(c) = self.copied_color {
                                self.push_undo();
                                self.scene.parts[sel_idx].color = c;
                                *dirty = true;
                            }
                        }
                        "paste_material" => {
                            if let Some((c, met, rough, cutoff)) = self.copied_material {
                                self.push_undo();
                                self.scene.parts[sel_idx].color = c;
                                self.scene.parts[sel_idx].metallic = met;
                                self.scene.parts[sel_idx].roughness = rough;
                                self.scene.parts[sel_idx].alpha_cutoff = cutoff;
                                *dirty = true;
                            }
                        }
                        "restore_original_color" => {
                            if let Some(orig) = self.scene.parts[sel_idx]
                                .custom_mesh
                                .as_ref()
                                .map(|m| m.material.base_color)
                            {
                                self.push_undo();
                                self.scene.parts[sel_idx].color = orig;
                                *dirty = true;
                            }
                        }
                        "ground" => {
                            if part_bounds.is_valid() {
                                self.push_undo();
                                self.scene.parts[sel_idx].position[1] -= part_bounds.min[1];
                                *dirty = true;
                            }
                        }
                        "stack" => self.stack_selected_on_below(),
                        "center_xz" => {
                            self.push_undo();
                            self.scene.parts[sel_idx].position[0] = 0.0;
                            self.scene.parts[sel_idx].position[2] = 0.0;
                            *dirty = true;
                        }
                        "normalize_2m" => {
                            if part_bounds.is_valid() {
                                let max_dim = (part_bounds.max[0] - part_bounds.min[0])
                                    .max(part_bounds.max[1] - part_bounds.min[1])
                                    .max(part_bounds.max[2] - part_bounds.min[2]);
                                if max_dim > 1e-4 {
                                    let factor = 2.0 / max_dim;
                                    self.push_undo();
                                    let p = &mut self.scene.parts[sel_idx];
                                    for axis in 0..3 {
                                        p.scale[axis] = (p.scale[axis] * factor).clamp(0.01, 100.0);
                                    }
                                    *dirty = true;
                                }
                            }
                        }
                        "duplicate" => {
                            self.push_undo();
                            let mut cloned = self.scene.parts[sel_idx].clone();
                            cloned.name = format!("{}_copia", cloned.name);
                            cloned.position[0] += 0.45;
                            self.scene.parts.push(cloned);
                            self.selection = Some(Selection::Part(self.scene.parts.len() - 1));
                            *dirty = true;
                        }
                        "delete" => {
                            self.push_undo();
                            self.scene.parts.remove(sel_idx);
                            self.selection = if self.scene.parts.is_empty() {
                                None
                            } else {
                                Some(Selection::Part(sel_idx.min(self.scene.parts.len() - 1)))
                            };
                            self.selected_vertex_idx = None;
                            *dirty = true;
                        }
                        _ => {}
                    }
                }
                if let Some(preset) = discrete_mat_preset {
                    self.push_undo();
                    self.scene.parts[sel_idx].apply_material_preset(preset);
                    self.status_note = format!("Material '{}' aplicado", preset.label);
                    *dirty = true;
                }
                if let Some(mode) = discrete_shading_change {
                    self.push_undo();
                    self.scene.parts[sel_idx].shading = mode;
                    *dirty = true;
                }
                if let Some(dedup) = discrete_dedup_toggle {
                    self.push_undo();
                    self.scene.parts[sel_idx].deduplicate = dedup;
                    *dirty = true;
                }
                if let Some((next_profile, next_sel, note)) = discrete_profile_replace {
                    self.push_undo();
                    self.scene.parts[sel_idx].profile = next_profile;
                    self.selected_profile_pt = next_sel;
                    self.dragging_profile_pt = None;
                    self.status_note = String::from(note);
                    *dirty = true;
                }
                if let Some(axis) = discrete_mirror_toggle {
                    self.push_undo();
                    self.scene.parts[sel_idx].mirror[axis] = !self.scene.parts[sel_idx].mirror[axis];
                    *dirty = true;
                }
                if let Some(assign) = new_group_assign {
                    self.push_undo();
                    self.scene.parts[sel_idx].group = assign;
                    *dirty = true;
                }
                if let Some(shape) = discrete_collider_change {
                    self.push_undo();
                    if !matches!(shape, ColliderShape::None) {
                        self.scene.physics_enabled = true;
                        self.scene.parts[sel_idx].collider_auto_fit = true;
                        self.scene.parts[sel_idx].collider_offset = [0.0; 3];
                    }
                    self.scene.parts[sel_idx].collider = shape;
                    *dirty = true;
                }
                if let Some(auto_fit) = discrete_autofit_toggle {
                    self.push_undo();
                    self.scene.parts[sel_idx].collider_auto_fit = auto_fit;
                    *dirty = true;
                }
                if global_tex_on != self.textures_enabled {
                    self.textures_enabled = global_tex_on;
                    self.status_note = if self.textures_enabled {
                        String::from("Texturas del modelo activadas")
                    } else {
                        String::from("Texturas del modelo desactivadas")
                    };
                }
                if let Some(t_en) = discrete_part_tex_toggle {
                    self.push_undo();
                    self.scene.parts[sel_idx].texture_enabled = t_en;
                }
                if let Some(t_idx) = discrete_part_tex_idx {
                    self.push_undo();
                    self.scene.parts[sel_idx].texture_index = t_idx;
                    self.scene.parts[sel_idx].texture_enabled = true;
                    self.textures_enabled = true;
                }
                if let Some(act) = tex_action {
                    match act {
                        "generate_tex" => {
                            self.push_undo();
                            let tex = self.build_texture_asset();
                            let idx = self.imported_textures.len();
                            self.imported_textures.push(tex);
                            self.sync_textures_cache();
                            self.textures_enabled = true;
                            self.scene.parts[sel_idx].texture_index = Some(idx);
                            self.scene.parts[sel_idx].texture_enabled = true;
                            self.status_note = String::from("Textura de paleta generada y aplicada");
                        }
                        "load_tex" => {
                            self.import_tex_for_selection_only = true;
                            self.request_import(ui.ctx(), ".png,.jpg,.jpeg,.webp,.f3t");
                        }
                        "apply_tex_all" => {
                            self.push_undo();
                            let t_idx = self.scene.parts[sel_idx].texture_index;
                            let t_en = self.scene.parts[sel_idx].texture_enabled;
                            for p in &mut self.scene.parts {
                                p.texture_index = t_idx;
                                p.texture_enabled = t_en;
                            }
                            self.status_note = String::from("Textura aplicada a todas las piezas");
                        }
                        _ => {}
                    }
                }

                ui.horizontal(|ui| {
                    ui.add_sized([120.0, btn_h], egui::TextEdit::singleline(&mut self.new_group_draft));
                    if icon_text_btn(ui, SvgIcon::Plus, 13.0, "Crear grupo", [106.0, btn_h], false).clicked() {
                        let trimmed = self.new_group_draft.trim().to_string();
                        if !trimmed.is_empty() {
                            self.push_undo();
                            self.scene.parts[sel_idx].group = Some(trimmed);
                            *dirty = true;
                        }
                    }
                });
            }
            None => {
                ui.label("Toca una pieza en el 3D o añádela en Piezas.");
            }
        }

        ui.add_space(8.0);
        ui.separator();
        self.render_model_physics_section(ui, dirty, compact);
    }

    fn render_model_physics_section(&mut self, ui: &mut egui::Ui, dirty: &mut bool, compact: bool) {
        let btn_h = if compact { 32.0 } else { 22.0 };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Físicas de exportación (.f3d)").strong());
            if ui.add_sized([108.0, btn_h], egui::Button::new("Auto-colisión")).clicked() {
                self.push_undo();
                self.scene.auto_generate_colliders();
                self.show_colliders = true;
                *dirty = true;
            }
            if ui.add_sized([118.0, btn_h], egui::Button::new("Copiar a todas")).clicked() {
                self.push_undo();
                for p in &mut self.scene.parts {
                    p.mass = self.scene.physics.mass;
                    p.friction = self.scene.physics.friction;
                    p.restitution = self.scene.physics.restitution;
                    p.gravity_scale = self.scene.physics.gravity_scale;
                    p.linear_damping = self.scene.physics.linear_damping;
                }
                self.status_note = String::from("Físicas aplicadas a todas las piezas");
                *dirty = true;
            }
        });
        let mut enabled = self.scene.physics_enabled;
        if ui.checkbox(&mut enabled, "Incluir colisionadores y propiedades en .f3d").changed() {
            self.push_undo();
            self.scene.physics_enabled = enabled;
            if enabled {
                self.scene.auto_generate_colliders();
                self.show_colliders = true;
            }
            *dirty = true;
        }
        let mut changed_continuous = false;
        if ui.add(egui::Slider::new(&mut self.scene.physics.mass, 0.1..=100.0).text("Masa (kg)")).changed() {
            changed_continuous = true;
            *dirty = true;
        }
        if ui.add(egui::Slider::new(&mut self.scene.physics.friction, 0.0..=1.5).text("Fricción")).changed() {
            changed_continuous = true;
            *dirty = true;
        }
        if ui.add(egui::Slider::new(&mut self.scene.physics.restitution, 0.0..=1.0).text("Rebote")).changed() {
            changed_continuous = true;
            *dirty = true;
        }
        if ui.add(egui::Slider::new(&mut self.scene.physics.gravity_scale, -2.0..=3.0).text("Escala gravedad")).changed() {
            changed_continuous = true;
            *dirty = true;
        }
        if changed_continuous {
            self.begin_continuous_edit(ui);
        }
        self.flush_continuous_edit(ui);
    }

    fn render_file_modal(&mut self, ctx: &egui::Context, compact: bool) {
        if !self.show_file_modal {
            return;
        }
        let screen = ctx.input(|i| i.content_rect());
        let win_w = if compact { (screen.width() - 20.0).max(280.0) } else { 580.0 };
        let win_h = if compact { (screen.height() - 56.0).max(320.0) } else { 540.0 };
        let mut open = self.show_file_modal;

        egui::Window::new("Archivo · Importar y Exportar")
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_size([win_w, win_h])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("file_modal_scroll").show(ui, |ui| {
                    let btn_h = if compact { 34.0 } else { 26.0 };

                    if !self.status_note.is_empty() {
                        ui.label(
                            egui::RichText::new(&self.status_note)
                                .size(11.5)
                                .color(egui::Color32::from_rgb(251, 191, 36)),
                        );
                        ui.add_space(4.0);
                    }

                    ui.label(egui::RichText::new("Proyecto (.forja) e Importación 3D / 2D").strong());
                    ui.horizontal_wrapped(|ui| {
                        if icon_text_btn(ui, SvgIcon::Folder, 15.0, "Abrir .forja", [122.0, btn_h], false)
                            .clicked()
                        {
                            self.import_tex_for_selection_only = false;
                            self.request_import(ui.ctx(), ".forja,.f3s");
                        }
                        if icon_text_btn(ui, SvgIcon::Upload, 15.0, "Importar GLB / GLTF", [164.0, btn_h], false)
                            .clicked()
                        {
                            self.import_tex_for_selection_only = false;
                            self.request_import(ui.ctx(), ".glb,.gltf");
                        }
                        if icon_text_btn(
                            ui,
                            SvgIcon::Folder,
                            15.0,
                            "Abrir .f3* / .png / .jpg",
                            [190.0, btn_h],
                            false,
                        )
                        .clicked()
                        {
                            self.import_tex_for_selection_only = false;
                            self.request_import(ui.ctx(), ".forja,.f3s,.f3d,.f3a,.f3p,.f2d,.f3t,.png,.jpg,.jpeg,.webp,.glb,.gltf");
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Guardar y Exportar").strong());
                    let tab_w = if compact { 92.0 } else { 104.0 };
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::ProjectForja, "Proyecto .forja")).clicked() {
                            self.export_tab = ExportTab::ProjectForja;
                        }
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::GlbBinary, "Escena .glb")).clicked() {
                            self.export_tab = ExportTab::GlbBinary;
                        }
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::F3dBinary, "Modelo .f3d")).clicked() {
                            self.export_tab = ExportTab::F3dBinary;
                        }
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::AssetFormats, "Assets .f3* / .f2d")).clicked() {
                            self.export_tab = ExportTab::AssetFormats;
                        }
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::RustSource, "Código .rs")).clicked() {
                            self.export_tab = ExportTab::RustSource;
                        }
                        if ui.add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::WavefrontObj, "Malla .obj")).clicked() {
                            self.export_tab = ExportTab::WavefrontObj;
                        }
                    });
                    ui.add_space(6.0);

                    match self.export_tab {
                        ExportTab::ProjectForja => {
                            let proj_bytes = self.build_project_asset().to_bytes();
                            ui.label(format!(
                                "Escena editable completa: {} piezas · {} soles · {} texturas · cámara ({}) · {} B",
                                self.scene.parts.len(),
                                self.scene.sun_lights.len(),
                                self.imported_textures.len(),
                                if self.camera.ortho { "Ortográfica" } else { "Perspectiva" },
                                proj_bytes.len(),
                            ));
                            ui.label(
                                egui::RichText::new("Conserva primitivas paramétricas, perfiles 2D, terrenos/stamps, materiales PBR, físicas y pantalla 2D.")
                                    .size(11.5)
                                    .color(egui::Color32::from_gray(175)),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                if icon_text_btn(ui, SvgIcon::Download, 14.0, "Guardar .forja", [140.0, btn_h], false)
                                    .clicked()
                                {
                                    let file = format!("{}.forja", self.scene.model_name.to_lowercase());
                                    match trigger_download(&file, &proj_bytes, "application/octet-stream") {
                                        Ok(Some(saved)) => self.status_note = format!("Proyecto guardado: {saved}"),
                                        Ok(None) => self.status_note = format!("Exportado {file} ({} B)", proj_bytes.len()),
                                        Err(err) => self.status_note = err,
                                    }
                                }
                                if ui.add_sized([148.0, btn_h], egui::Button::new("Verificar .forja")).clicked() {
                                    match ForjaProject::from_bytes(&proj_bytes) {
                                        Ok(dec) => {
                                            let h = fnv1a_32(&proj_bytes[12..]);
                                            self.status_note = format!(
                                                "Proyecto OK: '{}' ({} piezas, {} soles, 0x{:08X})",
                                                dec.scene.model_name,
                                                dec.scene.parts.len(),
                                                dec.scene.sun_lights.len(),
                                                h
                                            );
                                        }
                                        Err(err) => self.status_note = format!("Error .forja: {err}"),
                                    }
                                }
                            });
                        }
                        ExportTab::GlbBinary => {
                            let active_textures: &[Texture] = if self.textures_enabled {
                                &self.imported_textures
                            } else {
                                &[]
                            };
                            let glb_bytes = self.scene.to_glb(active_textures);
                            let sun_count = self
                                .scene
                                .sun_lights
                                .iter()
                                .filter(|s| s.enabled && s.intensity > 0.0001)
                                .count();
                            ui.label(format!(
                                "glTF 2.0 binario (.glb): {} mallas · {} texturas PNG · {} luces Sol · {} B",
                                self.baked_model.parts.len(),
                                active_textures.len(),
                                sun_count,
                                glb_bytes.len(),
                            ));
                            ui.label(
                                egui::RichText::new("Incluye mallas, UVs (tiling/offset), materiales PBR, Alpha Blend/Cutout, texturas PNG y luces KHR_lights_punctual.")
                                    .size(11.5)
                                    .color(egui::Color32::from_gray(175)),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                if icon_text_btn(ui, SvgIcon::Download, 14.0, "Descargar .glb", [140.0, btn_h], false)
                                    .clicked()
                                {
                                    let file = format!("{}.glb", self.scene.model_name.to_lowercase());
                                    match trigger_download(&file, &glb_bytes, "model/gltf-binary") {
                                        Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                                        Ok(None) => self.status_note = format!("Exportado {file} ({} B)", glb_bytes.len()),
                                        Err(err) => self.status_note = err,
                                    }
                                }
                                if ui.add_sized([148.0, btn_h], egui::Button::new("Verificar .glb")).clicked() {
                                    match gltf::Gltf::from_slice(&glb_bytes) {
                                        Ok(parsed) => {
                                            let meshes = parsed.document.meshes().count();
                                            let mats = parsed.document.materials().count();
                                            let imgs = parsed.document.images().count();
                                            self.status_note = format!(
                                                "GLB válido: {meshes} mallas, {mats} materiales, {imgs} imágenes"
                                            );
                                        }
                                        Err(err) => self.status_note = format!("Error GLB: {err}"),
                                    }
                                }
                            });
                        }
                        ExportTab::F3dBinary => {
                            ui.checkbox(&mut self.f3d_compact, "Compresión compacta (17 B/vértice + u16 idx)");
                            let std_len = self.cached_f3d_bytes.len();
                            let cmp_len = self.cached_f3d_compact_bytes.len();
                            let active_bytes = if self.f3d_compact {
                                self.cached_f3d_compact_bytes.clone()
                            } else {
                                self.cached_f3d_bytes.clone()
                            };
                            ui.label(format!(
                                "Partes: {} · Vértices: {} · Triángulos: {} · {} B (Estándar {} B / Compacto {} B)",
                                self.baked_model.parts.len(),
                                self.baked_model.vertex_count(),
                                self.baked_model.triangle_count(),
                                active_bytes.len(),
                                std_len,
                                cmp_len,
                            ));
                            ui.horizontal_wrapped(|ui| {
                                if icon_text_btn(ui, SvgIcon::Download, 14.0, "Descargar .f3d", [136.0, btn_h], false)
                                    .clicked()
                                {
                                    let file = format!("{}.f3d", self.scene.model_name.to_lowercase());
                                    match trigger_download(&file, &active_bytes, "application/octet-stream") {
                                        Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                                        Ok(None) => self.status_note = format!("Exportado {file} ({} B)", active_bytes.len()),
                                        Err(err) => self.status_note = err,
                                    }
                                }
                                if ui.add_sized([148.0, btn_h], egui::Button::new("Probar from_bytes")).clicked() {
                                    match Model::from_bytes(&active_bytes) {
                                        Ok(decoded) => {
                                            let h = fnv1a_32(&active_bytes[16..]);
                                            self.status_note = format!(
                                                "Roundtrip OK: '{}' ({} tri, 0x{:08X})",
                                                decoded.name,
                                                decoded.triangle_count(),
                                                h
                                            );
                                        }
                                        Err(err) => self.status_note = format!("Error loader: {err}"),
                                    }
                                }
                            });
                            let hex_preview: String = active_bytes
                                .iter()
                                .take(80)
                                .enumerate()
                                .map(|(i, b)| if i % 16 == 15 { format!("{b:02X}\n") } else { format!("{b:02X} ") })
                                .collect();
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(10, 12, 16))
                                .inner_margin(8.0)
                                .corner_radius(4.0)
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(hex_preview).monospace().size(11.0)));
                                });
                        }
                        ExportTab::AssetFormats => {
                            let base_name = self.scene.model_name.to_lowercase();
                            let pack_bytes = self.build_pack_asset().to_bytes();
                            let anim_bytes = self.build_animation_asset().to_bytes();
                            let f2d_bytes = self.doc2d.to_bytes();
                            let f2d_rs = self.doc2d.to_rust_code();
                            let export_tex = self.export_texture_asset();
                            let f3t_desc = format!(
                                "Textura '{}' ({}x{} px)",
                                export_tex.name, export_tex.width, export_tex.height
                            );
                            let f3t_bytes = export_tex.to_bytes();
                            let card_fill = egui::Color32::from_rgb(22, 26, 35);

                            for (title, desc, ext, bytes) in [
                                (".f3p · Paquete Todo-en-Uno (F3P1)", String::from("Malla + texturas + animaciones"), "f3p", pack_bytes),
                                (".f3a · Animación (F3A1)", String::from("Clips y keyframes de escena"), "f3a", anim_bytes),
                                (".f2d · Pantalla 2D / UI (F2D1)", String::from("Controles del Visor 2D + acciones .rs"), "f2d", f2d_bytes),
                                (".f3t · Textura Propia (F3T1)", f3t_desc, "f3t", f3t_bytes),
                            ] {
                                egui::Frame::new()
                                    .fill(card_fill)
                                    .inner_margin(8.0)
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(title).strong());
                                        ui.label(
                                            egui::RichText::new(format!("{desc} · {} B", bytes.len()))
                                                .size(11.5)
                                                .color(egui::Color32::from_gray(180)),
                                        );
                                        ui.horizontal_wrapped(|ui| {
                                            if icon_text_btn(
                                                ui,
                                                SvgIcon::Download,
                                                14.0,
                                                &format!("Descargar .{ext}"),
                                                [128.0, btn_h],
                                                false,
                                            )
                                            .clicked()
                                            {
                                                let file = format!("{base_name}.{ext}");
                                                match trigger_download(&file, &bytes, "application/octet-stream") {
                                                    Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                                                    Ok(None) => self.status_note = format!("Exportado {file} ({} B)", bytes.len()),
                                                    Err(err) => self.status_note = err,
                                                }
                                            }
                                            if ext == "f2d"
                                                && icon_text_btn(ui, SvgIcon::Copy, 14.0, "Copiar .rs UI", [118.0, btn_h], false).clicked()
                                            {
                                                ui.ctx().copy_text(f2d_rs.clone());
                                                self.status_note = String::from("Código .rs de Pantalla 2D copiado");
                                            }
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                        }
                        ExportTab::RustSource => {
                            ui.horizontal_wrapped(|ui| {
                                for target in RustExportTarget::ALL {
                                    if ui.selectable_label(self.rust_target == target, target.label()).clicked() {
                                        self.rust_target = target;
                                        self.cached_rust_src =
                                            self.baked_model.to_rust_source(&self.scene.model_name, self.rust_target);
                                    }
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                if icon_text_btn(ui, SvgIcon::Download, 14.0, "Descargar .rs", [128.0, btn_h], false).clicked() {
                                    let file = format!("{}.rs", self.scene.model_name.to_lowercase());
                                    match trigger_download(&file, self.cached_rust_src.as_bytes(), "text/plain") {
                                        Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                                        Ok(None) => self.status_note = format!("Exportado {file}"),
                                        Err(err) => self.status_note = err,
                                    }
                                }
                                if icon_text_btn(ui, SvgIcon::Copy, 14.0, "Copiar .rs", [116.0, btn_h], false).clicked() {
                                    ui.ctx().copy_text(self.cached_rust_src.clone());
                                    self.status_note = String::from("Código .rs copiado");
                                }
                            });
                            let preview = if self.cached_rust_src.len() > 6000 {
                                format!("{}\n// ... ({} B)", &self.cached_rust_src[..6000], self.cached_rust_src.len())
                            } else {
                                self.cached_rust_src.clone()
                            };
                            ui.add(
                                egui::TextEdit::multiline(&mut preview.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(14)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        ExportTab::WavefrontObj => {
                            ui.horizontal_wrapped(|ui| {
                                if icon_text_btn(ui, SvgIcon::Download, 14.0, "Descargar .obj", [128.0, btn_h], false).clicked() {
                                    let file = format!("{}.obj", self.scene.model_name.to_lowercase());
                                    match trigger_download(&file, self.cached_obj_src.as_bytes(), "text/plain") {
                                        Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                                        Ok(None) => self.status_note = format!("Exportado {file}"),
                                        Err(err) => self.status_note = err,
                                    }
                                }
                                if icon_text_btn(ui, SvgIcon::Copy, 14.0, "Copiar .obj", [116.0, btn_h], false).clicked() {
                                    ui.ctx().copy_text(self.cached_obj_src.clone());
                                    self.status_note = String::from("Malla .obj copiada");
                                }
                            });
                            let preview = if self.cached_obj_src.len() > 6000 {
                                format!("{}\n# ... ({} B)", &self.cached_obj_src[..6000], self.cached_obj_src.len())
                            } else {
                                self.cached_obj_src.clone()
                            };
                            ui.add(
                                egui::TextEdit::multiline(&mut preview.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(14)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    }
                });
            });

        self.show_file_modal = open;
    }

    fn gizmo_handles(
        &self,
        rect: egui::Rect,
    ) -> Option<(egui::Pos2, f32, [(GizmoAxis, egui::Pos2, egui::Color32); 3])> {
        let pivot = self.selection_pivot()?;
        let world_len = (self.camera.distance * 0.24).clamp(0.45, 2.5);
        let rmin = [rect.left(), rect.top()];
        let rsize = [rect.width(), rect.height().max(1.0)];

        let o = self.camera.project_to_screen(pivot, rmin, rsize)?;
        let origin_px = egui::pos2(o[0], o[1]);

        let axes = [
            (GizmoAxis::X, [pivot[0] + world_len, pivot[1], pivot[2]], egui::Color32::from_rgb(239, 68, 68)),
            (GizmoAxis::Y, [pivot[0], pivot[1] + world_len, pivot[2]], egui::Color32::from_rgb(34, 197, 94)),
            (GizmoAxis::Z, [pivot[0], pivot[1], pivot[2] + world_len], egui::Color32::from_rgb(59, 130, 246)),
        ];

        let mut projected = Vec::with_capacity(3);
        for (axis, tip_world, col) in axes {
            let tip = self.camera.project_to_screen(tip_world, rmin, rsize)?;
            projected.push((axis, egui::pos2(tip[0], tip[1]), col));
        }
        Some((origin_px, world_len, [projected[0], projected[1], projected[2]]))
    }

    fn hit_test_gizmo(
        pointer_pos: egui::Pos2,
        origin_px: egui::Pos2,
        handles: &[(GizmoAxis, egui::Pos2, egui::Color32); 3],
        hit_radius: f32,
    ) -> Option<GizmoAxis> {
        let mut best_axis = None;
        let mut best_dist = hit_radius;
        for &(axis, tip_px, _) in handles {
            let d = dist_point_to_segment(pointer_pos, origin_px, tip_px);
            if d < best_dist {
                best_dist = d;
                best_axis = Some(axis);
            }
        }
        best_axis
    }

    fn apply_gizmo_drag(&mut self, world_delta: f32) {
        let Some(drag) = &self.active_gizmo_drag else { return };
        let axis_idx = drag.axis as usize;
        let snap = self.snap_enabled;

        if let Some(Selection::Sun(sun_idx)) = self.selection {
            if let (Some(start_sun), Some(sun)) = (
                drag.start_suns.get(sun_idx),
                self.scene.sun_lights.get_mut(sun_idx),
            ) {
                match drag.mode {
                    GizmoMode::Translate => {
                        let mut v = start_sun.position[axis_idx] + world_delta;
                        if snap {
                            v = Self::snap_step(v, 0.25);
                        }
                        sun.position[axis_idx] = v;
                    }
                    GizmoMode::Rotate => {
                        let angle = world_delta * 0.75;
                        sun.aim_at_origin = false;
                        sun.direction = rotate_vec3_axis(start_sun.direction, axis_idx, angle);
                    }
                    GizmoMode::Scale => {
                        let mut v = (start_sun.intensity + world_delta).clamp(0.0, 5.0);
                        if snap {
                            v = Self::snap_step(v, 0.25);
                        }
                        sun.intensity = v;
                    }
                    GizmoMode::Vertices => {}
                }
            }
            return;
        }

        let indices = self.selected_part_indices();
        for idx in indices {
            let Some(start_part) = drag.start_parts.get(idx) else { continue };
            let Some(part) = self.scene.parts.get_mut(idx) else { continue };
            match drag.mode {
                GizmoMode::Translate => {
                    let mut v = start_part.position[axis_idx] + world_delta;
                    if snap {
                        v = Self::snap_step(v, 0.25);
                    }
                    part.position[axis_idx] = v;
                }
                GizmoMode::Rotate => {
                    let mut v = start_part.rotation_deg[axis_idx] + world_delta * 45.0;
                    if snap {
                        v = Self::snap_step(v, 15.0);
                    }
                    part.rotation_deg[axis_idx] = v;
                }
                GizmoMode::Scale => {
                    let mut v = (start_part.scale[axis_idx] + world_delta).clamp(0.05, 20.0);
                    if snap {
                        v = Self::snap_step(v, 0.25).max(0.25);
                    }
                    part.scale[axis_idx] = v;
                }
                GizmoMode::Vertices => {}
            }
        }
        self.rebuild_outputs();
    }

    fn render_viewport_3d(&mut self, ui: &mut egui::Ui, compact: bool) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let rmin = [rect.left(), rect.top()];
        let rsize = [rect.width(), rect.height().max(1.0)];

        // Collect unique editable vertex handles if in GizmoMode::Vertices
        let vertex_handles: Vec<(usize, [f32; 3], egui::Pos2)> = if self.sim_world.is_none()
            && self.gizmo_mode == GizmoMode::Vertices
        {
            if let Some(Selection::Part(p_idx)) = self.selection {
                if let Some(part) = self.scene.parts.get(p_idx) {
                    if let Some(mesh) = &part.custom_mesh {
                        let mat = Mat4::from_trs(part.position, part.rotation_deg, part.scale);
                        let mut seen: Vec<[i32; 3]> = Vec::new();
                        let mut out = Vec::new();
                        for (v_idx, v) in mesh.vertices.iter().enumerate() {
                            let key = [
                                (v.position[0] * 1000.0).round() as i32,
                                (v.position[1] * 1000.0).round() as i32,
                                (v.position[2] * 1000.0).round() as i32,
                            ];
                            if !seen.contains(&key) {
                                seen.push(key);
                                let world = mat.transform_point(v.position);
                                if let Some(px) = self.camera.project_to_screen(world, rmin, rsize) {
                                    out.push((v_idx, v.position, egui::pos2(px[0], px[1])));
                                }
                                if out.len() >= 300 {
                                    break;
                                }
                            }
                        }
                        out
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let gizmo_data = if self.gizmo_mode != GizmoMode::Vertices {
            self.gizmo_handles(rect)
        } else {
            None
        };
        let hit_radius = if compact { 26.0 } else { 18.0 };
        let pointer_pos = ui.ctx().pointer_interact_pos();
        let hovered_axis = if self.active_gizmo_drag.is_none() {
            match (pointer_pos, &gizmo_data) {
                (Some(p), Some((origin, _, handles))) if rect.contains(p) => {
                    Self::hit_test_gizmo(p, *origin, handles, hit_radius)
                }
                _ => None,
            }
        } else {
            self.active_gizmo_drag.as_ref().map(|d| d.axis)
        };

        let mut hit_vertex: Option<(usize, [f32; 3])> = None;
        if self.gizmo_mode == GizmoMode::Vertices && self.active_vertex_drag.is_none() {
            if let Some(p) = pointer_pos {
                if rect.contains(p) {
                    let mut best_d = if compact { 22.0 } else { 14.0 };
                    for &(v_idx, local_pos, px) in &vertex_handles {
                        let d = p.distance(px);
                        if d < best_d {
                            best_d = d;
                            hit_vertex = Some((v_idx, local_pos));
                        }
                    }
                }
            }
        }

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let (Some((v_idx, local_anchor)), Some(Selection::Part(part_idx)), Some(pos)) =
                (hit_vertex, &self.selection, pointer_pos)
            {
                if let Some(part) = self.scene.parts.get(*part_idx) {
                    if let Some(mesh) = &part.custom_mesh {
                        let mat = Mat4::from_trs(part.position, part.rotation_deg, part.scale);
                        self.selected_vertex_idx = Some(v_idx);
                        self.active_vertex_drag = Some(ActiveVertexDrag {
                            part_idx: *part_idx,
                            local_anchor,
                            world_anchor: mat.transform_point(local_anchor),
                            start_vertices: mesh.vertices.clone(),
                            start_pointer_px: pos,
                        });
                    }
                }
            } else if let Some(axis) = hovered_axis {
                self.active_gizmo_drag = Some(ActiveGizmoDrag {
                    axis,
                    mode: self.gizmo_mode,
                    start_parts: self.scene.parts.clone(),
                    start_suns: self.scene.sun_lights.clone(),
                    accum_pixels: 0.0,
                });
            }
        } else if response.clicked() {
            if let Some((v_idx, _)) = hit_vertex {
                self.selected_vertex_idx = Some(v_idx);
            }
        }

        if let Some(vdrag) = self.active_vertex_drag.as_ref() {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pos) = pointer_pos {
                    let dx_px = pos.x - vdrag.start_pointer_px.x;
                    let dy_px = pos.y - vdrag.start_pointer_px.y;
                    let eye = self.camera.eye();
                    let forward = normalize_v([
                        self.camera.target[0] - eye[0],
                        self.camera.target[1] - eye[1],
                        self.camera.target[2] - eye[2],
                    ]);
                    let right = normalize_v(cross_v(forward, [0.0, 1.0, 0.0]));
                    let up = cross_v(right, forward);
                    let dist = ((vdrag.world_anchor[0] - eye[0]).powi(2)
                        + (vdrag.world_anchor[1] - eye[1]).powi(2)
                        + (vdrag.world_anchor[2] - eye[2]).powi(2))
                    .sqrt()
                    .max(0.2);
                    let world_per_px = (2.0 * dist * (45.0_f32.to_radians() * 0.5).tan())
                        / rect.height().max(1.0);
                    let world_delta = [
                        (right[0] * dx_px - up[0] * dy_px) * world_per_px,
                        (right[1] * dx_px - up[1] * dy_px) * world_per_px,
                        (right[2] * dx_px - up[2] * dy_px) * world_per_px,
                    ];

                    if let Some(part) = self.scene.parts.get_mut(vdrag.part_idx) {
                        let mat = Mat4::from_trs(part.position, part.rotation_deg, part.scale);
                        let local_delta = mat3_inv_mul(&mat, world_delta);
                        if let Some(mesh) = part.custom_mesh.as_mut() {
                            for (i, v) in mesh.vertices.iter_mut().enumerate() {
                                let sv = vdrag.start_vertices[i];
                                if (sv.position[0] - vdrag.local_anchor[0]).abs() < 1e-4
                                    && (sv.position[1] - vdrag.local_anchor[1]).abs() < 1e-4
                                    && (sv.position[2] - vdrag.local_anchor[2]).abs() < 1e-4
                                {
                                    v.position = [
                                        sv.position[0] + local_delta[0],
                                        sv.position[1] + local_delta[1],
                                        sv.position[2] + local_delta[2],
                                    ];
                                }
                            }
                            recompute_mesh_normals(mesh);
                        }
                    }
                    self.rebuild_outputs();
                }
            }
        } else if let (Some(drag), Some((_, world_len, handles))) = (self.active_gizmo_drag.as_mut(), &gizmo_data) {
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                let (origin, _, _) = gizmo_data.unwrap();
                let tip = handles[drag.axis as usize].1;
                let axis_vec = tip - origin;
                let px_len = axis_vec.length().max(12.0);
                let axis_dir = axis_vec / px_len;
                let proj_px = delta.x * axis_dir.x + delta.y * axis_dir.y;
                drag.accum_pixels += proj_px;
                let world_delta = (drag.accum_pixels / px_len) * world_len;
                self.apply_gizmo_drag(world_delta);
            }
        } else if let Some(mt) = ui.input(|i| i.multi_touch()) {
            if (mt.zoom_delta - 1.0).abs() > 0.001 {
                self.camera.distance = (self.camera.distance / mt.zoom_delta.max(0.5)).clamp(0.15, 5000.0);
            }
            if mt.translation_delta.length_sq() > 0.01 {
                self.camera.pan(mt.translation_delta.x, mt.translation_delta.y);
            }
        } else if response.dragged_by(egui::PointerButton::Primary) {
            let shift = ui.input(|i| i.modifiers.shift);
            let delta = response.drag_delta();
            if shift {
                self.camera.pan(delta.x, delta.y);
            } else {
                self.camera.yaw += delta.x * 0.01;
                self.camera.pitch = (self.camera.pitch + delta.y * 0.01).clamp(-1.45, 1.45);
            }
        } else if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            self.camera.pan(delta.x, delta.y);
        }

        if response.drag_stopped() {
            if let Some(drag) = self.active_gizmo_drag.take() {
                if drag.start_parts != self.scene.parts || drag.start_suns != self.scene.sun_lights {
                    let mut prev_scene = self.scene.clone();
                    prev_scene.parts = drag.start_parts;
                    prev_scene.sun_lights = drag.start_suns;
                    self.push_undo_entry(HistoryEntry {
                        scene: prev_scene,
                        selection: self.selection.clone(),
                    });
                    self.status_note = format!("{} aplicado", drag.mode.label());
                }
            }
            if let Some(vdrag) = self.active_vertex_drag.take() {
                let mut prev_scene = self.scene.clone();
                if let Some(p) = prev_scene.parts.get_mut(vdrag.part_idx) {
                    if let Some(m) = p.custom_mesh.as_mut() {
                        m.vertices = vdrag.start_vertices;
                        recompute_mesh_normals(m);
                    }
                }
                self.push_undo_entry(HistoryEntry {
                    scene: prev_scene,
                    selection: self.selection.clone(),
                });
                self.status_note = String::from("Vértice editado");
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.distance = (self.camera.distance * (1.0 - scroll * 0.0018)).clamp(0.15, 5000.0);
            }
        }

        if self.sim_world.is_none() && response.clicked() && hovered_axis.is_none() && hit_vertex.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()) * 2.0 - 1.0;
                let norm_y = 1.0 - ((pos.y - rect.top()) / rect.height()) * 2.0;
                let aspect = rect.width() / rect.height().max(1.0);
                let (ray_origin, ray_dir) = self.camera.screen_ray(norm_x, norm_y, aspect);

                let mut best_t = f32::INFINITY;
                let mut best_sel: Option<Selection> = None;
                for (mesh_idx, baked) in self.baked_model.parts.iter().enumerate() {
                    if let Some(t) = ray_intersects_aabb(ray_origin, ray_dir, baked.bounds) {
                        if t < best_t {
                            best_t = t;
                            best_sel = self.part_map.get(mesh_idx).copied().map(Selection::Part);
                        }
                    }
                }
                for (sun_idx, sun) in self.scene.sun_lights.iter().enumerate() {
                    if !sun.visible {
                        continue;
                    }
                    if let Some(t) = ray_intersects_sphere(ray_origin, ray_dir, sun.position, 0.36) {
                        if t < best_t {
                            best_t = t;
                            best_sel = Some(Selection::Sun(sun_idx));
                        }
                    }
                }
                self.selection = best_sel;
                self.selected_vertex_idx = None;
            }
        }

        let renderer_arc = Arc::clone(&self.gl_renderer);
        let model_clone = self.baked_model.clone();
        let generation = self.generation;
        let camera = self.camera;
        let selected_mask = self.selected_mesh_mask();
        let show_grid = self.show_grid;
        let show_colliders = self.show_colliders || self.sim_world.is_some();
        let wireframe = self.wireframe;
        let textures_enabled = self.textures_enabled;
        let tex_gen = self.texture_generation;
        let textures_arc = Arc::clone(&self.textures_shared);
        let part_use_tex: Vec<bool> = self
            .part_map
            .iter()
            .map(|&src_idx| {
                self.scene
                    .parts
                    .get(src_idx)
                    .map_or(true, |p| p.texture_enabled)
            })
            .collect();
        let part_tex_indices: Vec<Option<usize>> = self
            .part_map
            .iter()
            .map(|&src_idx| {
                self.scene
                    .parts
                    .get(src_idx)
                    .and_then(|p| p.texture_index)
            })
            .collect();
        let part_alpha_cutoffs: Vec<f32> = self
            .part_map
            .iter()
            .map(|&src_idx| {
                self.scene
                    .parts
                    .get(src_idx)
                    .map_or(0.0, |p| p.alpha_cutoff)
            })
            .collect();
        let has_scene_suns = !self.scene.sun_lights.is_empty();
        let sun_lights: Vec<RenderSunLight> = self
            .scene
            .sun_lights
            .iter()
            .filter(|s| s.enabled && s.intensity > 0.0001)
            .take(4)
            .map(|s| RenderSunLight {
                dir_to_light: s.light_vector(),
                color: [
                    s.color[0] as f32 / 255.0,
                    s.color[1] as f32 / 255.0,
                    s.color[2] as f32 / 255.0,
                ],
                intensity: s.intensity,
                ambient: s.ambient,
            })
            .collect();
        let selected_sun_idx = match self.selection {
            Some(Selection::Sun(i)) => Some(i),
            _ => None,
        };
        let sun_markers: Vec<RenderSunMarker> = self
            .scene
            .sun_lights
            .iter()
            .enumerate()
            .filter(|(_, s)| s.visible)
            .map(|(i, s)| RenderSunMarker {
                position: s.position,
                ray_dir: s.ray_direction(),
                color: [
                    s.color[0] as f32 / 255.0,
                    s.color[1] as f32 / 255.0,
                    s.color[2] as f32 / 255.0,
                ],
                intensity: s.intensity,
                enabled: s.enabled,
                selected: selected_sun_idx == Some(i),
            })
            .collect();
        let part_transforms = self.sim_part_transforms.clone();
        let aspect = rect.width() / rect.height().max(1.0);

        let callback = egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, painter| {
                let gl = painter.gl();
                if let Ok(mut guard) = renderer_arc.lock() {
                    if let Some(renderer) = guard.as_mut() {
                        unsafe {
                            renderer.sync_model(gl, &model_clone, generation);
                            renderer.sync_textures(gl, &textures_arc, tex_gen);
                            renderer.sync_collider_debug(gl, &model_clone, generation, show_colliders);
                            renderer.paint(
                                gl,
                                aspect,
                                &camera,
                                &selected_mask,
                                show_grid,
                                wireframe,
                                textures_enabled,
                                &part_use_tex,
                                &part_tex_indices,
                                &part_alpha_cutoffs,
                                has_scene_suns,
                                &sun_lights,
                                &sun_markers,
                                &part_transforms,
                            );
                        }
                    }
                }
            })),
        };
        ui.painter().add(callback);

        // Draw vertex handles in Vertex Edit mode
        if self.sim_world.is_none() && self.gizmo_mode == GizmoMode::Vertices {
            let painter = ui.painter_at(rect);
            let vr = if compact { 6.5 } else { 4.8 };
            for &(v_idx, _, px) in &vertex_handles {
                let is_sel_v = self.selected_vertex_idx == Some(v_idx);
                let fill = if is_sel_v {
                    egui::Color32::from_rgb(251, 191, 36)
                } else {
                    egui::Color32::from_rgb(56, 189, 248)
                };
                painter.circle_filled(px, vr, fill);
                painter.circle_stroke(px, vr + 1.0, egui::Stroke::new(1.2, egui::Color32::BLACK));
            }
            if let Some(Selection::Part(p_idx)) = self.selection {
                if self.scene.parts.get(p_idx).map_or(false, |p| p.kind != PrimitiveKind::CustomMesh) {
                    let btn_rect = egui::Rect::from_min_size(
                        rect.center_top() + egui::vec2(-105.0, 50.0),
                        egui::vec2(210.0, 32.0),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                        if icon_text_btn(ui, SvgIcon::Vertices, 14.0, "Convertir a Malla Editable", [210.0, 32.0], false)
                            .clicked()
                        {
                            self.convert_selected_to_mesh();
                        }
                    });
                }
            }
        }

        // Draw 3D gizmo axes
        if self.sim_world.is_none() && self.gizmo_mode != GizmoMode::Vertices {
            if let Some((origin, _, handles)) = self.gizmo_handles(rect) {
                let painter = ui.painter_at(rect);
                let axis_names = ["X", "Y", "Z"];
                for (i, &(axis, tip, base_col)) in handles.iter().enumerate() {
                    let is_active = hovered_axis == Some(axis)
                        || self.active_gizmo_drag.as_ref().map(|d| d.axis) == Some(axis);
                    let col = if is_active {
                        egui::Color32::from_rgb(251, 191, 36)
                    } else {
                        base_col
                    };
                    let width = if is_active { 4.2 } else { 2.8 };
                    painter.line_segment([origin, tip], egui::Stroke::new(width, col));

                    let handle_r = if compact { 8.5 } else { 6.0 };
                    match self.gizmo_mode {
                        GizmoMode::Translate | GizmoMode::Vertices => painter.circle_filled(tip, handle_r, col),
                        GizmoMode::Scale => painter.rect_filled(
                            egui::Rect::from_center_size(tip, egui::vec2(handle_r * 1.9, handle_r * 1.9)),
                            2.0,
                            col,
                        ),
                        GizmoMode::Rotate => painter.circle_stroke(tip, handle_r + 1.5, egui::Stroke::new(2.2, col)),
                    };
                    painter.text(
                        tip + egui::vec2(10.0, -10.0),
                        egui::Align2::CENTER_CENTER,
                        axis_names[i],
                        egui::FontId::proportional(11.5),
                        col,
                    );
                }
                painter.circle_filled(origin, 4.5, egui::Color32::WHITE);
            }
        }

        // Floating tool strip inside 3D Viewport
        let top_bar_w = if compact { (rect.width() - 16.0).max(200.0) } else { 420.0 };
        let top_bar_h = if compact { 38.0 } else { 32.0 };
        let top_rect = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::vec2(top_bar_w, top_bar_h),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(top_rect), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 24, 222))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("viewport_top_bar_scroll")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let bh = if compact { 28.0 } else { 22.0 };
                                for mode in [
                                    GizmoMode::Translate,
                                    GizmoMode::Rotate,
                                    GizmoMode::Scale,
                                    GizmoMode::Vertices,
                                ] {
                                    let w = if compact { 76.0 } else { 78.0 };
                                    if icon_text_btn(
                                        ui,
                                        mode.icon(),
                                        14.0,
                                        mode.label(),
                                        [w, bh],
                                        self.gizmo_mode == mode,
                                    )
                                    .clicked()
                                    {
                                        self.gizmo_mode = mode;
                                    }
                                }
                                if icon_text_btn(ui, SvgIcon::Snap, 14.0, "Snap", [64.0, bh], self.snap_enabled).clicked() {
                                    self.snap_enabled = !self.snap_enabled;
                                }
                            });
                        });
                });
        });

        // Bottom-left floating viewport display & camera preset controls
        let bar_w = if compact { (rect.width() - 16.0).max(200.0) } else { (rect.width() - 16.0).min(805.0) };
        let bar_h = if compact { 38.0 } else { 32.0 };
        let overlay_rect = egui::Rect::from_min_size(
            rect.left_bottom() + egui::vec2(8.0, -(bar_h + 8.0)),
            egui::vec2(bar_w, bar_h),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(overlay_rect), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 24, 220))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("viewport_bottom_bar_scroll")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let bh = if compact { 28.0 } else { 22.0 };
                                if icon_text_btn(ui, SvgIcon::Texture, 14.0, "Texturas", [82.0, bh], self.textures_enabled)
                                    .on_hover_text("Activar/desactivar texturas del modelo")
                                    .clicked()
                                {
                                    self.textures_enabled = !self.textures_enabled;
                                    self.status_note = if self.textures_enabled {
                                        String::from("Texturas del modelo activadas")
                                    } else {
                                        String::from("Texturas del modelo desactivadas")
                                    };
                                }
                                if icon_text_btn(ui, SvgIcon::Wire, 14.0, "Malla", [66.0, bh], self.wireframe).clicked() {
                                    self.wireframe = !self.wireframe;
                                }
                                if icon_text_btn(ui, SvgIcon::Grid, 14.0, "Grilla", [68.0, bh], self.show_grid).clicked() {
                                    self.show_grid = !self.show_grid;
                                }
                                if icon_text_btn(ui, SvgIcon::Collider, 14.0, "Col.", [60.0, bh], self.show_colliders).clicked() {
                                    self.show_colliders = !self.show_colliders;
                                }
                                if icon_btn(ui, SvgIcon::Plus, 14.0, [28.0, bh], false)
                                    .on_hover_text("Acercar")
                                    .clicked()
                                {
                                    self.camera.distance = (self.camera.distance * 0.82).clamp(0.15, 5000.0);
                                }
                                if icon_btn(ui, SvgIcon::Minus, 14.0, [28.0, bh], false)
                                    .on_hover_text("Alejar")
                                    .clicked()
                                {
                                    self.camera.distance = (self.camera.distance * 1.22).clamp(0.15, 5000.0);
                                }
                                if icon_text_btn(ui, SvgIcon::Center, 14.0, "Centrar", [82.0, bh], false).clicked() {
                                    let keep_ortho = self.camera.ortho;
                                    self.camera = OrbitCamera {
                                        ortho: keep_ortho,
                                        ..OrbitCamera::default()
                                    };
                                    self.camera.frame_bounds(self.baked_model.bounds());
                                }
                                ui.separator();
                                for (lbl, tip, preset) in [
                                    ("Frente", "Vista Frontal (tecla 1)", CameraViewPreset::Front),
                                    ("Lat", "Vista Lateral Derecha (tecla 3)", CameraViewPreset::Right),
                                    ("Sup", "Vista Superior (tecla 7)", CameraViewPreset::Top),
                                    ("Iso", "Vista Isométrica", CameraViewPreset::Iso),
                                ] {
                                    if ui
                                        .add_sized([46.0, bh], egui::Button::new(lbl))
                                        .on_hover_text(tip)
                                        .clicked()
                                    {
                                        self.camera.set_view_preset(preset);
                                    }
                                }
                                let proj_lbl = if self.camera.ortho { "Orto" } else { "Persp" };
                                if ui
                                    .add_sized([50.0, bh], egui::Button::selectable(self.camera.ortho, proj_lbl))
                                    .on_hover_text("Alternar Perspectiva / Ortográfica (tecla 5)")
                                    .clicked()
                                {
                                    self.camera.ortho = !self.camera.ortho;
                                    self.status_note = if self.camera.ortho {
                                        String::from("Cámara ortográfica activada")
                                    } else {
                                        String::from("Cámara en perspectiva activada")
                                    };
                                }
                            });
                        });
                });
        });
    }

    fn render_top_header(&mut self, ui: &mut egui::Ui, compact: bool) {
        let bh = if compact { 32.0 } else { 24.0 };
        egui::ScrollArea::horizontal()
            .id_salt("studio_header_scroll")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if icon_text_btn(
                        ui,
                        SvgIcon::Folder,
                        15.0,
                        "Archivo",
                        [86.0, bh],
                        self.show_file_modal,
                    )
                    .on_hover_text("Importar / Exportar (.f3d, .f2d, .f3*, .glb, .rs, .obj)")
                    .clicked()
                    {
                        self.show_file_modal = !self.show_file_modal;
                    }

                    ui.separator();

                    if icon_text_btn(
                        ui,
                        SvgIcon::View3d,
                        15.0,
                        "Vista 3D",
                        [84.0, bh],
                        self.view_mode == ViewMode::Mode3D,
                    )
                    .clicked()
                    {
                        self.view_mode = ViewMode::Mode3D;
                    }
                    if icon_text_btn(
                        ui,
                        SvgIcon::View2d,
                        15.0,
                        "Visor 2D",
                        [84.0, bh],
                        self.view_mode == ViewMode::Mode2D,
                    )
                    .clicked()
                    {
                        self.view_mode = ViewMode::Mode2D;
                    }

                    ui.separator();

                    let can_undo = !self.undo_stack.is_empty();
                    let can_redo = !self.redo_stack.is_empty();
                    if icon_btn_enabled(ui, can_undo, SvgIcon::Undo, 15.0, [30.0, bh])
                        .on_hover_text("Deshacer (Ctrl+Z)")
                        .clicked()
                    {
                        self.undo();
                    }
                    if icon_btn_enabled(ui, can_redo, SvgIcon::Redo, 15.0, [30.0, bh])
                        .on_hover_text("Rehacer (Ctrl+Y)")
                        .clicked()
                    {
                        self.redo();
                    }

                    if self.view_mode == ViewMode::Mode3D {
                        ui.separator();
                        let is_simulating = self.sim_world.is_some();
                        let (sim_icon, sim_lbl) = if is_simulating {
                            (SvgIcon::Stop, "Parar")
                        } else {
                            (SvgIcon::Play, "Física")
                        };
                        if icon_text_btn(ui, sim_icon, 14.0, sim_lbl, [76.0, bh], is_simulating)
                            .on_hover_text("Simular físicas por pieza con forja3d-physics")
                            .clicked()
                        {
                            if is_simulating {
                                self.stop_physics_sim();
                                self.status_note = String::from("Simulación detenida");
                            } else {
                                self.start_physics_sim();
                            }
                        }
                        if is_simulating {
                            if icon_text_btn(ui, SvgIcon::Impulse, 14.0, "Impulso", [82.0, bh], false).clicked() {
                                let target_mesh = match self.selection {
                                    Some(Selection::Part(p_idx)) => self
                                        .part_map
                                        .iter()
                                        .position(|&src| src == p_idx),
                                    _ => None,
                                };
                                if let Some(world) = self.sim_world.as_mut() {
                                    for &(m_idx, handle, _) in &self.sim_part_bodies {
                                        if target_mesh.map_or(true, |t| t == m_idx) {
                                            if let Some(b) = world.body(handle) {
                                                if b.body_kind == RigidBodyKind::Dynamic {
                                                    let mass = b.mass.max(0.5);
                                                    world.apply_impulse(handle, [0.4 * mass, 6.2 * mass, 0.0]);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !compact && !self.status_note.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.status_note)
                                .size(11.5)
                                .color(egui::Color32::from_gray(190)),
                        );
                    }
                });
            });
    }
}

impl eframe::App for StudioApp {
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            if let Ok(mut guard) = self.gl_renderer.lock() {
                if let Some(mut renderer) = guard.take() {
                    unsafe {
                        renderer.destroy(gl);
                    }
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.step_physics_sim(ui.ctx());

        let picked = self.pending_import.lock().ok().and_then(|mut g| g.take());
        if let Some((name, bytes, base_dir)) = picked {
            self.import_model_bytes(Some(&name), &bytes, base_dir.as_deref());
        }
        let pending_note = self.pending_status.lock().ok().and_then(|mut g| g.take());
        if let Some(note) = pending_note {
            self.status_note = note;
        }
        #[cfg(target_os = "android")]
        if self.android_picking {
            match android_poll_file_picker() {
                Some(Ok((name, bytes))) => {
                    self.android_picking = false;
                    self.import_model_bytes(Some(&name), &bytes, None);
                }
                Some(Err(Some(err))) => {
                    self.android_picking = false;
                    self.status_note = err;
                }
                Some(Err(None)) => {
                    self.android_picking = false;
                    self.status_note = String::from("Importación cancelada");
                }
                None => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }

        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.into_iter().next() {
            self.import_tex_for_selection_only = false;
            let name = file
                .path()
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("modelo.glb")
                .to_string();
            #[cfg(target_arch = "wasm32")]
            {
                let pending = Arc::clone(&self.pending_import);
                let ctx = ui.ctx().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(bytes) = file.bytes_async().await {
                        if let Ok(mut guard) = pending.lock() {
                            *guard = Some((name, bytes, None));
                        }
                        ctx.request_repaint();
                    }
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Ok(bytes) = std::fs::read(file.path()) {
                    self.import_model_bytes(Some(&name), &bytes, file.path().parent());
                }
            }
        }

        let (want_undo, want_redo, cam_shortcut) = ui.ctx().input(|i| {
            let cmd = i.modifiers.command;
            let undo = cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z);
            let redo = (cmd && i.key_pressed(egui::Key::Y))
                || (cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z));
            let shortcut = if !cmd && !i.modifiers.alt {
                if i.key_pressed(egui::Key::Num1) {
                    Some(if i.modifiers.shift { -1 } else { 1 })
                } else if i.key_pressed(egui::Key::Num3) {
                    Some(if i.modifiers.shift { -3 } else { 3 })
                } else if i.key_pressed(egui::Key::Num7) {
                    Some(7)
                } else if i.key_pressed(egui::Key::Num5) {
                    Some(5)
                } else {
                    None
                }
            } else {
                None
            };
            (undo, redo, shortcut)
        });
        if want_undo {
            self.undo();
        } else if want_redo {
            self.redo();
        } else if self.view_mode == ViewMode::Mode3D && !ui.ctx().egui_wants_keyboard_input() {
            match cam_shortcut {
                Some(1) => self.camera.set_view_preset(CameraViewPreset::Front),
                Some(-1) => self.camera.set_view_preset(CameraViewPreset::Back),
                Some(3) => self.camera.set_view_preset(CameraViewPreset::Right),
                Some(-3) => self.camera.set_view_preset(CameraViewPreset::Left),
                Some(7) => self.camera.set_view_preset(CameraViewPreset::Top),
                Some(5) => {
                    self.camera.ortho = !self.camera.ortho;
                    self.status_note = if self.camera.ortho {
                        String::from("Cámara ortográfica activada")
                    } else {
                        String::from("Cámara en perspectiva activada")
                    };
                }
                _ => {}
            }
        }

        #[cfg(target_os = "android")]
        {
            let (now, focused, viewport_rect, content_rect) = ui.ctx().input(|i| {
                (
                    i.time,
                    i.viewport().focused.unwrap_or(true),
                    i.viewport_rect(),
                    i.content_rect(),
                )
            });
            if !self.immersive_applied || (!self.was_focused && focused) {
                android_enable_immersive();
                self.immersive_applied = true;
            }
            let size_changed = (viewport_rect.size() - content_rect.size()).length_sq() > 1.0;
            if self.last_inset_poll < 0.0 || (!self.was_focused && focused) || size_changed || (now - self.last_inset_poll) > 0.75 {
                let measured = android_safe_insets_dp(ui.ctx(), viewport_rect);
                let fallback_t = if measured[1] < 1.0 && cfg!(target_os = "android") { 24.0 } else { 0.0 };
                let fallback_b = if measured[3] < 1.0 && cfg!(target_os = "android") { 14.0 } else { 0.0 };
                self.safe_insets_dp = [
                    measured[0].max(0.0),
                    measured[1].max(fallback_t),
                    measured[2].max(0.0),
                    measured[3].max(fallback_b),
                ];
                self.last_inset_poll = now;
            }
            self.was_focused = focused;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (self.last_inset_poll, self.was_focused, self.immersive_applied);
        }

        let [safe_l, safe_t, safe_r, safe_b] = self.safe_insets_dp;
        if safe_t > 0.5 {
            egui::Panel::top("android_safe_top")
                .exact_size(safe_t)
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |_| {});
        }
        if safe_b > 0.5 {
            egui::Panel::bottom("android_safe_bottom")
                .exact_size(safe_b)
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(16, 19, 25)))
                .show(ui, |_| {});
        }
        if safe_l > 0.5 {
            egui::Panel::left("android_safe_left")
                .exact_size(safe_l)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |_| {});
        }
        if safe_r > 0.5 {
            egui::Panel::right("android_safe_right")
                .exact_size(safe_r)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |_| {});
        }

        let mut dirty = false;
        let root_rect = ui.available_rect_before_wrap();
        let is_compact = root_rect.width() < 760.0;

        let style = ui.style_mut();
        if is_compact {
            style.spacing.interact_size.y = 32.0;
            style.spacing.button_padding = egui::vec2(8.0, 5.0);
        } else {
            style.spacing.interact_size.y = 22.0;
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
        }

        egui::Panel::top("studio_top_bar")
            .exact_size(if is_compact { 44.0 } else { 36.0 })
            .show(ui, |ui| {
                self.render_top_header(ui, is_compact);
            });

        if is_compact && !self.status_note.is_empty() {
            egui::Panel::top("mobile_status_strip")
                .exact_size(22.0)
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(20, 25, 34)).inner_margin(egui::Margin::symmetric(8, 2)))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&self.status_note)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(251, 191, 36)),
                    );
                });
        }

        self.render_file_modal(ui.ctx(), is_compact);

        let mut pending_f2d_dl: Option<(String, Vec<u8>)> = None;
        let mut pending_rs_dl: Option<(String, String)> = None;

        if is_compact {
            egui::Panel::bottom("mobile_studio_tabs")
                .exact_size(46.0)
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        let tab_w = ((root_rect.width() - 28.0) / 3.0).max(74.0);
                        let (v_icon, v_lbl, s_lbl) = match self.view_mode {
                            ViewMode::Mode3D => (SvgIcon::View3d, "Vista 3D", "Piezas"),
                            ViewMode::Mode2D => (SvgIcon::View2d, "Visor 2D", "Controles"),
                        };
                        let tabs = [
                            (MobileTab::Viewport, v_icon, v_lbl),
                            (MobileTab::Scene, SvgIcon::Cubes, s_lbl),
                            (MobileTab::Inspect, SvgIcon::Sliders, "Editar"),
                        ];
                        for (tab, icon, label) in tabs {
                            if icon_text_btn(ui, icon, 14.0, label, [tab_w, 34.0], self.mobile_tab == tab)
                                .clicked()
                            {
                                self.mobile_tab = tab;
                            }
                        }
                    });
                });

            if self.mobile_tab != MobileTab::Viewport {
                let drawer_h = (root_rect.height() * 0.56).clamp(240.0, 480.0);
                egui::Panel::bottom("mobile_drawer")
                    .resizable(true)
                    .default_size(drawer_h)
                    .min_size(180.0)
                    .max_size((root_rect.height() * 0.72).max(260.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("mobile_drawer_scroll").show(ui, |ui| {
                            ui.add_space(4.0);
                            match (self.mobile_tab, self.view_mode) {
                                (MobileTab::Scene, ViewMode::Mode3D) => self.render_scene_panel(ui, &mut dirty, true),
                                (MobileTab::Inspect, ViewMode::Mode3D) => self.render_inspector(ui, &mut dirty, true),
                                (MobileTab::Scene, ViewMode::Mode2D) => {
                                    let sel3d = match self.selection {
                                        Some(Selection::Part(i)) => Some(i),
                                        _ => None,
                                    };
                                    render_scene_panel_2d(
                                        ui,
                                        &mut self.doc2d,
                                        &mut self.selected_widget_2d,
                                        &self.scene,
                                        sel3d,
                                        &mut self.status_note,
                                        true,
                                    );
                                }
                                (MobileTab::Inspect, ViewMode::Mode2D) => {
                                    render_inspector_2d(
                                        ui,
                                        &mut self.doc2d,
                                        &mut self.selected_widget_2d,
                                        &mut self.status_note,
                                        true,
                                        &mut pending_f2d_dl,
                                        &mut pending_rs_dl,
                                    );
                                }
                                (MobileTab::Viewport, _) => {}
                            }
                        });
                    });
            }

            if dirty {
                self.rebuild_outputs();
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |ui| match self.view_mode {
                    ViewMode::Mode3D => self.render_viewport_3d(ui, true),
                    ViewMode::Mode2D => render_viewport_2d(
                        ui,
                        &mut self.doc2d,
                        &mut self.selected_widget_2d,
                        &mut self.dragging_widget_2d,
                        &mut self.snap_enabled,
                        &mut self.status_note,
                        true,
                    ),
                });
        } else {
            egui::Panel::left("left_scene_panel")
                .resizable(true)
                .default_size(276.0)
                .min_size(220.0)
                .max_size(460.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("desktop_left_scroll").show(ui, |ui| {
                        ui.add_space(6.0);
                        match self.view_mode {
                            ViewMode::Mode3D => self.render_scene_panel(ui, &mut dirty, false),
                            ViewMode::Mode2D => {
                                let sel3d = match self.selection {
                                    Some(Selection::Part(i)) => Some(i),
                                    _ => None,
                                };
                                render_scene_panel_2d(
                                    ui,
                                    &mut self.doc2d,
                                    &mut self.selected_widget_2d,
                                    &self.scene,
                                    sel3d,
                                    &mut self.status_note,
                                    false,
                                );
                            }
                        }
                    });
                });

            egui::Panel::right("right_inspector_panel")
                .resizable(true)
                .default_size(340.0)
                .min_size(250.0)
                .max_size(520.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("desktop_right_scroll").show(ui, |ui| {
                        ui.add_space(6.0);
                        match self.view_mode {
                            ViewMode::Mode3D => self.render_inspector(ui, &mut dirty, false),
                            ViewMode::Mode2D => render_inspector_2d(
                                ui,
                                &mut self.doc2d,
                                &mut self.selected_widget_2d,
                                &mut self.status_note,
                                false,
                                &mut pending_f2d_dl,
                                &mut pending_rs_dl,
                            ),
                        }
                    });
                });

            if dirty {
                self.rebuild_outputs();
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |ui| match self.view_mode {
                    ViewMode::Mode3D => self.render_viewport_3d(ui, false),
                    ViewMode::Mode2D => render_viewport_2d(
                        ui,
                        &mut self.doc2d,
                        &mut self.selected_widget_2d,
                        &mut self.dragging_widget_2d,
                        &mut self.snap_enabled,
                        &mut self.status_note,
                        false,
                    ),
                });
        }

        if let Some((file, bytes)) = pending_f2d_dl {
            match trigger_download(&file, &bytes, "application/octet-stream") {
                Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                Ok(None) => self.status_note = format!("Exportado {file} ({} B)", bytes.len()),
                Err(err) => self.status_note = err,
            }
        }
        if let Some((file, code)) = pending_rs_dl {
            match trigger_download(&file, code.as_bytes(), "text/plain") {
                Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                Ok(None) => self.status_note = format!("Exportado {file}"),
                Err(err) => self.status_note = err,
            }
        }
    }
}

fn recompute_mesh_normals(mesh: &mut MeshPart) {
    let mut acc = vec![Vec3::ZERO; mesh.vertices.len()];
    let mut bounds = forja3d::Aabb::EMPTY;
    for v in &mesh.vertices {
        bounds.include_point(v.position);
    }
    mesh.bounds = bounds;

    for tri in mesh.indices.chunks_exact(3) {
        let (Some(va), Some(vb), Some(vc)) = (
            mesh.vertices.get(tri[0] as usize),
            mesh.vertices.get(tri[1] as usize),
            mesh.vertices.get(tri[2] as usize),
        ) else {
            continue;
        };
        let pa = Vec3::new(va.position[0], va.position[1], va.position[2]);
        let pb = Vec3::new(vb.position[0], vb.position[1], vb.position[2]);
        let pc = Vec3::new(vc.position[0], vc.position[1], vc.position[2]);
        let w = (pb - pa).cross(pc - pa);
        acc[tri[0] as usize] = acc[tri[0] as usize] + w;
        acc[tri[1] as usize] = acc[tri[1] as usize] + w;
        acc[tri[2] as usize] = acc[tri[2] as usize] + w;
    }
    for (i, v) in mesh.vertices.iter_mut().enumerate() {
        let n = acc[i];
        if n.dot(n) > 1e-8 {
            v.normal = n.normalized().to_array();
        }
    }
}

fn cross_v(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot_v(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize_v(v: [f32; 3]) -> [f32; 3] {
    let l = dot_v(v, v).sqrt();
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn mat3_inv_mul(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    let a = [m.cols[0][0], m.cols[0][1], m.cols[0][2]];
    let b = [m.cols[1][0], m.cols[1][1], m.cols[1][2]];
    let c = [m.cols[2][0], m.cols[2][1], m.cols[2][2]];
    let r0 = cross_v(b, c);
    let r1 = cross_v(c, a);
    let r2 = cross_v(a, b);
    let det = dot_v(a, r0);
    if det.abs() < 1e-7 {
        return v;
    }
    let inv = 1.0 / det;
    [
        dot_v(r0, v) * inv,
        dot_v(r1, v) * inv,
        dot_v(r2, v) * inv,
    ]
}

fn dist_point_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-4 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    p.distance(proj)
}

fn ray_intersects_sphere(origin: [f32; 3], dir: [f32; 3], center: [f32; 3], radius: f32) -> Option<f32> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot_v(dir, dir);
    let half_b = dot_v(oc, dir);
    let c = dot_v(oc, oc) - radius * radius;
    let disc = half_b * half_b - a * c;
    if disc < 0.0 || a <= 1e-8 {
        return None;
    }
    let sqrt_d = disc.sqrt();
    let t1 = (-half_b - sqrt_d) / a;
    if t1 >= 0.0 {
        return Some(t1);
    }
    let t2 = (-half_b + sqrt_d) / a;
    if t2 >= 0.0 {
        return Some(t2);
    }
    None
}

fn rotate_vec3_axis(v: [f32; 3], axis: usize, angle_rad: f32) -> [f32; 3] {
    let (s, c) = angle_rad.sin_cos();
    let out = match axis {
        0 => [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c],
        1 => [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c],
        _ => [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]],
    };
    normalize_v(out)
}

#[cfg(target_os = "android")]
static ANDROID_BASE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<winit::platform::android::activity::AndroidApp> =
    std::sync::OnceLock::new();

#[cfg(target_os = "android")]
static SYSTEM_BARS_CLASS: std::sync::OnceLock<jni::objects::GlobalRef> =
    std::sync::OnceLock::new();

#[cfg(target_os = "android")]
static SYSTEM_BARS_DEX_HEAP: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
const SYSTEM_BARS_DEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classes.dex"));

#[cfg(target_os = "android")]
fn with_android_activity<F, R>(f: F) -> Result<R, String>
where
    F: for<'a, 'b, 'c, 'd> FnOnce(&'a mut jni::JNIEnv<'b>, &'c jni::objects::JObject<'d>) -> R,
{
    if let Some(app) = ANDROID_APP.get() {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
            .map_err(|e| format!("JavaVM::from_raw: {e}"))?;
        let mut guard = vm
            .attach_current_thread()
            .map_err(|e| format!("attach_current_thread: {e}"))?;
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr().cast()) };
        return Ok(f(&mut guard, &activity));
    }
    robius_android_env::with_activity(|env, activity| f(env, activity))
        .map_err(|e| format!("robius_android_env: {e}"))
}

#[cfg(target_os = "android")]
fn jni_take_exception<'local>(env: &mut jni::JNIEnv<'local>) -> Option<String> {
    if !env.exception_check().unwrap_or(false) {
        return None;
    }
    let exc = env.exception_occurred().ok();
    let _ = env.exception_clear();
    if let Some(exc) = exc {
        if let Ok(msg_val) = env.call_method(&exc, "toString", "()Ljava/lang/String;", &[]) {
            if let Ok(msg_obj) = msg_val.l() {
                let jstr: jni::objects::JString<'_> = msg_obj.into();
                let s_opt = env
                    .get_string(&jstr)
                    .ok()
                    .map(|s| s.to_string_lossy().into_owned());
                if let Some(s) = s_opt {
                    return Some(s);
                }
            }
        }
    }
    Some(String::from("Excepción JNI desconocida"))
}

#[cfg(target_os = "android")]
fn android_system_bars_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'_>,
) -> Result<&'static jni::objects::GlobalRef, String> {
    if let Some(cls) = SYSTEM_BARS_CLASS.get() {
        return Ok(cls);
    }
    let dex_buf = SYSTEM_BARS_DEX_HEAP.get_or_init(|| SYSTEM_BARS_DEX.to_vec());
    let byte_buffer = unsafe {
        env.new_direct_byte_buffer(dex_buf.as_ptr() as *mut u8, dex_buf.len())
    }
    .map_err(|e| {
        let ex = jni_take_exception(env).unwrap_or_default();
        format!("new_direct_byte_buffer: {e} {ex}")
    })?;

    let parent_loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .unwrap_or_else(|_| {
            let _ = jni_take_exception(env);
            jni::objects::JObject::null()
        });

    let loader = env
        .new_object(
            "dalvik/system/InMemoryDexClassLoader",
            "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
            &[
                jni::objects::JValueGen::Object(&jni::objects::JObject::from(byte_buffer)),
                jni::objects::JValueGen::Object(&parent_loader),
            ],
        )
        .map_err(|e| {
            let ex = jni_take_exception(env).unwrap_or_default();
            format!("InMemoryDexClassLoader: {e} {ex}")
        })?;

    let class_name = env
        .new_string("dev.aethravox.forja3d.SystemBarsHelper")
        .map_err(|e| format!("new_string: {e}"))?;
    let cls_obj = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValueGen::Object(&jni::objects::JObject::from(
                class_name,
            ))],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            let ex = jni_take_exception(env).unwrap_or_default();
            format!("loadClass: {e} {ex}")
        })?;
    let global = env
        .new_global_ref(cls_obj)
        .map_err(|e| format!("new_global_ref: {e}"))?;
    Ok(SYSTEM_BARS_CLASS.get_or_init(|| global))
}

#[cfg(target_os = "android")]
fn android_enable_immersive() {
    let _ = with_android_activity(|env, activity| {
        if let Ok(cls) = android_system_bars_class(env, activity) {
            if env
                .call_static_method(
                    cls,
                    "enableImmersive",
                    "(Landroid/app/Activity;)V",
                    &[jni::objects::JValueGen::Object(activity)],
                )
                .is_err()
            {
                let _ = jni_take_exception(env);
            }
        }
    });
}

#[cfg(target_os = "android")]
fn android_open_file_picker() -> Result<(), String> {
    with_android_activity(|env, activity| -> Result<(), String> {
        let cls = android_system_bars_class(env, activity)?;
        let res = env
            .call_static_method(
                cls,
                "openFilePicker",
                "(Landroid/app/Activity;)Ljava/lang/String;",
                &[jni::objects::JValueGen::Object(activity)],
            )
            .and_then(|v| v.l())
            .map_err(|e| {
                let ex = jni_take_exception(env).unwrap_or_default();
                format!("openFilePicker: {e} {ex}")
            })?;
        let jstr: jni::objects::JString<'_> = res.into();
        let status = env
            .get_string(&jstr)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|_| String::from("ERR:Cadena vacía"));
        if status == "OK" {
            Ok(())
        } else {
            Err(status.strip_prefix("ERR:").unwrap_or(&status).to_string())
        }
    })?
}

#[cfg(target_os = "android")]
fn android_poll_file_picker() -> Option<Result<(String, Vec<u8>), Option<String>>> {
    with_android_activity(|env, activity| -> Option<Result<(String, Vec<u8>), Option<String>>> {
        let cls = android_system_bars_class(env, activity).ok()?;
        let state = env
            .call_static_method(cls, "pollPickState", "()I", &[])
            .and_then(|v| v.i())
            .unwrap_or_else(|_| {
                let _ = jni_take_exception(env);
                0
            });
        match state {
            2 => {
                let name = env
                    .call_static_method(cls, "takePickedName", "()Ljava/lang/String;", &[])
                    .and_then(|v| v.l())
                    .ok()
                    .and_then(|obj| {
                        if obj.is_null() {
                            None
                        } else {
                            let jstr: jni::objects::JString<'_> = obj.into();
                            env.get_string(&jstr)
                                .ok()
                                .map(|s| s.to_string_lossy().into_owned())
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| String::from("modelo.glb"));
                let bytes_obj = env
                    .call_static_method(cls, "takePickedBytes", "()[B", &[])
                    .and_then(|v| v.l())
                    .ok()?;
                if bytes_obj.is_null() {
                    return Some(Err(Some(String::from("Archivo vacío en Android"))));
                }
                let arr: jni::objects::JByteArray<'_> = bytes_obj.into();
                match env.convert_byte_array(&arr) {
                    Ok(bytes) => Some(Ok((name, bytes))),
                    Err(e) => Some(Err(Some(format!("Error leyendo bytes: {e}")))),
                }
            }
            3 => {
                let _ = env.call_static_method(cls, "clearPickState", "()V", &[]);
                Some(Err(None))
            }
            4 => {
                let err = env
                    .call_static_method(cls, "takePickedError", "()Ljava/lang/String;", &[])
                    .and_then(|v| v.l())
                    .ok()
                    .and_then(|obj| {
                        if obj.is_null() {
                            None
                        } else {
                            let jstr: jni::objects::JString<'_> = obj.into();
                            env.get_string(&jstr)
                                .ok()
                                .map(|s| s.to_string_lossy().into_owned())
                        }
                    })
                    .unwrap_or_else(|| String::from("Error desconocido en selector de Android"));
                Some(Err(Some(err)))
            }
            _ => None,
        }
    })
    .ok()
    .flatten()
}

#[cfg(target_os = "android")]
fn android_safe_insets_dp(ctx: &egui::Context, screen_rect: egui::Rect) -> [f32; 4] {
    let ppp = ctx.pixels_per_point().max(1.0);
    let jni_px = with_android_activity(|env, activity| -> [i32; 4] {
        let Ok(cls) = android_system_bars_class(env, activity) else {
            return [0; 4];
        };
        let Ok(val) = env.call_static_method(
            cls,
            "getSafeInsetsPx",
            "(Landroid/app/Activity;)[I",
            &[jni::objects::JValueGen::Object(activity)],
        ) else {
            let _ = jni_take_exception(env);
            return [0; 4];
        };
        let Ok(obj) = val.l() else {
            return [0; 4];
        };
        if obj.is_null() {
            return [0; 4];
        }
        let arr: jni::objects::JIntArray<'_> = obj.into();
        let mut buf = [0_i32; 4];
        let _ = env.get_int_array_region(&arr, 0, &mut buf);
        buf
    })
    .unwrap_or([0; 4]);

    let mut left_px = jni_px[0].max(0) as f32;
    let mut top_px = jni_px[1].max(0) as f32;
    let mut right_px = jni_px[2].max(0) as f32;
    let mut bottom_px = jni_px[3].max(0) as f32;

    if let Some(app) = ANDROID_APP.get() {
        let cr = app.content_rect();
        let phys_w = screen_rect.width() * ppp;
        let phys_h = screen_rect.height() * ppp;
        if cr.right > cr.left && cr.bottom > cr.top && phys_w > 0.0 && phys_h > 0.0 {
            let cr_l = (cr.left.max(0) as f32).min(phys_w * 0.25);
            let cr_t = (cr.top.max(0) as f32).min(phys_h * 0.25);
            let cr_r = ((phys_w - cr.right as f32).max(0.0)).min(phys_w * 0.25);
            let cr_b = ((phys_h - cr.bottom as f32).max(0.0)).min(phys_h * 0.25);
            left_px = left_px.max(cr_l);
            top_px = top_px.max(cr_t);
            right_px = right_px.max(cr_r);
            bottom_px = bottom_px.max(cr_b);
        }
    }

    [
        (left_px / ppp).min(screen_rect.width() * 0.2),
        (top_px / ppp).min(screen_rect.height() * 0.2),
        (right_px / ppp).min(screen_rect.width() * 0.2),
        (bottom_px / ppp).min(screen_rect.height() * 0.2),
    ]
}

#[cfg(target_os = "android")]
fn android_storage_root() -> std::path::PathBuf {
    ANDROID_BASE_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::path::PathBuf::from("/storage/emulated/0/Download/Forja3D"))
}

impl StudioApp {
    fn request_import(&mut self, ctx: &egui::Context, accept: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            trigger_import_model(ctx, &self.pending_import, accept, &mut self.status_note);
        }
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            let _ = ctx;
            let mut exts: Vec<String> = accept
                .split(',')
                .map(|s| s.trim().trim_start_matches('.'))
                .filter(|s| !s.is_empty())
                .flat_map(|s| [s.to_ascii_lowercase(), s.to_ascii_uppercase()])
                .collect();
            exts.dedup();
            let ext_refs: Vec<&str> = exts.iter().map(String::as_str).collect();
            let picked = rfd::FileDialog::new()
                .set_title("Importar Forja3D (.f3d / .f3a / .f3p / .f2d / .f3t / .glb)")
                .add_filter("Assets Forja3D & 3D", &ext_refs)
                .add_filter("Todos los archivos", &["*"])
                .pick_file();
            if let Some(path) = picked {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("modelo.glb")
                            .to_string();
                        self.import_model_bytes(Some(&name), &bytes, path.parent());
                    }
                    Err(err) => {
                        self.status_note = format!("No se pudo leer el archivo: {err}");
                    }
                }
            } else {
                self.status_note = String::from("Importación cancelada");
            }
        }
        #[cfg(target_os = "android")]
        {
            let _ = (accept, ctx);
            match android_open_file_picker() {
                Ok(()) => {
                    self.android_picking = true;
                    self.status_note = String::from("Abriendo gestor de archivos...");
                }
                Err(err) => {
                    self.android_picking = false;
                    self.status_note = format!("No se pudo abrir el gestor de archivos: {err}");
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn trigger_download(filename: &str, data: &[u8], mime: &str) -> Result<Option<String>, String> {
    use eframe::wasm_bindgen::JsCast;
    let Some(window) = eframe::web_sys::window() else {
        return Err(String::from("Sin ventana de navegador"));
    };
    let Some(document) = window.document() else {
        return Err(String::from("Sin documento DOM"));
    };

    let uint8 = js_sys::Uint8Array::from(data);
    let array = js_sys::Array::new();
    array.push(&uint8.buffer());

    let opts = eframe::web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = eframe::web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &opts)
        .map_err(|_| String::from("Error creando Blob de descarga"))?;
    let url = eframe::web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| String::from("Error creando URL temporal"))?;
    if let Ok(el) = document.create_element("a") {
        if let Ok(anchor) = el.dyn_into::<eframe::web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download(filename);
            anchor.click();
        }
    }
    let _ = eframe::web_sys::Url::revoke_object_url(&url);
    Ok(None)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn trigger_download(filename: &str, data: &[u8], _mime: &str) -> Result<Option<String>, String> {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("bin");
    if let Some(path) = rfd::FileDialog::new()
        .set_title("Guardar exportación de Forja3D")
        .set_file_name(filename)
        .add_filter("Archivo exportado", &[ext])
        .save_file()
    {
        std::fs::write(&path, data).map_err(|e| format!("Error al guardar: {e}"))?;
        Ok(Some(path.display().to_string()))
    } else {
        Err(String::from("Guardado cancelado"))
    }
}

#[cfg(target_os = "android")]
fn trigger_download(filename: &str, data: &[u8], _mime: &str) -> Result<Option<String>, String> {
    let download_dir = std::path::PathBuf::from("/storage/emulated/0/Download/Forja3D");
    if std::fs::create_dir_all(&download_dir).is_ok() {
        let target = download_dir.join(filename);
        if std::fs::write(&target, data).is_ok() {
            return Ok(Some(target.display().to_string()));
        }
    }
    let exports_dir = android_storage_root().join("exports");
    std::fs::create_dir_all(&exports_dir)
        .map_err(|e| format!("No se pudo crear carpeta de exportación: {e}"))?;
    let target = exports_dir.join(filename);
    std::fs::write(&target, data)
        .map_err(|e| format!("Error escribiendo {}: {e}", target.display()))?;
    Ok(Some(target.display().to_string()))
}

#[cfg(target_arch = "wasm32")]
fn trigger_import_model(
    ctx: &egui::Context,
    pending: &Arc<Mutex<Option<(String, Vec<u8>, Option<std::path::PathBuf>)>>>,
    accept: &str,
    status_note: &mut String,
) {
    use eframe::wasm_bindgen::{closure::Closure, JsCast};
    let Some(window) = eframe::web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(el) = document.create_element("input") else { return };
    let Ok(input) = el.dyn_into::<eframe::web_sys::HtmlInputElement>() else { return };
    input.set_type("file");
    if accept.contains(".f3") || accept.contains(".f2") || accept.contains(".glb") || accept.contains(".gltf") {
        // Keep picker open to all files so mobile/case-sensitive browsers never hide .F3T/.F3D/.GLB.
        input.set_accept("*/*");
    } else {
        input.set_accept(accept);
    }
    let _ = input.set_attribute("style", "display:none");
    if let Some(body) = document.body() {
        let _ = body.append_child(&input);
    }

    let pending_clone = Arc::clone(pending);
    let ctx_clone = ctx.clone();
    let input_ref = input.clone();
    let onchange = Closure::wrap(Box::new(move |_event: eframe::web_sys::Event| {
        let files_opt = input_ref.files();
        input_ref.remove();
        let Some(files) = files_opt else { return };
        let Some(file) = files.get(0) else { return };
        let name = file.name();
        let promise = file.array_buffer();
        let pending_async = Arc::clone(&pending_clone);
        let ctx_async = ctx_clone.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(buf) = wasm_bindgen_futures::JsFuture::from(promise).await {
                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                if let Ok(mut guard) = pending_async.lock() {
                    *guard = Some((name, bytes, None));
                }
                ctx_async.request_repaint();
            }
        });
    }) as Box<dyn FnMut(_)>);

    input.set_onchange(Some(onchange.as_ref().unchecked_ref()));
    onchange.forget();
    input.click();
    *status_note = format!("Selecciona un archivo ({accept})...");
}

#[cfg(target_arch = "wasm32")]
mod wasm_entry {
    use super::StudioApp;
    use eframe::wasm_bindgen::{self, prelude::*, JsCast};

    #[wasm_bindgen]
    pub fn start_forja3d_canvas(canvas_id: String) -> Result<(), JsValue> {
        let window = eframe::web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
        let document = window.document().ok_or_else(|| JsValue::from_str("No document"))?;
        let canvas = document
            .get_element_by_id(&canvas_id)
            .ok_or_else(|| JsValue::from_str("Canvas not found"))?
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()?;

        let web_options = eframe::WebOptions::default();
        wasm_bindgen_futures::spawn_local(async move {
            let runner = eframe::WebRunner::new();
            let _ = runner
                .start(
                    canvas,
                    web_options,
                    Box::new(|cc| Ok(Box::new(StudioApp::new(cc)))),
                )
                .await;
        });
        Ok(())
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let _ = ANDROID_APP.set(app.clone());
    app.set_window_flags(
        winit::platform::android::activity::WindowManagerFlags::FULLSCREEN
            | winit::platform::android::activity::WindowManagerFlags::LAYOUT_IN_SCREEN,
        winit::platform::android::activity::WindowManagerFlags::empty(),
    );
    android_enable_immersive();
    if let Some(root) = app.external_data_path().or_else(|| app.internal_data_path()) {
        let _ = std::fs::create_dir_all(root.join("imports"));
        let _ = std::fs::create_dir_all(root.join("exports"));
        let _ = ANDROID_BASE_DIR.set(root);
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        depth_buffer: 24,
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Forja3D Studio",
        options,
        Box::new(|cc| Ok(Box::new(StudioApp::new(cc)))),
    );
}
