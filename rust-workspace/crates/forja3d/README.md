# forja3d

Librería en Rust **sin dependencias externas** (`no_std` + `alloc`) para cargar, validar y generar modelos 3D en formato propio **`.f3d`** o directamente como módulos **`.rs`**.

## Cargar un modelo `.f3d`

```rust
use forja3d::Model;

static MODEL_BYTES: &[u8] = include_bytes!("../assets/drone.f3d");

fn load() -> Result<Model, forja3d::DecodeError> {
    let model = Model::from_bytes(MODEL_BYTES)?;
    println!("Partes: {}, Triángulos: {}", model.parts.len(), model.triangle_count());
    Ok(model)
}
```

## Exportar a `.rs`

```rust
use forja3d::{RustExportTarget, SceneDocument};

let scene = SceneDocument::preset_drone();
let model = scene.bake();
let rust_code = model.to_rust_source("drone_model", RustExportTarget::ZeroDepConst);
```
