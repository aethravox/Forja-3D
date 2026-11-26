use forja3d::{
    default_extrude_profile, encode_texture_to_png, AlignMode, AnimationClip, AnimationProperty,
    AnimationSet, AnimationTargetKind, AnimationTrack, ColliderShape, DecodeError, ForjaPack,
    ForjaProject, InterpolationMode, Keyframe, MaterialPreset, Model, PartSpec, PrimitiveKind,
    ProjectCameraState, ProjectViewportState, RigidBodyKind, RustExportTarget, SceneDocument,
    ShadingMode, SunLightSpec, TerrainPreset, TerrainStamp, Texture, UiDocument2D, Widget2D,
    F2D_MAGIC, F3A_MAGIC, F3P_MAGIC, F3S_MAGIC, F3T_MAGIC, FLAG_COMPACT, FLAG_PHYSICS, GLB_MAGIC,
    GLB_VERSION, MAGIC,
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

#[test]
fn terrain_generates_non_flat_relief_supports_stamps_and_roundtrips_f3d() {
    let mut terrain = PartSpec::new("colinas", PrimitiveKind::Terrain);
    terrain.segments = 24;
    terrain.terrain.preset = TerrainPreset::Mountains;
    terrain.terrain.amplitude = 0.8;
    terrain.terrain.stamps.push(TerrainStamp {
        x: 0.0,
        z: 0.0,
        radius: 0.25,
        height: 1.1,
    });

    let baked = terrain.bake();
    assert_eq!(baked.vertices.len(), 25 * 25);
    assert_eq!(baked.indices.len(), 24 * 24 * 6);
    assert!(
        baked.bounds.max[1] - baked.bounds.min[1] > 0.5,
        "terrain should have non-flat vertical relief: {:?}",
        baked.bounds
    );

    let mut scene = SceneDocument::preset_terrain();
    scene.parts[0] = terrain;
    let model = scene.bake();
    let f3d_bytes = model.to_bytes_compact();
    let decoded = Model::from_bytes(&f3d_bytes).expect("compact terrain .f3d should decode");
    assert_eq!(decoded.parts.len(), model.parts.len());
    assert_eq!(decoded.triangle_count(), model.triangle_count());
}

#[test]
fn roundtrip_forja_project_preserves_scene_lights_textures_and_camera() {
    let mut scene = SceneDocument::preset_terrain();
    let mut custom = PartSpec::new("cristal", PrimitiveKind::Pyramid);
    custom.convert_to_custom_mesh();
    custom.texture_index = Some(0);
    custom.texture_enabled = true;
    scene.parts.push(custom);

    let mut sun = SunLightSpec::new("SolPrincipal");
    sun.position = [4.0, 6.0, -2.5];
    sun.intensity = 1.8;
    scene.sun_lights.push(sun);

    let tex = Texture::new_rgba("roca_albedo", 2, 2, vec![128; 16]);
    let doc2d = UiDocument2D::from_profile("HudJuego", default_extrude_profile(), true);

    let project = ForjaProject {
        scene,
        textures: vec![tex],
        camera: ProjectCameraState {
            yaw: 1.25,
            pitch: 0.62,
            distance: 8.5,
            target: [1.0, 0.5, -0.5],
            ortho: true,
        },
        viewport: ProjectViewportState {
            textures_enabled: true,
            wireframe: true,
            show_grid: false,
            show_colliders: true,
        },
        doc2d: Some(doc2d),
    };

    let bytes = project.to_bytes();
    assert_eq!(&bytes[0..4], &F3S_MAGIC);
    let decoded = ForjaProject::from_bytes(&bytes).expect("valid .forja project should decode");
    assert_eq!(decoded, project);

    let mut corrupted = bytes.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 0xAA;
    assert!(matches!(
        ForjaProject::from_bytes(&corrupted),
        Err(DecodeError::ChecksumMismatch { .. })
    ));
    corrupted[0] = b'B';
    assert_eq!(ForjaProject::from_bytes(&corrupted), Err(DecodeError::InvalidMagic));
}

#[test]
fn uv_tiling_and_offset_transform_baked_uvs_and_persist_in_forja() {
    let mut plane = PartSpec::new("suelo_tiled", PrimitiveKind::Plane);
    plane.uv_scale = [3.0, 2.0];
    plane.uv_offset = [0.25, -0.5];
    plane.alpha_cutoff = 0.45;

    let baked = plane.bake();
    assert_eq!(baked.vertices.len(), 4);
    assert!((baked.vertices[0].uv[0] - 0.25).abs() < 1e-5);
    assert!((baked.vertices[0].uv[1] - (-0.5)).abs() < 1e-5);
    assert!((baked.vertices[2].uv[0] - 3.25).abs() < 1e-5);
    assert!((baked.vertices[2].uv[1] - 1.5).abs() < 1e-5);

    let mut scene = SceneDocument::new("UvScene");
    scene.parts.push(plane);
    let proj = ForjaProject::new(scene);
    let bytes = proj.to_bytes();
    let decoded = ForjaProject::from_bytes(&bytes).expect("v2 .forja project should decode");
    assert_eq!(decoded.scene.parts[0].uv_scale, [3.0, 2.0]);
    assert_eq!(decoded.scene.parts[0].uv_offset, [0.25, -0.5]);
    assert!((decoded.scene.parts[0].alpha_cutoff - 0.45).abs() < 1e-5);
}

#[test]
fn material_presets_cover_metals_and_transparent_materials() {
    let mut part = PartSpec::new("ventana", PrimitiveKind::Cube);
    let glass = MaterialPreset::ALL
        .iter()
        .find(|p| p.id == "glass")
        .expect("glass preset");
    part.apply_material_preset(*glass);
    assert!(part.color[3] < 255);
    assert!(part.bake().material.has_alpha());

    let gold = MaterialPreset::ALL
        .iter()
        .find(|p| p.id == "gold")
        .expect("gold preset");
    part.apply_material_preset(*gold);
    assert!(part.metallic > 0.9);
    assert_eq!(part.color[3], 255);
}

#[test]
fn glb_export_writes_valid_container_meshes_png_textures_and_lights() {
    let mut scene = SceneDocument::new("EscenaGLB");
    let mut gold_box = PartSpec::new("lingote", PrimitiveKind::Cube);
    gold_box.apply_material_preset(
        *MaterialPreset::ALL
            .iter()
            .find(|p| p.id == "gold")
            .unwrap(),
    );
    gold_box.uv_scale = [2.0, 2.0];
    gold_box.texture_index = Some(0);
    gold_box.texture_enabled = true;

    let mut glass = PartSpec::new("panel_vidrio", PrimitiveKind::Plane);
    glass.apply_material_preset(
        *MaterialPreset::ALL
            .iter()
            .find(|p| p.id == "glass")
            .unwrap(),
    );
    glass.texture_enabled = false;

    let mut foliage = PartSpec::new("hoja", PrimitiveKind::Plane);
    foliage.alpha_cutoff = 0.5;
    foliage.texture_enabled = false;

    scene.parts.push(gold_box);
    scene.parts.push(glass);
    scene.parts.push(foliage);

    let sun = SunLightSpec::new("SolMediodia");
    scene.sun_lights.push(sun);

    let tex = Texture::new_rgba(
        "metal_tex",
        2,
        2,
        vec![
            255, 220, 120, 255, 240, 200, 100, 255, 210, 170, 80, 255, 255, 240, 150, 255,
        ],
    );
    let png = encode_texture_to_png(&tex);
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");

    let glb = scene.to_glb(&[tex]);
    assert!(glb.len() > 28);
    assert_eq!(&glb[0..4], &GLB_MAGIC);
    assert_eq!(u32::from_le_bytes([glb[4], glb[5], glb[6], glb[7]]), GLB_VERSION);
    let total_len = u32::from_le_bytes([glb[8], glb[9], glb[10], glb[11]]) as usize;
    assert_eq!(total_len, glb.len());

    let json_len = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]) as usize;
    assert_eq!(json_len % 4, 0);
    let json_type = u32::from_le_bytes([glb[16], glb[17], glb[18], glb[19]]);
    assert_eq!(json_type, 0x4E4F_534A);

    let json_str = core::str::from_utf8(&glb[20..20 + json_len]).expect("valid utf8 JSON chunk");
    assert!(json_str.contains(r#""KHR_lights_punctual""#));
    assert!(json_str.contains(r#""type":"directional""#));
    assert!(json_str.contains(r#""alphaMode":"BLEND""#));
    assert!(json_str.contains(r#""alphaMode":"MASK""#));
    assert!(json_str.contains(r#""mimeType":"image/png""#));
    assert!(json_str.contains("SolMediodia"));

    let bin_hdr = 20 + json_len;
    let bin_len = u32::from_le_bytes([
        glb[bin_hdr],
        glb[bin_hdr + 1],
        glb[bin_hdr + 2],
        glb[bin_hdr + 3],
    ]) as usize;
    assert_eq!(bin_len % 4, 0);
    let bin_type = u32::from_le_bytes([
        glb[bin_hdr + 4],
        glb[bin_hdr + 5],
        glb[bin_hdr + 6],
        glb[bin_hdr + 7],
    ]);
    assert_eq!(bin_type, 0x004E_4942);
    assert_eq!(bin_hdr + 8 + bin_len, glb.len());
    // Embedded PNG sits at offset 0 of BIN chunk
    assert_eq!(&glb[bin_hdr + 8..bin_hdr + 16], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn stl_ply_exports_and_watertight_validator_work_correctly() {
    let mut cube_scene = SceneDocument::new("Impresion3D");
    cube_scene.parts.push(PartSpec::new("cubo_estanco", PrimitiveKind::Cube));
    let cube_report = cube_scene.validate_watertight();
    assert!(cube_report.is_watertight, "cube should be watertight: {cube_report:?}");
    assert_eq!(cube_report.boundary_edges, 0);
    assert_eq!(cube_report.non_manifold_edges, 0);
    assert_eq!(cube_report.inverted_edges, 0);
    assert!((cube_report.signed_volume - 1.0).abs() < 1e-3);

    let stl = cube_scene.to_stl_binary();
    assert_eq!(stl.len(), 84 + 12 * 50);
    let tri_count = u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]);
    assert_eq!(tri_count, 12);

    let ply = String::from_utf8(cube_scene.to_ply_ascii()).expect("valid ASCII PLY");
    assert!(ply.starts_with("ply\nformat ascii 1.0\n"));
    assert!(ply.contains("element face 12\n"));

    let mut open_plane = SceneDocument::new("PlanoAbierto");
    open_plane.parts.push(PartSpec::new("plano", PrimitiveKind::Plane));
    let plane_report = open_plane.validate_watertight();
    assert!(!plane_report.is_watertight);
    assert!(plane_report.boundary_edges > 0);
}

#[test]
fn ground_snapping_alignment_and_locked_flag_persist_in_forja_v3() {
    let mut scene = SceneDocument::preset_terrain();
    let mut box_part = PartSpec::new("caja", PrimitiveKind::Cube);
    box_part.position = [0.25, 4.0, -0.3];
    box_part.locked = true;
    scene.parts.push(box_part);

    let box_idx = scene.parts.len() - 1;
    assert!(scene.snap_part_to_ground(box_idx));
    let ground_y = scene.ground_height_at(0.25, -0.3, Some(box_idx));
    let aabb = scene.part_world_aabb(box_idx).unwrap();
    assert!((aabb.min[1] - ground_y).abs() < 1e-3);

    assert!(scene.align_part_to_coord(box_idx, 0, AlignMode::Center, 0.0));
    let aligned_aabb = scene.part_world_aabb(box_idx).unwrap();
    assert!(((aligned_aabb.min[0] + aligned_aabb.max[0]) * 0.5).abs() < 1e-4);

    let bytes = ForjaProject::new(scene).to_bytes();
    let decoded = ForjaProject::from_bytes(&bytes).expect(".forja v3 should decode");
    assert!(decoded.scene.parts[box_idx].locked);
}




