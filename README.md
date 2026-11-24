# Forja3D

Motor y editor visual 3D multiplataforma escrito 100% en Rust (`#![no_std]` en el núcleo `forja3d` y renderizado OpenGL / WebGL2 con `eframe` + `glow` en `forja3d-studio`).

## Crates del Workspace

- **`forja3d`** (`rust-workspace/crates/forja3d`): Formato binario `.f3d` (`F3D1`) con modo estándar y compacto (`17 B/vértice`), checksum FNV-1a, primitivas procedurales, extrusión, revolución (*lathe*), sombreado `Flat`/`Smooth` y exportadores a código `.rs` y Wavefront `.obj`. **Cero dependencias externas** (`#![no_std]` + `alloc`).
- **`forja3d-studio`** (`rust-workspace/crates/forja3d-studio`): Estudio de modelado 3D para **Windows, Linux, macOS, Android y WebAssembly**, con importador **`.glb`/`.gltf`** (vía `gltf`) y `.f3d`, gizmo X/Y/Z, espejo de simetría, grupos y deshacer/rehacer.

## Compilación rápida

```bash
# Tests del workspace y validación #![no_std]
cargo test --manifest-path rust-workspace/Cargo.toml --workspace
cargo check --manifest-path rust-workspace/Cargo.toml -p forja3d --no-default-features --target wasm32-unknown-unknown

# Ejecutar Studio en escritorio
cargo run --manifest-path rust-workspace/Cargo.toml -p forja3d-studio --release
```

## Releases Multiplataforma

Cada tag `v*` publicado dispara GitHub Actions para compilar y adjuntar en **Releases**:
- Windows (`x86_64`)
- Linux (`x86_64`)
- macOS Universal (`arm64` + `x86_64`)
- Android (`arm64-v8a` `.apk`)
- WebAssembly (`.wasm` + `.js` + `index.html`)
