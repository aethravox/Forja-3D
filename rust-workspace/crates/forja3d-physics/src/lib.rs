#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use alloc::{string::String, vec, vec::Vec};
use forja3d::{
    math::sqrtf, Aabb, ColliderShape, ColliderSpec, DecodeError, ForjaPack, Model, PhysicsSpec,
    RigidBodyKind, Vec3,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyHandle(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct WorldCollider {
    pub offset: [f32; 3],
    pub shape: ColliderShape,
}

impl From<&ColliderSpec> for WorldCollider {
    fn from(spec: &ColliderSpec) -> Self {
        Self {
            offset: spec.offset,
            shape: spec.shape,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RigidBody {
    pub name: String,
    pub body_kind: RigidBodyKind,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub force_accum: [f32; 3],
    pub mass: f32,
    pub inv_mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub gravity_scale: f32,
    pub colliders: Vec<WorldCollider>,
    pub mesh_triangles: Vec<[[f32; 3]; 3]>,
    pub mesh_local_aabb: Aabb,
    pub on_ground: bool,
    pub sleeping: bool,
}

impl RigidBody {
    pub fn from_model(model: &Model, position: [f32; 3]) -> Self {
        if let Some(phys) = &model.physics {
            Self::from_physics_spec(model.name.clone(), position, phys, model.bounds())
        } else {
            let mut fallback = Vec::new();
            let bounds = model.bounds();
            if bounds.is_valid() {
                let (offset, shape) = ColliderShape::fit_box(bounds);
                fallback.push(WorldCollider { offset, shape });
            } else {
                fallback.push(WorldCollider {
                    offset: [0.0; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.5, 0.5, 0.5],
                    },
                });
            }
            let mass = 1.0_f32;
            Self {
                name: model.name.clone(),
                body_kind: RigidBodyKind::Dynamic,
                position,
                orientation: [0.0, 0.0, 0.0, 1.0],
                velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                force_accum: [0.0; 3],
                mass,
                inv_mass: 1.0 / mass,
                friction: 0.45,
                restitution: 0.3,
                linear_damping: 0.04,
                gravity_scale: 1.0,
                colliders: fallback,
                mesh_triangles: Vec::new(),
                mesh_local_aabb: Aabb::EMPTY,
                on_ground: false,
                sleeping: false,
            }
        }
    }

    pub fn from_physics_spec(
        name: impl Into<String>,
        position: [f32; 3],
        phys: &PhysicsSpec,
        fallback_bounds: Aabb,
    ) -> Self {
        let mut colliders: Vec<WorldCollider> = phys
            .colliders
            .iter()
            .filter(|c| !matches!(c.shape, ColliderShape::None))
            .map(WorldCollider::from)
            .collect();

        if colliders.is_empty() {
            let (offset, shape) = if fallback_bounds.is_valid() {
                ColliderShape::fit_box(fallback_bounds)
            } else {
                ([0.0; 3], ColliderShape::Box { half_extents: [0.5; 3] })
            };
            colliders.push(WorldCollider { offset, shape });
        }

        let mass = phys.mass.max(0.01);
        let inv_mass = match phys.body_kind {
            RigidBodyKind::Dynamic => 1.0 / mass,
            RigidBodyKind::Static | RigidBodyKind::Kinematic | RigidBodyKind::Sensor => 0.0,
        };

        Self {
            name: name.into(),
            body_kind: phys.body_kind,
            position,
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass,
            inv_mass,
            friction: phys.friction.clamp(0.0, 2.0),
            restitution: phys.restitution.clamp(0.0, 1.0),
            linear_damping: phys.linear_damping.clamp(0.0, 10.0),
            gravity_scale: phys.gravity_scale,
            colliders,
            mesh_triangles: Vec::new(),
            mesh_local_aabb: Aabb::EMPTY,
            on_ground: false,
            sleeping: false,
        }
    }

    pub fn with_mesh_triangles(mut self, triangles: Vec<[[f32; 3]; 3]>) -> Self {
        let mut aabb = Aabb::EMPTY;
        for tri in &triangles {
            aabb.include_point(tri[0]);
            aabb.include_point(tri[1]);
            aabb.include_point(tri[2]);
        }
        self.mesh_local_aabb = aabb;
        self.mesh_triangles = triangles;
        self
    }

    pub fn from_pack(pack: &ForjaPack, position: [f32; 3]) -> Option<Self> {
        pack.model.as_ref().map(|m| Self::from_model(m, position))
    }

    pub fn world_aabb(&self) -> Aabb {
        let mut out = Aabb::EMPTY;
        let axes = quat_axes(self.orientation);
        for c in &self.colliders {
            let center = Vec3::new(self.position[0], self.position[1], self.position[2])
                + quat_rotate(self.orientation, Vec3::new(c.offset[0], c.offset[1], c.offset[2]));
            let aabb = match c.shape {
                ColliderShape::None => Aabb::EMPTY,
                ColliderShape::Sphere { radius } => Aabb {
                    min: [center.x - radius, center.y - radius, center.z - radius],
                    max: [center.x + radius, center.y + radius, center.z + radius],
                },
                ColliderShape::Box { half_extents } => obb_world_aabb(center, axes, half_extents),
                ColliderShape::Capsule { radius, half_height } => {
                    let up = axes[1] * half_height;
                    let p0 = center - up;
                    let p1 = center + up;
                    Aabb {
                        min: [
                            p0.x.min(p1.x) - radius,
                            p0.y.min(p1.y) - radius,
                            p0.z.min(p1.z) - radius,
                        ],
                        max: [
                            p0.x.max(p1.x) + radius,
                            p0.y.max(p1.y) + radius,
                            p0.z.max(p1.z) + radius,
                        ],
                    }
                }
            };
            out = out.merge(aabb);
        }
        if self.mesh_local_aabb.is_valid() {
            let lc = Vec3::new(
                (self.mesh_local_aabb.min[0] + self.mesh_local_aabb.max[0]) * 0.5,
                (self.mesh_local_aabb.min[1] + self.mesh_local_aabb.max[1]) * 0.5,
                (self.mesh_local_aabb.min[2] + self.mesh_local_aabb.max[2]) * 0.5,
            );
            let lh = [
                (self.mesh_local_aabb.max[0] - self.mesh_local_aabb.min[0]) * 0.5,
                (self.mesh_local_aabb.max[1] - self.mesh_local_aabb.min[1]) * 0.5,
                (self.mesh_local_aabb.max[2] - self.mesh_local_aabb.min[2]) * 0.5,
            ];
            let wc = Vec3::new(self.position[0], self.position[1], self.position[2])
                + quat_rotate(self.orientation, lc);
            out = out.merge(obb_world_aabb(wc, axes, lh));
        }
        out
    }

    fn local_inv_inertia(&self) -> Vec3 {
        if self.body_kind != RigidBodyKind::Dynamic || self.inv_mass <= 0.0 {
            return Vec3::ZERO;
        }
        let m = self.mass.max(0.01);
        for c in &self.colliders {
            match c.shape {
                ColliderShape::Sphere { radius } => {
                    let r = radius.max(0.05);
                    let inv_i = 1.0 / (0.4 * m * r * r).max(1e-4);
                    return Vec3::new(inv_i, inv_i, inv_i);
                }
                ColliderShape::Box { half_extents } => {
                    let hx = half_extents[0].max(0.05);
                    let hy = half_extents[1].max(0.05);
                    let hz = half_extents[2].max(0.05);
                    let ix = (m * (hy * hy + hz * hz) / 3.0).max(1e-4);
                    let iy = (m * (hx * hx + hz * hz) / 3.0).max(1e-4);
                    let iz = (m * (hx * hx + hy * hy) / 3.0).max(1e-4);
                    return Vec3::new(1.0 / ix, 1.0 / iy, 1.0 / iz);
                }
                ColliderShape::Capsule { radius, half_height } => {
                    let r = radius.max(0.05);
                    let hy = (half_height + r).max(0.05);
                    let ix = (m * (hy * hy + r * r) / 3.0).max(1e-4);
                    let iy = (0.5 * m * r * r).max(1e-4);
                    return Vec3::new(1.0 / ix, 1.0 / iy, 1.0 / ix);
                }
                ColliderShape::None => {}
            }
        }
        if self.mesh_local_aabb.is_valid() {
            let hx = ((self.mesh_local_aabb.max[0] - self.mesh_local_aabb.min[0]) * 0.5).max(0.08);
            let hy = ((self.mesh_local_aabb.max[1] - self.mesh_local_aabb.min[1]) * 0.5).max(0.08);
            let hz = ((self.mesh_local_aabb.max[2] - self.mesh_local_aabb.min[2]) * 0.5).max(0.08);
            let ix = (m * (hy * hy + hz * hz) / 3.0).max(1e-4);
            let iy = (m * (hx * hx + hz * hz) / 3.0).max(1e-4);
            let iz = (m * (hx * hx + hy * hy) / 3.0).max(1e-4);
            return Vec3::new(1.0 / ix, 1.0 / iy, 1.0 / iz);
        }
        let default_inv = 1.0 / (0.4 * m * 0.25).max(1e-4);
        Vec3::new(default_inv, default_inv, default_inv)
    }

    fn apply_world_inv_inertia(&self, torque: Vec3) -> Vec3 {
        let inv_i = self.local_inv_inertia();
        if inv_i.x == 0.0 && inv_i.y == 0.0 && inv_i.z == 0.0 {
            return Vec3::ZERO;
        }
        let local = quat_inv_rotate(self.orientation, torque);
        quat_rotate(
            self.orientation,
            Vec3::new(local.x * inv_i.x, local.y * inv_i.y, local.z * inv_i.z),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionEvent {
    pub body_a: BodyHandle,
    pub body_b: BodyHandle,
    pub normal: [f32; 3],
    pub penetration: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaycastHit {
    pub body: BodyHandle,
    pub distance: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct PhysicsWorld {
    pub gravity: [f32; 3],
    pub ground_y: Option<f32>,
    pub ground_friction: f32,
    pub ground_restitution: f32,
    bodies: Vec<RigidBody>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: [0.0, -9.81, 0.0],
            ground_y: Some(0.0),
            ground_friction: 0.55,
            ground_restitution: 0.25,
            bodies: Vec::new(),
        }
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn_from_model(&mut self, model: &Model, position: [f32; 3]) -> BodyHandle {
        let body = RigidBody::from_model(model, position);
        self.add_body(body)
    }

    pub fn spawn_from_pack(
        &mut self,
        pack: &ForjaPack,
        position: [f32; 3],
    ) -> Option<BodyHandle> {
        let body = RigidBody::from_pack(pack, position)?;
        Some(self.add_body(body))
    }

    pub fn spawn_from_f3d_bytes(
        &mut self,
        bytes: &[u8],
        position: [f32; 3],
    ) -> Result<BodyHandle, DecodeError> {
        let model = Model::from_bytes(bytes)?;
        Ok(self.spawn_from_model(&model, position))
    }

    pub fn spawn_from_f3p_bytes(
        &mut self,
        bytes: &[u8],
        position: [f32; 3],
    ) -> Result<Option<BodyHandle>, DecodeError> {
        let pack = ForjaPack::from_bytes(bytes)?;
        Ok(pack.model.as_ref().map(|m| self.spawn_from_model(m, position)))
    }

    pub fn add_static_box(
        &mut self,
        name: impl Into<String>,
        center: [f32; 3],
        half_extents: [f32; 3],
    ) -> BodyHandle {
        self.add_body(RigidBody {
            name: name.into(),
            body_kind: RigidBodyKind::Static,
            position: center,
            orientation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            force_accum: [0.0; 3],
            mass: 0.0,
            inv_mass: 0.0,
            friction: 0.6,
            restitution: 0.2,
            linear_damping: 0.0,
            gravity_scale: 0.0,
            colliders: vec![WorldCollider {
                offset: [0.0; 3],
                shape: ColliderShape::Box { half_extents },
            }],
            mesh_triangles: Vec::new(),
            mesh_local_aabb: Aabb::EMPTY,
            on_ground: true,
            sleeping: false,
        })
    }

    pub fn add_body(&mut self, body: RigidBody) -> BodyHandle {
        let idx = self.bodies.len();
        self.bodies.push(body);
        BodyHandle(idx)
    }

    pub fn body(&self, handle: BodyHandle) -> Option<&RigidBody> {
        self.bodies.get(handle.0)
    }

    pub fn body_mut(&mut self, handle: BodyHandle) -> Option<&mut RigidBody> {
        self.bodies.get_mut(handle.0)
    }

    pub fn bodies(&self) -> &[RigidBody] {
        &self.bodies
    }

    pub fn clear(&mut self) {
        self.bodies.clear();
    }

    pub fn apply_impulse(&mut self, handle: BodyHandle, impulse: [f32; 3]) {
        if let Some(b) = self.bodies.get_mut(handle.0) {
            if b.body_kind == RigidBodyKind::Dynamic && b.inv_mass > 0.0 {
                b.sleeping = false;
                b.velocity[0] += impulse[0] * b.inv_mass;
                b.velocity[1] += impulse[1] * b.inv_mass;
                b.velocity[2] += impulse[2] * b.inv_mass;
            }
        }
    }

    pub fn apply_force(&mut self, handle: BodyHandle, force: [f32; 3]) {
        if let Some(b) = self.bodies.get_mut(handle.0) {
            if b.body_kind == RigidBodyKind::Dynamic {
                b.sleeping = false;
                b.force_accum[0] += force[0];
                b.force_accum[1] += force[1];
                b.force_accum[2] += force[2];
            }
        }
    }

    pub fn step(&mut self, dt: f32) -> Vec<CollisionEvent> {
        let dt = dt.clamp(0.0, 0.1);
        if dt <= 1e-6 {
            return Vec::new();
        }
        let max_substep = 1.0 / 90.0;
        let mut substeps = ((dt / max_substep) + 0.9999) as usize;

        // Adapt substep count so fast small dynamic bodies cannot tunnel past thin walls or curved meshes
        let mut min_feature = 0.25_f32;
        let mut max_speed = 0.0_f32;
        let grav_mag = sqrtf(
            self.gravity[0] * self.gravity[0]
                + self.gravity[1] * self.gravity[1]
                + self.gravity[2] * self.gravity[2],
        );
        for body in &self.bodies {
            if body.body_kind != RigidBodyKind::Dynamic {
                continue;
            }
            for c in &body.colliders {
                match c.shape {
                    ColliderShape::Sphere { radius } => {
                        min_feature = min_feature.min(radius.max(0.04));
                    }
                    ColliderShape::Box { half_extents } => {
                        let m = half_extents[0]
                            .min(half_extents[1])
                            .min(half_extents[2])
                            .max(0.04);
                        min_feature = min_feature.min(m);
                    }
                    ColliderShape::Capsule { radius, .. } => {
                        min_feature = min_feature.min(radius.max(0.04));
                    }
                    ColliderShape::None => {}
                }
            }
            if body.colliders.is_empty() && body.mesh_local_aabb.is_valid() {
                let hx = (body.mesh_local_aabb.max[0] - body.mesh_local_aabb.min[0]) * 0.5;
                let hy = (body.mesh_local_aabb.max[1] - body.mesh_local_aabb.min[1]) * 0.5;
                let hz = (body.mesh_local_aabb.max[2] - body.mesh_local_aabb.min[2]) * 0.5;
                min_feature = min_feature.min(hx.min(hy).min(hz).max(0.04));
            }
            let v_sq = body.velocity[0] * body.velocity[0]
                + body.velocity[1] * body.velocity[1]
                + body.velocity[2] * body.velocity[2];
            let spd = sqrtf(v_sq) + grav_mag * body.gravity_scale.abs() * dt;
            if spd > max_speed {
                max_speed = spd;
            }
        }
        let max_disp = (min_feature * 0.45).max(0.02);
        if max_speed * dt > max_disp {
            let needed = ((max_speed * dt / max_disp) + 0.9999) as usize;
            if needed > substeps {
                substeps = needed;
            }
        }

        let substeps = substeps.clamp(1, 24);
        let sub_dt = dt / substeps as f32;
        let mut events = Vec::new();
        for s in 0..substeps {
            let sub_events = self.step_substep(sub_dt);
            if s + 1 == substeps {
                events = sub_events;
            }
        }
        events
    }

    fn step_substep(&mut self, dt: f32) -> Vec<CollisionEvent> {
        let mut events = Vec::new();

        // 1. Integrate forces, linear & angular velocities, and positions
        for body in &mut self.bodies {
            body.on_ground = false;
            match body.body_kind {
                RigidBodyKind::Dynamic => {
                    let damp = (1.0 - body.linear_damping * dt).clamp(0.0, 1.0);
                    let ang_damp = (1.0 - 0.12 * dt).clamp(0.0, 1.0);
                    for axis in 0..3 {
                        let accel = self.gravity[axis] * body.gravity_scale
                            + body.force_accum[axis] * body.inv_mass;
                        body.velocity[axis] = (body.velocity[axis] + accel * dt) * damp;
                        body.position[axis] += body.velocity[axis] * dt;
                        body.angular_velocity[axis] *= ang_damp;
                        body.force_accum[axis] = 0.0;
                    }
                    body.orientation = integrate_quat(body.orientation, body.angular_velocity, dt);
                }
                RigidBodyKind::Kinematic => {
                    for axis in 0..3 {
                        body.position[axis] += body.velocity[axis] * dt;
                    }
                    body.orientation = integrate_quat(body.orientation, body.angular_velocity, dt);
                }
                RigidBodyKind::Static | RigidBodyKind::Sensor => {
                    body.force_accum = [0.0; 3];
                }
            }
        }

        // 2. Collect contacts (ground + body-vs-body)
        let mut contacts: Vec<Contact> = Vec::new();
        if let Some(gy) = self.ground_y {
            for (idx, body) in self.bodies.iter().enumerate() {
                if body.body_kind != RigidBodyKind::Dynamic {
                    continue;
                }
                collect_ground_contacts(idx, body, gy, &mut contacts);
            }
        }

        let n = self.bodies.len();
        let mut pair_hits: Vec<Contact> = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                let kind_a = self.bodies[i].body_kind;
                let kind_b = self.bodies[j].body_kind;
                let inv_a = self.bodies[i].inv_mass;
                let inv_b = self.bodies[j].inv_mass;
                if inv_a == 0.0
                    && inv_b == 0.0
                    && kind_a != RigidBodyKind::Sensor
                    && kind_b != RigidBodyKind::Sensor
                {
                    continue;
                }
                if !aabb_overlaps(self.bodies[i].world_aabb(), self.bodies[j].world_aabb()) {
                    continue;
                }
                pair_hits.clear();
                collect_body_pair_contacts(&self.bodies[i], &self.bodies[j], &mut pair_hits);
                if pair_hits.is_empty() {
                    continue;
                }
                let deepest = pair_hits
                    .iter()
                    .max_by(|a, b| a.penetration.total_cmp(&b.penetration))
                    .unwrap();
                events.push(CollisionEvent {
                    body_a: BodyHandle(i),
                    body_b: BodyHandle(j),
                    normal: deepest.normal.to_array(),
                    penetration: deepest.penetration,
                });
                if kind_a == RigidBodyKind::Sensor || kind_b == RigidBodyKind::Sensor {
                    continue;
                }
                for mut c in pair_hits.drain(..) {
                    c.body_a = Some(i);
                    c.body_b = j;
                    contacts.push(c);
                }
            }
        }

        // Mark grounded bodies
        for c in &contacts {
            if c.normal.y > 0.55 {
                if self.bodies[c.body_b].body_kind == RigidBodyKind::Dynamic {
                    self.bodies[c.body_b].on_ground = true;
                }
            } else if c.normal.y < -0.55 {
                if let Some(a_idx) = c.body_a {
                    if self.bodies[a_idx].body_kind == RigidBodyKind::Dynamic {
                        self.bodies[a_idx].on_ground = true;
                    }
                }
            }
        }

        // 3. Sequential impulse solver (velocity + angular torque + contact-point friction)
        let num_contacts = contacts.len();
        let mut acc_jn = vec![0.0_f32; num_contacts];
        let mut acc_jt = vec![Vec3::ZERO; num_contacts];
        let mut init_vn = vec![0.0_f32; num_contacts];
        for (ci, c) in contacts.iter().enumerate() {
            let vb = contact_point_velocity(&self.bodies[c.body_b], c.point);
            let va = match c.body_a {
                Some(ai) => contact_point_velocity(&self.bodies[ai], c.point),
                None => Vec3::ZERO,
            };
            init_vn[ci] = (vb - va).dot(c.normal);
        }

        const VEL_ITERS: usize = 10;
        for _iter in 0..VEL_ITERS {
            for (ci, c) in contacts.iter().enumerate() {
                let b_idx = c.body_b;
                let a_opt = c.body_a;

                let pb = Vec3::new(
                    self.bodies[b_idx].position[0],
                    self.bodies[b_idx].position[1],
                    self.bodies[b_idx].position[2],
                );
                let rb = c.point - pb;
                let inv_mb = self.bodies[b_idx].inv_mass;

                let (pa, ra, inv_ma) = match a_opt {
                    Some(ai) => {
                        let p = Vec3::new(
                            self.bodies[ai].position[0],
                            self.bodies[ai].position[1],
                            self.bodies[ai].position[2],
                        );
                        (p, c.point - p, self.bodies[ai].inv_mass)
                    }
                    None => (Vec3::ZERO, Vec3::ZERO, 0.0),
                };
                let _ = pa;

                if inv_ma + inv_mb <= 1e-8 {
                    continue;
                }

                // Relative velocity at contact point
                let vb = contact_point_velocity(&self.bodies[b_idx], c.point);
                let va = match a_opt {
                    Some(ai) => contact_point_velocity(&self.bodies[ai], c.point),
                    None => Vec3::ZERO,
                };
                let rel_v = vb - va;
                let vn = rel_v.dot(c.normal);

                let (rest_a, fric_a) = match a_opt {
                    Some(ai) => (self.bodies[ai].restitution, self.bodies[ai].friction),
                    None => (self.ground_restitution, self.ground_friction),
                };
                let rest_b = self.bodies[b_idx].restitution;
                let fric_b = self.bodies[b_idx].friction;
                let e = if init_vn[ci] < -0.45 {
                    rest_a.max(rest_b)
                } else {
                    0.0
                };
                let target_vn = if init_vn[ci] < -0.45 {
                    -e * init_vn[ci]
                } else {
                    0.0
                };

                let r_a_cross_n = ra.cross(c.normal);
                let r_b_cross_n = rb.cross(c.normal);
                let ang_a = match a_opt {
                    Some(ai) => self.bodies[ai].apply_world_inv_inertia(r_a_cross_n).cross(ra),
                    None => Vec3::ZERO,
                };
                let ang_b = self.bodies[b_idx].apply_world_inv_inertia(r_b_cross_n).cross(rb);
                let eff_mass_n = inv_ma + inv_mb + (ang_a + ang_b).dot(c.normal);
                if eff_mass_n <= 1e-8 {
                    continue;
                }

                let raw_jn = (target_vn - vn) / eff_mass_n;
                let old_jn = acc_jn[ci];
                let new_jn = (old_jn + raw_jn).max(0.0);
                let d_jn = new_jn - old_jn;
                acc_jn[ci] = new_jn;

                if d_jn.abs() > 1e-7 {
                    let imp = c.normal * d_jn;
                    apply_impulse_at_point(&mut self.bodies, a_opt, b_idx, ra, rb, imp);
                }

                // Recompute contact velocity for tangential friction at contact point
                let vb2 = contact_point_velocity(&self.bodies[b_idx], c.point);
                let va2 = match a_opt {
                    Some(ai) => contact_point_velocity(&self.bodies[ai], c.point),
                    None => Vec3::ZERO,
                };
                let rel_v2 = vb2 - va2;
                let vn2 = rel_v2.dot(c.normal);
                let vt = rel_v2 - c.normal * vn2;
                let vt_len = vt.length();
                if vt_len > 1e-5 && acc_jn[ci] > 0.0 {
                    let t_dir = vt * (1.0 / vt_len);
                    let r_a_cross_t = ra.cross(t_dir);
                    let r_b_cross_t = rb.cross(t_dir);
                    let ang_ta = match a_opt {
                        Some(ai) => self.bodies[ai].apply_world_inv_inertia(r_a_cross_t).cross(ra),
                        None => Vec3::ZERO,
                    };
                    let ang_tb = self.bodies[b_idx].apply_world_inv_inertia(r_b_cross_t).cross(rb);
                    let eff_mass_t = inv_ma + inv_mb + (ang_ta + ang_tb).dot(t_dir);
                    if eff_mass_t > 1e-8 {
                        let mu = sqrtf((fric_a * fric_b).max(0.0));
                        let max_f = acc_jn[ci] * mu;
                        let old_jt = acc_jt[ci];
                        let mut new_jt = old_jt + t_dir * (-vt_len / eff_mass_t);
                        let new_jt_len = new_jt.length();
                        if new_jt_len > max_f && new_jt_len > 1e-8 {
                            new_jt = new_jt * (max_f / new_jt_len);
                        }
                        let delta_jt = new_jt - old_jt;
                        acc_jt[ci] = new_jt;
                        if delta_jt.dot(delta_jt) > 1e-12 {
                            apply_impulse_at_point(&mut self.bodies, a_opt, b_idx, ra, rb, delta_jt);
                        }
                    }
                }
            }
        }

        // Gentle rolling resistance for contacting dynamic bodies so rolling settles naturally
        for c in &contacts {
            let decay = (1.0 - 0.15 * dt).clamp(0.0, 1.0);
            if self.bodies[c.body_b].body_kind == RigidBodyKind::Dynamic {
                for axis in 0..3 {
                    self.bodies[c.body_b].angular_velocity[axis] *= decay;
                }
            }
            if let Some(ai) = c.body_a {
                if self.bodies[ai].body_kind == RigidBodyKind::Dynamic {
                    for axis in 0..3 {
                        self.bodies[ai].angular_velocity[axis] *= decay;
                    }
                }
            }
        }

        // 4. Tracked sequential positional penetration projection (prevents multi-contact stacking & teleports)
        const POS_ITERS: usize = 4;
        let slop = 0.0015_f32;
        let percent = 0.45_f32;
        let mut pos_delta = vec![Vec3::ZERO; n];
        for _ in 0..POS_ITERS {
            for c in &contacts {
                let inv_mb = self.bodies[c.body_b].inv_mass;
                let inv_ma = c.body_a.map_or(0.0, |ai| self.bodies[ai].inv_mass);
                let total_inv = inv_ma + inv_mb;
                if total_inv <= 1e-8 {
                    continue;
                }
                let delta_b = pos_delta[c.body_b];
                let delta_a = c.body_a.map_or(Vec3::ZERO, |ai| pos_delta[ai]);
                let resolved = (delta_b - delta_a).dot(c.normal);
                let remaining_pen = c.penetration - resolved;
                if remaining_pen <= slop {
                    continue;
                }
                let step_dist = ((remaining_pen - slop) * percent).min(0.25);
                let corr = step_dist / total_inv;
                let move_vec = c.normal * corr;
                if let Some(ai) = c.body_a {
                    let da = move_vec * inv_ma;
                    pos_delta[ai] = pos_delta[ai] - da;
                    let arr = da.to_array();
                    for axis in 0..3 {
                        self.bodies[ai].position[axis] -= arr[axis];
                    }
                }
                let db = move_vec * inv_mb;
                pos_delta[c.body_b] = pos_delta[c.body_b] + db;
                let arr = db.to_array();
                for axis in 0..3 {
                    self.bodies[c.body_b].position[axis] += arr[axis];
                }
            }
        }

        events
    }

    pub fn sensor_overlaps(&self, sensor: BodyHandle) -> Vec<BodyHandle> {
        let Some(sb) = self.bodies.get(sensor.0) else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        let mut scratch = Vec::new();
        for (idx, other) in self.bodies.iter().enumerate() {
            if idx == sensor.0 {
                continue;
            }
            if !aabb_overlaps(sb.world_aabb(), other.world_aabb()) {
                continue;
            }
            scratch.clear();
            collect_body_pair_contacts(sb, other, &mut scratch);
            if !scratch.is_empty() {
                hits.push(BodyHandle(idx));
            }
        }
        hits
    }

    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<RaycastHit> {
        let dir_n = Vec3::new(dir[0], dir[1], dir[2]).normalized().to_array();
        let mut best: Option<RaycastHit> = None;
        for (idx, body) in self.bodies.iter().enumerate() {
            let bpos = Vec3::new(body.position[0], body.position[1], body.position[2]);
            if !body.mesh_triangles.is_empty() {
                let local_orig = quat_inv_rotate(
                    body.orientation,
                    Vec3::new(origin[0], origin[1], origin[2]) - bpos,
                )
                .to_array();
                let local_dir = quat_inv_rotate(
                    body.orientation,
                    Vec3::new(dir_n[0], dir_n[1], dir_n[2]),
                )
                .to_array();
                for tri in &body.mesh_triangles {
                    if let Some((dist, n_local)) =
                        ray_triangle(local_orig, local_dir, tri[0], tri[1], tri[2], max_dist)
                    {
                        if best.map_or(true, |prev| dist < prev.distance) {
                            let n_world = quat_rotate(
                                body.orientation,
                                Vec3::new(n_local[0], n_local[1], n_local[2]),
                            )
                            .normalized()
                            .to_array();
                            best = Some(RaycastHit {
                                body: BodyHandle(idx),
                                distance: dist,
                                point: [
                                    origin[0] + dir_n[0] * dist,
                                    origin[1] + dir_n[1] * dist,
                                    origin[2] + dir_n[2] * dist,
                                ],
                                normal: n_world,
                            });
                        }
                    }
                }
                continue;
            }

            for c in &body.colliders {
                let center = bpos
                    + quat_rotate(body.orientation, Vec3::new(c.offset[0], c.offset[1], c.offset[2]));
                let hit = match c.shape {
                    ColliderShape::None => None,
                    ColliderShape::Box { half_extents } => {
                        let local_orig = quat_inv_rotate(
                            body.orientation,
                            Vec3::new(origin[0], origin[1], origin[2]) - center,
                        )
                        .to_array();
                        let local_dir = quat_inv_rotate(
                            body.orientation,
                            Vec3::new(dir_n[0], dir_n[1], dir_n[2]),
                        )
                        .to_array();
                        ray_box(local_orig, local_dir, [0.0; 3], half_extents, max_dist).map(
                            |(dist, n_loc)| {
                                let nw = quat_rotate(
                                    body.orientation,
                                    Vec3::new(n_loc[0], n_loc[1], n_loc[2]),
                                )
                                .normalized()
                                .to_array();
                                (dist, nw)
                            },
                        )
                    }
                    ColliderShape::Sphere { radius } => {
                        ray_sphere(origin, dir_n, center.to_array(), radius, max_dist)
                    }
                    ColliderShape::Capsule { radius, half_height } => {
                        let up = quat_rotate(body.orientation, Vec3::new(0.0, half_height, 0.0));
                        let mut cap_best: Option<(f32, [f32; 3])> = None;
                        for end in [center - up, center, center + up] {
                            if let Some(h) =
                                ray_sphere(origin, dir_n, end.to_array(), radius, max_dist)
                            {
                                if cap_best.map_or(true, |p| h.0 < p.0) {
                                    cap_best = Some(h);
                                }
                            }
                        }
                        cap_best
                    }
                };
                if let Some((dist, normal)) = hit {
                    if best.map_or(true, |prev| dist < prev.distance) {
                        best = Some(RaycastHit {
                            body: BodyHandle(idx),
                            distance: dist,
                            point: [
                                origin[0] + dir_n[0] * dist,
                                origin[1] + dir_n[1] * dist,
                                origin[2] + dir_n[2] * dist,
                            ],
                            normal,
                        });
                    }
                }
            }
        }
        best
    }
}

#[derive(Debug, Clone, Copy)]
struct Contact {
    body_a: Option<usize>,
    body_b: usize,
    point: Vec3,
    normal: Vec3,
    penetration: f32,
}

fn contact_point_velocity(body: &RigidBody, world_point: Vec3) -> Vec3 {
    let p = Vec3::new(body.position[0], body.position[1], body.position[2]);
    let v = Vec3::new(body.velocity[0], body.velocity[1], body.velocity[2]);
    let w = Vec3::new(
        body.angular_velocity[0],
        body.angular_velocity[1],
        body.angular_velocity[2],
    );
    v + w.cross(world_point - p)
}

fn apply_impulse_at_point(
    bodies: &mut [RigidBody],
    a_opt: Option<usize>,
    b_idx: usize,
    ra: Vec3,
    rb: Vec3,
    impulse: Vec3,
) {
    if let Some(ai) = a_opt {
        let inv_ma = bodies[ai].inv_mass;
        if inv_ma > 0.0 {
            let arr = impulse.to_array();
            for axis in 0..3 {
                bodies[ai].velocity[axis] -= arr[axis] * inv_ma;
            }
            let dw = bodies[ai].apply_world_inv_inertia(ra.cross(impulse * -1.0));
            bodies[ai].angular_velocity[0] += dw.x;
            bodies[ai].angular_velocity[1] += dw.y;
            bodies[ai].angular_velocity[2] += dw.z;
        }
    }
    let inv_mb = bodies[b_idx].inv_mass;
    if inv_mb > 0.0 {
        let arr = impulse.to_array();
        for axis in 0..3 {
            bodies[b_idx].velocity[axis] += arr[axis] * inv_mb;
        }
        let dw = bodies[b_idx].apply_world_inv_inertia(rb.cross(impulse));
        bodies[b_idx].angular_velocity[0] += dw.x;
        bodies[b_idx].angular_velocity[1] += dw.y;
        bodies[b_idx].angular_velocity[2] += dw.z;
    }
}

fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let len = sqrtf(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    if len > 1e-6 {
        [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn integrate_quat(q: [f32; 4], w: [f32; 3], dt: f32) -> [f32; 4] {
    let half_dt = 0.5 * dt;
    let dq = [
        half_dt * (w[0] * q[3] + w[1] * q[2] - w[2] * q[1]),
        half_dt * (w[1] * q[3] + w[2] * q[0] - w[0] * q[2]),
        half_dt * (w[2] * q[3] + w[0] * q[1] - w[1] * q[0]),
        half_dt * (-w[0] * q[0] - w[1] * q[1] - w[2] * q[2]),
    ];
    quat_normalize([
        q[0] + dq[0],
        q[1] + dq[1],
        q[2] + dq[2],
        q[3] + dq[3],
    ])
}

fn quat_rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let qv = Vec3::new(q[0], q[1], q[2]);
    let t = qv.cross(v) * 2.0;
    v + t * q[3] + qv.cross(t)
}

fn quat_inv_rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let qv = Vec3::new(-q[0], -q[1], -q[2]);
    let t = qv.cross(v) * 2.0;
    v + t * q[3] + qv.cross(t)
}

fn quat_axes(q: [f32; 4]) -> [Vec3; 3] {
    [
        quat_rotate(q, Vec3::new(1.0, 0.0, 0.0)),
        quat_rotate(q, Vec3::new(0.0, 1.0, 0.0)),
        quat_rotate(q, Vec3::new(0.0, 0.0, 1.0)),
    ]
}

fn obb_world_aabb(center: Vec3, axes: [Vec3; 3], h: [f32; 3]) -> Aabb {
    let rx = axes[0].x.abs() * h[0] + axes[1].x.abs() * h[1] + axes[2].x.abs() * h[2];
    let ry = axes[0].y.abs() * h[0] + axes[1].y.abs() * h[1] + axes[2].y.abs() * h[2];
    let rz = axes[0].z.abs() * h[0] + axes[1].z.abs() * h[1] + axes[2].z.abs() * h[2];
    Aabb {
        min: [center.x - rx, center.y - ry, center.z - rz],
        max: [center.x + rx, center.y + ry, center.z + rz],
    }
}

fn collect_ground_contacts(body_idx: usize, body: &RigidBody, gy: f32, out: &mut Vec<Contact>) {
    let pos = Vec3::new(body.position[0], body.position[1], body.position[2]);
    let up_n = Vec3::new(0.0, 1.0, 0.0);

    if !body.mesh_triangles.is_empty() && body.colliders.is_empty() {
        let waabb = body.world_aabb();
        if !waabb.is_valid() || waabb.min[1] > gy {
            return;
        }
        let mut pts: Vec<(Vec3, f32)> = Vec::new();
        for tri in &body.mesh_triangles {
            for v in tri {
                let w = pos + quat_rotate(body.orientation, Vec3::new(v[0], v[1], v[2]));
                let pen = gy - w.y;
                if pen > 0.0 {
                    let dup = pts.iter().any(|(prev, _)| {
                        let d = *prev - w;
                        d.dot(d) < 1e-4
                    });
                    if !dup {
                        pts.push((w, pen));
                    }
                }
            }
        }
        pts.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (pt, pen) in pts.into_iter().take(8) {
            out.push(Contact {
                body_a: None,
                body_b: body_idx,
                point: Vec3::new(pt.x, gy, pt.z),
                normal: up_n,
                penetration: pen,
            });
        }
        return;
    }

    for c in &body.colliders {
        let cw = pos + quat_rotate(body.orientation, Vec3::new(c.offset[0], c.offset[1], c.offset[2]));
        match c.shape {
            ColliderShape::None => {}
            ColliderShape::Sphere { radius } => {
                let pen = gy - (cw.y - radius);
                if pen > 0.0 {
                    out.push(Contact {
                        body_a: None,
                        body_b: body_idx,
                        point: Vec3::new(cw.x, gy, cw.z),
                        normal: up_n,
                        penetration: pen,
                    });
                }
            }
            ColliderShape::Capsule { radius, half_height } => {
                let up = quat_rotate(body.orientation, Vec3::new(0.0, half_height, 0.0));
                for end in [cw - up, cw + up] {
                    let pen = gy - (end.y - radius);
                    if pen > 0.0 {
                        out.push(Contact {
                            body_a: None,
                            body_b: body_idx,
                            point: Vec3::new(end.x, gy, end.z),
                            normal: up_n,
                            penetration: pen,
                        });
                    }
                }
            }
            ColliderShape::Box { half_extents } => {
                for &sx in &[-half_extents[0], half_extents[0]] {
                    for &sy in &[-half_extents[1], half_extents[1]] {
                        for &sz in &[-half_extents[2], half_extents[2]] {
                            let corner = cw
                                + quat_rotate(body.orientation, Vec3::new(sx, sy, sz));
                            let pen = gy - corner.y;
                            if pen > 0.0 {
                                out.push(Contact {
                                    body_a: None,
                                    body_b: body_idx,
                                    point: Vec3::new(corner.x, gy, corner.z),
                                    normal: up_n,
                                    penetration: pen,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn collect_body_pair_contacts(a: &RigidBody, b: &RigidBody, out: &mut Vec<Contact>) {
    if !a.mesh_triangles.is_empty() && !b.colliders.is_empty() && b.mesh_triangles.is_empty() {
        collect_mesh_vs_body_contacts(a, b, false, out);
        return;
    }
    if !b.mesh_triangles.is_empty() && !a.colliders.is_empty() && a.mesh_triangles.is_empty() {
        collect_mesh_vs_body_contacts(b, a, true, out);
        return;
    }
    if !a.mesh_triangles.is_empty() && !b.mesh_triangles.is_empty() {
        collect_mesh_vs_mesh_contacts(a, b, out);
        return;
    }

    let pa_base = Vec3::new(a.position[0], a.position[1], a.position[2]);
    let pb_base = Vec3::new(b.position[0], b.position[1], b.position[2]);
    for ca in &a.colliders {
        let pa = pa_base + quat_rotate(a.orientation, Vec3::new(ca.offset[0], ca.offset[1], ca.offset[2]));
        for cb in &b.colliders {
            let pb = pb_base
                + quat_rotate(b.orientation, Vec3::new(cb.offset[0], cb.offset[1], cb.offset[2]));
            collect_shape_pair_contacts(pa, a.orientation, ca.shape, pb, b.orientation, cb.shape, out);
        }
    }
}

fn world_aabb_to_local(world_aabb: Aabb, body_pos: Vec3, body_q: [f32; 4], margin: f32) -> Aabb {
    if !world_aabb.is_valid() {
        return Aabb::EMPTY;
    }
    let wc = Vec3::new(
        (world_aabb.min[0] + world_aabb.max[0]) * 0.5,
        (world_aabb.min[1] + world_aabb.max[1]) * 0.5,
        (world_aabb.min[2] + world_aabb.max[2]) * 0.5,
    );
    let h = [
        (world_aabb.max[0] - world_aabb.min[0]) * 0.5 + margin,
        (world_aabb.max[1] - world_aabb.min[1]) * 0.5 + margin,
        (world_aabb.max[2] - world_aabb.min[2]) * 0.5 + margin,
    ];
    let inv_q = [-body_q[0], -body_q[1], -body_q[2], body_q[3]];
    let axes = quat_axes(inv_q);
    let lc = quat_inv_rotate(body_q, wc - body_pos);
    obb_world_aabb(lc, axes, h)
}

fn filter_tris_in_aabb(tris: &[[[f32; 3]; 3]], bounds: Aabb) -> Vec<[[f32; 3]; 3]> {
    if !bounds.is_valid() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &tri in tris {
        let min_x = tri[0][0].min(tri[1][0]).min(tri[2][0]);
        let max_x = tri[0][0].max(tri[1][0]).max(tri[2][0]);
        if max_x < bounds.min[0] || min_x > bounds.max[0] {
            continue;
        }
        let min_y = tri[0][1].min(tri[1][1]).min(tri[2][1]);
        let max_y = tri[0][1].max(tri[1][1]).max(tri[2][1]);
        if max_y < bounds.min[1] || min_y > bounds.max[1] {
            continue;
        }
        let min_z = tri[0][2].min(tri[1][2]).min(tri[2][2]);
        let max_z = tri[0][2].max(tri[1][2]).max(tri[2][2]);
        if max_z < bounds.min[2] || min_z > bounds.max[2] {
            continue;
        }
        out.push(tri);
    }
    out
}

fn aabb_contains_point(aabb: Aabb, p: Vec3, tol: f32) -> bool {
    aabb.is_valid()
        && p.x >= aabb.min[0] - tol
        && p.x <= aabb.max[0] + tol
        && p.y >= aabb.min[1] - tol
        && p.y <= aabb.max[1] + tol
        && p.z >= aabb.min[2] - tol
        && p.z <= aabb.max[2] + tol
}

fn is_point_inside_closed_mesh(tris: &[[[f32; 3]; 3]], aabb: Aabb, p: Vec3) -> bool {
    if tris.len() < 4 || !aabb_contains_point(aabb, p, 1e-4) {
        return false;
    }
    let mut min_signed_d = f32::INFINITY;
    for tri in tris {
        let a = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let b = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let c = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let tri_n = (b - a).cross(c - a);
        if tri_n.dot(tri_n) <= 1e-10 {
            continue;
        }
        let tri_n = tri_n.normalized();
        let closest = closest_point_on_triangle(p, a, b, c);
        let signed_d = (p - closest).dot(tri_n);
        if signed_d < min_signed_d {
            min_signed_d = signed_d;
        }
    }
    if min_signed_d >= 0.0 {
        return false;
    }

    // Single-pass 6 cardinal raycast (+/-X, +/-Y, +/-Z): track nearest hit in each direction.
    // Inside a watertight/closed mesh, >= 5 cardinal rays hit an outward-facing triangle on exit.
    let mut best_t = [f32::INFINITY; 6];
    let mut best_outward = [false; 6];
    let dirs = [
        [1.0_f32, 0.0, 0.0],
        [-1.0_f32, 0.0, 0.0],
        [0.0_f32, 1.0, 0.0],
        [0.0_f32, -1.0, 0.0],
        [0.0_f32, 0.0, 1.0],
        [0.0_f32, 0.0, -1.0],
    ];
    let po = p.to_array();
    for tri in tris {
        let min_x = tri[0][0].min(tri[1][0]).min(tri[2][0]);
        let max_x = tri[0][0].max(tri[1][0]).max(tri[2][0]);
        let min_y = tri[0][1].min(tri[1][1]).min(tri[2][1]);
        let max_y = tri[0][1].max(tri[1][1]).max(tri[2][1]);
        let min_z = tri[0][2].min(tri[1][2]).min(tri[2][2]);
        let max_z = tri[0][2].max(tri[1][2]).max(tri[2][2]);

        let in_yz = po[1] >= min_y - 1e-5 && po[1] <= max_y + 1e-5 && po[2] >= min_z - 1e-5 && po[2] <= max_z + 1e-5;
        let in_xz = po[0] >= min_x - 1e-5 && po[0] <= max_x + 1e-5 && po[2] >= min_z - 1e-5 && po[2] <= max_z + 1e-5;
        let in_xy = po[0] >= min_x - 1e-5 && po[0] <= max_x + 1e-5 && po[1] >= min_y - 1e-5 && po[1] <= max_y + 1e-5;

        if !in_yz && !in_xz && !in_xy {
            continue;
        }
        let check_dir = |axis_mask: bool, dir_idx: usize, best_t: &mut [f32; 6], best_outward: &mut [bool; 6]| {
            if !axis_mask {
                return;
            }
            let d = dirs[dir_idx];
            if let Some((t, n)) = ray_triangle(po, d, tri[0], tri[1], tri[2], 1e6) {
                if t > 1e-5 && t < best_t[dir_idx] {
                    best_t[dir_idx] = t;
                    let dot = d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
                    best_outward[dir_idx] = dot > 0.0;
                }
            }
        };
        check_dir(in_yz && max_x >= po[0], 0, &mut best_t, &mut best_outward);
        check_dir(in_yz && min_x <= po[0], 1, &mut best_t, &mut best_outward);
        check_dir(in_xz && max_y >= po[1], 2, &mut best_t, &mut best_outward);
        check_dir(in_xz && min_y <= po[1], 3, &mut best_t, &mut best_outward);
        check_dir(in_xy && max_z >= po[2], 4, &mut best_t, &mut best_outward);
        check_dir(in_xy && min_z <= po[2], 5, &mut best_t, &mut best_outward);
    }
    let hit_count = best_t.iter().filter(|&&t| t.is_finite()).count();
    let outward_count = best_outward.iter().filter(|&&b| b).count();
    hit_count >= 5 && outward_count >= 4
}

fn closest_interior_escape(tris: &[[[f32; 3]; 3]], p: Vec3) -> Option<(Vec3, Vec3, f32)> {
    let mut best: Option<(Vec3, Vec3, f32)> = None;
    for tri in tris {
        let a = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let b = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let c = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let tri_n = (b - a).cross(c - a);
        if tri_n.dot(tri_n) <= 1e-10 {
            continue;
        }
        let tri_n = tri_n.normalized();
        let closest = closest_point_on_triangle(p, a, b, c);
        let diff = p - closest;
        let signed_d = diff.dot(tri_n);
        if signed_d < 0.0 {
            let escape_dist = -signed_d;
            if best.map_or(true, |(_, _, prev_esc)| escape_dist < prev_esc) {
                best = Some((closest, tri_n, escape_dist));
            }
        }
    }
    best
}

fn mesh_point_penetration(
    local_tris: &[[[f32; 3]; 3]],
    all_tris: &[[[f32; 3]; 3]],
    all_aabb: Aabb,
    p_loc: Vec3,
    max_depth: f32,
) -> Option<(Vec3, Vec3, f32)> {
    if !aabb_contains_point(all_aabb, p_loc, 1e-4) {
        return None;
    }
    let mut best_pos_dist = f32::INFINITY;
    let mut best_neg: Option<(Vec3, Vec3, f32)> = None;

    for tri in local_tris {
        let min_x = tri[0][0].min(tri[1][0]).min(tri[2][0]);
        let max_x = tri[0][0].max(tri[1][0]).max(tri[2][0]);
        if p_loc.x + max_depth < min_x || p_loc.x - max_depth > max_x {
            continue;
        }
        let min_y = tri[0][1].min(tri[1][1]).min(tri[2][1]);
        let max_y = tri[0][1].max(tri[1][1]).max(tri[2][1]);
        if p_loc.y + max_depth < min_y || p_loc.y - max_depth > max_y {
            continue;
        }
        let min_z = tri[0][2].min(tri[1][2]).min(tri[2][2]);
        let max_z = tri[0][2].max(tri[1][2]).max(tri[2][2]);
        if p_loc.z + max_depth < min_z || p_loc.z - max_depth > max_z {
            continue;
        }

        let a = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let b = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let c = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let tri_n = (b - a).cross(c - a);
        if tri_n.dot(tri_n) <= 1e-10 {
            continue;
        }
        let tri_n = tri_n.normalized();
        let closest = closest_point_on_triangle(p_loc, a, b, c);
        let diff = p_loc - closest;
        let dist_sq = diff.dot(diff);
        if dist_sq > max_depth * max_depth {
            continue;
        }
        let dist = sqrtf(dist_sq);
        let signed_d = diff.dot(tri_n);
        if signed_d >= -1e-4 {
            if dist < best_pos_dist {
                best_pos_dist = dist;
            }
        } else if dist <= (-signed_d) + 1e-4 {
            if best_neg.map_or(true, |(_, _, prev_d)| dist < prev_d) {
                best_neg = Some((closest, tri_n, dist));
            }
        }
    }

    if let Some((pt, n, pen)) = best_neg {
        if pen <= best_pos_dist + 1e-4 {
            return Some((pt, n, pen.max(0.001)));
        }
    }

    if is_point_inside_closed_mesh(all_tris, all_aabb, p_loc) {
        if let Some((pt, n, dist)) = closest_interior_escape(all_tris, p_loc) {
            return Some((pt, n, dist.max(0.001)));
        }
    }
    None
}

fn push_contact_dedup(tmp: &mut Vec<Contact>, pt: Vec3, nw: Vec3, pen: f32) {
    if let Some(existing) = tmp.iter_mut().find(|c| {
        let d = c.point - pt;
        d.dot(d) < 1e-4 && c.normal.dot(nw) > 0.9
    }) {
        if pen > existing.penetration {
            *existing = Contact {
                body_a: None,
                body_b: 0,
                point: pt,
                normal: nw,
                penetration: pen,
            };
        }
    } else {
        tmp.push(Contact {
            body_a: None,
            body_b: 0,
            point: pt,
            normal: nw,
            penetration: pen,
        });
    }
}

fn collect_mesh_vs_mesh_contacts(a: &RigidBody, b: &RigidBody, out: &mut Vec<Contact>) {
    let pa = Vec3::new(a.position[0], a.position[1], a.position[2]);
    let pb = Vec3::new(b.position[0], b.position[1], b.position[2]);
    let waabb_a = a.world_aabb();
    let waabb_b = b.world_aabb();
    if !aabb_overlaps(waabb_a, waabb_b) {
        return;
    }

    let b_in_a = world_aabb_to_local(waabb_b, pa, a.orientation, 0.12);
    let a_in_b = world_aabb_to_local(waabb_a, pb, b.orientation, 0.12);
    let tris_a_near = filter_tris_in_aabb(&a.mesh_triangles, b_in_a);
    let tris_b_near = filter_tris_in_aabb(&b.mesh_triangles, a_in_b);
    if tris_a_near.is_empty() || tris_b_near.is_empty() {
        return;
    }

    let a_ext = [
        (a.mesh_local_aabb.max[0] - a.mesh_local_aabb.min[0]).max(0.05),
        (a.mesh_local_aabb.max[1] - a.mesh_local_aabb.min[1]).max(0.05),
        (a.mesh_local_aabb.max[2] - a.mesh_local_aabb.min[2]).max(0.05),
    ];
    let b_ext = [
        (b.mesh_local_aabb.max[0] - b.mesh_local_aabb.min[0]).max(0.05),
        (b.mesh_local_aabb.max[1] - b.mesh_local_aabb.min[1]).max(0.05),
        (b.mesh_local_aabb.max[2] - b.mesh_local_aabb.min[2]).max(0.05),
    ];
    let max_depth_a = (a_ext[0].min(a_ext[1]).min(a_ext[2]) * 0.45).clamp(0.12, 1.5);
    let max_depth_b = (b_ext[0].min(b_ext[1]).min(b_ext[2]) * 0.45).clamp(0.12, 1.5);

    let mut tmp: Vec<Contact> = Vec::new();

    // Sample B vertices + edge midpoints/centers in the overlap region against A's nearby triangles
    let dense_b = tris_b_near.len() <= 64;
    let stride_b = (tris_b_near.len() / 160).max(1);
    for tri in tris_b_near.iter().step_by(stride_b) {
        let v0 = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let v1 = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let v2 = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let samples_small = [v0, v1, v2];
        let samples_dense = [
            v0,
            v1,
            v2,
            (v0 + v1) * 0.5,
            (v1 + v2) * 0.5,
            (v2 + v0) * 0.5,
            (v0 + v1 + v2) * (1.0 / 3.0),
        ];
        let iter_slice: &[Vec3] = if dense_b { &samples_dense } else { &samples_small };
        for &v_loc in iter_slice {
            let pw = pb + quat_rotate(b.orientation, v_loc);
            let p_loc = quat_inv_rotate(a.orientation, pw - pa);
            if let Some((pt_loc, n_loc, pen)) =
                mesh_point_penetration(&tris_a_near, &a.mesh_triangles, a.mesh_local_aabb, p_loc, max_depth_a)
            {
                let pw_hit = pa + quat_rotate(a.orientation, pt_loc);
                let nw = quat_rotate(a.orientation, n_loc).normalized();
                push_contact_dedup(&mut tmp, pw_hit, nw, pen);
            }
        }
    }

    // Sample A vertices + edge midpoints/centers in the overlap region against B's nearby triangles
    let dense_a = tris_a_near.len() <= 64;
    let stride_a = (tris_a_near.len() / 160).max(1);
    for tri in tris_a_near.iter().step_by(stride_a) {
        let v0 = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let v1 = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let v2 = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let samples_small = [v0, v1, v2];
        let samples_dense = [
            v0,
            v1,
            v2,
            (v0 + v1) * 0.5,
            (v1 + v2) * 0.5,
            (v2 + v0) * 0.5,
            (v0 + v1 + v2) * (1.0 / 3.0),
        ];
        let iter_slice: &[Vec3] = if dense_a { &samples_dense } else { &samples_small };
        for &v_loc in iter_slice {
            let pw = pa + quat_rotate(a.orientation, v_loc);
            let p_loc = quat_inv_rotate(b.orientation, pw - pb);
            if let Some((pt_loc, n_loc, pen)) =
                mesh_point_penetration(&tris_b_near, &b.mesh_triangles, b.mesh_local_aabb, p_loc, max_depth_b)
            {
                let pw_hit = pb + quat_rotate(b.orientation, pt_loc);
                let nw = quat_rotate(b.orientation, n_loc).normalized() * -1.0;
                push_contact_dedup(&mut tmp, pw_hit, nw, pen);
            }
        }
    }

    // Also test B's center against A (and A's center against B) in case a small mesh sits completely inside a large mesh
    if tmp.is_empty() && b.mesh_local_aabb.is_valid() {
        let bc_loc = Vec3::new(
            (b.mesh_local_aabb.min[0] + b.mesh_local_aabb.max[0]) * 0.5,
            (b.mesh_local_aabb.min[1] + b.mesh_local_aabb.max[1]) * 0.5,
            (b.mesh_local_aabb.min[2] + b.mesh_local_aabb.max[2]) * 0.5,
        );
        let bc_w = pb + quat_rotate(b.orientation, bc_loc);
        let p_in_a = quat_inv_rotate(a.orientation, bc_w - pa);
        if is_point_inside_closed_mesh(&a.mesh_triangles, a.mesh_local_aabb, p_in_a) {
            if let Some((pt_loc, n_loc, dist)) = closest_interior_escape(&a.mesh_triangles, p_in_a) {
                let b_r = b_ext[0].min(b_ext[1]).min(b_ext[2]) * 0.5;
                let pw_hit = pa + quat_rotate(a.orientation, pt_loc);
                let nw = quat_rotate(a.orientation, n_loc).normalized();
                push_contact_dedup(&mut tmp, pw_hit, nw, dist + b_r);
            }
        }
    }
    if tmp.is_empty() && a.mesh_local_aabb.is_valid() {
        let ac_loc = Vec3::new(
            (a.mesh_local_aabb.min[0] + a.mesh_local_aabb.max[0]) * 0.5,
            (a.mesh_local_aabb.min[1] + a.mesh_local_aabb.max[1]) * 0.5,
            (a.mesh_local_aabb.min[2] + a.mesh_local_aabb.max[2]) * 0.5,
        );
        let ac_w = pa + quat_rotate(a.orientation, ac_loc);
        let p_in_b = quat_inv_rotate(b.orientation, ac_w - pb);
        if is_point_inside_closed_mesh(&b.mesh_triangles, b.mesh_local_aabb, p_in_b) {
            if let Some((pt_loc, n_loc, dist)) = closest_interior_escape(&b.mesh_triangles, p_in_b) {
                let a_r = a_ext[0].min(a_ext[1]).min(a_ext[2]) * 0.5;
                let pw_hit = pb + quat_rotate(b.orientation, pt_loc);
                let nw = quat_rotate(b.orientation, n_loc).normalized() * -1.0;
                push_contact_dedup(&mut tmp, pw_hit, nw, dist + a_r);
            }
        }
    }

    if !tmp.is_empty() {
        tmp.sort_by(|x, y| y.penetration.total_cmp(&x.penetration));
        out.extend(tmp.into_iter().take(6));
    }
}

fn collect_mesh_vs_body_contacts(
    mesh_body: &RigidBody,
    shape_body: &RigidBody,
    flip_normal: bool,
    out: &mut Vec<Contact>,
) {
    let mp = Vec3::new(mesh_body.position[0], mesh_body.position[1], mesh_body.position[2]);
    let sp = Vec3::new(shape_body.position[0], shape_body.position[1], shape_body.position[2]);
    let mq = mesh_body.orientation;
    let sq = shape_body.orientation;
    let sign = if flip_normal { -1.0 } else { 1.0 };

    for c in &shape_body.colliders {
        let cw = sp + quat_rotate(sq, Vec3::new(c.offset[0], c.offset[1], c.offset[2]));
        match c.shape {
            ColliderShape::None => {}
            ColliderShape::Sphere { radius } => {
                let p_loc = quat_inv_rotate(mq, cw - mp);
                let mut local_hits = Vec::new();
                collect_mesh_vs_sphere_contacts_with_aabb(
                    &mesh_body.mesh_triangles,
                    mesh_body.mesh_local_aabb,
                    p_loc,
                    radius,
                    &mut local_hits,
                );
                for (pt_loc, n_loc, pen) in local_hits {
                    out.push(Contact {
                        body_a: None,
                        body_b: 0,
                        point: mp + quat_rotate(mq, pt_loc),
                        normal: quat_rotate(mq, n_loc).normalized() * sign,
                        penetration: pen,
                    });
                }
            }
            ColliderShape::Capsule { radius, half_height } => {
                let up = quat_rotate(sq, Vec3::new(0.0, half_height, 0.0));
                for end in [cw - up, cw, cw + up] {
                    let p_loc = quat_inv_rotate(mq, end - mp);
                    let mut local_hits = Vec::new();
                    collect_mesh_vs_sphere_contacts_with_aabb(
                        &mesh_body.mesh_triangles,
                        mesh_body.mesh_local_aabb,
                        p_loc,
                        radius,
                        &mut local_hits,
                    );
                    for (pt_loc, n_loc, pen) in local_hits {
                        out.push(Contact {
                            body_a: None,
                            body_b: 0,
                            point: mp + quat_rotate(mq, pt_loc),
                            normal: quat_rotate(mq, n_loc).normalized() * sign,
                            penetration: pen,
                        });
                    }
                }
            }
            ColliderShape::Box { half_extents } => {
                let min_h = half_extents[0].min(half_extents[1]).min(half_extents[2]);
                let pr = (min_h * 0.12).clamp(0.015, 0.045);
                let hx = (half_extents[0] - pr).max(0.0);
                let hy = (half_extents[1] - pr).max(0.0);
                let hz = (half_extents[2] - pr).max(0.0);
                let mut probes: Vec<(Vec3, f32)> = Vec::with_capacity(27);
                for &dx in &[-hx, hx] {
                    for &dy in &[-hy, hy] {
                        for &dz in &[-hz, hz] {
                            probes.push((cw + quat_rotate(sq, Vec3::new(dx, dy, dz)), pr));
                        }
                    }
                }
                // 12 edge midpoints
                for &sy in &[-hy, hy] {
                    for &sz in &[-hz, hz] {
                        probes.push((cw + quat_rotate(sq, Vec3::new(0.0, sy, sz)), pr));
                    }
                }
                for &sx in &[-hx, hx] {
                    for &sz in &[-hz, hz] {
                        probes.push((cw + quat_rotate(sq, Vec3::new(sx, 0.0, sz)), pr));
                    }
                }
                for &sx in &[-hx, hx] {
                    for &sy in &[-hy, hy] {
                        probes.push((cw + quat_rotate(sq, Vec3::new(sx, sy, 0.0)), pr));
                    }
                }
                // 6 face centers
                for local in [
                    Vec3::new(-hx, 0.0, 0.0),
                    Vec3::new(hx, 0.0, 0.0),
                    Vec3::new(0.0, -hy, 0.0),
                    Vec3::new(0.0, hy, 0.0),
                    Vec3::new(0.0, 0.0, -hz),
                    Vec3::new(0.0, 0.0, hz),
                ] {
                    probes.push((cw + quat_rotate(sq, local), pr));
                }

                let box_waabb = obb_world_aabb(cw, quat_axes(sq), half_extents);
                let box_in_mesh = world_aabb_to_local(box_waabb, mp, mq, pr + 0.05);
                let near_tris = filter_tris_in_aabb(&mesh_body.mesh_triangles, box_in_mesh);

                let mut box_hits: Vec<Contact> = Vec::new();
                for (pw, rad) in probes {
                    let p_loc = quat_inv_rotate(mq, pw - mp);
                    let mut probe_hits = Vec::new();
                    collect_mesh_vs_sphere_contacts_with_aabb(
                        &near_tris,
                        Aabb::EMPTY,
                        p_loc,
                        rad,
                        &mut probe_hits,
                    );
                    if let Some((pt_loc, n_loc, pen)) = probe_hits
                        .into_iter()
                        .max_by(|a, b| a.2.total_cmp(&b.2))
                    {
                        box_hits.push(Contact {
                            body_a: None,
                            body_b: 0,
                            point: mp + quat_rotate(mq, pt_loc),
                            normal: quat_rotate(mq, n_loc).normalized() * sign,
                            penetration: pen,
                        });
                    } else if let Some((pt_loc, n_loc, depth)) = mesh_point_penetration(
                        &near_tris,
                        &mesh_body.mesh_triangles,
                        mesh_body.mesh_local_aabb,
                        p_loc,
                        min_h.max(0.25),
                    ) {
                        box_hits.push(Contact {
                            body_a: None,
                            body_b: 0,
                            point: mp + quat_rotate(mq, pt_loc),
                            normal: quat_rotate(mq, n_loc).normalized() * sign,
                            penetration: depth + rad,
                        });
                    }
                }

                // Also test mesh vertices inside the OBB (using AABB-filtered triangles instead of blind stride)
                for tri in &near_tris {
                    let a = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
                    let b = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
                    let c = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
                    let tri_n_loc = (b - a).cross(c - a);
                    if tri_n_loc.dot(tri_n_loc) <= 1e-10 {
                        continue;
                    }
                    let tri_nw = quat_rotate(mq, tri_n_loc).normalized();
                    let tri_n_box = quat_inv_rotate(sq, tri_nw);
                    for v in tri {
                        let vw = mp + quat_rotate(mq, Vec3::new(v[0], v[1], v[2]));
                        let vl = quat_inv_rotate(sq, vw - cw);
                        if vl.x.abs() < half_extents[0]
                            && vl.y.abs() < half_extents[1]
                            && vl.z.abs() < half_extents[2]
                        {
                            let px = half_extents[0] - vl.x.abs();
                            let py = half_extents[1] - vl.y.abs();
                            let pz = half_extents[2] - vl.z.abs();
                            let (n_box_out, pen) = if px <= py && px <= pz {
                                let sx = if vl.x >= 0.0 { -1.0 } else { 1.0 };
                                (Vec3::new(sx, 0.0, 0.0), px)
                            } else if py <= pz {
                                let sy = if vl.y >= 0.0 { -1.0 } else { 1.0 };
                                (Vec3::new(0.0, sy, 0.0), py)
                            } else {
                                let sz = if vl.z >= 0.0 { -1.0 } else { 1.0 };
                                (Vec3::new(0.0, 0.0, sz), pz)
                            };
                            if tri_n_box.dot(n_box_out) > 0.1 {
                                let nw = quat_rotate(sq, n_box_out).normalized() * sign;
                                box_hits.push(Contact {
                                    body_a: None,
                                    body_b: 0,
                                    point: vw,
                                    normal: nw,
                                    penetration: pen,
                                });
                            }
                        }
                    }
                }

                box_hits.sort_by(|a, b| b.penetration.total_cmp(&a.penetration));
                out.extend(box_hits.into_iter().take(8));
            }
        }
    }
}

fn collect_mesh_vs_sphere_contacts_with_aabb(
    tris: &[[[f32; 3]; 3]],
    mesh_aabb: Aabb,
    sp: Vec3,
    radius: f32,
    out: &mut Vec<(Vec3, Vec3, f32)>,
) {
    let r_sq = radius * radius;
    let s_min = [sp.x - radius, sp.y - radius, sp.z - radius];
    let s_max = [sp.x + radius, sp.y + radius, sp.z + radius];

    for tri in tris {
        let t_min_x = tri[0][0].min(tri[1][0]).min(tri[2][0]);
        let t_max_x = tri[0][0].max(tri[1][0]).max(tri[2][0]);
        if t_max_x < s_min[0] || t_min_x > s_max[0] {
            continue;
        }
        let t_min_y = tri[0][1].min(tri[1][1]).min(tri[2][1]);
        let t_max_y = tri[0][1].max(tri[1][1]).max(tri[2][1]);
        if t_max_y < s_min[1] || t_min_y > s_max[1] {
            continue;
        }
        let t_min_z = tri[0][2].min(tri[1][2]).min(tri[2][2]);
        let t_max_z = tri[0][2].max(tri[1][2]).max(tri[2][2]);
        if t_max_z < s_min[2] || t_min_z > s_max[2] {
            continue;
        }

        let a = Vec3::new(tri[0][0], tri[0][1], tri[0][2]);
        let b = Vec3::new(tri[1][0], tri[1][1], tri[1][2]);
        let c = Vec3::new(tri[2][0], tri[2][1], tri[2][2]);
        let tri_n = (b - a).cross(c - a);
        if tri_n.dot(tri_n) <= 1e-10 {
            continue;
        }
        let tri_n = tri_n.normalized();
        let closest = closest_point_on_triangle(sp, a, b, c);
        let diff = sp - closest;
        let dist_sq = diff.dot(diff);
        if dist_sq > r_sq {
            continue;
        }
        let signed_d = diff.dot(tri_n);
        if signed_d < -radius * 0.9 {
            continue;
        }

        let dist = sqrtf(dist_sq);
        let (mut normal, pen) = if signed_d < 0.0 {
            // Only accept slight backface tunneling when closest lies strictly inside the face interior;
            // if closest is on a ridge/apex of an adjacent backfacing slope, dist > -signed_d, so skip!
            if dist > (-signed_d) + 1e-4 {
                continue;
            }
            (tri_n, radius + (-signed_d))
        } else {
            let n = if dist > 1e-5 {
                diff * (1.0 / dist)
            } else {
                tri_n
            };
            (n, radius - dist)
        };

        // Break razor-sharp symmetric apex equilibrium on sloped meshes (e.g. pyramid/cone tip)
        if tri_n.y.abs() < 0.96 && normal.y > 0.97 && normal.x.abs() < 0.08 && normal.z.abs() < 0.08 {
            let horiz = Vec3::new(tri_n.x, 0.0, tri_n.z);
            let h_len = horiz.length();
            if h_len > 1e-4 {
                normal = (normal + horiz * (0.22 / h_len)).normalized();
            }
        }
        if let Some(existing) = out.iter_mut().find(|(ec, en, _)| {
            let d = *ec - closest;
            d.dot(d) < 1e-6 || en.dot(normal) > 0.88
        }) {
            if pen > existing.2 {
                *existing = (closest, normal, pen);
            }
        } else if out.len() < 4 {
            out.push((closest, normal, pen));
        } else if let Some((min_idx, min_pen)) = out
            .iter()
            .enumerate()
            .min_by(|a, b| a.1 .2.total_cmp(&b.1 .2))
            .map(|(idx, v)| (idx, v.2))
        {
            if pen > min_pen {
                out[min_idx] = (closest, normal, pen);
            }
        }
    }

    // Tier 2: If no shallow surface contact was found, prevent spheres from staying trapped inside closed solid meshes
    if out.is_empty() && is_point_inside_closed_mesh(tris, mesh_aabb, sp) {
        if let Some((closest, tri_n, dist)) = closest_interior_escape(tris, sp) {
            out.push((closest, tri_n, radius + dist));
        }
    }
}

fn collect_shape_pair_contacts(
    pa: Vec3,
    qa: [f32; 4],
    sa: ColliderShape,
    pb: Vec3,
    qb: [f32; 4],
    sb: ColliderShape,
    out: &mut Vec<Contact>,
) {
    match (sa, sb) {
        (ColliderShape::None, _) | (_, ColliderShape::None) => {}
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            if let Some((pt, n, pen)) = collide_spheres(pa, ra, pb, rb) {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n,
                    penetration: pen,
                });
            }
        }
        (ColliderShape::Sphere { radius }, ColliderShape::Box { half_extents }) => {
            if let Some((pt, n_box_to_sph, pen)) = collide_sphere_obb(pa, radius, pb, qb, half_extents)
            {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n_box_to_sph * -1.0, // A(sphere) -> B(box)
                    penetration: pen,
                });
            }
        }
        (ColliderShape::Box { half_extents }, ColliderShape::Sphere { radius }) => {
            if let Some((pt, n_box_to_sph, pen)) = collide_sphere_obb(pb, radius, pa, qa, half_extents)
            {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n_box_to_sph, // A(box) -> B(sphere)
                    penetration: pen,
                });
            }
        }
        (ColliderShape::Box { half_extents: ha }, ColliderShape::Box { half_extents: hb }) => {
            collide_obbs(pa, qa, ha, pb, qb, hb, out);
        }
        (
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
            ColliderShape::Sphere { radius: rs },
        ) => {
            let up = quat_rotate(qa, Vec3::new(0.0, hh, 0.0));
            let closest = closest_point_on_segment(pb, pa - up, pa + up);
            if let Some((pt, n, pen)) = collide_spheres(closest, r, pb, rs) {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n,
                    penetration: pen,
                });
            }
        }
        (
            ColliderShape::Sphere { radius: rs },
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
        ) => {
            let up = quat_rotate(qb, Vec3::new(0.0, hh, 0.0));
            let closest = closest_point_on_segment(pa, pb - up, pb + up);
            if let Some((pt, n, pen)) = collide_spheres(pa, rs, closest, r) {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n,
                    penetration: pen,
                });
            }
        }
        (
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
            ColliderShape::Box { half_extents: h },
        ) => {
            let up = quat_rotate(qa, Vec3::new(0.0, hh, 0.0));
            let p_mid = closest_point_on_segment(pb, pa - up, pa + up);
            for pt_cap in [pa - up, p_mid, pa + up] {
                if let Some((pt, n_box_to_cap, pen)) = collide_sphere_obb(pt_cap, r, pb, qb, h) {
                    out.push(Contact {
                        body_a: None,
                        body_b: 0,
                        point: pt,
                        normal: n_box_to_cap * -1.0,
                        penetration: pen,
                    });
                }
            }
        }
        (
            ColliderShape::Box { half_extents: h },
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
        ) => {
            let up = quat_rotate(qb, Vec3::new(0.0, hh, 0.0));
            let p_mid = closest_point_on_segment(pa, pb - up, pb + up);
            for pt_cap in [pb - up, p_mid, pb + up] {
                if let Some((pt, n_box_to_cap, pen)) = collide_sphere_obb(pt_cap, r, pa, qa, h) {
                    out.push(Contact {
                        body_a: None,
                        body_b: 0,
                        point: pt,
                        normal: n_box_to_cap,
                        penetration: pen,
                    });
                }
            }
        }
        (
            ColliderShape::Capsule {
                radius: ra,
                half_height: ha,
            },
            ColliderShape::Capsule {
                radius: rb,
                half_height: hb,
            },
        ) => {
            let ua = quat_rotate(qa, Vec3::new(0.0, ha, 0.0));
            let ub = quat_rotate(qb, Vec3::new(0.0, hb, 0.0));
            let (ca, cb) = closest_points_segments(pa - ua, pa + ua, pb - ub, pb + ub);
            if let Some((pt, n, pen)) = collide_spheres(ca, ra, cb, rb) {
                out.push(Contact {
                    body_a: None,
                    body_b: 0,
                    point: pt,
                    normal: n,
                    penetration: pen,
                });
            }
        }
    }
}

fn collide_spheres(pa: Vec3, ra: f32, pb: Vec3, rb: f32) -> Option<(Vec3, Vec3, f32)> {
    let d = pb - pa;
    let dist_sq = d.dot(d);
    let r_sum = ra + rb;
    if dist_sq >= r_sum * r_sum {
        return None;
    }
    let dist = sqrtf(dist_sq);
    let n = if dist > 1e-6 {
        d * (1.0 / dist)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let pen = r_sum - dist;
    let pt = pa + n * (ra - pen * 0.5);
    Some((pt, n, pen))
}

fn collide_sphere_obb(
    ps: Vec3,
    r: f32,
    pb: Vec3,
    qb: [f32; 4],
    hb: [f32; 3],
) -> Option<(Vec3, Vec3, f32)> {
    let local = quat_inv_rotate(qb, ps - pb);
    let closest_local = Vec3::new(
        local.x.clamp(-hb[0], hb[0]),
        local.y.clamp(-hb[1], hb[1]),
        local.z.clamp(-hb[2], hb[2]),
    );
    let diff = local - closest_local;
    let dist_sq = diff.dot(diff);
    if dist_sq > r * r {
        return None;
    }
    let dist = sqrtf(dist_sq);
    let (n_local, pen) = if dist > 1e-6 {
        (diff * (1.0 / dist), r - dist)
    } else {
        let px = hb[0] - local.x.abs();
        let py = hb[1] - local.y.abs();
        let pz = hb[2] - local.z.abs();
        if px <= py && px <= pz {
            let sx = if local.x >= 0.0 { 1.0 } else { -1.0 };
            (Vec3::new(sx, 0.0, 0.0), r + px)
        } else if py <= pz {
            let sy = if local.y >= 0.0 { 1.0 } else { -1.0 };
            (Vec3::new(0.0, sy, 0.0), r + py)
        } else {
            let sz = if local.z >= 0.0 { 1.0 } else { -1.0 };
            (Vec3::new(0.0, 0.0, sz), r + pz)
        }
    };
    let closest_world = pb + quat_rotate(qb, closest_local);
    let normal_world = quat_rotate(qb, n_local).normalized();
    Some((closest_world, normal_world, pen))
}

fn collide_obbs(
    pa: Vec3,
    qa: [f32; 4],
    ha: [f32; 3],
    pb: Vec3,
    qb: [f32; 4],
    hb: [f32; 3],
    out: &mut Vec<Contact>,
) {
    let ua = quat_axes(qa);
    let ub = quat_axes(qb);
    let t = pb - pa;

    let mut abs_r = [[0.0_f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let d = ua[i].dot(ub[j]);
            abs_r[i][j] = d.abs() + 1e-5;
        }
    }

    let mut min_pen = f32::INFINITY;
    let mut best_axis = Vec3::new(0.0, 1.0, 0.0);

    let t_a = [t.dot(ua[0]), t.dot(ua[1]), t.dot(ua[2])];
    // 3 face axes of A
    for i in 0..3 {
        let ra = ha[i];
        let rb = hb[0] * abs_r[i][0] + hb[1] * abs_r[i][1] + hb[2] * abs_r[i][2];
        let dist = t_a[i].abs();
        let pen = ra + rb - dist;
        if pen <= 0.0 {
            return;
        }
        if pen < min_pen {
            min_pen = pen;
            best_axis = if t_a[i] >= 0.0 { ua[i] } else { ua[i] * -1.0 };
        }
    }

    // 3 face axes of B
    let t_b = [t.dot(ub[0]), t.dot(ub[1]), t.dot(ub[2])];
    for j in 0..3 {
        let ra = ha[0] * abs_r[0][j] + ha[1] * abs_r[1][j] + ha[2] * abs_r[2][j];
        let rb = hb[j];
        let dist = t_b[j].abs();
        let pen = ra + rb - dist;
        if pen <= 0.0 {
            return;
        }
        if pen < min_pen {
            min_pen = pen;
            best_axis = if t_b[j] >= 0.0 { ub[j] } else { ub[j] * -1.0 };
        }
    }

    // 9 edge cross axes (with slight bias favoring faces)
    for i in 0..3 {
        for j in 0..3 {
            let axis_raw = ua[i].cross(ub[j]);
            let len_sq = axis_raw.dot(axis_raw);
            if len_sq < 1e-6 {
                continue;
            }
            let len = sqrtf(len_sq);
            let axis = axis_raw * (1.0 / len);
            let ra = ha[0] * ua[0].dot(axis).abs()
                + ha[1] * ua[1].dot(axis).abs()
                + ha[2] * ua[2].dot(axis).abs();
            let rb = hb[0] * ub[0].dot(axis).abs()
                + hb[1] * ub[1].dot(axis).abs()
                + hb[2] * ub[2].dot(axis).abs();
            let proj = t.dot(axis);
            let pen = ra + rb - proj.abs();
            if pen <= 0.0 {
                return;
            }
            if pen * 1.04 < min_pen {
                min_pen = pen;
                best_axis = if proj >= 0.0 { axis } else { axis * -1.0 };
            }
        }
    }

    // Collect penetrating corner contacts (B corners in A, A corners in B)
    let mut contacts: Vec<(Vec3, f32)> = Vec::new();
    let tol = 0.015_f32;
    for &sx in &[-hb[0], hb[0]] {
        for &sy in &[-hb[1], hb[1]] {
            for &sz in &[-hb[2], hb[2]] {
                let cw = pb + quat_rotate(qb, Vec3::new(sx, sy, sz));
                let cl = quat_inv_rotate(qa, cw - pa);
                if cl.x.abs() <= ha[0] + tol && cl.y.abs() <= ha[1] + tol && cl.z.abs() <= ha[2] + tol {
                    let depth = (ha[0] - cl.x.abs())
                        .min(ha[1] - cl.y.abs())
                        .min(ha[2] - cl.z.abs())
                        .max(0.001)
                        .min(min_pen);
                    contacts.push((cw, depth));
                }
            }
        }
    }
    for &sx in &[-ha[0], ha[0]] {
        for &sy in &[-ha[1], ha[1]] {
            for &sz in &[-ha[2], ha[2]] {
                let cw = pa + quat_rotate(qa, Vec3::new(sx, sy, sz));
                let cl = quat_inv_rotate(qb, cw - pb);
                if cl.x.abs() <= hb[0] + tol && cl.y.abs() <= hb[1] + tol && cl.z.abs() <= hb[2] + tol {
                    let depth = (hb[0] - cl.x.abs())
                        .min(hb[1] - cl.y.abs())
                        .min(hb[2] - cl.z.abs())
                        .max(0.001)
                        .min(min_pen);
                    contacts.push((cw, depth));
                }
            }
        }
    }

    if contacts.is_empty() {
        let mid = (pa + pb) * 0.5;
        out.push(Contact {
            body_a: None,
            body_b: 0,
            point: mid,
            normal: best_axis,
            penetration: min_pen,
        });
    } else {
        contacts.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (pt, pen) in contacts.into_iter().take(6) {
            out.push(Contact {
                body_a: None,
                body_b: 0,
                point: pt,
                normal: best_axis,
                penetration: pen.max(min_pen * 0.5),
            });
        }
    }
}

fn closest_point_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let denom = ab.dot(ab);
    if denom <= 1e-8 {
        return a;
    }
    let t = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    a + ab * t
}

fn closest_points_segments(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t) = if a <= 1e-6 && e <= 1e-6 {
        (0.0, 0.0)
    } else if a <= 1e-6 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-6 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom != 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p1 + d1 * s, p2 + d2 * t)
}

fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

fn ray_triangle(
    origin: [f32; 3],
    dir: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    max_dist: f32,
) -> Option<(f32, [f32; 3])> {
    let o = Vec3::new(origin[0], origin[1], origin[2]);
    let d = Vec3::new(dir[0], dir[1], dir[2]);
    let va = Vec3::new(a[0], a[1], a[2]);
    let vb = Vec3::new(b[0], b[1], b[2]);
    let vc = Vec3::new(c[0], c[1], c[2]);
    let e1 = vb - va;
    let e2 = vc - va;
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-6 {
        return None;
    }
    let inv_det = 1.0 / det;
    let s = o - va;
    let u = inv_det * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = inv_det * d.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = inv_det * e2.dot(q);
    if t >= 0.0 && t <= max_dist {
        let n = e1.cross(e2).normalized().to_array();
        Some((t, n))
    } else {
        None
    }
}

fn aabb_overlaps(a: Aabb, b: Aabb) -> bool {
    a.is_valid()
        && b.is_valid()
        && a.max[0] >= b.min[0]
        && a.min[0] <= b.max[0]
        && a.max[1] >= b.min[1]
        && a.min[1] <= b.max[1]
        && a.max[2] >= b.min[2]
        && a.min[2] <= b.max[2]
}

fn ray_box(
    origin: [f32; 3],
    dir: [f32; 3],
    center: [f32; 3],
    half: [f32; 3],
    max_dist: f32,
) -> Option<(f32, [f32; 3])> {
    let min = [center[0] - half[0], center[1] - half[1], center[2] - half[2]];
    let max = [center[0] + half[0], center[1] + half[1], center[2] + half[2]];
    let mut tmin = 0.0_f32;
    let mut tmax = max_dist;
    let mut hit_normal = [0.0, 1.0, 0.0];
    for axis in 0..3 {
        if dir[axis].abs() < 1e-6 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let inv = 1.0 / dir[axis];
            let mut t1 = (min[axis] - origin[axis]) * inv;
            let mut t2 = (max[axis] - origin[axis]) * inv;
            let mut n1 = [0.0; 3];
            n1[axis] = -1.0;
            if t1 > t2 {
                core::mem::swap(&mut t1, &mut t2);
                n1[axis] = 1.0;
            }
            if t1 > tmin {
                tmin = t1;
                hit_normal = n1;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some((tmin, hit_normal))
}

fn ray_sphere(
    origin: [f32; 3],
    dir: [f32; 3],
    center: [f32; 3],
    radius: f32,
    max_dist: f32,
) -> Option<(f32, [f32; 3])> {
    let oc = [origin[0] - center[0], origin[1] - center[1], origin[2] - center[2]];
    let b = oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2];
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - sqrtf(disc);
    if t < 0.0 || t > max_dist {
        return None;
    }
    let p = [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t];
    let n = Vec3::new(p[0] - center[0], p[1] - center[1], p[2] - center[2])
        .normalized()
        .to_array();
    Some((t, n))
}
