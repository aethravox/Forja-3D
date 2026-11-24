mod app;
pub use app::StudioApp;

#[cfg(target_arch = "wasm32")]
pub use app::wasm_entry;

#[cfg(target_os = "android")]
pub use app::android_main;
