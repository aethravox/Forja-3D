use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgIcon {
    Move,
    Rotate,
    Scale,
    Vertices,
    Snap,
    Undo,
    Redo,
    Play,
    Stop,
    Impulse,
    Folder,
    Upload,
    Download,
    Copy,
    Trash,
    Duplicate,
    Plus,
    Minus,
    Center,
    Wire,
    Grid,
    Collider,
    View3d,
    View2d,
    Cubes,
    Sliders,
    Stack,
    Merge,
    Subdivide,
    Close,
    Eye,
    Sun,
    Texture,
}

impl SvgIcon {
    pub fn source(self) -> egui::ImageSource<'static> {
        match self {
            Self::Move => egui::include_image!("../assets/icons/move.svg"),
            Self::Rotate => egui::include_image!("../assets/icons/rotate.svg"),
            Self::Scale => egui::include_image!("../assets/icons/scale.svg"),
            Self::Vertices => egui::include_image!("../assets/icons/vertices.svg"),
            Self::Snap => egui::include_image!("../assets/icons/snap.svg"),
            Self::Undo => egui::include_image!("../assets/icons/undo.svg"),
            Self::Redo => egui::include_image!("../assets/icons/redo.svg"),
            Self::Play => egui::include_image!("../assets/icons/play.svg"),
            Self::Stop => egui::include_image!("../assets/icons/stop.svg"),
            Self::Impulse => egui::include_image!("../assets/icons/impulse.svg"),
            Self::Folder => egui::include_image!("../assets/icons/folder.svg"),
            Self::Upload => egui::include_image!("../assets/icons/upload.svg"),
            Self::Download => egui::include_image!("../assets/icons/download.svg"),
            Self::Copy => egui::include_image!("../assets/icons/copy.svg"),
            Self::Trash => egui::include_image!("../assets/icons/trash.svg"),
            Self::Duplicate => egui::include_image!("../assets/icons/duplicate.svg"),
            Self::Plus => egui::include_image!("../assets/icons/plus.svg"),
            Self::Minus => egui::include_image!("../assets/icons/minus.svg"),
            Self::Center => egui::include_image!("../assets/icons/center.svg"),
            Self::Wire => egui::include_image!("../assets/icons/wire.svg"),
            Self::Grid => egui::include_image!("../assets/icons/grid.svg"),
            Self::Collider => egui::include_image!("../assets/icons/collider.svg"),
            Self::View3d => egui::include_image!("../assets/icons/view_3d.svg"),
            Self::View2d => egui::include_image!("../assets/icons/view_2d.svg"),
            Self::Cubes => egui::include_image!("../assets/icons/cubes.svg"),
            Self::Sliders => egui::include_image!("../assets/icons/sliders.svg"),
            Self::Stack => egui::include_image!("../assets/icons/stack.svg"),
            Self::Merge => egui::include_image!("../assets/icons/merge.svg"),
            Self::Subdivide => egui::include_image!("../assets/icons/subdivide.svg"),
            Self::Close => egui::include_image!("../assets/icons/close.svg"),
            Self::Eye => egui::include_image!("../assets/icons/eye.svg"),
            Self::Sun => egui::include_image!("../assets/icons/sun.svg"),
            Self::Texture => egui::include_image!("../assets/icons/texture.svg"),
        }
    }
}

pub fn svg_img(icon: SvgIcon, size: f32) -> egui::Image<'static> {
    egui::Image::new(icon.source()).fit_to_exact_size(egui::vec2(size, size))
}

pub fn icon_btn(
    ui: &mut egui::Ui,
    icon: SvgIcon,
    icon_sz: f32,
    btn_sz: [f32; 2],
    selected: bool,
) -> egui::Response {
    ui.add_sized(
        btn_sz,
        egui::Button::image(svg_img(icon, icon_sz))
            .image_tint_follows_text_color(true)
            .selected(selected),
    )
}

pub fn icon_btn_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: SvgIcon,
    icon_sz: f32,
    btn_sz: [f32; 2],
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::image(svg_img(icon, icon_sz))
            .image_tint_follows_text_color(true)
            .min_size(egui::vec2(btn_sz[0], btn_sz[1])),
    )
}

pub fn icon_text_btn(
    ui: &mut egui::Ui,
    icon: SvgIcon,
    icon_sz: f32,
    text: &str,
    btn_sz: [f32; 2],
    selected: bool,
) -> egui::Response {
    ui.add_sized(
        btn_sz,
        egui::Button::image_and_text(svg_img(icon, icon_sz), text)
            .image_tint_follows_text_color(true)
            .selected(selected),
    )
}

pub fn icon_text_btn_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: SvgIcon,
    icon_sz: f32,
    text: &str,
    btn_sz: [f32; 2],
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(svg_img(icon, icon_sz), text)
            .image_tint_follows_text_color(true)
            .min_size(egui::vec2(btn_sz[0], btn_sz[1])),
    )
}
