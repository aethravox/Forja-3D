const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result<()> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let mut viewport = eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Forja3D Studio");
        if let Ok(icon) = eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
            viewport = viewport.with_icon(std::sync::Arc::new(icon));
        }
        let native_options = eframe::NativeOptions {
            viewport,
            depth_buffer: 24,
            ..Default::default()
        };
        eframe::run_native(
            "Forja3D Studio",
            native_options,
            Box::new(|cc| Ok(Box::new(forja3d_studio::StudioApp::new(cc)))),
        )
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        Ok(())
    }
}
