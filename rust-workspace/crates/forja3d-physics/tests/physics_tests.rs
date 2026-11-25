use forja3d::{
    ColliderShape, ColliderSpec, ForjaPack, Model, PartSpec, PhysicsSpec, PrimitiveKind,
    RigidBodyKind, SceneDocument,
};
use forja3d_physics::PhysicsWorld;

#[test]
fn spawn_from_f3d_bytes_inherits_embedded_physics_and_bounces_on_ground() {
    let mut scene = SceneDocument::new("PelotaRebota");
    let mut sphere = PartSpec::new("esfera", PrimitiveKind::Sphere);
    sphere.position = [0.0, 0.5, 0.0];
    sphere.collider = ColliderShape::Sphere { radius: 0.5 };
    scene.parts.push(sphere);
    scene.physics_enabled = true;
    scene.physics = PhysicsSpec {
        body_kind: RigidBodyKind::Dynamic,
        mass: 1.5,
        friction: 0.4,
        restitution: 0.75,
        linear_damping: 0.01,
        gravity_scale: 1.0,
        colliders: Vec::new(),
    };

    let bytes = scene.bake().to_bytes();
    let decoded = Model::from_bytes(&bytes).expect("valid .f3d with physics");

    let mut world = PhysicsWorld::new();
    let handle = world.spawn_from_model(&decoded, [0.0, 3.0, 0.0]);

    let mut bounced_up = false;
    let mut prev_vy = 0.0;
    for _ in 0..180 {
        world.step(1.0 / 60.0);
        let b = world.body(handle).unwrap();
        if prev_vy < -1.0 && b.velocity[1] > 0.5 {
            bounced_up = true;
        }
        prev_vy = b.velocity[1];
        assert!(b.position[1] >= -0.05, "should not tunnel below ground plane");
    }
    assert!(bounced_up, "restitution 0.75 sphere should bounce upward");
}

#[test]
fn dynamic_box_collides_with_static_obstacle_and_triggers_sensor() {
    let mut box_model = Model::new("CajaDinamica");
    box_model.parts.push(PartSpec::new("cubo", PrimitiveKind::Cube).bake());
    let mut dyn_model = box_model.with_auto_physics(RigidBodyKind::Dynamic);
    dyn_model.physics.as_mut().unwrap().restitution = 0.0;

    let sensor_model = Model {
        name: String::from("MetaSensor"),
        parts: Vec::new(),
        physics: Some(PhysicsSpec {
            body_kind: RigidBodyKind::Sensor,
            colliders: vec![ColliderSpec {
                part_hash: 1,
                offset: [0.0, 0.0, 0.0],
                shape: ColliderShape::Box {
                    half_extents: [1.0, 1.0, 1.0],
                },
            }],
            ..PhysicsSpec::default()
        }),
    };

    let mut world = PhysicsWorld::new();
    let platform = world.add_static_box("plataforma", [0.0, 0.5, 0.0], [2.0, 0.5, 2.0]);
    let crate_body = world.spawn_from_model(&dyn_model, [0.0, 3.0, 0.0]);
    let sensor = world.spawn_from_model(&sensor_model, [0.0, 1.4, 0.0]);

    for _ in 0..150 {
        world.step(1.0 / 60.0);
    }

    let b = world.body(crate_body).unwrap();
    // Platform top is y=1.0, crate bottom offset puts center near y=1.0
    assert!(
        b.position[1] >= 0.9 && b.position[1] <= 1.25,
        "crate should rest on static platform, got y={}",
        b.position[1]
    );

    let overlaps = world.sensor_overlaps(sensor);
    assert!(overlaps.contains(&crate_body), "sensor should detect crate resting inside zone");
    assert!(overlaps.contains(&platform), "sensor overlaps static platform");
}

#[test]
fn raycast_hits_spawned_f3d_collider() {
    let model = SceneDocument::preset_obelisk().bake();
    let mut world = PhysicsWorld::new();
    let obelisk = world.spawn_from_model(&model, [0.0, 0.0, 0.0]);

    let hit = world
        .raycast([0.0, 0.2, 5.0], [0.0, 0.0, -1.0], 20.0)
        .expect("ray should hit obelisk base");
    assert_eq!(hit.body, obelisk);
    assert!((hit.distance - 4.1).abs() < 0.25, "hit distance {}", hit.distance);
    assert!(hit.normal[2] > 0.9);
}

#[test]
fn spawn_from_f3p_pack_bytes_simulates() {
    let mut model = Model::new("CapsulaLunar");
    let mut part = PartSpec::new("cuerpo", PrimitiveKind::Sphere);
    part.collider = ColliderShape::Sphere { radius: 0.5 };
    model.parts.push(part.bake());
    model.physics = Some(PhysicsSpec {
        body_kind: RigidBodyKind::Dynamic,
        mass: 2.0,
        friction: 0.5,
        restitution: 0.6,
        linear_damping: 0.01,
        gravity_scale: 1.0,
        colliders: vec![ColliderSpec {
            part_hash: 42,
            offset: [0.0, 0.0, 0.0],
            shape: ColliderShape::Sphere { radius: 0.5 },
        }],
    });

    let mut pack = ForjaPack::new("PackLunar");
    pack.model = Some(model);
    let f3p_bytes = pack.to_bytes();

    let mut world = PhysicsWorld::new();
    let handle = world
        .spawn_from_f3p_bytes(&f3p_bytes, [0.0, 2.5, 0.0])
        .expect("valid .f3p should decode")
        .expect("model should be present in pack");

    for _ in 0..120 {
        world.step(1.0 / 60.0);
    }
    let b = world.body(handle).unwrap();
    assert_eq!(b.name, "CapsulaLunar");
    assert!(b.position[1] >= 0.45 && b.position[1] < 2.5);
}


#[test]
fn dynamic_sphere_bounces_and_rolls_down_static_pyramid_slope() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut pyramid = PartSpec::new("piramide", PrimitiveKind::Pyramid);
    pyramid.position = [0.0, 0.5, 0.0];
    pyramid.scale = [2.0, 1.4, 2.0];
    let pyr_baked = pyramid.bake();
    let mut tris = Vec::new();
    for tri in pyr_baked.indices.chunks_exact(3) {
        let a = pyr_baked.vertices[tri[0] as usize].position;
        let b = pyr_baked.vertices[tri[1] as usize].position;
        let c = pyr_baked.vertices[tri[2] as usize].position;
        tris.push([a, b, c]);
    }

    let mut sphere = PartSpec::new("esfera", PrimitiveKind::Sphere);
    sphere.position = [0.12, 2.1, 0.0];
    sphere.scale = [0.5, 0.5, 0.5];

    let mut world = PhysicsWorld::new();
    let mut pyr_body = RigidBody::from_model(&Model::new("Piramide"), [0.0, 0.0, 0.0]).with_mesh_triangles(tris);
    pyr_body.body_kind = RigidBodyKind::Static;
    pyr_body.inv_mass = 0.0;
    world.add_body(pyr_body);

    let sph_body = RigidBody {
        name: String::from("Esfera"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.12, 2.1, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.35,
        restitution: 0.45,
        linear_damping: 0.02,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.25 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    };
    let sph_handle = world.add_body(sph_body);

    for _ in 0..180 {
        world.step(1.0 / 60.0);
    }
    let final_sph = world.body(sph_handle).unwrap();
    assert!(
        final_sph.position[0] > 0.6,
        "sphere should bounce/roll laterally down the pyramid slope, got x={}",
        final_sph.position[0]
    );
    assert!(
        final_sph.position[1] < 0.8,
        "sphere should descend onto ground/lower slope, got y={}",
        final_sph.position[1]
    );
}

#[test]
fn sphere_dropped_dead_center_on_pyramid_apex_tips_and_rolls_off() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut pyramid = PartSpec::new("piramide", PrimitiveKind::Pyramid);
    pyramid.position = [0.0, 0.5, 0.0];
    pyramid.scale = [1.4, 1.2, 1.4];
    let pyr_baked = pyramid.bake();
    let mut tris = Vec::new();
    for tri in pyr_baked.indices.chunks_exact(3) {
        tris.push([
            pyr_baked.vertices[tri[0] as usize].position,
            pyr_baked.vertices[tri[1] as usize].position,
            pyr_baked.vertices[tri[2] as usize].position,
        ]);
    }

    let mut world = PhysicsWorld::new();
    let mut pyr_body = RigidBody::from_model(&Model::new("Piramide"), [0.0, 0.0, 0.0]).with_mesh_triangles(tris);
    pyr_body.body_kind = RigidBodyKind::Static;
    pyr_body.inv_mass = 0.0;
    pyr_body.colliders.clear();
    world.add_body(pyr_body);

    let sph_body = RigidBody {
        name: String::from("EsferaCentro"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.0, 1.85, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.55,
        restitution: 0.3,
        linear_damping: 0.02,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.35 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    };
    let sph_handle = world.add_body(sph_body);

    for _ in 0..240 {
        world.step(1.0 / 60.0);
    }
    let final_sph = world.body(sph_handle).unwrap();
    let lateral = (final_sph.position[0] * final_sph.position[0]
        + final_sph.position[2] * final_sph.position[2])
        .sqrt();
    assert!(
        lateral > 0.65,
        "sphere dropped dead-center on pyramid apex should tip off and roll, lateral={}, pos={:?}",
        lateral,
        final_sph.position
    );
    assert!(
        final_sph.position[1] < 0.7,
        "sphere should descend onto ground/lower slope, got y={}",
        final_sph.position[1]
    );
}

#[test]
fn dynamic_sphere_rolls_down_dynamic_pyramid_with_spin() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut pyramid = PartSpec::new("piramide_din", PrimitiveKind::Pyramid);
    pyramid.position = [0.0, 0.5, 0.0];
    pyramid.scale = [1.6, 1.2, 1.6];
    let pyr_baked = pyramid.bake();
    let pivot = [0.0_f32, 0.5, 0.0];
    let mut tris = Vec::new();
    for tri in pyr_baked.indices.chunks_exact(3) {
        let a = pyr_baked.vertices[tri[0] as usize].position;
        let b = pyr_baked.vertices[tri[1] as usize].position;
        let c = pyr_baked.vertices[tri[2] as usize].position;
        tris.push([
            [a[0] - pivot[0], a[1] - pivot[1], a[2] - pivot[2]],
            [b[0] - pivot[0], b[1] - pivot[1], b[2] - pivot[2]],
            [c[0] - pivot[0], c[1] - pivot[1], c[2] - pivot[2]],
        ]);
    }

    let mut world = PhysicsWorld::new();
    let mut pyr_body = RigidBody::from_model(&Model::new("PiramideDin"), pivot);
    pyr_body.colliders.clear();
    pyr_body = pyr_body.with_mesh_triangles(tris);
    pyr_body.body_kind = RigidBodyKind::Dynamic;
    pyr_body.mass = 4.0;
    pyr_body.inv_mass = 0.25;
    pyr_body.friction = 0.7;
    world.add_body(pyr_body);

    let sph_body = RigidBody {
        name: String::from("Esfera"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.18, 1.7, 0.12],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.65,
        restitution: 0.25,
        linear_damping: 0.02,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.3 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    };
    let sph_handle = world.add_body(sph_body);

    let mut max_spin = 0.0_f32;
    for _ in 0..240 {
        world.step(1.0 / 60.0);
        let b = world.body(sph_handle).unwrap();
        let w = (b.angular_velocity[0] * b.angular_velocity[0]
            + b.angular_velocity[1] * b.angular_velocity[1]
            + b.angular_velocity[2] * b.angular_velocity[2])
            .sqrt();
        if w > max_spin {
            max_spin = w;
        }
    }
    let final_sph = world.body(sph_handle).unwrap();
    let lateral = (final_sph.position[0] * final_sph.position[0]
        + final_sph.position[2] * final_sph.position[2])
        .sqrt();
    assert!(
        lateral > 0.75 && final_sph.position[1] < 0.65,
        "dynamic sphere should roll off dynamic pyramid instead of sticking on edge/corner, pos={:?}",
        final_sph.position
    );
    assert!(
        max_spin > 1.0,
        "sphere should acquire rolling angular velocity via contact torque, max_spin={}",
        max_spin
    );
}

#[test]
fn tilted_dynamic_cube_topples_and_settles_on_ground() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let angle = 38.0_f32.to_radians();
    let half = angle * 0.5;
    let mut cube = RigidBody {
        name: String::from("CuboInclinado"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.0, 1.0, 0.0],
        orientation: [0.0, 0.0, half.sin(), half.cos()],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.6,
        restitution: 0.1,
        linear_damping: 0.04,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Box {
                half_extents: [0.5, 0.5, 0.5],
            },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    };
    cube.restitution = 0.05;

    let mut world = PhysicsWorld::new();
    let handle = world.add_body(cube);

    for _ in 0..240 {
        world.step(1.0 / 60.0);
    }

    let b = world.body(handle).unwrap();
    assert!(
        b.position[1] >= 0.45 && b.position[1] <= 0.58,
        "tilted cube should topple flat onto ground near y=0.5, got y={}",
        b.position[1]
    );
    assert!(
        b.on_ground,
        "cube should settle on ground"
    );
}

#[test]
fn sphere_and_cube_descend_rotated_ramp_mesh() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut ramp = PartSpec::new("rampa", PrimitiveKind::Cube);
    ramp.position = [0.0, 1.0, 0.0];
    ramp.rotation_deg = [0.0, 0.0, -25.0];
    ramp.scale = [4.0, 0.25, 2.0];
    let ramp_baked = ramp.bake();
    let pivot = [0.0_f32, 1.0, 0.0];
    let mut tris = Vec::new();
    for tri in ramp_baked.indices.chunks_exact(3) {
        let a = ramp_baked.vertices[tri[0] as usize].position;
        let b = ramp_baked.vertices[tri[1] as usize].position;
        let c = ramp_baked.vertices[tri[2] as usize].position;
        tris.push([
            [a[0] - pivot[0], a[1] - pivot[1], a[2] - pivot[2]],
            [b[0] - pivot[0], b[1] - pivot[1], b[2] - pivot[2]],
            [c[0] - pivot[0], c[1] - pivot[1], c[2] - pivot[2]],
        ]);
    }

    let mut world = PhysicsWorld::new();
    let mut ramp_body = RigidBody::from_model(&Model::new("Rampa"), pivot);
    ramp_body.colliders.clear();
    ramp_body = ramp_body.with_mesh_triangles(tris);
    ramp_body.body_kind = RigidBodyKind::Static;
    ramp_body.inv_mass = 0.0;
    ramp_body.friction = 0.25;
    world.add_body(ramp_body);

    let sph = world.add_body(RigidBody {
        name: String::from("EsferaRampa"),
        body_kind: RigidBodyKind::Dynamic,
        position: [-1.0, 2.2, -0.4],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.45,
        restitution: 0.2,
        linear_damping: 0.01,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.28 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    });

    let bx = world.add_body(RigidBody {
        name: String::from("CuboRampa"),
        body_kind: RigidBodyKind::Dynamic,
        position: [-1.0, 2.2, 0.45],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.15,
        restitution: 0.05,
        linear_damping: 0.01,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Box {
                half_extents: [0.25, 0.25, 0.25],
            },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    });

    for _ in 0..180 {
        world.step(1.0 / 60.0);
    }

    let fs = world.body(sph).unwrap();
    let fb = world.body(bx).unwrap();
    assert!(
        fs.position[0] > 0.3 && fs.position[1] < 1.3,
        "sphere should roll down +X along ramp, got {:?}",
        fs.position
    );
    assert!(
        fb.position[0] > 0.0 && fb.position[1] < 1.5,
        "box should slide down +X along ramp, got {:?}",
        fb.position
    );
}

#[test]
fn sphere_rolls_off_dynamic_pyramid_without_dragging_pyramid_along() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut pyramid = PartSpec::new("piramide", PrimitiveKind::Pyramid);
    pyramid.position = [0.0, 0.5, 0.0];
    pyramid.scale = [1.4, 1.2, 1.4];
    let pyr_baked = pyramid.bake();
    let pivot = [0.0_f32, 0.5, 0.0];
    let mut tris = Vec::new();
    for tri in pyr_baked.indices.chunks_exact(3) {
        let a = pyr_baked.vertices[tri[0] as usize].position;
        let b = pyr_baked.vertices[tri[1] as usize].position;
        let c = pyr_baked.vertices[tri[2] as usize].position;
        tris.push([
            [a[0] - pivot[0], a[1] - pivot[1], a[2] - pivot[2]],
            [b[0] - pivot[0], b[1] - pivot[1], b[2] - pivot[2]],
            [c[0] - pivot[0], c[1] - pivot[1], c[2] - pivot[2]],
        ]);
    }

    let mut world = PhysicsWorld::new();
    let mut pyr_body = RigidBody::from_model(&Model::new("Piramide1kg"), pivot);
    pyr_body.colliders.clear();
    pyr_body = pyr_body.with_mesh_triangles(tris);
    pyr_body.body_kind = RigidBodyKind::Dynamic;
    pyr_body.mass = 1.0;
    pyr_body.inv_mass = 1.0;
    pyr_body.friction = 0.5;
    let pyr_h = world.add_body(pyr_body);

    let sph_body = RigidBody {
        name: String::from("Esfera1kg"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.16, 1.55, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.45,
        restitution: 0.25,
        linear_damping: 0.02,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.5 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    };
    let sph_h = world.add_body(sph_body);

    for _ in 0..200 {
        world.step(1.0 / 60.0);
        let p = world.body(pyr_h).unwrap();
        // Newton's 3rd law: sphere rolling +X down +X slope can only push pyramid -X, NEVER drag it +X!
        assert!(
            p.position[0] <= 0.03,
            "pyramid must never be hooked/dragged +X with the sphere, got pyr.x={}",
            p.position[0]
        );
    }
    let s = world.body(sph_h).unwrap();
    assert!(
        s.position[0] > 0.7 && s.position[1] < 0.7,
        "sphere should cleanly roll down +X slope off pyramid, got {:?}",
        s.position
    );
}

#[test]
fn heavy_sphere_8kg_pushes_light_cube_4kg_much_farther_than_reverse() {
    use forja3d_physics::{RigidBody, WorldCollider};

    fn run_collision(sph_mass: f32, box_mass: f32) -> (f32, f32) {
        let mut world = PhysicsWorld::new();
        let sph = world.add_body(RigidBody {
            name: String::from("Esfera"),
            body_kind: RigidBodyKind::Dynamic,
            position: [-1.5, 0.5, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [6.0, 0.0, 0.0],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass: sph_mass,
            inv_mass: 1.0 / sph_mass,
            friction: 0.2,
            restitution: 0.5,
            linear_damping: 0.02,
            gravity_scale: 1.0,
            colliders: vec![WorldCollider {
                offset: [0.0; 3],
                shape: ColliderShape::Sphere { radius: 0.5 },
            }],
            mesh_triangles: Vec::new(),
            mesh_local_aabb: forja3d::Aabb::EMPTY,
            on_ground: false,
            sleeping: false,
        });
        let bx = world.add_body(RigidBody {
            name: String::from("Cubo"),
            body_kind: RigidBodyKind::Dynamic,
            position: [0.0, 0.5, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass: box_mass,
            inv_mass: 1.0 / box_mass,
            friction: 0.2,
            restitution: 0.5,
            linear_damping: 0.02,
            gravity_scale: 1.0,
            colliders: vec![WorldCollider {
                offset: [0.0; 3],
                shape: ColliderShape::Box {
                    half_extents: [0.5, 0.5, 0.5],
                },
            }],
            mesh_triangles: Vec::new(),
            mesh_local_aabb: forja3d::Aabb::EMPTY,
            on_ground: false,
            sleeping: false,
        });
        for _ in 0..90 {
            world.step(1.0 / 60.0);
        }
        (world.body(sph).unwrap().position[0], world.body(bx).unwrap().position[0])
    }

    let (sph_8_end, box_4_end) = run_collision(8.0, 4.0);
    let (sph_4_end, box_8_end) = run_collision(4.0, 8.0);
    assert!(
        box_4_end > box_8_end + 1.0,
        "8kg sphere should push 4kg cube much farther than 4kg sphere pushes 8kg cube (4kg_end={}, 8kg_end={})",
        box_4_end,
        box_8_end
    );
    assert!(
        sph_8_end > sph_4_end + 0.5,
        "8kg sphere should retain forward momentum while 4kg sphere bounces back more (8kg={}, 4kg={})",
        sph_8_end,
        sph_4_end
    );
}

fn bake_centered_tris(part: &PartSpec) -> ([f32; 3], Vec<[[f32; 3]; 3]>) {
    let baked = part.bake();
    let pivot = if baked.bounds.is_valid() {
        [
            (baked.bounds.min[0] + baked.bounds.max[0]) * 0.5,
            (baked.bounds.min[1] + baked.bounds.max[1]) * 0.5,
            (baked.bounds.min[2] + baked.bounds.max[2]) * 0.5,
        ]
    } else {
        part.position
    };
    let mut tris = Vec::with_capacity(baked.indices.len() / 3);
    for tri in baked.indices.chunks_exact(3) {
        let a = baked.vertices[tri[0] as usize].position;
        let b = baked.vertices[tri[1] as usize].position;
        let c = baked.vertices[tri[2] as usize].position;
        tris.push([
            [a[0] - pivot[0], a[1] - pivot[1], a[2] - pivot[2]],
            [b[0] - pivot[0], b[1] - pivot[1], b[2] - pivot[2]],
            [c[0] - pivot[0], c[1] - pivot[1], c[2] - pivot[2]],
        ]);
    }
    (pivot, tris)
}

#[test]
fn large_analytical_sphere_and_three_small_spheres_never_penetrate_or_teleport() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut world = PhysicsWorld::new();
    let big_r = 5.0_f32;
    let small_r = 0.25_f32;

    let big = world.add_body(RigidBody {
        name: String::from("EsferaGrande"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.0, big_r, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.45,
        restitution: 0.35,
        linear_damping: 0.04,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: big_r },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    });

    let small_starts = [
        [0.4, big_r * 2.0 + 0.6, 0.0],
        [-0.35, big_r * 2.0 + 1.2, 0.3],
        [0.15, big_r * 2.0 + 1.8, -0.35],
    ];
    let mut smalls = Vec::new();
    for (idx, &pos) in small_starts.iter().enumerate() {
        let h = world.add_body(RigidBody {
            name: format!("EsferaPequena_{idx}"),
            body_kind: RigidBodyKind::Dynamic,
            position: pos,
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass: 1.0,
            inv_mass: 1.0,
            friction: 0.45,
            restitution: 0.35,
            linear_damping: 0.04,
            gravity_scale: 1.0,
            colliders: vec![WorldCollider {
                offset: [0.0; 3],
                shape: ColliderShape::Sphere { radius: small_r },
            }],
            mesh_triangles: Vec::new(),
            mesh_local_aabb: forja3d::Aabb::EMPTY,
            on_ground: false,
            sleeping: false,
        });
        smalls.push((h, pos));
    }

    for step in 0..240 {
        world.step(1.0 / 60.0);
        let bp = world.body(big).unwrap().position;
        for (h, prev_pos) in &mut smalls {
            let sp = world.body(*h).unwrap().position;
            let dx = sp[0] - bp[0];
            let dy = sp[1] - bp[1];
            let dz = sp[2] - bp[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                dist >= big_r + small_r - 0.15,
                "step {step}: small sphere entered large sphere (dist={dist}, pos={sp:?}, big={bp:?})"
            );
            let step_jump = ((sp[0] - prev_pos[0]).powi(2)
                + (sp[1] - prev_pos[1]).powi(2)
                + (sp[2] - prev_pos[2]).powi(2))
            .sqrt();
            assert!(
                step_jump < 1.0,
                "step {step}: small sphere teleported (jump={step_jump}, from={prev_pos:?}, to={sp:?})"
            );
            assert!(sp[1] >= 0.12, "step {step}: fell below ground y={}", sp[1]);
            *prev_pos = sp;
        }
    }
}

#[test]
fn large_mesh_sphere_and_three_small_spheres_never_sink_inside_or_teleport() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut big_part = PartSpec::new("esfera_grande_malla", PrimitiveKind::Sphere);
    big_part.position = [0.0, 5.0, 0.0];
    big_part.scale = [10.0, 10.0, 10.0];
    let (pivot, tris) = bake_centered_tris(&big_part);

    let mut world = PhysicsWorld::new();
    let mut big_body = RigidBody::from_model(&Model::new("EsferaGrandeMalla"), pivot);
    big_body.colliders.clear();
    big_body = big_body.with_mesh_triangles(tris);
    big_body.body_kind = RigidBodyKind::Static;
    big_body.inv_mass = 0.0;
    let big = world.add_body(big_body);

    // 2 small spheres drop on top of the large mesh, 1 sits in the outer AABB corner (must NOT hit phantom box!)
    let small_r = 0.25_f32;
    let small_starts = [
        [0.45, 10.7, 0.0],
        [-0.4, 11.4, 0.25],
        [4.2, 8.8, 0.0],
    ];
    let mut smalls = Vec::new();
    for (idx, &pos) in small_starts.iter().enumerate() {
        let h = world.add_body(RigidBody {
            name: format!("Pequena_{idx}"),
            body_kind: RigidBodyKind::Dynamic,
            position: pos,
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass: 1.0,
            inv_mass: 1.0,
            friction: 0.4,
            restitution: 0.3,
            linear_damping: 0.03,
            gravity_scale: 1.0,
            colliders: vec![WorldCollider {
                offset: [0.0; 3],
                shape: ColliderShape::Sphere { radius: small_r },
            }],
            mesh_triangles: Vec::new(),
            mesh_local_aabb: forja3d::Aabb::EMPTY,
            on_ground: false,
            sleeping: false,
        });
        smalls.push((h, pos));
    }

    for step in 0..240 {
        world.step(1.0 / 60.0);
        let bp = world.body(big).unwrap().position;
        for (idx, (h, prev_pos)) in smalls.iter_mut().enumerate() {
            let sp = world.body(*h).unwrap().position;
            let dx = sp[0] - bp[0];
            let dy = sp[1] - bp[1];
            let dz = sp[2] - bp[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                dist >= 4.75,
                "step {step}: small sphere {idx} went inside large mesh sphere (dist={dist}, pos={sp:?})"
            );
            let jump = ((sp[0] - prev_pos[0]).powi(2)
                + (sp[1] - prev_pos[1]).powi(2)
                + (sp[2] - prev_pos[2]).powi(2))
            .sqrt();
            assert!(
                jump < 1.0,
                "step {step}: small sphere {idx} teleported (jump={jump}, pos={sp:?})"
            );
            *prev_pos = sp;
        }
    }
}

#[test]
fn large_mesh_vs_small_mesh_spheres_no_obb_teleport_and_no_tunneling() {
    use forja3d_physics::RigidBody;

    let mut big_part = PartSpec::new("elipsoide_grande", PrimitiveKind::Sphere);
    big_part.position = [0.0, 4.5, 0.0];
    big_part.scale = [10.0, 9.0, 10.0];
    let (big_pivot, big_tris) = bake_centered_tris(&big_part);

    let mut world = PhysicsWorld::new();
    let mut big_body = RigidBody::from_model(&Model::new("ElipsoideGrande"), big_pivot);
    big_body.colliders.clear();
    big_body = big_body.with_mesh_triangles(big_tris);
    big_body.body_kind = RigidBodyKind::Static;
    big_body.inv_mass = 0.0;
    let big = world.add_body(big_body);

    // Corner body sits inside big_body's AABB (5x4.5x5) at [4.2, 8.2, 0.0], yet outside ellipsoid surface!
    let starts = [
        [0.5, 9.6, 0.0],
        [-0.4, 10.2, 0.3],
        [4.2, 8.2, 0.0],
    ];
    let mut smalls = Vec::new();
    for (idx, &pos) in starts.iter().enumerate() {
        let mut p = PartSpec::new(format!("malla_peq_{idx}"), PrimitiveKind::Sphere);
        p.position = pos;
        p.scale = [0.5, 0.5, 0.5];
        let (piv, tris) = bake_centered_tris(&p);
        let mut rb = RigidBody::from_model(&Model::new(format!("MallaPeq_{idx}")), piv);
        rb.colliders.clear();
        rb = rb.with_mesh_triangles(tris);
        rb.body_kind = RigidBodyKind::Dynamic;
        rb.mass = 1.0;
        rb.inv_mass = 1.0;
        let h = world.add_body(rb);
        smalls.push((h, piv));
    }

    for step in 0..180 {
        world.step(1.0 / 60.0);
        let bp = world.body(big).unwrap().position;
        for (idx, (h, prev_pos)) in smalls.iter_mut().enumerate() {
            let sp = world.body(*h).unwrap().position;
            let nx = (sp[0] - bp[0]) / 5.0;
            let ny = (sp[1] - bp[1]) / 4.5;
            let nz = (sp[2] - bp[2]) / 5.0;
            let ellip_norm = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!(
                ellip_norm >= 0.92,
                "step {step}: small mesh {idx} entered inside large ellipsoid (ellip_norm={ellip_norm}, pos={sp:?})"
            );
            let jump = ((sp[0] - prev_pos[0]).powi(2)
                + (sp[1] - prev_pos[1]).powi(2)
                + (sp[2] - prev_pos[2]).powi(2))
            .sqrt();
            assert!(
                jump < 1.0,
                "step {step}: small mesh {idx} teleported (jump={jump}, from={prev_pos:?}, to={sp:?})"
            );
            *prev_pos = sp;
        }
    }
    // Verify the corner mesh fell downward with gravity instead of getting OBB-teleported horizontally away
    let corner_pos = world.body(smalls[2].0).unwrap().position;
    assert!(
        corner_pos[1] < 8.0,
        "corner small mesh outside ellipsoid should fall under gravity, got {:?}",
        corner_pos
    );
}

#[test]
fn small_sphere_deep_inside_large_mesh_sphere_escapes_outward() {
    use forja3d_physics::{RigidBody, WorldCollider};

    let mut big_part = PartSpec::new("esfera_grande", PrimitiveKind::Sphere);
    big_part.position = [0.0, 5.0, 0.0];
    big_part.scale = [10.0, 10.0, 10.0];
    let (pivot, tris) = bake_centered_tris(&big_part);

    let mut world = PhysicsWorld::new();
    let mut big_body = RigidBody::from_model(&Model::new("EsferaGrande"), pivot);
    big_body.colliders.clear();
    big_body = big_body.with_mesh_triangles(tris);
    big_body.body_kind = RigidBodyKind::Static;
    big_body.inv_mass = 0.0;
    world.add_body(big_body);

    // Start 0.6m inside top surface (well beyond the old 0.55*radius backface cutoff)
    let small = world.add_body(RigidBody {
        name: String::from("EsferaAtrapada"),
        body_kind: RigidBodyKind::Dynamic,
        position: [0.1, 9.4, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        force_accum: [0.0; 3],
        mass: 1.0,
        inv_mass: 1.0,
        friction: 0.3,
        restitution: 0.2,
        linear_damping: 0.04,
        gravity_scale: 1.0,
        colliders: vec![WorldCollider {
            offset: [0.0; 3],
            shape: ColliderShape::Sphere { radius: 0.25 },
        }],
        mesh_triangles: Vec::new(),
        mesh_local_aabb: forja3d::Aabb::EMPTY,
        on_ground: false,
        sleeping: false,
    });

    for _ in 0..60 {
        world.step(1.0 / 60.0);
    }
    let sp = world.body(small).unwrap().position;
    let dist = (sp[0] * sp[0] + (sp[1] - 5.0).powi(2) + sp[2] * sp[2]).sqrt();
    assert!(
        dist >= 5.1,
        "sphere trapped inside large mesh should be pushed outside cleanly, got dist={dist}, pos={sp:?}"
    );
}

