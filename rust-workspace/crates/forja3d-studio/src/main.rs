#![cfg(not(target_os = "android"))]

fn main() -> eframe::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    {
        Ok(())
    }
}
