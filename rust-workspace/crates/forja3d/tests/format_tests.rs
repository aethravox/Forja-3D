use forja3d::{
    ColliderShape, DecodeError, Model, PartSpec, PrimitiveKind, RigidBodyKind, RustExportTarget,
    SceneDocument, ShadingMode, FLAG_COMPACT, FLAG_PHYSICS, MAGIC,
};

#[test]
fn roundtrip_f3d_binary_format() {
    let scene = SceneDocument::preset_drone();
    let original = scene.bake();
    let bytes = original.to_bytes();

    assert_eq!(&bytes[0..4], &MAGIC);
    let decoded = Model::from_bytes(&bytes).expect("valid .f3d bytes should decode");

    assert_eq!(decoded.name, original.name);
    assert_eq!(decoded.parts.len(), original.parts.len());
    assert_eq!(decoded.vertex_count(), original.vertex_count());
    assert_eq!(decoded.triangle_count(), original.triangle_count());
    assert_eq!(decoded, original);
}

#[test]
fn detects_corrupt_checksum_and_bad_magic() {
    let scene = SceneDocument::preset_obelisk();
    let mut bytes = scene.bake().to_bytes();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;

    assert!(matches!(
        Model::from_bytes(&bytes),
        Err(DecodeError::ChecksumMismatch { .. })
    ));

    bytes[0] = b'X';
    assert_eq!(Model::from_bytes(&bytes), Err(DecodeError::InvalidMagic));
}

#[test]
fn generates_valid_rust_source_targets() {
    let mut scene = SceneDocument {
        model_name: String::from("Torreta"),
        parts: vec![PartSpec::new("base", PrimitiveKind::Cylinder)],
        ..SceneDocument::default()
    };
    scene.parts[0].scale = [1.5, 0.5, 1.5];
    let model = scene.bake();

    let zero_dep = model.to_rust_source("torreta", RustExportTarget::ZeroDepConst);
    assert!(zero_dep.contains("pub const POSITIONS: &[[f32; 3]]"));
    assert!(zero_dep.contains("pub const INDICES: &[u32]"));

    let f3d_src = model.to_rust_source("torreta", RustExportTarget::Forja3dModel);
    assert!(f3d_src.contains("pub fn build_torreta() -> Model"));

    let wgpu_src = model.to_rust_source("torreta", RustExportTarget::WgpuBytemuck);
    assert!(wgpu_src.contains("bytemuck::Pod"));
}

#[test]
fn mirror_symmetry_duplicates_and_flips_winding() {
    let mut part = PartSpec::new("ala", PrimitiveKind::Cube);
    part.position = [2.0, 1.0, 3.0];
    part.mirror = [true, false, true];

    let baked = part.bake_all();
    assert_eq!(baked.len(), 4);
    assert_eq!(baked[0].name, "ala");
    assert_eq!(baked[1].name, "ala_mz");
    assert_eq!(baked[2].name, "ala_mx");
    assert_eq!(baked[3].name, "ala_mx_mz");

    assert_ne!(baked[0].indices[1], baked[1].indices[1]);
    assert_eq!(baked[0].indices[1], baked[3].indices[1]);
}

#[test]
fn extrude_and_lathe_generate_valid_closed_geometry() {
    let mut ext = PartSpec::new("aleta", PrimitiveKind::Extrude);
    ext.extrude_depth = 0.5;
    let ext_mesh = ext.bake();
    assert!(ext_mesh.vertices.len() >= 14);
    assert!(ext_mesh.indices.len() >= 30);

    let lathe = PartSpec::new("tobera", PrimitiveKind::Lathe);
    let lathe_mesh = lathe.bake();
    assert!(lathe_mesh.vertices.len() > 20);
    assert!(lathe_mesh.indices.len() > 30);
}

#[test]
fn smooth_shading_welds_and_reduces_vertex_count() {
    let mut flat_cube = PartSpec::new("cubo_flat", PrimitiveKind::Cube);
    flat_cube.shading = ShadingMode::Flat;
    let flat_mesh = flat_cube.bake();
    assert_eq!(flat_mesh.vertices.len(), 36);

    let mut smooth_cube = PartSpec::new("cubo_smooth", PrimitiveKind::Cube);
    smooth_cube.shading = ShadingMode::Smooth;
    let smooth_mesh = smooth_cube.bake();
    // 8 unique cube corners welded smoothly!
    assert_eq!(smooth_mesh.vertices.len(), 8);
    assert_eq!(smooth_mesh.indices.len(), 36);
}

#[test]
fn compact_f3d_reduces_size_and_roundtrips() {
    let model = SceneDocument::preset_drone().bake();
    let std_bytes = model.to_bytes();
    let compact_bytes = model.to_bytes_compact();

    assert_eq!(&compact_bytes[0..4], &MAGIC);
    let flags = u16::from_le_bytes([compact_bytes[6], compact_bytes[7]]);
    assert_ne!(flags & FLAG_COMPACT, 0);
    assert!(
        compact_bytes.len() * 100 < std_bytes.len() * 60,
        "compact {} should be <60% of standard {}",
        compact_bytes.len(),
        std_bytes.len()
    );

    let decoded = Model::from_bytes(&compact_bytes).expect("compact .f3d should decode");
    assert_eq!(decoded.name, model.name);
    assert_eq!(decoded.parts.len(), model.parts.len());
    assert_eq!(decoded.vertex_count(), model.vertex_count());
    assert_eq!(decoded.triangle_count(), model.triangle_count());

    for (orig_part, dec_part) in model.parts.iter().zip(decoded.parts.iter()) {
        for (ov, dv) in orig_part.vertices.iter().zip(dec_part.vertices.iter()) {
            for axis in 0..3 {
                assert!((ov.position[axis] - dv.position[axis]).abs() < 1e-3);
            }
        }
    }
}

#[test]
fn wavefront_obj_export_and_scene_from_model() {
    let model = SceneDocument::preset_obelisk().bake();
    let obj = model.to_obj();
    assert!(obj.contains("o base_pedestal"));
    assert!(obj.contains("\nv "));
    assert!(obj.contains("\nvn "));
    assert!(obj.contains("\nf "));

    let imported_scene = SceneDocument::from_model(model.clone());
    assert_eq!(imported_scene.parts.len(), model.parts.len());
    assert_eq!(imported_scene.parts[0].kind, PrimitiveKind::CustomMesh);
    assert!(imported_scene.physics_enabled);
    let rebaked = imported_scene.bake();
    assert_eq!(rebaked.triangle_count(), model.triangle_count());
    assert!(rebaked.physics.is_some());
}

#[test]
fn roundtrip_f3d_with_physics_standard_and_compact() {
    let mut scene = SceneDocument::preset_drone();
    scene.auto_generate_colliders();
    let model = scene.bake();
    assert!(model.physics.is_some());
    assert!(!model.physics.as_ref().unwrap().colliders.is_empty());

    let std_bytes = model.to_bytes();
    let std_flags = u16::from_le_bytes([std_bytes[6], std_bytes[7]]);
    assert_ne!(std_flags & FLAG_PHYSICS, 0);
    assert_eq!(std_flags & FLAG_COMPACT, 0);

    let decoded_std = Model::from_bytes(&std_bytes).expect("physics standard .f3d should decode");
    assert_eq!(decoded_std, model);

    let cmp_bytes = model.to_bytes_compact();
    let cmp_flags = u16::from_le_bytes([cmp_bytes[6], cmp_bytes[7]]);
    assert_ne!(cmp_flags & FLAG_PHYSICS, 0);
    assert_ne!(cmp_flags & FLAG_COMPACT, 0);

    let decoded_cmp = Model::from_bytes(&cmp_bytes).expect("physics compact .f3d should decode");
    assert_eq!(decoded_cmp.physics, model.physics);
}

#[test]
fn f3d_without_physics_flag_decodes_physics_none_and_supports_with_auto_physics() {
    let mut model = SceneDocument::new("SoloMalla").bake();
    model.parts.push(PartSpec::new("bloque", PrimitiveKind::Cube).bake());
    let bytes = model.to_bytes();
    let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
    assert_eq!(flags & FLAG_PHYSICS, 0);

    let decoded = Model::from_bytes(&bytes).expect("model without physics should decode");
    assert!(decoded.physics.is_none());

    let with_phys = decoded.with_auto_physics(RigidBodyKind::Dynamic);
    let phys = with_phys.physics.as_ref().expect("with_auto_physics should populate PhysicsSpec");
    assert_eq!(phys.body_kind, RigidBodyKind::Dynamic);
    assert_eq!(phys.colliders.len(), 1);
    assert!(matches!(phys.colliders[0].shape, ColliderShape::Box { .. }));
}

