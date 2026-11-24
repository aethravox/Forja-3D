# forja3d-studio

Motor y editor 3D multiplataforma escrito 100% en Rust (`eframe` + `glow` + `forja3d`).

## Compilar por plataforma

- **Linux / Windows (nativo):**
  ```bash
  cargo run -p forja3d-studio --release
  ```
- **Android (APK con `cargo-apk`):**
  ```bash
  cargo apk build -p forja3d-studio --release
  ```
- **WebAssembly (WebGL2):**
  ```bash
  cargo build -p forja3d-studio --target wasm32-unknown-unknown --release
  wasm-bindgen --out-dir dist/wasm --target web target/wasm32-unknown-unknown/release/forja3d_studio.wasm
  ```
