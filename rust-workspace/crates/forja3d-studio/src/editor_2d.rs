use eframe::egui;
use forja3d::{
    default_extrude_profile, Anchor2D, PrimitiveKind, SceneDocument, UiDocument2D, Widget2D,
    WidgetKind2D,
};

use crate::icons::{icon_btn, icon_text_btn, svg_img, SvgIcon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode2D {
    Move,
    ResizeBottomRight,
}

#[derive(Debug, Clone)]
pub struct ActiveWidgetDrag2D {
    pub index: usize,
    pub mode: DragMode2D,
    pub start_rect: [f32; 4],
    pub start_pointer_px: egui::Pos2,
}

pub fn default_ui_document_2d(model_name: &str) -> UiDocument2D {
    let mut doc = UiDocument2D::new(format!("Pantalla{}", model_name), [1280.0, 720.0]);
    doc.widgets.push(Widget2D::new_container(
        "panel_hud",
        "Panel Principal",
        [48.0, 48.0, 420.0, 260.0],
        [20, 26, 38, 230],
    ));
    doc.widgets.push(Widget2D {
        id: String::from("lbl_titulo"),
        name: String::from("Titulo HUD"),
        rect: [76.0, 72.0, 320.0, 36.0],
        anchor: Anchor2D::TopLeft,
        visible: true,
        kind: WidgetKind2D::Text {
            content: format!("HUD - {model_name}"),
            font_size: 24.0,
            color: [243, 244, 246, 255],
        },
    });
    doc.widgets.push(Widget2D::new_button(
        "btn_iniciar",
        "Boton Iniciar",
        "Comenzar",
        "on_iniciar",
        [76.0, 148.0, 180.0, 52.0],
    ));
    doc.widgets.push(Widget2D {
        id: String::from("tgl_sonido"),
        name: String::from("Interruptor Sonido"),
        rect: [76.0, 224.0, 200.0, 36.0],
        anchor: Anchor2D::TopLeft,
        visible: true,
        kind: WidgetKind2D::Toggle {
            label: String::from("Audio activo"),
            action_id: String::from("on_toggle_audio"),
            is_checked: true,
        },
    });
    doc.actions.push(String::from("on_iniciar"));
    doc.actions.push(String::from("on_toggle_audio"));
    doc
}

fn c32(rgba: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

pub fn render_viewport_2d(
    ui: &mut egui::Ui,
    doc2d: &mut UiDocument2D,
    selected: &mut Option<usize>,
    drag_state: &mut Option<ActiveWidgetDrag2D>,
    snap_enabled: &mut bool,
    status_note: &mut String,
    compact: bool,
) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Canvas backdrop
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(12, 15, 22));

    let dw = doc2d.design_resolution[0].max(64.0);
    let dh = doc2d.design_resolution[1].max(64.0);
    let pad = if compact { 16.0 } else { 36.0 };
    let top_reserve = if compact { 50.0 } else { 44.0 };
    let avail_w = (rect.width() - pad * 2.0).max(60.0);
    let avail_h = (rect.height() - pad * 2.0 - top_reserve).max(60.0);
    let scale = (avail_w / dw).min(avail_h / dh).max(0.05);
    let frame_w = dw * scale;
    let frame_h = dh * scale;
    let frame_min = egui::pos2(
        rect.center().x - frame_w * 0.5,
        rect.top() + top_reserve + (avail_h - frame_h) * 0.5 + 8.0,
    );
    let canvas_rect = egui::Rect::from_min_size(frame_min, egui::vec2(frame_w, frame_h));

    // Screen boundary & subtle grid
    painter.rect_filled(canvas_rect, 4.0, egui::Color32::from_rgb(17, 21, 30));
    let grid_step = 64.0_f32;
    let mut gx = grid_step;
    while gx < dw {
        let sx = canvas_rect.left() + gx * scale;
        painter.line_segment(
            [
                egui::pos2(sx, canvas_rect.top()),
                egui::pos2(sx, canvas_rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(24, 30, 42)),
        );
        gx += grid_step;
    }
    let mut gy = grid_step;
    while gy < dh {
        let sy = canvas_rect.top() + gy * scale;
        painter.line_segment(
            [
                egui::pos2(canvas_rect.left(), sy),
                egui::pos2(canvas_rect.right(), sy),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(24, 30, 42)),
        );
        gy += grid_step;
    }
    painter.rect_stroke(
        canvas_rect,
        4.0,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(64, 78, 102)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        canvas_rect.left_top() + egui::vec2(6.0, -14.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{} ({:.0}x{:.0})", doc2d.name, dw, dh),
        egui::FontId::monospace(11.0),
        egui::Color32::from_gray(165),
    );

    let to_screen_rect = |r: [f32; 4]| -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(
                canvas_rect.left() + r[0] * scale,
                canvas_rect.top() + r[1] * scale,
            ),
            egui::vec2((r[2] * scale).max(6.0), (r[3] * scale).max(6.0)),
        )
    };

    // Draw widgets in order
    for (idx, w) in doc2d.widgets.iter().enumerate() {
        if !w.visible {
            continue;
        }
        let wrect = to_screen_rect(w.rect);
        match &w.kind {
            WidgetKind2D::Container {
                background_color,
                corner_radius,
                border_color,
                border_width,
            } => {
                painter.rect_filled(wrect, (*corner_radius * scale).max(0.0), c32(*background_color));
                if *border_width > 0.0 {
                    painter.rect_stroke(
                        wrect,
                        (*corner_radius * scale).max(0.0),
                        egui::Stroke::new((*border_width * scale).max(1.0), c32(*border_color)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            WidgetKind2D::Button {
                label,
                action_id,
                background_color,
                text_color,
            } => {
                painter.rect_filled(wrect, (8.0 * scale).max(3.0), c32(*background_color));
                painter.rect_stroke(
                    wrect,
                    (8.0 * scale).max(3.0),
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 50)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    wrect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional((16.0 * scale).clamp(9.0, 28.0)),
                    c32(*text_color),
                );
                if !action_id.is_empty() {
                    painter.text(
                        wrect.right_bottom() + egui::vec2(-4.0, -2.0),
                        egui::Align2::RIGHT_BOTTOM,
                        action_id,
                        egui::FontId::monospace((9.5 * scale).clamp(8.0, 12.0)),
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 160),
                    );
                }
            }
            WidgetKind2D::Text {
                content,
                font_size,
                color,
            } => {
                painter.text(
                    wrect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    content,
                    egui::FontId::proportional((*font_size * scale).clamp(9.0, 48.0)),
                    c32(*color),
                );
            }
            WidgetKind2D::Image {
                texture_name, tint, ..
            } => {
                painter.rect_filled(wrect, 3.0, egui::Color32::from_rgb(28, 35, 48));
                painter.rect_stroke(
                    wrect,
                    3.0,
                    egui::Stroke::new(1.5, c32(*tint)),
                    egui::StrokeKind::Inside,
                );
                painter.line_segment([wrect.left_top(), wrect.right_bottom()], egui::Stroke::new(1.0, c32(*tint)));
                painter.line_segment([wrect.right_top(), wrect.left_bottom()], egui::Stroke::new(1.0, c32(*tint)));
                painter.text(
                    wrect.center(),
                    egui::Align2::CENTER_CENTER,
                    texture_name,
                    egui::FontId::monospace((11.0 * scale).clamp(8.0, 16.0)),
                    c32(*tint),
                );
            }
            WidgetKind2D::Toggle {
                label, is_checked, ..
            } => {
                let box_sz = (wrect.height() * 0.72).clamp(12.0, 26.0);
                let box_rect = egui::Rect::from_min_size(
                    egui::pos2(wrect.left() + 4.0, wrect.center().y - box_sz * 0.5),
                    egui::vec2(box_sz, box_sz),
                );
                painter.rect_filled(
                    box_rect,
                    3.0,
                    if *is_checked {
                        egui::Color32::from_rgb(217, 119, 6)
                    } else {
                        egui::Color32::from_rgb(30, 36, 48)
                    },
                );
                painter.rect_stroke(
                    box_rect,
                    3.0,
                    egui::Stroke::new(1.2, egui::Color32::from_gray(200)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    egui::pos2(box_rect.right() + 8.0, wrect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional((15.0 * scale).clamp(9.0, 24.0)),
                    egui::Color32::from_gray(235),
                );
            }
            WidgetKind2D::VectorProfile {
                closed,
                stroke_color,
                fill_color,
                stroke_width,
                points,
            } => {
                painter.rect_stroke(
                    wrect,
                    2.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(90, 120, 160, 110)),
                    egui::StrokeKind::Inside,
                );
                if points.len() >= 2 {
                    let spts: Vec<egui::Pos2> = points
                        .iter()
                        .map(|&[px, py]| {
                            egui::pos2(
                                wrect.center().x + px * wrect.width() * 0.45,
                                wrect.center().y - py * wrect.height() * 0.45,
                            )
                        })
                        .collect();
                    if *closed && spts.len() >= 3 {
                        painter.add(egui::Shape::convex_polygon(
                            spts.clone(),
                            c32(*fill_color),
                            egui::Stroke::NONE,
                        ));
                    }
                    let stroke = egui::Stroke::new((*stroke_width * scale).max(1.2), c32(*stroke_color));
                    for pair in spts.windows(2) {
                        painter.line_segment([pair[0], pair[1]], stroke);
                    }
                    if *closed && spts.len() >= 3 {
                        painter.line_segment([spts[spts.len() - 1], spts[0]], stroke);
                    }
                }
            }
        }

        if *selected == Some(idx) {
            painter.rect_stroke(
                wrect.expand(1.5),
                2.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(251, 191, 36)),
                egui::StrokeKind::Outside,
            );
            let handle = egui::Rect::from_center_size(wrect.right_bottom(), egui::vec2(10.0, 10.0));
            painter.rect_filled(handle, 2.0, egui::Color32::from_rgb(251, 191, 36));
        }
    }

    // Pointer hit-testing / dragging
    if response.drag_started_by(egui::PointerButton::Primary) || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut hit: Option<(usize, DragMode2D)> = None;
            if let Some(sel) = *selected {
                if let Some(w) = doc2d.widgets.get(sel) {
                    if w.visible {
                        let wr = to_screen_rect(w.rect);
                        let handle_r = if compact { 18.0 } else { 11.0 };
                        if pos.distance(wr.right_bottom()) <= handle_r {
                            hit = Some((sel, DragMode2D::ResizeBottomRight));
                        }
                    }
                }
            }
            if hit.is_none() {
                for idx in (0..doc2d.widgets.len()).rev() {
                    if !doc2d.widgets[idx].visible {
                        continue;
                    }
                    let wr = to_screen_rect(doc2d.widgets[idx].rect);
                    if wr.expand(4.0).contains(pos) {
                        hit = Some((idx, DragMode2D::Move));
                        break;
                    }
                }
            }
            if let Some((idx, mode)) = hit {
                *selected = Some(idx);
                if response.drag_started_by(egui::PointerButton::Primary) {
                    *drag_state = Some(ActiveWidgetDrag2D {
                        index: idx,
                        mode,
                        start_rect: doc2d.widgets[idx].rect,
                        start_pointer_px: pos,
                    });
                }
            } else if response.clicked() {
                *selected = None;
            }
        }
    }

    if response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(drag), Some(pos)) = (drag_state.as_ref(), response.interact_pointer_pos()) {
            if let Some(w) = doc2d.widgets.get_mut(drag.index) {
                let dx = (pos.x - drag.start_pointer_px.x) / scale;
                let dy = (pos.y - drag.start_pointer_px.y) / scale;
                let snap_val = |v: f32| -> f32 {
                    if *snap_enabled {
                        (v / 8.0).round() * 8.0
                    } else {
                        v.round()
                    }
                };
                match drag.mode {
                    DragMode2D::Move => {
                        w.rect[0] = snap_val(drag.start_rect[0] + dx);
                        w.rect[1] = snap_val(drag.start_rect[1] + dy);
                    }
                    DragMode2D::ResizeBottomRight => {
                        w.rect[2] = snap_val((drag.start_rect[2] + dx).max(24.0));
                        w.rect[3] = snap_val((drag.start_rect[3] + dy).max(20.0));
                    }
                }
            }
        }
    }

    if response.drag_stopped() {
        *drag_state = None;
    }

    // Floating quick toolbar at top-left of 2D canvas
    let bar_w = if compact { (rect.width() - 16.0).max(200.0) } else { 520.0 };
    let bar_h = if compact { 38.0 } else { 30.0 };
    let bar_rect = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(8.0, 8.0),
        egui::vec2(bar_w, bar_h),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 24, 225))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .corner_radius(6.0)
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("viewport_2d_toolbar")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let bh = if compact { 28.0 } else { 22.0 };
                            if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Panel", [68.0, bh], false).clicked() {
                                let n = doc2d.widgets.len() + 1;
                                doc2d.widgets.push(Widget2D::new_container(
                                    format!("panel_{n}"),
                                    format!("Panel {n}"),
                                    [80.0 + (n as f32 * 12.0), 80.0, 260.0, 140.0],
                                    [24, 30, 44, 225],
                                ));
                                *selected = Some(doc2d.widgets.len() - 1);
                                *status_note = String::from("Panel 2D añadido");
                            }
                            if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Botón", [72.0, bh], false).clicked() {
                                let n = doc2d.widgets.len() + 1;
                                let act = format!("on_accion_{n}");
                                doc2d.widgets.push(Widget2D::new_button(
                                    format!("btn_{n}"),
                                    format!("Boton {n}"),
                                    format!("Botón {n}"),
                                    act.clone(),
                                    [100.0, 100.0 + (n as f32 * 16.0), 160.0, 44.0],
                                ));
                                *selected = Some(doc2d.widgets.len() - 1);
                                *status_note = format!("Botón añadido ({act})");
                            }
                            if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Texto", [70.0, bh], false).clicked() {
                                let n = doc2d.widgets.len() + 1;
                                doc2d.widgets.push(Widget2D {
                                    id: format!("txt_{n}"),
                                    name: format!("Texto {n}"),
                                    rect: [100.0, 120.0, 220.0, 30.0],
                                    anchor: Anchor2D::TopLeft,
                                    visible: true,
                                    kind: WidgetKind2D::Text {
                                        content: String::from("Texto HUD"),
                                        font_size: 18.0,
                                        color: [240, 244, 248, 255],
                                    },
                                });
                                *selected = Some(doc2d.widgets.len() - 1);
                            }
                            if icon_text_btn(ui, SvgIcon::Snap, 14.0, "Snap 8px", [84.0, bh], *snap_enabled).clicked() {
                                *snap_enabled = !*snap_enabled;
                            }
                        });
                    });
            });
    });
}

pub fn render_scene_panel_2d(
    ui: &mut egui::Ui,
    doc2d: &mut UiDocument2D,
    selected: &mut Option<usize>,
    scene3d: &SceneDocument,
    sel3d_idx: Option<usize>,
    status_note: &mut String,
    compact: bool,
) {
    let btn_h = if compact { 32.0 } else { 24.0 };
    ui.label(egui::RichText::new("Pantalla 2D (.f2d)").strong());
    ui.horizontal(|ui| {
        ui.label("Nombre:");
        ui.text_edit_singleline(&mut doc2d.name);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Resolución:");
        for (lbl, res) in [
            ("1280x720", [1280.0_f32, 720.0]),
            ("1920x1080", [1920.0, 1080.0]),
            ("720x1280", [720.0, 1280.0]),
        ] {
            let active = (doc2d.design_resolution[0] - res[0]).abs() < 1.0
                && (doc2d.design_resolution[1] - res[1]).abs() < 1.0;
            if ui.add_sized([76.0, btn_h], egui::Button::selectable(active, lbl)).clicked() {
                doc2d.design_resolution = res;
            }
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.label(egui::RichText::new("Añadir control 2D").strong());
    ui.horizontal_wrapped(|ui| {
        let n = doc2d.widgets.len() + 1;
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Panel", [78.0, btn_h], false).clicked() {
            doc2d.widgets.push(Widget2D::new_container(
                format!("panel_{n}"),
                format!("Panel {n}"),
                [60.0, 60.0, 280.0, 160.0],
                [20, 26, 38, 220],
            ));
            *selected = Some(doc2d.widgets.len() - 1);
        }
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Botón", [80.0, btn_h], false).clicked() {
            doc2d.widgets.push(Widget2D::new_button(
                format!("btn_{n}"),
                format!("Boton {n}"),
                format!("Acción {n}"),
                format!("on_accion_{n}"),
                [80.0, 80.0, 160.0, 44.0],
            ));
            *selected = Some(doc2d.widgets.len() - 1);
        }
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Texto", [76.0, btn_h], false).clicked() {
            doc2d.widgets.push(Widget2D {
                id: format!("txt_{n}"),
                name: format!("Texto {n}"),
                rect: [80.0, 80.0, 220.0, 32.0],
                anchor: Anchor2D::TopLeft,
                visible: true,
                kind: WidgetKind2D::Text {
                    content: String::from("Etiqueta HUD"),
                    font_size: 18.0,
                    color: [240, 244, 248, 255],
                },
            });
            *selected = Some(doc2d.widgets.len() - 1);
        }
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Toggle", [82.0, btn_h], false).clicked() {
            doc2d.widgets.push(Widget2D {
                id: format!("tgl_{n}"),
                name: format!("Toggle {n}"),
                rect: [80.0, 120.0, 200.0, 34.0],
                anchor: Anchor2D::TopLeft,
                visible: true,
                kind: WidgetKind2D::Toggle {
                    label: String::from("Opción"),
                    action_id: format!("on_toggle_{n}"),
                    is_checked: true,
                },
            });
            *selected = Some(doc2d.widgets.len() - 1);
        }
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Imagen", [84.0, btn_h], false).clicked() {
            doc2d.widgets.push(Widget2D {
                id: format!("img_{n}"),
                name: format!("Imagen {n}"),
                rect: [100.0, 100.0, 140.0, 140.0],
                anchor: Anchor2D::Center,
                visible: true,
                kind: WidgetKind2D::Image {
                    texture_name: format!("{}_albedo", scene3d.model_name),
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [255, 255, 255, 255],
                },
            });
            *selected = Some(doc2d.widgets.len() - 1);
        }
        if icon_text_btn(ui, SvgIcon::Plus, 14.0, "Perfil 3D", [92.0, btn_h], false).clicked() {
            let pts = sel3d_idx
                .and_then(|i| scene3d.parts.get(i))
                .filter(|p| !p.profile.is_empty())
                .map(|p| p.profile.clone())
                .unwrap_or_else(default_extrude_profile);
            let closed = sel3d_idx
                .and_then(|i| scene3d.parts.get(i))
                .map_or(true, |p| p.kind != PrimitiveKind::Lathe);
            doc2d.widgets.push(Widget2D {
                id: format!("vec_{n}"),
                name: format!("Vector {n}"),
                rect: [120.0, 120.0, 200.0, 200.0],
                anchor: Anchor2D::Center,
                visible: true,
                kind: WidgetKind2D::VectorProfile {
                    closed,
                    stroke_color: [255, 255, 255, 255],
                    fill_color: [245, 158, 11, 220],
                    stroke_width: 2.0,
                    points: pts,
                },
            });
            *selected = Some(doc2d.widgets.len() - 1);
            *status_note = String::from("Vector 2D importado desde perfil 3D");
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.label(egui::RichText::new(format!("Controles ({})", doc2d.widgets.len())).strong());

    let mut to_delete = None;
    let mut to_dup = None;
    for (idx, w) in doc2d.widgets.iter_mut().enumerate() {
        let is_sel = *selected == Some(idx);
        ui.horizontal(|ui| {
            let eye_sz = if compact { [30.0, 28.0] } else { [24.0, 22.0] };
            let mut eye_btn = egui::Button::image(svg_img(SvgIcon::Eye, 14.0))
                .image_tint_follows_text_color(true)
                .selected(w.visible);
            if !w.visible {
                eye_btn = eye_btn.fill(egui::Color32::from_rgb(24, 28, 36));
            }
            if ui.add_sized(eye_sz, eye_btn).on_hover_text("Mostrar/ocultar").clicked() {
                w.visible = !w.visible;
            }
            if ui.selectable_label(is_sel, &w.name).clicked() {
                *selected = Some(idx);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let act_sz = if compact { [32.0, 28.0] } else { [24.0, 22.0] };
                if icon_btn(ui, SvgIcon::Trash, 13.0, act_sz, false)
                    .on_hover_text("Eliminar control")
                    .clicked()
                {
                    to_delete = Some(idx);
                }
                if icon_btn(ui, SvgIcon::Duplicate, 13.0, act_sz, false)
                    .on_hover_text("Duplicar control")
                    .clicked()
                {
                    to_dup = Some(idx);
                }
            });
        });
    }

    if let Some(idx) = to_dup {
        let mut c = doc2d.widgets[idx].clone();
        c.id = format!("{}_copia", c.id);
        c.name = format!("{} copia", c.name);
        c.rect[0] += 24.0;
        c.rect[1] += 24.0;
        doc2d.widgets.push(c);
        *selected = Some(doc2d.widgets.len() - 1);
    }
    if let Some(idx) = to_delete {
        doc2d.widgets.remove(idx);
        *selected = if doc2d.widgets.is_empty() {
            None
        } else {
            Some(idx.min(doc2d.widgets.len() - 1))
        };
    }
}

pub fn render_inspector_2d(
    ui: &mut egui::Ui,
    doc2d: &mut UiDocument2D,
    selected: &mut Option<usize>,
    status_note: &mut String,
    compact: bool,
    on_download_f2d: &mut Option<(String, Vec<u8>)>,
    on_download_rs: &mut Option<(String, String)>,
) {
    let btn_h = if compact { 32.0 } else { 24.0 };
    if let Some(idx) = selected.filter(|&i| i < doc2d.widgets.len()) {
        let w = &mut doc2d.widgets[idx];
        ui.label(egui::RichText::new("Inspector Control 2D").strong());
        ui.horizontal(|ui| {
            ui.label("Nombre:");
            ui.text_edit_singleline(&mut w.name);
        });
        ui.horizontal(|ui| {
            ui.label("ID:");
            ui.text_edit_singleline(&mut w.id);
        });
        egui::Grid::new("rect_2d_grid").num_columns(5).spacing([4.0, 4.0]).show(ui, |ui| {
            ui.label("Rect");
            for (i, lbl) in ["X", "Y", "W", "H"].iter().enumerate() {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(*lbl).size(10.0).color(egui::Color32::from_gray(155)));
                    let min_val = if i >= 2 { 12.0 } else { -2000.0 };
                    ui.add_sized(
                        [52.0, if compact { 28.0 } else { 22.0 }],
                        egui::DragValue::new(&mut w.rect[i]).speed(2.0).range(min_val..=4000.0),
                    );
                });
            }
            ui.end_row();
        });

        ui.horizontal(|ui| {
            ui.label("Anclaje:");
            egui::ComboBox::from_id_salt("anchor_2d_combo")
                .selected_text(w.anchor.label())
                .show_ui(ui, |ui| {
                    for code in 0..=9u8 {
                        if let Some(anc) = Anchor2D::from_u8(code) {
                            ui.selectable_value(&mut w.anchor, anc, anc.label());
                        }
                    }
                });
        });

        ui.add_space(4.0);
        match &mut w.kind {
            WidgetKind2D::Container {
                background_color,
                corner_radius,
                border_color,
                border_width,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Fondo:");
                    ui.color_edit_button_srgba_unmultiplied(background_color);
                    ui.label("Borde:");
                    ui.color_edit_button_srgba_unmultiplied(border_color);
                });
                ui.add(egui::Slider::new(corner_radius, 0.0..=32.0).text("Radio esquina"));
                ui.add(egui::Slider::new(border_width, 0.0..=8.0).text("Grosor borde"));
            }
            WidgetKind2D::Button {
                label,
                action_id,
                background_color,
                text_color,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Texto:");
                    ui.text_edit_singleline(label);
                });
                ui.horizontal(|ui| {
                    ui.label("Acción .rs:");
                    ui.text_edit_singleline(action_id);
                });
                ui.horizontal(|ui| {
                    ui.label("Fondo:");
                    ui.color_edit_button_srgba_unmultiplied(background_color);
                    ui.label("Texto:");
                    ui.color_edit_button_srgba_unmultiplied(text_color);
                });
            }
            WidgetKind2D::Text {
                content,
                font_size,
                color,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Contenido:");
                    ui.text_edit_singleline(content);
                });
                ui.add(egui::Slider::new(font_size, 10.0..=64.0).text("Tamaño fuente"));
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    ui.color_edit_button_srgba_unmultiplied(color);
                });
            }
            WidgetKind2D::Toggle {
                label,
                action_id,
                is_checked,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Etiqueta:");
                    ui.text_edit_singleline(label);
                });
                ui.horizontal(|ui| {
                    ui.label("Acción .rs:");
                    ui.text_edit_singleline(action_id);
                });
                ui.checkbox(is_checked, "Marcado por defecto");
            }
            WidgetKind2D::Image {
                texture_name, tint, ..
            } => {
                ui.horizontal(|ui| {
                    ui.label("Textura:");
                    ui.text_edit_singleline(texture_name);
                });
                ui.horizontal(|ui| {
                    ui.label("Tinte:");
                    ui.color_edit_button_srgba_unmultiplied(tint);
                });
            }
            WidgetKind2D::VectorProfile {
                closed,
                stroke_color,
                fill_color,
                stroke_width,
                ..
            } => {
                ui.checkbox(closed, "Trazado cerrado");
                ui.horizontal(|ui| {
                    ui.label("Relleno:");
                    ui.color_edit_button_srgba_unmultiplied(fill_color);
                    ui.label("Trazo:");
                    ui.color_edit_button_srgba_unmultiplied(stroke_color);
                });
                ui.add(egui::Slider::new(stroke_width, 0.5..=8.0).text("Grosor"));
            }
        }
    } else {
        ui.label("Toca un control en el visor 2D o añádelo a la izquierda.");
    }

    ui.add_space(10.0);
    ui.separator();
    ui.label(egui::RichText::new("Lógica Rust generada (.f2d -> .rs)").strong());
    let f2d_bytes = doc2d.to_bytes();
    let rs_code = doc2d.to_rust_code();
    let base = doc2d.name.to_lowercase();
    ui.label(
        egui::RichText::new(format!(
            "{} controles · .f2d {} bytes",
            doc2d.widgets.len(),
            f2d_bytes.len()
        ))
        .size(11.5)
        .color(egui::Color32::from_rgb(251, 191, 36)),
    );
    ui.horizontal_wrapped(|ui| {
        if icon_text_btn(ui, SvgIcon::Download, 14.0, "Guardar .f2d", [112.0, btn_h], false).clicked() {
            *on_download_f2d = Some((format!("{base}.f2d"), f2d_bytes));
        }
        if icon_text_btn(ui, SvgIcon::Download, 14.0, "Guardar .rs", [106.0, btn_h], false).clicked() {
            *on_download_rs = Some((format!("{base}_ui.rs"), rs_code.clone()));
        }
        if icon_text_btn(ui, SvgIcon::Copy, 14.0, "Copiar .rs", [98.0, btn_h], false).clicked() {
            ui.ctx().copy_text(rs_code.clone());
            *status_note = String::from("Lógica Rust 2D copiada al portapapeles");
        }
    });
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(&mut rs_code.as_str())
            .font(egui::TextStyle::Monospace)
            .desired_rows(10)
            .desired_width(f32::INFINITY),
    );
}
