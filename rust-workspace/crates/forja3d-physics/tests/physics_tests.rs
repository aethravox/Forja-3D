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
