use alloc::{
    format,
    string::String,
    vec::Vec,
};

use crate::format::{fnv1a_32, write_str, Cursor, DecodeError};

/// Binary magic header for Forja 2D screens and UI documents (.f2d).
pub const F2D_MAGIC: [u8; 4] = *b"F2D1";
pub const F2D_VERSION: u16 = 1;

/// Anchoring modes for 2D UI elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Anchor2D {
    #[default]
    TopLeft = 0,
    TopCenter = 1,
    TopRight = 2,
    CenterLeft = 3,
    Center = 4,
    CenterRight = 5,
    BottomLeft = 6,
    BottomCenter = 7,
    BottomRight = 8,
    Stretch = 9,
}

impl Anchor2D {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::TopLeft),
            1 => Some(Self::TopCenter),
            2 => Some(Self::TopRight),
            3 => Some(Self::CenterLeft),
            4 => Some(Self::Center),
            5 => Some(Self::CenterRight),
            6 => Some(Self::BottomLeft),
            7 => Some(Self::BottomCenter),
            8 => Some(Self::BottomRight),
            9 => Some(Self::Stretch),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Arriba Izquierda",
            Self::TopCenter => "Arriba Centro",
            Self::TopRight => "Arriba Derecha",
            Self::CenterLeft => "Centro Izquierda",
            Self::Center => "Centro",
            Self::CenterRight => "Centro Derecha",
            Self::BottomLeft => "Abajo Izquierda",
            Self::BottomCenter => "Abajo Centro",
            Self::BottomRight => "Abajo Derecha",
            Self::Stretch => "Estirar",
        }
    }
}

/// Specialized UI and 2D visual controls supported in .f2d.
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetKind2D {
    Container {
        background_color: [u8; 4],
        corner_radius: f32,
        border_color: [u8; 4],
        border_width: f32,
    },
    Button {
        label: String,
        action_id: String,
        background_color: [u8; 4],
        text_color: [u8; 4],
    },
    Text {
        content: String,
        font_size: f32,
        color: [u8; 4],
    },
    Image {
        texture_name: String,
        uv_rect: [f32; 4],
        tint: [u8; 4],
    },
    Toggle {
        label: String,
        action_id: String,
        is_checked: bool,
    },
    VectorProfile {
        closed: bool,
        stroke_color: [u8; 4],
        fill_color: [u8; 4],
        stroke_width: f32,
        points: Vec<[f32; 2]>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Widget2D {
    pub id: String,
    pub name: String,
    pub rect: [f32; 4], // [x, y, width, height]
    pub anchor: Anchor2D,
    pub visible: bool,
    pub kind: WidgetKind2D,
}

impl Widget2D {
    pub fn new_button(
        id: impl Into<String>,
        name: impl Into<String>,
        label: impl Into<String>,
        action_id: impl Into<String>,
        rect: [f32; 4],
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            rect,
            anchor: Anchor2D::Center,
            visible: true,
            kind: WidgetKind2D::Button {
                label: label.into(),
                action_id: action_id.into(),
                background_color: [217, 119, 6, 255],
                text_color: [255, 255, 255, 255],
            },
        }
    }

    pub fn new_container(
        id: impl Into<String>,
        name: impl Into<String>,
        rect: [f32; 4],
        bg: [u8; 4],
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            rect,
            anchor: Anchor2D::Stretch,
            visible: true,
            kind: WidgetKind2D::Container {
                background_color: bg,
                corner_radius: 8.0,
                border_color: [45, 52, 68, 255],
                border_width: 1.0,
            },
        }
    }
}

/// Forja 2D (.f2d / F2D1) - Estructura visual 2D (contenedores, botones, textos,
/// anclajes/layout) y eventos/acciones tipadas en .rs para diseñar pantallas y compilarlas directo en la app.
#[derive(Debug, Clone, PartialEq)]
pub struct UiDocument2D {
    pub name: String,
    pub design_resolution: [f32; 2],
    pub widgets: Vec<Widget2D>,
    pub actions: Vec<String>,
}

pub type Document2D = UiDocument2D;

impl Default for UiDocument2D {
    fn default() -> Self {
        Self {
            name: String::from("PantallaPrincipal"),
            design_resolution: [1920.0, 1080.0],
            widgets: Vec::new(),
            actions: Vec::new(),
        }
    }
}

impl UiDocument2D {
    pub fn new(name: impl Into<String>, design_resolution: [f32; 2]) -> Self {
        Self {
            name: name.into(),
            design_resolution,
            widgets: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn from_profile(name: impl Into<String>, points: Vec<[f32; 2]>, closed: bool) -> Self {
        let name_str = name.into();
        let mut doc = Self::new(name_str.clone(), [1920.0, 1080.0]);
        doc.widgets.push(Widget2D {
            id: String::from("vector_perfil_0"),
            name: name_str,
            rect: [0.0, 0.0, 200.0, 200.0],
            anchor: Anchor2D::Center,
            visible: true,
            kind: WidgetKind2D::VectorProfile {
                closed,
                stroke_color: [255, 255, 255, 255],
                fill_color: [245, 158, 11, 255],
                stroke_width: 2.0,
                points,
            },
        });
        doc
    }

    pub fn primary_profile(&self) -> Option<&[[f32; 2]]> {
        for w in &self.widgets {
            if let WidgetKind2D::VectorProfile { points, .. } = &w.kind {
                return Some(points.as_slice());
            }
        }
        None
    }

    /// Generate typed Rust code defining actions and UI structure for this screen.
    pub fn to_rust_code(&self) -> String {
        let mut src = String::new();
        let safe_type_name = sanitize_ident(&self.name);

        src.push_str("// Código de interfaz generado por Forja 2D (.f2d)\n");
        src.push_str("// Documento: ");
        src.push_str(&self.name);
        src.push_str(&format!(
            " (Resolución base: {:.0}x{:.0})\n\n",
            self.design_resolution[0], self.design_resolution[1]
        ));

        // Typed enum of screen actions
        src.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        src.push_str(&format!("pub enum {safe_type_name}Action {{\n"));

        let mut actions = self.actions.clone();
        for w in &self.widgets {
            match &w.kind {
                WidgetKind2D::Button { action_id, .. } | WidgetKind2D::Toggle { action_id, .. } => {
                    if !action_id.is_empty() && !actions.contains(action_id) {
                        actions.push(action_id.clone());
                    }
                }
                _ => {}
            }
        }

        if actions.is_empty() {
            src.push_str("    Ninguna,\n");
        } else {
            for act in &actions {
                let variant = to_pascal_case(act);
                src.push_str(&format!("    {variant},\n"));
            }
        }
        src.push_str("}\n\n");

        src.push_str(&format!(
            "pub const RESOLUTION: [f32; 2] = [{:.1}, {:.1}];\n",
            self.design_resolution[0], self.design_resolution[1]
        ));
        src.push_str(&format!("pub const WIDGET_COUNT: usize = {};\n", self.widgets.len()));
        src
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        write_str(&mut payload, &self.name);
        payload.extend_from_slice(&self.design_resolution[0].to_le_bytes());
        payload.extend_from_slice(&self.design_resolution[1].to_le_bytes());

        let widget_count = self.widgets.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&widget_count.to_le_bytes());

        for w in self.widgets.iter().take(widget_count as usize) {
            write_str(&mut payload, &w.id);
            write_str(&mut payload, &w.name);
            for v in w.rect {
                payload.extend_from_slice(&v.to_le_bytes());
            }
            payload.push(w.anchor as u8);
            payload.push(if w.visible { 1 } else { 0 });

            match &w.kind {
                WidgetKind2D::Container {
                    background_color,
                    corner_radius,
                    border_color,
                    border_width,
                } => {
                    payload.push(0);
                    payload.extend_from_slice(background_color);
                    payload.extend_from_slice(&corner_radius.to_le_bytes());
                    payload.extend_from_slice(border_color);
                    payload.extend_from_slice(&border_width.to_le_bytes());
                }
                WidgetKind2D::Button {
                    label,
                    action_id,
                    background_color,
                    text_color,
                } => {
                    payload.push(1);
                    write_str(&mut payload, label);
                    write_str(&mut payload, action_id);
                    payload.extend_from_slice(background_color);
                    payload.extend_from_slice(text_color);
                }
                WidgetKind2D::Text {
                    content,
                    font_size,
                    color,
                } => {
                    payload.push(2);
                    write_str(&mut payload, content);
                    payload.extend_from_slice(&font_size.to_le_bytes());
                    payload.extend_from_slice(color);
                }
                WidgetKind2D::Image {
                    texture_name,
                    uv_rect,
                    tint,
                } => {
                    payload.push(3);
                    write_str(&mut payload, texture_name);
                    for v in uv_rect {
                        payload.extend_from_slice(&v.to_le_bytes());
                    }
                    payload.extend_from_slice(tint);
                }
                WidgetKind2D::Toggle {
                    label,
                    action_id,
                    is_checked,
                } => {
                    payload.push(4);
                    write_str(&mut payload, label);
                    write_str(&mut payload, action_id);
                    payload.push(if *is_checked { 1 } else { 0 });
                }
                WidgetKind2D::VectorProfile {
                    closed,
                    stroke_color,
                    fill_color,
                    stroke_width,
                    points,
                } => {
                    payload.push(5);
                    payload.push(if *closed { 1 } else { 0 });
                    payload.extend_from_slice(stroke_color);
                    payload.extend_from_slice(fill_color);
                    payload.extend_from_slice(&stroke_width.to_le_bytes());
                    let pt_count = points.len().min(u32::MAX as usize) as u32;
                    payload.extend_from_slice(&pt_count.to_le_bytes());
                    for pt in points.iter().take(pt_count as usize) {
                        payload.extend_from_slice(&pt[0].to_le_bytes());
                        payload.extend_from_slice(&pt[1].to_le_bytes());
                    }
                }
            }
        }

        let action_count = self.actions.len().min(u16::MAX as usize) as u16;
        payload.extend_from_slice(&action_count.to_le_bytes());
        for act in self.actions.iter().take(action_count as usize) {
            write_str(&mut payload, act);
        }

        let checksum = fnv1a_32(&payload);
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&F2D_MAGIC);
        out.extend_from_slice(&F2D_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::UnexpectedEof);
        }
        if bytes[0..4] != F2D_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != F2D_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let expected_checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let payload = &bytes[12..];
        let actual_checksum = fnv1a_32(payload);
        if expected_checksum != actual_checksum {
            return Err(DecodeError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        let mut cursor = Cursor::new(payload);
        let name = cursor.read_str()?;
        let design_resolution = [cursor.read_f32()?, cursor.read_f32()?];

        let widget_count = cursor.read_u16()? as usize;
        let mut widgets = Vec::with_capacity(widget_count.min(4096));

        for _ in 0..widget_count {
            let id = cursor.read_str()?;
            let w_name = cursor.read_str()?;
            let rect = [
                cursor.read_f32()?,
                cursor.read_f32()?,
                cursor.read_f32()?,
                cursor.read_f32()?,
            ];
            let anchor = Anchor2D::from_u8(cursor.take(1)?[0]).unwrap_or(Anchor2D::Center);
            let visible = cursor.take(1)?[0] != 0;

            let tag = cursor.take(1)?[0];
            let kind = match tag {
                0 => {
                    let background_color = cursor.read_4()?;
                    let corner_radius = cursor.read_f32()?;
                    let border_color = cursor.read_4()?;
                    let border_width = cursor.read_f32()?;
                    WidgetKind2D::Container {
                        background_color,
                        corner_radius,
                        border_color,
                        border_width,
                    }
                }
                1 => {
                    let label = cursor.read_str()?;
                    let action_id = cursor.read_str()?;
                    let background_color = cursor.read_4()?;
                    let text_color = cursor.read_4()?;
                    WidgetKind2D::Button {
                        label,
                        action_id,
                        background_color,
                        text_color,
                    }
                }
                2 => {
                    let content = cursor.read_str()?;
                    let font_size = cursor.read_f32()?;
                    let color = cursor.read_4()?;
                    WidgetKind2D::Text {
                        content,
                        font_size,
                        color,
                    }
                }
                3 => {
                    let texture_name = cursor.read_str()?;
                    let uv_rect = [
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                        cursor.read_f32()?,
                    ];
                    let tint = cursor.read_4()?;
                    WidgetKind2D::Image {
                        texture_name,
                        uv_rect,
                        tint,
                    }
                }
                4 => {
                    let label = cursor.read_str()?;
                    let action_id = cursor.read_str()?;
                    let is_checked = cursor.take(1)?[0] != 0;
                    WidgetKind2D::Toggle {
                        label,
                        action_id,
                        is_checked,
                    }
                }
                5 => {
                    let closed = cursor.take(1)?[0] != 0;
                    let stroke_color = cursor.read_4()?;
                    let fill_color = cursor.read_4()?;
                    let stroke_width = cursor.read_f32()?;
                    let pt_count = cursor.read_u32()? as usize;
                    let mut points = Vec::with_capacity(pt_count.min(65536));
                    for _ in 0..pt_count {
                        points.push([cursor.read_f32()?, cursor.read_f32()?]);
                    }
                    WidgetKind2D::VectorProfile {
                        closed,
                        stroke_color,
                        fill_color,
                        stroke_width,
                        points,
                    }
                }
                _ => return Err(DecodeError::InvalidPhysicsData),
            };

            widgets.push(Widget2D {
                id,
                name: w_name,
                rect,
                anchor,
                visible,
                kind,
            });
        }

        let action_count = cursor.read_u16()? as usize;
        let mut actions = Vec::with_capacity(action_count.min(1024));
        for _ in 0..action_count {
            actions.push(cursor.read_str()?);
        }

        Ok(Self {
            name,
            design_resolution,
            widgets,
            actions,
        })
    }
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        }
    }
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, 'S');
    }
    out
}

fn to_pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' || ch == '.' {
            cap_next = true;
        } else if cap_next {
            out.push(ch.to_ascii_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        String::from("Accion")
    } else {
        out
    }
}
