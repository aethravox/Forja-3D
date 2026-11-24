# Forja3D Workspace

Workspace en Rust compuesto por dos crates publicables en `crates.io`:

1. **`forja3d`** (`crates/forja3d`): Crate **sin dependencias externas** (`#![no_std]` + `alloc`, `std` por defecto) que define:
   - El formato binario propio **`.f3d`** (`F3D1`) con validación de magic bytes, versión, modo compacto (`FLAG_COMPACT`, 17 B/vértice + índices `u16`), límites de índices y checksum FNV-1a.
   - El cargador `forja3d::Model::from_bytes(...)` (decodifica estándar y compacto) listo para `include_bytes!`.
   - Generador de código fuente **`.rs`** (`forja3d::Model`, `const` zero-dep, y `wgpu` / `bytemuck`) y exportador **Wavefront `.obj`**.
   - Primitivas 3D procedurales, extrusión 2D (incluyendo polígonos cóncavos por *ear-clipping*), revolución (*lathe*), sombreado `Flat`/`Smooth`, deduplicación de vértices e importación de mallas `.f3d` editables.

2. **`forja3d-studio`** (`crates/forja3d-studio`): Motor y editor visual 3D con interfaz y renderizado WebGL2/OpenGL 100% en Rust (`eframe` + `glow`), importador **`.glb`/`.gltf`** (mediante el crate `gltf`) y `.f3d`, compatible con:
   - **Windows** (`cargo run -p forja3d-studio --release`)
   - **Linux** (`cargo run -p forja3d-studio --release`)
   - **macOS** (`cargo run -p forja3d-studio --release`)
   - **Android** (`cargo apk build -p forja3d-studio --lib --release`)
   - **WebAssembly** (`cargo build -p forja3d-studio --lib --target wasm32-unknown-unknown --release`)

Repositorio oficial y Releases automáticos: <https://github.com/aethravox/Forja-3D>

## Verificación y tests

```bash
cargo test --manifest-path rust-workspace/Cargo.toml --workspace
```

## Publicar en crates.io

```bash
cargo publish -p forja3d
cargo publish -p forja3d-studio
```
