mod gltf_import;
mod renderer;

use eframe::egui;
use forja3d::{
    default_extrude_profile, default_lathe_profile, format::fnv1a_32, ColliderShape, Model,
    PartSpec, PrimitiveKind, RigidBodyKind, RustExportTarget, SceneDocument, ShadingMode,
    FLAG_COMPACT, FLAG_PHYSICS,
};
use forja3d_physics::{BodyHandle, PhysicsWorld};
use renderer::{ray_intersects_aabb, GlViewportRenderer, OrbitCamera};
use std::sync::{Arc, Mutex};

const MAX_UNDO_HISTORY: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportTab {
    F3dBinary,
    RustSource,
    WavefrontObj,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobileTab {
    Viewport,
    Scene,
    Inspect,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

impl GizmoMode {
    const fn label(self) -> &'static str {
        match self {
            Self::Translate => "Mover",
            Self::Rotate => "Rotar",
            Self::Scale => "Escalar",
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
    accum_pixels: f32,
}

pub struct StudioApp {
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
    sim_body: Option<BodyHandle>,
    sim_offset: [f32; 3],
    sim_last_time: Option<f64>,
    gizmo_mode: GizmoMode,
    active_gizmo_drag: Option<ActiveGizmoDrag>,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    continuous_edit_snapshot: Option<HistoryEntry>,
    selected_profile_pt: Option<usize>,
    dragging_profile_pt: Option<usize>,
    new_group_draft: String,
    export_tab: ExportTab,
    mobile_tab: MobileTab,
    rust_target: RustExportTarget,
    f3d_compact: bool,
    cached_f3d_bytes: Vec<u8>,
    cached_f3d_compact_bytes: Vec<u8>,
    cached_rust_src: String,
    cached_obj_src: String,
    pending_import: Arc<Mutex<Option<(String, Vec<u8>, Option<std::path::PathBuf>)>>>,
    import_modal_open: bool,
    import_modal_filter: String,
    import_modal_files: Vec<(String, std::path::PathBuf, u64)>,
    import_path_draft: String,
    status_note: String,
    pub lang_table: langes::LangTable,
    gl_renderer: Arc<Mutex<Option<GlViewportRenderer>>>,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
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

        Self {
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
            sim_body: None,
            sim_offset: [0.0; 3],
            sim_last_time: None,
            gizmo_mode: GizmoMode::Translate,
            active_gizmo_drag: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            continuous_edit_snapshot: None,
            selected_profile_pt: Some(0),
            dragging_profile_pt: None,
            new_group_draft: String::from("Grupo"),
            export_tab: ExportTab::F3dBinary,
            mobile_tab: MobileTab::Viewport,
            rust_target,
            f3d_compact: false,
            cached_f3d_bytes,
            cached_f3d_compact_bytes,
            cached_rust_src,
            cached_obj_src,
            pending_import: Arc::new(Mutex::new(None)),
            import_modal_open: false,
            import_modal_filter: String::from(".glb,.gltf,.f3d"),
            import_modal_files: Vec::new(),
            import_path_draft: String::new(),
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
            self.rebuild_outputs();
            self.status_note = String::from("Rehacer aplicado");
        }
    }

    fn stop_physics_sim(&mut self) {
        self.sim_world = None;
        self.sim_body = None;
        self.sim_offset = [0.0; 3];
        self.sim_last_time = None;
    }

    fn start_physics_sim(&mut self, drop_y: f32) {
        let mut world = PhysicsWorld::new();
        let is_dynamic = self
            .baked_model
            .physics
            .as_ref()
            .map_or(true, |p| p.body_kind == RigidBodyKind::Dynamic);
        let start_y = if is_dynamic { drop_y } else { 0.0 };
        let handle = world.spawn_from_model(&self.baked_model, [0.0, start_y, 0.0]);
        self.sim_world = Some(world);
        self.sim_body = Some(handle);
        self.sim_offset = [0.0, start_y, 0.0];
        self.sim_last_time = None;
        self.status_note = if is_dynamic {
            String::from("Simulación física activa (forja3d-physics)")
        } else {
            String::from("Cuerpo no dinámico (cambia Tipo a Dinámico para caer)")
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
        if let Some(handle) = self.sim_body {
            if let Some(body) = world.body(handle) {
                self.sim_offset = body.position;
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
        self.cached_rust_src = self.baked_model.to_rust_source(&self.scene.model_name, self.rust_target);
        self.cached_obj_src = self.baked_model.to_obj();
    }

    fn import_model_bytes(
        &mut self,
        filename: Option<&str>,
        bytes: &[u8],
        base_dir: Option<&std::path::Path>,
    ) {
        let lower = filename.unwrap_or("").to_ascii_lowercase();
        let is_f3d_magic = bytes.len() >= 4 && &bytes[0..4] == &forja3d::MAGIC;
        let is_glb_magic = bytes.len() >= 4 && &bytes[0..4] == b"glTF";
        let is_gltf = lower.ends_with(".glb") || lower.ends_with(".gltf") || is_glb_magic;

        if is_f3d_magic || (lower.ends_with(".f3d") && !is_gltf) {
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
                    self.selection = if self.scene.parts.is_empty() {
                        None
                    } else {
                        Some(Selection::Part(0))
                    };
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
                }
                Err(err) => {
                    self.status_note = format!("Error al importar .f3d: {err}");
                }
            }
        } else {
            match gltf_import::import_gltf_or_glb(filename, bytes, base_dir) {
                Ok(scene) => {
                    self.push_undo();
                    self.scene = scene;
                    self.selection = if self.scene.parts.is_empty() {
                        None
                    } else {
                        Some(Selection::Part(0))
                    };
                    self.rebuild_outputs();
                    self.camera.frame_bounds(self.baked_model.bounds());
                    let src = filename.unwrap_or(&self.scene.model_name);
                    self.status_note = format!(
                        "GLB/GLTF '{src}' ({} piezas, {} tri)",
                        self.scene.parts.len(),
                        self.baked_model.triangle_count()
                    );
                }
                Err(err) => {
                    self.status_note = err;
                }
            }
        }
    }

    fn load_preset(&mut self, scene: SceneDocument) {
        self.push_undo();
        self.scene = scene;
        self.selection = if self.scene.parts.is_empty() {
            None
        } else {
            Some(Selection::Part(0))
        };
        self.rebuild_outputs();
        self.status_note = format!("Escena: {}", self.scene.model_name);
    }

    fn add_primitive(&mut self, kind: PrimitiveKind) {
        self.push_undo();
        let idx = self.scene.parts.len() + 1;
        let mut part = PartSpec::new(format!("{}_{}", kind.label().to_lowercase(), idx), kind);
        part.position = [((idx % 3) as f32 - 1.0) * 0.4, 0.5, 0.0];
        if let Some(Selection::Group(g)) = &self.selection {
            part.group = Some(g.clone());
        }
        self.scene.parts.push(part);
        self.selection = Some(Selection::Part(self.scene.parts.len() - 1));
        self.rebuild_outputs();
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
        if !compact {
            ui.add_space(6.0);
            ui.heading("Forja3D Studio");
            ui.label(
                egui::RichText::new("Motor 3D en Rust · .f3d & .rs")
                    .size(11.5)
                    .color(egui::Color32::from_gray(145)),
            );
            ui.separator();
        }

        ui.label(egui::RichText::new("Modelo").strong());
        if ui.text_edit_singleline(&mut self.scene.model_name).changed() {
            *dirty = true;
        }

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Escenas base").strong());
        let btn_h = if compact { 34.0 } else { 24.0 };
        ui.horizontal_wrapped(|ui| {
            if ui.add_sized([74.0, btn_h], egui::Button::new("Dron")).clicked() {
                self.load_preset(SceneDocument::preset_drone());
            }
            if ui.add_sized([82.0, btn_h], egui::Button::new("Monolito")).clicked() {
                self.load_preset(SceneDocument::preset_obelisk());
            }
            if ui.add_sized([70.0, btn_h], egui::Button::new("Vacío")).clicked() {
                self.load_preset(SceneDocument::new("MiModelo"));
            }
            if ui
                .add_sized([124.0, btn_h], egui::Button::new("Importar GLB/GLTF"))
                .on_hover_text("Importar archivo .glb / .gltf con el crate gltf")
                .clicked()
            {
                self.request_import(ui.ctx(), ".glb,.gltf");
            }
            if ui
                .add_sized([92.0, btn_h], egui::Button::new("Abrir .f3d"))
                .on_hover_text("Importar archivo .f3d / .glb / .gltf")
                .clicked()
            {
                self.request_import(ui.ctx(), ".f3d,.glb,.gltf");
            }
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Añadir primitiva").strong());
        let cols = if compact { 3 } else { 2 };
        let prim_w = if compact { 98.0 } else { 100.0 };
        let prim_h = if compact { 34.0 } else { 26.0 };
        egui::Grid::new("add_primitives_grid")
            .num_columns(cols)
            .spacing([6.0, 6.0])
            .show(ui, |ui| {
                for (i, kind) in PrimitiveKind::ALL.iter().enumerate() {
                    if ui
                        .add_sized([prim_w, prim_h], egui::Button::new(format!("+ {}", kind.label())))
                        .clicked()
                    {
                        self.add_primitive(*kind);
                    }
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });

        // Groups Section
        let groups = self.scene.group_names();
        ui.add_space(10.0);
        ui.separator();
        ui.label(egui::RichText::new(format!("Grupos ({})", groups.len())).strong());
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
                    let mut vis = all_vis;
                    if ui.checkbox(&mut vis, "").on_hover_text("Mostrar/ocultar grupo").changed() {
                        toggle_group_vis = Some(g.clone());
                    }
                    if ui.selectable_label(is_sel, format!("[G] {g} ({count})")).clicked() {
                        self.selection = Some(Selection::Group(g.clone()));
                        self.status_note = format!("Grupo '{g}' seleccionado");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dup_sz = if compact { [44.0, 28.0] } else { [34.0, 20.0] };
                        if ui.add_sized(dup_sz, egui::Button::new("Dup")).on_hover_text("Duplicar grupo").clicked() {
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

        ui.add_space(10.0);
        ui.separator();
        ui.label(egui::RichText::new(format!("Piezas ({})", self.scene.parts.len())).strong());

        let mut to_delete: Option<usize> = None;
        let mut to_duplicate: Option<usize> = None;
        let mut vis_toggled = false;

        egui::ScrollArea::vertical().id_salt("parts_list").show(ui, |ui| {
            for (idx, part) in self.scene.parts.iter_mut().enumerate() {
                let is_sel = self.selection == Some(Selection::Part(idx));
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut part.visible, "").changed() {
                        vis_toggled = true;
                    }
                    let mirror_tag = if part.mirror.iter().any(|&m| m) { " [M]" } else { "" };
                    let label = format!("{}{}", part.name, mirror_tag);
                    if ui.selectable_label(is_sel, label).clicked() {
                        self.selection = Some(Selection::Part(idx));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let action_sz = if compact { [34.0, 30.0] } else { [24.0, 20.0] };
                        if ui.add_sized(action_sz, egui::Button::new("×")).on_hover_text("Eliminar").clicked() {
                            to_delete = Some(idx);
                        }
                        let dup_sz = if compact { [44.0, 30.0] } else { [34.0, 20.0] };
                        if ui.add_sized(dup_sz, egui::Button::new("Dup")).on_hover_text("Duplicar").clicked() {
                            to_duplicate = Some(idx);
                        }
                    });
                });
            }
        });

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
            *dirty = true;
        }
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui, dirty: &mut bool, compact: bool) {
        let btn_h = if compact { 32.0 } else { 22.0 };
        match self.selection.clone() {
            Some(Selection::Group(group_name)) => {
                let count = self
                    .scene
                    .parts
                    .iter()
                    .filter(|p| p.group.as_deref() == Some(group_name.as_str()))
                    .count();
                ui.label(egui::RichText::new(format!("Grupo: {group_name} ({count} piezas)")).strong());
                ui.label(
                    egui::RichText::new("Arrastra el gizmo 3D para mover, rotar o escalar todo el grupo.")
                        .size(11.5)
                        .color(egui::Color32::from_gray(165)),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_sized([120.0, btn_h], egui::Button::new("Espejo X grupo")).clicked() {
                        self.push_undo();
                        for p in &mut self.scene.parts {
                            if p.group.as_deref() == Some(group_name.as_str()) {
                                p.mirror[0] = !p.mirror[0];
                            }
                        }
                        *dirty = true;
                    }
                    if ui.add_sized([110.0, btn_h], egui::Button::new("Desagrupar")).clicked() {
                        self.push_undo();
                        let mut first_idx = None;
                        for (i, p) in self.scene.parts.iter_mut().enumerate() {
                            if p.group.as_deref() == Some(group_name.as_str()) {
                                p.group = None;
                                if first_idx.is_none() {
                                    first_idx = Some(i);
                                }
                            }
                        }
                        self.selection = first_idx.map(Selection::Part);
                        *dirty = true;
                    }
                });
            }
            Some(Selection::Part(sel_idx)) => {
                if sel_idx >= self.scene.parts.len() {
                    ui.label("Toca una pieza en el 3D o añádela en Piezas.");
                    return;
                }
                let snap = self.snap_enabled;
                let existing_groups = self.scene.group_names();
                let (part_verts, part_tris) = self
                    .part_map
                    .iter()
                    .enumerate()
                    .filter(|(_, &src)| src == sel_idx)
                    .fold((0usize, 0usize), |(vc, tc), (m_idx, _)| {
                        let m = &self.baked_model.parts[m_idx];
                        (vc + m.vertices.len(), tc + m.indices.len() / 3)
                    });
                let mut changed_continuous = false;
                let mut discrete_mirror_toggle: Option<usize> = None;
                let mut discrete_shading_change: Option<ShadingMode> = None;
                let mut discrete_dedup_toggle: Option<bool> = None;
                let mut discrete_profile_replace: Option<(Vec<[f32; 2]>, Option<usize>, &'static str)> = None;
                let mut new_group_assign: Option<Option<String>> = None;
                let mut discrete_collider_change: Option<ColliderShape> = None;
                let mut discrete_autofit_toggle: Option<bool> = None;
                let mut selected_pt = self.selected_profile_pt;
                let mut dragging_pt = self.dragging_profile_pt;

                {
                    let part = &mut self.scene.parts[sel_idx];
                    ui.label(egui::RichText::new("Inspector de pieza").strong());
                    if part.kind == PrimitiveKind::CustomMesh {
                        ui.label(
                            egui::RichText::new("Malla importada (.glb / .gltf / .f3d) · editable con gizmo/espejo/sombreado")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(251, 191, 36)),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Nombre:");
                        if ui.text_edit_singleline(&mut part.name).changed() {
                            *dirty = true;
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
                                    .range(0.05..=20.0)
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

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut rgba = [part.color[0], part.color[1], part.color[2], part.color[3]];
                        if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                            part.color = rgba;
                            changed_continuous = true;
                            *dirty = true;
                        }
                        ui.label("Seg:");
                        if ui.add(egui::DragValue::new(&mut part.segments).range(6..=48)).changed() {
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
                            .on_hover_text("Fusiona vértices idénticos para reducir tamaño .f3d/.rs")
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
                            let resp = ui.add(
                                egui::Slider::new(&mut part.extrude_depth, 0.05..=3.0)
                                    .text("Profundidad Z"),
                            );
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
                        if let Some(s) = selected_pt {
                            if s >= part.profile.len() {
                                selected_pt = Some(part.profile.len().saturating_sub(1));
                            }
                        } else {
                            selected_pt = Some(0);
                        }

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
                            if is_lathe {
                                x = x.clamp(0.0, 0.95);
                            } else {
                                x = x.clamp(-0.95, 0.95);
                            }
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

                        // Canvas background + grid
                        painter.rect_filled(crect, 6.0, egui::Color32::from_rgb(11, 14, 20));
                        painter.rect_stroke(
                            crect,
                            6.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 48, 60)),
                            egui::StrokeKind::Inside,
                        );
                        let grid_col = egui::Color32::from_rgb(27, 32, 42);
                        for step in [-0.75_f32, -0.5, -0.25, 0.25, 0.5, 0.75] {
                            let gx = origin.x + step * scale;
                            if gx > crect.left() + 2.0 && gx < crect.right() - 2.0 {
                                painter.line_segment(
                                    [egui::pos2(gx, crect.top() + 2.0), egui::pos2(gx, crect.bottom() - 2.0)],
                                    egui::Stroke::new(1.0, grid_col),
                                );
                            }
                            let gy = origin.y - step * scale;
                            if gy > crect.top() + 2.0 && gy < crect.bottom() - 2.0 {
                                painter.line_segment(
                                    [egui::pos2(crect.left() + 2.0, gy), egui::pos2(crect.right() - 2.0, gy)],
                                    egui::Stroke::new(1.0, grid_col),
                                );
                            }
                        }
                        // Axes
                        painter.line_segment(
                            [egui::pos2(crect.left() + 2.0, origin.y), egui::pos2(crect.right() - 2.0, origin.y)],
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 66, 82)),
                        );
                        let y_axis_col = if is_lathe {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(58, 66, 82)
                        };
                        painter.line_segment(
                            [egui::pos2(origin.x, crect.top() + 2.0), egui::pos2(origin.x, crect.bottom() - 2.0)],
                            egui::Stroke::new(if is_lathe { 1.6 } else { 1.0 }, y_axis_col),
                        );

                        // Interaction: pick or drag 2D profile point
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

                        // Draw Lathe mirrored ghost silhouette
                        if is_lathe && part.profile.len() >= 2 {
                            let ghost_pts: Vec<egui::Pos2> = part
                                .profile
                                .iter()
                                .map(|&[r, y]| to_px([-r.max(0.0), y]))
                                .collect();
                            for w in ghost_pts.windows(2) {
                                painter.line_segment(
                                    [w[0], w[1]],
                                    egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(120, 145, 180, 90)),
                                );
                            }
                        }

                        // Draw profile edges
                        let screen_pts: Vec<egui::Pos2> = part.profile.iter().map(|&p| to_px(p)).collect();
                        let edge_col = egui::Color32::from_rgb(56, 189, 248);
                        if is_lathe {
                            for w in screen_pts.windows(2) {
                                painter.line_segment([w[0], w[1]], egui::Stroke::new(2.0, edge_col));
                            }
                        } else if screen_pts.len() >= 2 {
                            for i in 0..screen_pts.len() {
                                let a = screen_pts[i];
                                let b = screen_pts[(i + 1) % screen_pts.len()];
                                painter.line_segment([a, b], egui::Stroke::new(2.0, edge_col));
                            }
                        }

                        // Draw vertex handles
                        let pt_r = if compact { 6.5 } else { 4.8 };
                        for (i, &pos_px) in screen_pts.iter().enumerate() {
                            let is_sel_pt = selected_pt == Some(i);
                            let fill = if is_sel_pt {
                                egui::Color32::from_rgb(251, 191, 36)
                            } else {
                                egui::Color32::from_rgb(226, 232, 240)
                            };
                            painter.circle_filled(pos_px, pt_r, fill);
                            painter.circle_stroke(
                                pos_px,
                                pt_r + 1.0,
                                egui::Stroke::new(1.2, egui::Color32::from_rgb(15, 23, 42)),
                            );
                        }

                        // Selected point numeric coords + Add/Remove point
                        if let Some(s_idx) = selected_pt.filter(|&i| i < part.profile.len()) {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("P{}:", s_idx + 1)).strong());
                                let min_x = if is_lathe { 0.0 } else { -0.95 };
                                let x_lbl = if is_lathe { "R" } else { "X" };
                                ui.label(x_lbl);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut part.profile[s_idx][0])
                                            .speed(0.01)
                                            .range(min_x..=0.95)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                                ui.label("Y");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut part.profile[s_idx][1])
                                            .speed(0.01)
                                            .range(-0.95..=0.95)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    changed_continuous = true;
                                    *dirty = true;
                                }
                            });
                        }

                        let min_pts = if is_lathe { 2 } else { 3 };
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_sized([74.0, btn_h], egui::Button::new("+ Punto")).clicked() {
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
                            if ui
                                .add_enabled(
                                    part.profile.len() > min_pts,
                                    egui::Button::new("- Punto").min_size(egui::vec2(74.0, btn_h)),
                                )
                                .clicked()
                            {
                                let mut next_prof = part.profile.clone();
                                let cur = selected_pt.unwrap_or(0).min(next_prof.len() - 1);
                                next_prof.remove(cur);
                                let new_sel = cur.min(next_prof.len() - 1);
                                discrete_profile_replace = Some((next_prof, Some(new_sel), "Punto eliminado del perfil"));
                            }

                            if is_lathe {
                                if ui.add_sized([68.0, btn_h], egui::Button::new("Tobera")).clicked() {
                                    discrete_profile_replace =
                                        Some((default_lathe_profile(), Some(0), "Perfil: Tobera"));
                                }
                                if ui.add_sized([58.0, btn_h], egui::Button::new("Copa")).clicked() {
                                    discrete_profile_replace = Some((
                                        vec![
                                            [0.0, -0.5],
                                            [0.32, -0.5],
                                            [0.08, -0.4],
                                            [0.08, -0.05],
                                            [0.42, 0.25],
                                            [0.36, 0.5],
                                            [0.0, 0.5],
                                        ],
                                        Some(0),
                                        "Perfil: Copa",
                                    ));
                                }
                                if ui.add_sized([66.0, btn_h], egui::Button::new("Cohete")).clicked() {
                                    discrete_profile_replace = Some((
                                        vec![
                                            [0.0, -0.5],
                                            [0.38, -0.5],
                                            [0.32, -0.15],
                                            [0.34, 0.18],
                                            [0.18, 0.42],
                                            [0.0, 0.55],
                                        ],
                                        Some(0),
                                        "Perfil: Cohete",
                                    ));
                                }
                            } else {
                                if ui.add_sized([68.0, btn_h], egui::Button::new("Corona")).clicked() {
                                    discrete_profile_replace =
                                        Some((default_extrude_profile(), Some(0), "Perfil: Corona"));
                                }
                                if ui.add_sized([52.0, btn_h], egui::Button::new("Ala")).clicked() {
                                    discrete_profile_replace = Some((
                                        vec![
                                            [-0.55, -0.22],
                                            [0.55, -0.08],
                                            [0.42, 0.14],
                                            [-0.15, 0.26],
                                            [-0.50, 0.12],
                                        ],
                                        Some(0),
                                        "Perfil: Ala",
                                    ));
                                }
                                if ui.add_sized([68.0, btn_h], egui::Button::new("Perfil L")).clicked() {
                                    discrete_profile_replace = Some((
                                        vec![
                                            [-0.45, -0.45],
                                            [0.45, -0.45],
                                            [0.45, -0.12],
                                            [-0.10, -0.12],
                                            [-0.10, 0.45],
                                            [-0.45, 0.45],
                                        ],
                                        Some(0),
                                        "Perfil: L cóncava",
                                    ));
                                }
                            }
                        });
                    }

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Simetría:");
                        for (axis, lbl) in [(0, "Espejo X"), (1, "Espejo Y"), (2, "Espejo Z")] {
                            if ui
                                .add_sized([74.0, btn_h], egui::Button::selectable(part.mirror[axis], lbl))
                                .clicked()
                            {
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
                        if !part.collider_auto_fit {
                            ui.horizontal(|ui| {
                                ui.label("Offset:");
                                for axis in 0..3 {
                                    if ui
                                        .add(egui::DragValue::new(&mut part.collider_offset[axis]).speed(0.02).fixed_decimals(2))
                                        .changed()
                                    {
                                        changed_continuous = true;
                                        *dirty = true;
                                    }
                                }
                            });
                            match &mut part.collider {
                                ColliderShape::Box { half_extents } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Mitades:");
                                        for axis in 0..3 {
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut half_extents[axis])
                                                        .speed(0.02)
                                                        .range(0.02..=25.0)
                                                        .fixed_decimals(2),
                                                )
                                                .changed()
                                            {
                                                changed_continuous = true;
                                                *dirty = true;
                                            }
                                        }
                                    });
                                }
                                ColliderShape::Sphere { radius } => {
                                    if ui.add(egui::Slider::new(radius, 0.02..=25.0).text("Radio")).changed() {
                                        changed_continuous = true;
                                        *dirty = true;
                                    }
                                }
                                ColliderShape::Capsule { radius, half_height } => {
                                    if ui.add(egui::Slider::new(radius, 0.02..=25.0).text("Radio")).changed() {
                                        changed_continuous = true;
                                        *dirty = true;
                                    }
                                    if ui.add(egui::Slider::new(half_height, 0.01..=25.0).text("Semi-alto")).changed() {
                                        changed_continuous = true;
                                        *dirty = true;
                                    }
                                }
                                ColliderShape::None => {}
                            }
                        }
                    }
                }

                self.selected_profile_pt = selected_pt;
                self.dragging_profile_pt = dragging_pt;

                if changed_continuous {
                    self.begin_continuous_edit(ui);
                }
                self.flush_continuous_edit(ui);

                if let Some(mode) = discrete_shading_change {
                    self.push_undo();
                    self.scene.parts[sel_idx].shading = mode;
                    self.status_note = format!("Sombreado: {}", mode.label());
                    *dirty = true;
                }
                if let Some(dedup) = discrete_dedup_toggle {
                    self.push_undo();
                    self.scene.parts[sel_idx].deduplicate = dedup;
                    self.status_note = if dedup {
                        String::from("Deduplicación de vértices activada")
                    } else {
                        String::from("Deduplicación desactivada")
                    };
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
                    self.status_note = format!("Colisionador: {}", shape.label());
                    *dirty = true;
                }
                if let Some(auto_fit) = discrete_autofit_toggle {
                    self.push_undo();
                    if !auto_fit {
                        // Seed manual collider dimensions from current baked mesh bounds
                        let baked = self.scene.parts[sel_idx].bake();
                        if let Some(fitted) = self.scene.parts[sel_idx].fitted_collider_for_mesh(&baked) {
                            self.scene.parts[sel_idx].collider = fitted.shape;
                            self.scene.parts[sel_idx].collider_offset = [0.0; 3];
                        }
                    }
                    self.scene.parts[sel_idx].collider_auto_fit = auto_fit;
                    *dirty = true;
                }

                ui.horizontal(|ui| {
                    ui.add_sized([120.0, btn_h], egui::TextEdit::singleline(&mut self.new_group_draft));
                    if ui.add_sized([105.0, btn_h], egui::Button::new("Crear grupo")).clicked() {
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
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Físicas del modelo (.f3d)").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized([116.0, btn_h], egui::Button::new("Auto-colisión"))
                    .on_hover_text("Activa físicas .f3d y genera colisionadores para todas las piezas")
                    .clicked()
                {
                    self.push_undo();
                    self.scene.auto_generate_colliders();
                    self.show_colliders = true;
                    self.status_note = String::from("Físicas y colisionadores generados en .f3d");
                    *dirty = true;
                }
            });
        });

        let mut phys_on = self.scene.physics_enabled;
        if ui
            .checkbox(&mut phys_on, "Embeber físicas en binario .f3d (FLAG_PHYSICS)")
            .changed()
        {
            self.push_undo();
            self.scene.physics_enabled = phys_on;
            if phys_on
                && self
                    .scene
                    .parts
                    .iter()
                    .all(|p| matches!(p.collider, ColliderShape::None))
            {
                self.scene.auto_generate_colliders();
            }
            *dirty = true;
        }

        if self.scene.physics_enabled {
            let mut body_kind_change = None;
            ui.horizontal_wrapped(|ui| {
                ui.label("Tipo:");
                for kind in RigidBodyKind::ALL {
                    if ui
                        .add_sized([74.0, btn_h], egui::Button::selectable(self.scene.physics.body_kind == kind, kind.label()))
                        .clicked()
                        && self.scene.physics.body_kind != kind
                    {
                        body_kind_change = Some(kind);
                    }
                }
            });
            if let Some(kind) = body_kind_change {
                self.push_undo();
                self.scene.physics.body_kind = kind;
                *dirty = true;
            }

            let mut changed_continuous = false;
            if ui
                .add(egui::Slider::new(&mut self.scene.physics.mass, 0.1..=100.0).text("Masa (kg)"))
                .changed()
            {
                changed_continuous = true;
                *dirty = true;
            }
            if ui
                .add(egui::Slider::new(&mut self.scene.physics.friction, 0.0..=1.5).text("Fricción"))
                .changed()
            {
                changed_continuous = true;
                *dirty = true;
            }
            if ui
                .add(egui::Slider::new(&mut self.scene.physics.restitution, 0.0..=1.0).text("Rebote"))
                .changed()
            {
                changed_continuous = true;
                *dirty = true;
            }
            if ui
                .add(egui::Slider::new(&mut self.scene.physics.gravity_scale, -2.0..=3.0).text("Escala gravedad"))
                .changed()
            {
                changed_continuous = true;
                *dirty = true;
            }
            if changed_continuous {
                self.begin_continuous_edit(ui);
            }
            self.flush_continuous_edit(ui);
        }
    }

    fn render_exporter(&mut self, ui: &mut egui::Ui, compact: bool) {
        ui.label(egui::RichText::new("Exportador e Importador").strong());
        if compact && !self.status_note.is_empty() {
            ui.label(
                egui::RichText::new(&self.status_note)
                    .size(11.5)
                    .color(egui::Color32::from_rgb(251, 191, 36)),
            );
        }

        let btn_h = if compact { 34.0 } else { 24.0 };
        let tab_w = if compact { 96.0 } else { 98.0 };
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::F3dBinary, "Formato .f3d"))
                .clicked()
            {
                self.export_tab = ExportTab::F3dBinary;
            }
            if ui
                .add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::RustSource, "Código .rs"))
                .clicked()
            {
                self.export_tab = ExportTab::RustSource;
            }
            if ui
                .add_sized([tab_w, btn_h], egui::Button::selectable(self.export_tab == ExportTab::WavefrontObj, "Malla .obj"))
                .clicked()
            {
                self.export_tab = ExportTab::WavefrontObj;
            }
        });
        ui.add_space(6.0);

        match self.export_tab {
            ExportTab::F3dBinary => {
                ui.checkbox(&mut self.f3d_compact, "Compresión compacta (17 B/vértice + u16 idx)");

                let std_len = self.cached_f3d_bytes.len();
                let cmp_len = self.cached_f3d_compact_bytes.len();
                let savings_pct = if std_len > 0 && cmp_len < std_len {
                    ((std_len - cmp_len) * 100) / std_len
                } else {
                    0
                };

                let active_bytes = if self.f3d_compact {
                    &self.cached_f3d_compact_bytes
                } else {
                    &self.cached_f3d_bytes
                };

                let checksum = if active_bytes.len() >= 16 {
                    u32::from_le_bytes([
                        active_bytes[12],
                        active_bytes[13],
                        active_bytes[14],
                        active_bytes[15],
                    ])
                } else {
                    0
                };

                ui.label(format!(
                    "Magic: F3D1  ·  Partes: {}  ·  Vértices: {}  ·  Triángulos: {}",
                    self.baked_model.parts.len(),
                    self.baked_model.vertex_count(),
                    self.baked_model.triangle_count()
                ));
                ui.label(
                    egui::RichText::new(format!(
                        "Estándar: {std_len} B  ·  Compacto: {cmp_len} B (-{savings_pct}%)"
                    ))
                    .size(11.5)
                    .color(egui::Color32::from_rgb(251, 191, 36)),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Activo: {} bytes  ·  FNV-1a: 0x{:08X}",
                        active_bytes.len(),
                        checksum
                    ))
                    .monospace()
                    .size(11.5),
                );
                let phys_summary = match &self.baked_model.physics {
                    Some(p) => format!(
                        "Física .f3d: Activa ({} · {} colisionadores · {:.1} kg)",
                        p.body_kind.label(),
                        p.colliders.len(),
                        p.mass
                    ),
                    None => String::from("Física .f3d: Desactivada (solo malla)"),
                };
                ui.label(
                    egui::RichText::new(phys_summary)
                        .size(11.5)
                        .color(if self.baked_model.physics.is_some() {
                            egui::Color32::from_rgb(52, 211, 153)
                        } else {
                            egui::Color32::from_gray(145)
                        }),
                );

                ui.add_space(6.0);
                let active_bytes_owned = active_bytes.clone();
                ui.horizontal_wrapped(|ui| {
                    if ui.add_sized([124.0, btn_h], egui::Button::new("Descargar .f3d")).clicked() {
                        let file = format!("{}.f3d", self.scene.model_name.to_lowercase());
                        match trigger_download(&file, &active_bytes_owned, "application/octet-stream") {
                            Ok(Some(saved)) => {
                                self.status_note = format!("Guardado: {saved}");
                            }
                            Ok(None) => {
                                self.status_note = format!("Exportado {file} ({} B)", active_bytes_owned.len());
                            }
                            Err(err) => {
                                self.status_note = err;
                            }
                        }
                    }
                    if ui.add_sized([110.0, btn_h], egui::Button::new("Importar .f3d")).clicked() {
                        self.request_import(ui.ctx(), ".f3d,.glb,.gltf");
                    }
                    if ui.add_sized([126.0, btn_h], egui::Button::new("Importar GLB/GLTF")).clicked() {
                        self.request_import(ui.ctx(), ".glb,.gltf");
                    }
                    if ui.add_sized([148.0, btn_h], egui::Button::new("Probar from_bytes")).clicked() {
                        match Model::from_bytes(&active_bytes_owned) {
                            Ok(decoded) => {
                                let h = fnv1a_32(&active_bytes_owned[16..]);
                                self.status_note = format!(
                                    "Roundtrip OK: '{}' ({} tri, 0x{:08X})",
                                    decoded.name,
                                    decoded.triangle_count(),
                                    h
                                );
                            }
                            Err(err) => {
                                self.status_note = format!("Error loader: {err}");
                            }
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Cabecera Hex (.f3d)")
                        .size(11.5)
                        .color(egui::Color32::from_gray(165)),
                );
                let hex_preview: String = active_bytes_owned
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
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_sized([124.0, btn_h], egui::Button::new("Descargar .rs")).clicked() {
                        let file = format!("{}.rs", self.scene.model_name.to_lowercase());
                        match trigger_download(&file, self.cached_rust_src.as_bytes(), "text/plain") {
                            Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                            Ok(None) => self.status_note = format!("Exportado {file}"),
                            Err(err) => self.status_note = err,
                        }
                    }
                    if ui.add_sized([124.0, btn_h], egui::Button::new("Copiar .rs")).clicked() {
                        ui.ctx().copy_text(self.cached_rust_src.clone());
                        self.status_note = String::from("Código .rs copiado");
                    }
                });

                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt("rs_preview_scroll").show(ui, |ui| {
                    let preview = if self.cached_rust_src.len() > 6000 {
                        format!(
                            "{}\n\n// ... ({} bytes totales — descarga el .rs completo)",
                            &self.cached_rust_src[..6000],
                            self.cached_rust_src.len()
                        )
                    } else {
                        self.cached_rust_src.clone()
                    };
                    ui.add(
                        egui::TextEdit::multiline(&mut preview.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            }
            ExportTab::WavefrontObj => {
                ui.label(format!(
                    "Wavefront .obj · {} vértices · {} triángulos · {:.1} KB",
                    self.baked_model.vertex_count(),
                    self.baked_model.triangle_count(),
                    self.cached_obj_src.len() as f32 / 1024.0
                ));
                ui.label(
                    egui::RichText::new("Compatible con Blender, Godot, Unity y Unreal Engine")
                        .size(11.0)
                        .color(egui::Color32::from_gray(165)),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_sized([124.0, btn_h], egui::Button::new("Descargar .obj")).clicked() {
                        let file = format!("{}.obj", self.scene.model_name.to_lowercase());
                        match trigger_download(&file, self.cached_obj_src.as_bytes(), "text/plain") {
                            Ok(Some(saved)) => self.status_note = format!("Guardado: {saved}"),
                            Ok(None) => self.status_note = format!("Exportado {file}"),
                            Err(err) => self.status_note = err,
                        }
                    }
                    if ui.add_sized([124.0, btn_h], egui::Button::new("Copiar .obj")).clicked() {
                        ui.ctx().copy_text(self.cached_obj_src.clone());
                        self.status_note = String::from("Malla .obj copiada");
                    }
                });

                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt("obj_preview_scroll").show(ui, |ui| {
                    let preview = if self.cached_obj_src.len() > 6000 {
                        format!(
                            "{}\n\n# ... ({} bytes totales — descarga el .obj completo)",
                            &self.cached_obj_src[..6000],
                            self.cached_obj_src.len()
                        )
                    } else {
                        self.cached_obj_src.clone()
                    };
                    ui.add(
                        egui::TextEdit::multiline(&mut preview.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            }
        }
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
        let indices = self.selected_part_indices();
        let snap = self.snap_enabled;

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
            }
        }
        self.rebuild_outputs();
    }

    fn render_viewport_3d(&mut self, ui: &mut egui::Ui, compact: bool) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        let gizmo_data = self.gizmo_handles(rect);
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

        // Start gizmo drag when pressing primary button on an axis
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(axis) = hovered_axis {
                self.active_gizmo_drag = Some(ActiveGizmoDrag {
                    axis,
                    mode: self.gizmo_mode,
                    start_parts: self.scene.parts.clone(),
                    accum_pixels: 0.0,
                });
            }
        }

        // Update active gizmo drag OR fall back to camera orbit / multi-touch
        if let (Some(drag), Some((_, world_len, handles))) = (self.active_gizmo_drag.as_mut(), &gizmo_data) {
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
                self.camera.distance = (self.camera.distance / mt.zoom_delta.max(0.5)).clamp(0.6, 250.0);
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

        // Commit gizmo drag to Undo stack on pointer release
        if response.drag_stopped() {
            if let Some(drag) = self.active_gizmo_drag.take() {
                if drag.start_parts != self.scene.parts {
                    let mut prev_scene = self.scene.clone();
                    prev_scene.parts = drag.start_parts;
                    self.push_undo_entry(HistoryEntry {
                        scene: prev_scene,
                        selection: self.selection.clone(),
                    });
                    self.status_note = format!(
                        "{} aplicado{}",
                        drag.mode.label(),
                        if self.snap_enabled { " (Snap)" } else { "" }
                    );
                }
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.distance = (self.camera.distance * (1.0 - scroll * 0.0018)).clamp(0.6, 250.0);
            }
        }

        // Click / Tap to select 3D part via Ray-AABB (skip if clicking gizmo handle or simulating)
        if self.sim_world.is_none() && response.clicked() && hovered_axis.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()) * 2.0 - 1.0;
                let norm_y = 1.0 - ((pos.y - rect.top()) / rect.height()) * 2.0;
                let aspect = rect.width() / rect.height().max(1.0);
                let (ray_origin, ray_dir) = self.camera.screen_ray(norm_x, norm_y, aspect);

                let mut best_t = f32::INFINITY;
                let mut best_part = None;
                for (mesh_idx, baked) in self.baked_model.parts.iter().enumerate() {
                    if let Some(t) = ray_intersects_aabb(ray_origin, ray_dir, baked.bounds) {
                        if t < best_t {
                            best_t = t;
                            best_part = self.part_map.get(mesh_idx).copied();
                        }
                    }
                }
                self.selection = best_part.map(Selection::Part);
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
        let model_offset = self.sim_offset;
        let aspect = rect.width() / rect.height().max(1.0);

        let callback = egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, painter| {
                let gl = painter.gl();
                if let Ok(mut guard) = renderer_arc.lock() {
                    if let Some(renderer) = guard.as_mut() {
                        unsafe {
                            renderer.sync_model(gl, &model_clone, generation);
                            renderer.sync_collider_debug(gl, &model_clone, generation, show_colliders);
                            renderer.paint(
                                gl,
                                aspect,
                                &camera,
                                &selected_mask,
                                show_grid,
                                wireframe,
                                model_offset,
                            );
                        }
                    }
                }
            })),
        };
        ui.painter().add(callback);

        // Crisp 3D Gizmo Overlay drawn from projected 3D coordinates (hidden during live physics playback)
        if self.sim_world.is_none() {
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
                    GizmoMode::Translate => {
                        painter.circle_filled(tip, handle_r, col);
                    }
                    GizmoMode::Scale => {
                        painter.rect_filled(
                            egui::Rect::from_center_size(tip, egui::vec2(handle_r * 1.9, handle_r * 1.9)),
                            2.0,
                            col,
                        );
                    }
                    GizmoMode::Rotate => {
                        painter.circle_stroke(tip, handle_r + 1.5, egui::Stroke::new(2.2, col));
                    }
                }
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

        // Top-left floating Gizmo Mode + Undo/Redo + Snap bar inside 3D Viewport
        let top_bar_w = if compact { (rect.width() - 16.0).min(430.0) } else { 520.0 };
        let top_bar_h = if compact { 38.0 } else { 30.0 };
        let top_rect = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::vec2(top_bar_w, top_bar_h),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(top_rect), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 24, 220))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let bh = if compact { 28.0 } else { 22.0 };
                        for mode in [GizmoMode::Translate, GizmoMode::Rotate, GizmoMode::Scale] {
                            if ui
                                .add_sized([54.0, bh], egui::Button::selectable(self.gizmo_mode == mode, mode.label()))
                                .clicked()
                            {
                                self.gizmo_mode = mode;
                            }
                        }
                        ui.toggle_value(&mut self.snap_enabled, "Snap");
                        ui.separator();
                        let can_undo = !self.undo_stack.is_empty();
                        let can_redo = !self.redo_stack.is_empty();
                        if ui
                            .add_enabled(can_undo, egui::Button::new("↶").min_size(egui::vec2(28.0, bh)))
                            .on_hover_text("Deshacer (Ctrl+Z)")
                            .clicked()
                        {
                            self.undo();
                        }
                        if ui
                            .add_enabled(can_redo, egui::Button::new("↷").min_size(egui::vec2(28.0, bh)))
                            .on_hover_text("Rehacer (Ctrl+Y)")
                            .clicked()
                        {
                            self.redo();
                        }
                    });
                });
        });

        // Floating Physics Playback Bar (forja3d-physics)
        let is_simulating = self.sim_world.is_some();
        let sim_w = if is_simulating { 280.0 } else { 132.0 };
        let sim_pos = if compact {
            rect.left_top() + egui::vec2(8.0, top_bar_h + 14.0)
        } else {
            rect.right_top() + egui::vec2(-(sim_w + 8.0), 8.0)
        };
        let sim_rect = egui::Rect::from_min_size(sim_pos, egui::vec2(sim_w, top_bar_h));
        ui.scope_builder(egui::UiBuilder::new().max_rect(sim_rect), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 22, 28, 225))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let bh = if compact { 28.0 } else { 22.0 };
                        let sim_lbl = if is_simulating { "⏹ Parar" } else { "▶ Física" };
                        if ui
                            .add_sized([84.0, bh], egui::Button::selectable(is_simulating, sim_lbl))
                            .on_hover_text("Probar físicas del .f3d con forja3d-physics")
                            .clicked()
                        {
                            if is_simulating {
                                self.stop_physics_sim();
                                self.status_note = String::from("Simulación detenida");
                            } else {
                                self.start_physics_sim(2.2);
                            }
                        }
                        if is_simulating {
                            if ui.add_sized([82.0, bh], egui::Button::new("↑ Impulso")).clicked() {
                                if let (Some(world), Some(handle)) = (self.sim_world.as_mut(), self.sim_body) {
                                    let mass = world.body(handle).map_or(1.0, |b| b.mass.max(0.5));
                                    world.apply_impulse(handle, [0.0, 6.5 * mass, 0.0]);
                                }
                            }
                            if let (Some(world), Some(handle)) = (&self.sim_world, self.sim_body) {
                                if let Some(body) = world.body(handle) {
                                    let ground_txt = if body.on_ground { "Suelo" } else { "Aire" };
                                    ui.label(
                                        egui::RichText::new(format!("Y:{:.2}m · {ground_txt}", body.position[1]))
                                            .size(11.0)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(52, 211, 153)),
                                    );
                                }
                            }
                        }
                    });
                });
        });

        // Bottom-left floating viewport controls (Wire / Grid / Col. / Zoom / Reset)
        let bar_w = if compact { (rect.width() - 16.0).max(200.0) } else { 540.0 };
        let bar_h = if compact { 38.0 } else { 28.0 };
        let overlay_rect = egui::Rect::from_min_size(
            rect.left_bottom() + egui::vec2(8.0, -(bar_h + 8.0)),
            egui::vec2(bar_w, bar_h),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(overlay_rect), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 24, 215))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let btn_h = if compact { 28.0 } else { 20.0 };
                        ui.toggle_value(&mut self.wireframe, "Wire");
                        ui.toggle_value(&mut self.show_grid, "Grid");
                        ui.toggle_value(&mut self.show_colliders, "Col.");
                        if ui.add_sized([28.0, btn_h], egui::Button::new("+")).on_hover_text("Acercar").clicked() {
                            self.camera.distance = (self.camera.distance * 0.82).clamp(0.6, 250.0);
                        }
                        if ui.add_sized([28.0, btn_h], egui::Button::new("-")).on_hover_text("Alejar").clicked() {
                            self.camera.distance = (self.camera.distance * 1.22).clamp(0.6, 250.0);
                        }
                        if ui.add_sized([64.0, btn_h], egui::Button::new("Centrar")).clicked() {
                            self.camera = OrbitCamera::default();
                        }
                        if !compact {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(&self.status_note)
                                    .size(11.5)
                                    .color(egui::Color32::from_gray(185)),
                            );
                        }
                    });
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

        // Handle pending async file-picker (.glb/.gltf/.f3d) import
        let picked = self.pending_import.lock().ok().and_then(|mut g| g.take());
        if let Some((name, bytes, base_dir)) = picked {
            self.import_model_bytes(Some(&name), &bytes, base_dir.as_deref());
        }

        // Handle drag & drop (.glb/.gltf/.f3d) files (Desktop + WebAssembly)
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.into_iter().next() {
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

        // Keyboard Undo / Redo shortcuts (Ctrl/Cmd + Z, Ctrl/Cmd + Y / Shift+Z)
        let (want_undo, want_redo) = ui.ctx().input(|i| {
            let cmd = i.modifiers.command;
            let undo = cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z);
            let redo = (cmd && i.key_pressed(egui::Key::Y))
                || (cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z));
            (undo, redo)
        });
        if want_undo {
            self.undo();
        } else if want_redo {
            self.redo();
        }

        let mut dirty = false;
        let root_rect = ui.available_rect_before_wrap();
        let is_compact = root_rect.width() < 760.0;

        let style = ui.style_mut();
        if is_compact {
            style.spacing.interact_size.y = 32.0;
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
        } else {
            style.spacing.interact_size.y = 22.0;
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
        }

        if is_compact {
            egui::Panel::bottom("mobile_studio_tabs")
                .exact_size(46.0)
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        let tab_w = ((root_rect.width() - 36.0) / 4.0).max(60.0);
                        let tabs = [
                            (MobileTab::Viewport, "Vista 3D"),
                            (MobileTab::Scene, "Piezas"),
                            (MobileTab::Inspect, "Editar"),
                            (MobileTab::Export, "Exportar"),
                        ];
                        for (tab, label) in tabs {
                            if ui
                                .add_sized([tab_w, 34.0], egui::Button::selectable(self.mobile_tab == tab, label))
                                .clicked()
                            {
                                self.mobile_tab = tab;
                            }
                        }
                    });
                });

            if self.mobile_tab != MobileTab::Viewport {
                let drawer_h = (root_rect.height() * 0.46).clamp(180.0, 360.0);
                egui::Panel::bottom("mobile_drawer")
                    .exact_size(drawer_h)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("mobile_drawer_scroll").show(ui, |ui| {
                            ui.add_space(4.0);
                            match self.mobile_tab {
                                MobileTab::Scene => self.render_scene_panel(ui, &mut dirty, true),
                                MobileTab::Inspect => self.render_inspector(ui, &mut dirty, true),
                                MobileTab::Export => self.render_exporter(ui, true),
                                MobileTab::Viewport => {}
                            }
                        });
                    });
            }

            if dirty {
                self.rebuild_outputs();
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |ui| {
                    self.render_viewport_3d(ui, true);
                });
        } else {
            egui::Panel::left("left_scene_panel")
                .resizable(false)
                .default_size(244.0)
                .show(ui, |ui| {
                    self.render_scene_panel(ui, &mut dirty, false);
                });

            egui::Panel::right("right_inspector_panel")
                .resizable(true)
                .default_size(356.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("desktop_right_scroll").show(ui, |ui| {
                        ui.add_space(6.0);
                        self.render_inspector(ui, &mut dirty, false);
                        ui.add_space(8.0);
                        ui.separator();
                        self.render_exporter(ui, false);
                    });
                });

            if dirty {
                self.rebuild_outputs();
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 16, 22)))
                .show(ui, |ui| {
                    self.render_viewport_3d(ui, false);
                });
        }

        self.render_import_modal(ui.ctx());
    }
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

#[cfg(target_os = "android")]
static ANDROID_BASE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

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
            let exts: Vec<&str> = accept
                .split(',')
                .map(|s| s.trim().trim_start_matches('.'))
                .filter(|s| !s.is_empty())
                .collect();
            let picked = rfd::FileDialog::new()
                .set_title("Importar modelo (.glb / .gltf / .f3d)")
                .add_filter("Modelos 3D", &exts)
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
            let _ = ctx;
            self.import_modal_filter = accept.to_string();
            self.refresh_import_modal_files();
            self.import_modal_open = true;
        }
    }

    fn refresh_import_modal_files(&mut self) {
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        #[cfg(target_os = "android")]
        {
            let root = android_storage_root();
            let imports = root.join("imports");
            let exports = root.join("exports");
            let _ = std::fs::create_dir_all(&imports);
            let _ = std::fs::create_dir_all(&exports);
            dirs.push(imports);
            dirs.push(exports);
            dirs.push(std::path::PathBuf::from("/storage/emulated/0/Download/Forja3D"));
            dirs.push(std::path::PathBuf::from("/storage/emulated/0/Download"));
            dirs.push(std::path::PathBuf::from("/storage/emulated/0/Documents"));
        }
        let exts: Vec<&str> = self
            .import_modal_filter
            .split(',')
            .map(|s| s.trim().trim_start_matches('.'))
            .filter(|s| !s.is_empty())
            .collect();
        let mut found = Vec::new();
        for dir in dirs {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_ascii_lowercase())
                        .unwrap_or_default();
                    if !exts.is_empty() && !exts.iter().any(|e| *e == ext) {
                        continue;
                    }
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("archivo")
                        .to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    found.push((name, path, size));
                }
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        self.import_modal_files = found;
    }

    fn render_import_modal(&mut self, ctx: &egui::Context) {
        if !self.import_modal_open {
            return;
        }
        let mut open = self.import_modal_open;
        let mut selected_to_load: Option<std::path::PathBuf> = None;
        let mut do_refresh = false;

        egui::Window::new("Importar modelo (Android)")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                #[cfg(target_os = "android")]
                let imports_dir = android_storage_root().join("imports");
                #[cfg(not(target_os = "android"))]
                let imports_dir = std::path::PathBuf::from(".");

                ui.label(
                    egui::RichText::new("Copia tus .glb, .gltf o .f3d en:")
                        .size(11.5)
                        .color(egui::Color32::from_gray(180)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(imports_dir.display().to_string())
                            .monospace()
                            .size(10.5)
                            .color(egui::Color32::from_rgb(251, 191, 36)),
                    );
                    if ui.button("Copiar ruta").clicked() {
                        ctx.copy_text(imports_dir.display().to_string());
                        self.status_note = String::from("Ruta de importación copiada");
                    }
                    if ui.button("Refrescar").clicked() {
                        do_refresh = true;
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new("Archivos detectados:").strong());
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    if self.import_modal_files.is_empty() {
                        ui.label(
                            egui::RichText::new("No se encontraron archivos en Descargas ni en la carpeta de la app.")
                                .size(11.5)
                                .color(egui::Color32::from_gray(150)),
                        );
                    } else {
                        for (name, path, size) in &self.import_modal_files {
                            let label = format!("{name} ({:.1} KB)", *size as f32 / 1024.0);
                            if ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label)).clicked() {
                                selected_to_load = Some(path.clone());
                            }
                        }
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new("O abre una ruta directa:").size(11.5));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_path_draft)
                            .hint_text("/storage/emulated/0/Download/modelo.glb")
                            .desired_width(210.0),
                    );
                    if ui.button("Cargar").clicked() && !self.import_path_draft.trim().is_empty() {
                        selected_to_load = Some(std::path::PathBuf::from(self.import_path_draft.trim()));
                    }
                });
            });

        self.import_modal_open = open;
        if do_refresh {
            self.refresh_import_modal_files();
        }
        if let Some(path) = selected_to_load {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("modelo.glb")
                        .to_string();
                    self.import_model_bytes(Some(&name), &bytes, path.parent());
                    self.import_modal_open = false;
                }
                Err(err) => {
                    self.status_note = format!("No se pudo leer '{}': {err}", path.display());
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
    input.set_accept(accept);

    let pending_clone = Arc::clone(pending);
    let ctx_clone = ctx.clone();
    let input_ref = input.clone();
    let onchange = Closure::wrap(Box::new(move |_event: eframe::web_sys::Event| {
        let Some(files) = input_ref.files() else { return };
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
