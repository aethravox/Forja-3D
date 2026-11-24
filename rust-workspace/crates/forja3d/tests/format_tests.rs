use forja3d::{
    default_extrude_profile, Anchor2D, AnimationClip, AnimationProperty, AnimationSet,
    AnimationTargetKind, AnimationTrack, ColliderShape, DecodeError, Document2D, ForjaPack,
    InterpolationMode, Keyframe, Model, PartSpec, PrimitiveKind, RigidBodyKind, RustExportTarget,
    SceneDocument, ShadingMode, Texture, UiDocument2D, Widget2D, WidgetKind2D, F2D_MAGIC,
    F3A_MAGIC, F3P_MAGIC, F3T_MAGIC, FLAG_COMPACT, FLAG_PHYSICS, MAGIC,
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

#[test]
fn roundtrip_f3a_animation_and_sampling() {
    let mut set = AnimationSet::new("DronAnim");
    let mut clip = AnimationClip::new("Idle", 2.0, true);

    let mut pos_track = AnimationTrack::new(
        "chasis",
        AnimationProperty::Translation,
        InterpolationMode::Linear,
    );
    pos_track.keyframes.push(Keyframe {
        time: 0.0,
        value: [0.0, 0.0, 0.0, 0.0],
    });
    pos_track.keyframes.push(Keyframe {
        time: 1.0,
        value: [0.0, 2.0, 0.0, 0.0],
    });
    pos_track.keyframes.push(Keyframe {
        time: 2.0,
        value: [0.0, 0.0, 0.0, 0.0],
    });

    let mut rot_track = AnimationTrack::new(
        "rotor",
        AnimationProperty::Rotation,
        InterpolationMode::Linear,
    );
    rot_track.keyframes.push(Keyframe {
        time: 0.0,
        value: [0.0, 0.0, 0.0, 1.0],
    });
    rot_track.keyframes.push(Keyframe {
        time: 2.0,
        value: [0.0, 1.0, 0.0, 0.0],
    });

    let mut scale_track = AnimationTrack::new(
        "faro",
        AnimationProperty::Scale,
        InterpolationMode::Step,
    );
    scale_track.keyframes.push(Keyframe {
        time: 0.0,
        value: [1.0, 1.0, 1.0, 0.0],
    });
    scale_track.keyframes.push(Keyframe {
        time: 1.0,
        value: [2.0, 2.0, 2.0, 0.0],
    });

    clip.tracks.push(pos_track);
    clip.tracks.push(rot_track);
    clip.tracks.push(scale_track);
    set.clips.push(clip);

    let bytes = set.to_bytes();
    assert_eq!(&bytes[0..4], &F3A_MAGIC);
    let decoded = AnimationSet::from_bytes(&bytes).expect("valid .f3a should decode");
    assert_eq!(decoded, set);
    assert_eq!(decoded.total_keyframes(), 7);

    let dec_clip = &decoded.clips[0];
    assert!((dec_clip.wrap_time(2.5) - 0.5).abs() < 1e-5);
    assert!((dec_clip.wrap_time(-0.25) - 1.75).abs() < 1e-5);

    let sampled_pos = dec_clip.tracks[0].sample(0.5).unwrap();
    assert!((sampled_pos[1] - 1.0).abs() < 1e-5);

    let sampled_rot = dec_clip.tracks[1].sample(1.0).unwrap();
    let q_len = (sampled_rot[0] * sampled_rot[0]
        + sampled_rot[1] * sampled_rot[1]
        + sampled_rot[2] * sampled_rot[2]
        + sampled_rot[3] * sampled_rot[3])
        .sqrt();
    assert!((q_len - 1.0).abs() < 1e-4);

    let sampled_scale = dec_clip.tracks[2].sample(0.9).unwrap();
    assert_eq!(sampled_scale, [1.0, 1.0, 1.0, 0.0]);

    let mut bad = bytes.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0x55;
    assert!(matches!(
        AnimationSet::from_bytes(&bad),
        Err(DecodeError::ChecksumMismatch { .. })
    ));
    bad[0] = b'Z';
    assert_eq!(AnimationSet::from_bytes(&bad), Err(DecodeError::InvalidMagic));
}

#[test]
fn roundtrip_f3p_pack_all_in_one() {
    let mut scene = SceneDocument::preset_drone();
    scene.auto_generate_colliders();
    let model = scene.bake();

    let tex = Texture::new_rgba("albedo_dron", 2, 2, vec![255; 16]);

    let mut anim_set = AnimationSet::new("VueloDron");
    let mut clip = AnimationClip::new("Flotar", 1.0, true);
    let mut track = AnimationTrack::new_with_kind(
        "chasis",
        AnimationTargetKind::Node,
        AnimationProperty::Translation,
        InterpolationMode::Linear,
    );
    track.keyframes.push(Keyframe {
        time: 0.0,
        value: [0.0, 0.0, 0.0, 0.0],
    });
    clip.tracks.push(track);
    anim_set.clips.push(clip);

    let mut pack = ForjaPack::new("DronCompleto");
    pack.model = Some(model.clone());
    pack.textures.push(tex.clone());
    pack.animations.push(anim_set.clone());

    let bytes = pack.to_bytes();
    assert_eq!(&bytes[0..4], &F3P_MAGIC);

    let decoded = ForjaPack::from_bytes(&bytes).expect("valid .f3p should decode");
    assert_eq!(decoded.name, "DronCompleto");
    assert_eq!(decoded.model, Some(model));
    assert_eq!(decoded.textures.len(), 1);
    assert_eq!(decoded.textures[0], tex);
    assert_eq!(decoded.animations.len(), 1);
    assert_eq!(decoded.animations[0], anim_set);
    assert_eq!(decoded.total_animation_clips(), 1);

    let mut bad = bytes.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0xFF;
    assert!(matches!(
        ForjaPack::from_bytes(&bad),
        Err(DecodeError::ChecksumMismatch { .. })
    ));
}

#[test]
fn roundtrip_f2d_screen_document_and_f3t_texture() {
    let profile = default_extrude_profile();
    let mut doc = UiDocument2D::from_profile("PerfilAla", profile.clone(), true);
    doc.widgets.push(Widget2D::new_container(
        "panel_hud",
        "Panel HUD",
        [10.0, 10.0, 300.0, 120.0],
        [20, 24, 34, 230],
    ));
    doc.widgets.push(Widget2D::new_button(
        "btn_iniciar",
        "Boton Iniciar",
        "Iniciar Misión",
        "on_start_mission",
        [20.0, 20.0, 160.0, 44.0],
    ));
    doc.actions.push(String::from("on_pause_game"));

    let rust_code = doc.to_rust_code();
    assert!(rust_code.contains("pub enum PerfilAlaAction"));
    assert!(rust_code.contains("OnStartMission"));
    assert!(rust_code.contains("OnPauseGame"));

    let f2d_bytes = doc.to_bytes();
    assert_eq!(&f2d_bytes[0..4], &F2D_MAGIC);
    let dec_doc = UiDocument2D::from_bytes(&f2d_bytes).expect("valid .f2d should decode");
    assert_eq!(dec_doc, doc);
    assert_eq!(dec_doc.primary_profile(), Some(profile.as_slice()));
    assert_eq!(dec_doc.widgets.len(), 3);

    let tex = Texture::new_rgba(
        "metal_albedo",
        2,
        2,
        vec![
            255, 128, 0, 255, 128, 255, 0, 255, 0, 128, 255, 255, 255, 255, 255, 255,
        ],
    );
    let f3t_bytes = tex.to_bytes();
    assert_eq!(&f3t_bytes[0..4], &F3T_MAGIC);
    let dec_tex = Texture::from_bytes(&f3t_bytes).expect("valid .f3t should decode");
    assert_eq!(dec_tex, tex);
}
