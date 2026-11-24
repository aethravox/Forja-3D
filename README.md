# Forja3D

Motor y editor visual 3D multiplataforma escrito 100% en Rust (`#![no_std]` en el núcleo `forja3d` y renderizado OpenGL / WebGL2 con `eframe` + `glow` en `forja3d-studio`).

## Crates del Workspace

- **`forja3d`** (`rust-workspace/crates/forja3d`): Formatos binarios `#![no_std]` con checksum FNV-1a (**`.f3d`**, **`.f3t`**, **`.f3a`**, **`.f3p`**, **`.f2d`**), primitivas procedurales, extrusión, revolución (*lathe*), sombreado `Flat`/`Smooth` y exportadores `.rs`/`.obj`. **Cero dependencias externas** (`#![no_std]` + `alloc`).
- **`forja3d-physics`** (`rust-workspace/crates/forja3d-physics`): Motor de físicas 3D `#![no_std]` **sin dependencias externas** nativo para modelos `.f3d` y paquetes `.f3p` (`Box`, `Sphere`, `Capsule`, cuerpos `Dynamic`/`Static`/`Kinematic`/`Sensor`, impulsos y *raycast*).
- **`forja3d-studio`** (`rust-workspace/crates/forja3d-studio`): Estudio de modelado y físicas 3D para **Windows, Linux, macOS, Android y WebAssembly**, con importador/exportador **`.f3d`, `.f3t`, `.f3a`, `.f3p`, `.f2d`** y **`.glb`/`.gltf`**, gizmo X/Y/Z, colisionadores, simulación en vivo, espejo de simetría, grupos y deshacer/rehacer.

## Formatos Forja3D

| Formato | Nombre | Qué guarda | Ventaja principal |
|---|---|---|---|
| **.f3d** | Forja 3D (Malla / Geometría) | Vértices (posición, normal, color sRGBA, UV), sub-mallas y propiedades base de material. | Permite reutilizar la misma malla con distintas texturas (.f3t) o animaciones (.f3a) en múltiples personajes o variantes. |
| **.f3t** | Forja 3D Texture (Textura) | Datos de textura en formato binario propio (píxeles listos/cuantizados, mipmaps o compresión, flags y checksum). | Evita depender de decodificadores externos en no_std y dificulta editar o reemplazar imágenes sueltas como un .png común. |
| **.f3a** | Forja 3D Animation (Animación) | Clips de animación por separado: keyframes, transformaciones, nodos o huesos (skinning). | Permite compartir clips entre varios modelos y cargar solo las animaciones necesarias sin inflar un solo archivo. |
| **.f3p** | Forja 3D Pack (Paquete completo) | Contenedor todo-en-uno con geometría, texturas y animaciones reunidas en un solo archivo. | Ideal para assets cerrados o personajes/objetos únicos que quieres mover e integrar de una sola pieza. |
| **.f2d** | Forja 2D (Pantallas e Interfaces) | Estructura visual 2D (contenedores, botones, textos, anclajes/layout) y eventos/acciones tipadas en .rs. | Permite diseñar pantallas visualmente y compilarlas directo en la app junto con las acciones de sus controles. |

## Compilación rápida

```bash
# Tests del workspace y validación #![no_std]
cargo test --manifest-path rust-workspace/Cargo.toml --workspace
cargo check --manifest-path rust-workspace/Cargo.toml -p forja3d --no-default-features --target wasm32-unknown-unknown
cargo check --manifest-path rust-workspace/Cargo.toml -p forja3d-physics --no-default-features --target wasm32-unknown-unknown

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
