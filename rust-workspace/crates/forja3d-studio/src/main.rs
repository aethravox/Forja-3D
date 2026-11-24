fn main() -> eframe::Result<()> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let native_options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_title("Forja3D Studio"),
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
