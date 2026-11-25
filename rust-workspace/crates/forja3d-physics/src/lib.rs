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
        for c in &self.colliders {
            let center = [
                self.position[0] + c.offset[0],
                self.position[1] + c.offset[1],
                self.position[2] + c.offset[2],
            ];
            let aabb = collider_world_aabb(center, c.shape);
            out = out.merge(aabb);
        }
        if self.mesh_local_aabb.is_valid() {
            let mesh_aabb = Aabb {
                min: [
                    self.mesh_local_aabb.min[0] + self.position[0],
                    self.mesh_local_aabb.min[1] + self.position[1],
                    self.mesh_local_aabb.min[2] + self.position[2],
                ],
                max: [
                    self.mesh_local_aabb.max[0] + self.position[0],
                    self.mesh_local_aabb.max[1] + self.position[1],
                    self.mesh_local_aabb.max[2] + self.position[2],
                ],
            };
            out = out.merge(mesh_aabb);
        }
        out
    }

    fn effective_radius(&self) -> f32 {
        for c in &self.colliders {
            match c.shape {
                ColliderShape::Sphere { radius } => return radius.max(0.05),
                ColliderShape::Capsule { radius, .. } => return radius.max(0.05),
                ColliderShape::Box { half_extents } => {
                    return ((half_extents[0] + half_extents[1] + half_extents[2]) / 3.0).max(0.05);
                }
                ColliderShape::None => {}
            }
        }
        0.5
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

    pub fn spawn_from_f3p_bytes(
        &mut self,
        bytes: &[u8],
        position: [f32; 3],
    ) -> Result<Option<BodyHandle>, DecodeError> {
        let pack = ForjaPack::from_bytes(bytes)?;
        Ok(self.spawn_from_pack(&pack, position))
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
        let max_substep = 1.0 / 60.0;
        let substeps = ((dt / max_substep) + 0.9999) as usize;
        let substeps = substeps.clamp(1, 6);
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
        // 1. Integrate forces, linear & angular velocities
        for body in &mut self.bodies {
            body.on_ground = false;
            match body.body_kind {
                RigidBodyKind::Dynamic => {
                    let damp = (1.0 - body.linear_damping * dt).clamp(0.0, 1.0);
                    let ang_damp = (1.0 - 0.6 * dt).clamp(0.0, 1.0);
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

        // 2. Ground plane collision at y = ground_y
        if let Some(gy) = self.ground_y {
            for body in &mut self.bodies {
                if body.body_kind != RigidBodyKind::Dynamic {
                    continue;
                }
                let mut deepest = 0.0_f32;
                for c in &body.colliders {
                    let cy = body.position[1] + c.offset[1];
                    let bottom = collider_bottom_y(cy, c.shape);
                    let pen = gy - bottom;
                    if pen > deepest {
                        deepest = pen;
                    }
                }
                if body.colliders.is_empty() && body.mesh_local_aabb.is_valid() {
                    let pen = gy - (body.position[1] + body.mesh_local_aabb.min[1]);
                    if pen > deepest {
                        deepest = pen;
                    }
                }
                if deepest > 0.0 {
                    body.position[1] += deepest;
                    body.on_ground = true;
                    let vn = body.velocity[1];
                    if vn < 0.0 {
                        let e = if vn.abs() < 0.45 {
                            0.0
                        } else {
                            body.restitution.max(self.ground_restitution)
                        };
                        let j_n = -(1.0 + e) * vn;
                        body.velocity[1] += j_n;
                        // Rolling + Coulomb friction on ground
                        let mu = sqrtf((body.friction * self.ground_friction).max(0.0));
                        let vt_sq = body.velocity[0] * body.velocity[0]
                            + body.velocity[2] * body.velocity[2];
                        let vt = sqrtf(vt_sq);
                        if vt > 1e-5 {
                            let max_fric = j_n * mu;
                            let scale = ((vt - max_fric * 0.25).max(0.0)) / vt;
                            body.velocity[0] *= scale;
                            body.velocity[2] *= scale;
                        }
                    }
                    let r_eff = body.effective_radius();
                    body.angular_velocity[0] = body.velocity[2] / r_eff;
                    body.angular_velocity[2] = -body.velocity[0] / r_eff;
                }
            }
        }

        // 3. Pairwise body-vs-body collision detection & impulse response
        let n = self.bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let kind_a = self.bodies[i].body_kind;
                let kind_b = self.bodies[j].body_kind;
                let inv_a = self.bodies[i].inv_mass;
                let inv_b = self.bodies[j].inv_mass;
                if inv_a == 0.0 && inv_b == 0.0 && kind_a != RigidBodyKind::Sensor && kind_b != RigidBodyKind::Sensor {
                    continue;
                }
                if !aabb_overlaps(self.bodies[i].world_aabb(), self.bodies[j].world_aabb()) {
                    continue;
                }

                let best_hit = collide_bodies(&self.bodies[i], &self.bodies[j]);
                let Some((normal, pen)) = best_hit else {
                    continue;
                };
                events.push(CollisionEvent {
                    body_a: BodyHandle(i),
                    body_b: BodyHandle(j),
                    normal,
                    penetration: pen,
                });
                if kind_a == RigidBodyKind::Sensor || kind_b == RigidBodyKind::Sensor {
                    continue;
                }
                let total_inv = inv_a + inv_b;
                if total_inv <= 1e-8 {
                    continue;
                }

                // Positional separation
                let slop = 0.001;
                let percent = 0.85;
                let corr = ((pen - slop).max(0.0) / total_inv) * percent;
                for axis in 0..3 {
                    self.bodies[i].position[axis] -= normal[axis] * corr * inv_a;
                    self.bodies[j].position[axis] += normal[axis] * corr * inv_b;
                }
                if normal[1] > 0.55 && kind_b == RigidBodyKind::Dynamic {
                    self.bodies[j].on_ground = true;
                }
                if normal[1] < -0.55 && kind_a == RigidBodyKind::Dynamic {
                    self.bodies[i].on_ground = true;
                }

                let rv = [
                    self.bodies[j].velocity[0] - self.bodies[i].velocity[0],
                    self.bodies[j].velocity[1] - self.bodies[i].velocity[1],
                    self.bodies[j].velocity[2] - self.bodies[i].velocity[2],
                ];
                let vel_along_normal = rv[0] * normal[0] + rv[1] * normal[1] + rv[2] * normal[2];
                if vel_along_normal > 0.0 {
                    continue;
                }
                let rest_a = self.bodies[i].restitution;
                let rest_b = self.bodies[j].restitution;
                let e = if vel_along_normal.abs() < 0.35 {
                    0.0
                } else {
                    rest_a.max(rest_b) * 0.85
                };
                let j_impulse = -(1.0 + e) * vel_along_normal / total_inv;
                for axis in 0..3 {
                    let imp = j_impulse * normal[axis];
                    self.bodies[i].velocity[axis] -= imp * inv_a;
                    self.bodies[j].velocity[axis] += imp * inv_b;
                }

                // Tangential friction + rolling spin along contact surface
                let rv2 = [
                    self.bodies[j].velocity[0] - self.bodies[i].velocity[0],
                    self.bodies[j].velocity[1] - self.bodies[i].velocity[1],
                    self.bodies[j].velocity[2] - self.bodies[i].velocity[2],
                ];
                let vn2 = rv2[0] * normal[0] + rv2[1] * normal[1] + rv2[2] * normal[2];
                let tangent = [
                    rv2[0] - vn2 * normal[0],
                    rv2[1] - vn2 * normal[1],
                    rv2[2] - vn2 * normal[2],
                ];
                let t_len = sqrtf(
                    tangent[0] * tangent[0]
                        + tangent[1] * tangent[1]
                        + tangent[2] * tangent[2],
                );
                if t_len > 1e-5 {
                    let t_dir = [tangent[0] / t_len, tangent[1] / t_len, tangent[2] / t_len];
                    let mu = sqrtf((self.bodies[i].friction * self.bodies[j].friction).max(0.0)) * 0.45;
                    let jt_raw = -(rv2[0] * t_dir[0] + rv2[1] * t_dir[1] + rv2[2] * t_dir[2]) / total_inv;
                    let jt = jt_raw.clamp(-j_impulse * mu, j_impulse * mu);
                    for axis in 0..3 {
                        let f_imp = jt * t_dir[axis];
                        self.bodies[i].velocity[axis] -= f_imp * inv_a;
                        self.bodies[j].velocity[axis] += f_imp * inv_b;
                    }
                    // Apply rolling angular velocity w = cross(n, v_t) / r
                    if kind_b == RigidBodyKind::Dynamic {
                        let r = self.bodies[j].effective_radius();
                        let n_v = Vec3::new(normal[0], normal[1], normal[2]);
                        let vt_v = Vec3::new(tangent[0], tangent[1], tangent[2]);
                        let w_roll = n_v.cross(vt_v) * (1.0 / r);
                        self.bodies[j].angular_velocity = [
                            w_roll.x * 0.65 + self.bodies[j].angular_velocity[0] * 0.35,
                            w_roll.y * 0.65 + self.bodies[j].angular_velocity[1] * 0.35,
                            w_roll.z * 0.65 + self.bodies[j].angular_velocity[2] * 0.35,
                        ];
                    }
                    if kind_a == RigidBodyKind::Dynamic {
                        let r = self.bodies[i].effective_radius();
                        let n_v = Vec3::new(-normal[0], -normal[1], -normal[2]);
                        let vt_v = Vec3::new(-tangent[0], -tangent[1], -tangent[2]);
                        let w_roll = n_v.cross(vt_v) * (1.0 / r);
                        self.bodies[i].angular_velocity = [
                            w_roll.x * 0.65 + self.bodies[i].angular_velocity[0] * 0.35,
                            w_roll.y * 0.65 + self.bodies[i].angular_velocity[1] * 0.35,
                            w_roll.z * 0.65 + self.bodies[i].angular_velocity[2] * 0.35,
                        ];
                    }
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
        for (idx, other) in self.bodies.iter().enumerate() {
            if idx == sensor.0 {
                continue;
            }
            if !aabb_overlaps(sb.world_aabb(), other.world_aabb()) {
                continue;
            }
            if collide_bodies(sb, other).is_some() {
                hits.push(BodyHandle(idx));
            }
        }
        hits
    }

    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<RaycastHit> {
        let dir_n = Vec3::new(dir[0], dir[1], dir[2]).normalized().to_array();
        let mut best: Option<RaycastHit> = None;
        for (idx, body) in self.bodies.iter().enumerate() {
            if !body.mesh_triangles.is_empty() {
                for tri in &body.mesh_triangles {
                    let a = [tri[0][0] + body.position[0], tri[0][1] + body.position[1], tri[0][2] + body.position[2]];
                    let b = [tri[1][0] + body.position[0], tri[1][1] + body.position[1], tri[1][2] + body.position[2]];
                    let c = [tri[2][0] + body.position[0], tri[2][1] + body.position[1], tri[2][2] + body.position[2]];
                    if let Some((dist, normal)) = ray_triangle(origin, dir_n, a, b, c, max_dist) {
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
                continue;
            }
            for c in &body.colliders {
                let center = [
                    body.position[0] + c.offset[0],
                    body.position[1] + c.offset[1],
                    body.position[2] + c.offset[2],
                ];
                let hit = match c.shape {
                    ColliderShape::None => None,
                    ColliderShape::Box { half_extents } => {
                        ray_box(origin, dir_n, center, half_extents, max_dist)
                    }
                    ColliderShape::Sphere { radius } => {
                        ray_sphere(origin, dir_n, center, radius, max_dist)
                    }
                    ColliderShape::Capsule { radius, half_height } => {
                        let aabb_h = [radius, half_height + radius, radius];
                        ray_box(origin, dir_n, center, aabb_h, max_dist)
                    }
                };
                if let Some((dist, normal)) = hit {
                    if best.map_or(true, |b| dist < b.distance) {
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

fn integrate_quat(q: [f32; 4], w: [f32; 3], dt: f32) -> [f32; 4] {
    let w_sq = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
    if w_sq < 1e-8 {
        return q;
    }
    let half_dt = 0.5 * dt;
    let dx = (w[0] * q[3] + w[1] * q[2] - w[2] * q[1]) * half_dt;
    let dy = (-w[0] * q[2] + w[1] * q[3] + w[2] * q[0]) * half_dt;
    let dz = (w[0] * q[1] - w[1] * q[0] + w[2] * q[3]) * half_dt;
    let dw = (-w[0] * q[0] - w[1] * q[1] - w[2] * q[2]) * half_dt;
    let nx = q[0] + dx;
    let ny = q[1] + dy;
    let nz = q[2] + dz;
    let nw = q[3] + dw;
    let len = sqrtf(nx * nx + ny * ny + nz * nz + nw * nw).max(1e-6);
    [nx / len, ny / len, nz / len, nw / len]
}

fn collide_bodies(a: &RigidBody, b: &RigidBody) -> Option<([f32; 3], f32)> {
    // Mesh-vs-shape collisions (returns normal pointing from A to B)
    if !a.mesh_triangles.is_empty() && !b.colliders.is_empty() && b.mesh_triangles.is_empty() {
        return collide_mesh_vs_body(a, b);
    }
    if !b.mesh_triangles.is_empty() && !a.colliders.is_empty() && a.mesh_triangles.is_empty() {
        return collide_mesh_vs_body(b, a).map(|(n, p)| ([-n[0], -n[1], -n[2]], p));
    }

    let mut best_hit: Option<([f32; 3], f32)> = None;
    for ca in &a.colliders {
        let pa = [
            a.position[0] + ca.offset[0],
            a.position[1] + ca.offset[1],
            a.position[2] + ca.offset[2],
        ];
        for cb in &b.colliders {
            let pb = [
                b.position[0] + cb.offset[0],
                b.position[1] + cb.offset[1],
                b.position[2] + cb.offset[2],
            ];
            if let Some((normal, pen)) = collide_shapes(pa, ca.shape, pb, cb.shape) {
                if best_hit.map_or(true, |(_, prev_p)| pen > prev_p) {
                    best_hit = Some((normal, pen));
                }
            }
        }
    }
    if best_hit.is_some() {
        return best_hit;
    }

    // Fallback: two mesh-triangle bodies (use bounding shapes / mesh AABBs)
    if !a.mesh_triangles.is_empty() && !b.mesh_triangles.is_empty() {
        let aa = a.world_aabb();
        let ab = b.world_aabb();
        if aa.is_valid() && ab.is_valid() {
            let (ca, sa) = ColliderShape::fit_box(aa);
            let (cb, sb) = ColliderShape::fit_box(ab);
            return collide_shapes(ca, sa, cb, sb);
        }
    }
    None
}

fn collide_mesh_vs_body(mesh_body: &RigidBody, shape_body: &RigidBody) -> Option<([f32; 3], f32)> {
    let mut best: Option<([f32; 3], f32)> = None;
    let mpos = mesh_body.position;
    for c in &shape_body.colliders {
        let pc = [
            shape_body.position[0] + c.offset[0],
            shape_body.position[1] + c.offset[1],
            shape_body.position[2] + c.offset[2],
        ];
        match c.shape {
            ColliderShape::Sphere { radius } => {
                if let Some(hit) = collide_mesh_vs_sphere(&mesh_body.mesh_triangles, mpos, pc, radius) {
                    if best.map_or(true, |(_, p)| hit.1 > p) {
                        best = Some(hit);
                    }
                }
            }
            ColliderShape::Capsule { radius, half_height } => {
                for dy in [-half_height, 0.0, half_height] {
                    let pt = [pc[0], pc[1] + dy, pc[2]];
                    if let Some(hit) = collide_mesh_vs_sphere(&mesh_body.mesh_triangles, mpos, pt, radius) {
                        if best.map_or(true, |(_, p)| hit.1 > p) {
                            best = Some(hit);
                        }
                    }
                }
            }
            ColliderShape::Box { half_extents } => {
                let r = half_extents[0].min(half_extents[1]).min(half_extents[2]).max(0.05);
                let sx = (half_extents[0] - r).max(0.0);
                let sy = (half_extents[1] - r).max(0.0);
                let sz = (half_extents[2] - r).max(0.0);
                for &dx in &[-sx, sx] {
                    for &dy in &[-sy, sy] {
                        for &dz in &[-sz, sz] {
                            let pt = [pc[0] + dx, pc[1] + dy, pc[2] + dz];
                            if let Some(hit) = collide_mesh_vs_sphere(&mesh_body.mesh_triangles, mpos, pt, r) {
                                if best.map_or(true, |(_, p)| hit.1 > p) {
                                    best = Some(hit);
                                }
                            }
                        }
                    }
                }
                if best.is_none() {
                    if let Some(hit) = collide_mesh_vs_sphere(&mesh_body.mesh_triangles, mpos, pc, r) {
                        best = Some(hit);
                    }
                }
            }
            ColliderShape::None => {}
        }
    }
    best
}

fn collide_mesh_vs_sphere(
    tris: &[[[f32; 3]; 3]],
    mesh_pos: [f32; 3],
    sphere_pos: [f32; 3],
    radius: f32,
) -> Option<([f32; 3], f32)> {
    let mut best: Option<([f32; 3], f32)> = None;
    let r_sq = radius * radius;
    let sp = Vec3::new(sphere_pos[0], sphere_pos[1], sphere_pos[2]);

    for tri in tris {
        let a = Vec3::new(tri[0][0] + mesh_pos[0], tri[0][1] + mesh_pos[1], tri[0][2] + mesh_pos[2]);
        let b = Vec3::new(tri[1][0] + mesh_pos[0], tri[1][1] + mesh_pos[1], tri[1][2] + mesh_pos[2]);
        let c = Vec3::new(tri[2][0] + mesh_pos[0], tri[2][1] + mesh_pos[1], tri[2][2] + mesh_pos[2]);

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
        // Ignore contacts deeply behind a backface
        if diff.dot(tri_n) < -radius * 0.45 {
            continue;
        }
        let dist = sqrtf(dist_sq);
        let normal = if dist > 1e-5 {
            (diff * (1.0 / dist)).to_array()
        } else {
            tri_n.to_array()
        };
        let pen = radius - dist;
        if best.map_or(true, |(_, prev_pen)| pen > prev_pen) {
            best = Some((normal, pen));
        }
    }
    best
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

fn collider_world_aabb(center: [f32; 3], shape: ColliderShape) -> Aabb {
    match shape {
        ColliderShape::None => Aabb::EMPTY,
        ColliderShape::Box { half_extents } => Aabb {
            min: [
                center[0] - half_extents[0],
                center[1] - half_extents[1],
                center[2] - half_extents[2],
            ],
            max: [
                center[0] + half_extents[0],
                center[1] + half_extents[1],
                center[2] + half_extents[2],
            ],
        },
        ColliderShape::Sphere { radius } => Aabb {
            min: [center[0] - radius, center[1] - radius, center[2] - radius],
            max: [center[0] + radius, center[1] + radius, center[2] + radius],
        },
        ColliderShape::Capsule { radius, half_height } => Aabb {
            min: [
                center[0] - radius,
                center[1] - half_height - radius,
                center[2] - radius,
            ],
            max: [
                center[0] + radius,
                center[1] + half_height + radius,
                center[2] + radius,
            ],
        },
    }
}

fn collider_bottom_y(center_y: f32, shape: ColliderShape) -> f32 {
    match shape {
        ColliderShape::None => center_y,
        ColliderShape::Box { half_extents } => center_y - half_extents[1],
        ColliderShape::Sphere { radius } => center_y - radius,
        ColliderShape::Capsule { radius, half_height } => center_y - half_height - radius,
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

fn collide_shapes(
    pa: [f32; 3],
    sa: ColliderShape,
    pb: [f32; 3],
    sb: ColliderShape,
) -> Option<([f32; 3], f32)> {
    match (sa, sb) {
        (ColliderShape::None, _) | (_, ColliderShape::None) => None,
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            collide_spheres(pa, ra, pb, rb)
        }
        (ColliderShape::Box { half_extents: ha }, ColliderShape::Box { half_extents: hb }) => {
            collide_boxes(pa, ha, pb, hb)
        }
        (ColliderShape::Sphere { radius }, ColliderShape::Box { half_extents }) => {
            collide_sphere_box(pa, radius, pb, half_extents)
        }
        (ColliderShape::Box { half_extents }, ColliderShape::Sphere { radius }) => {
            collide_sphere_box(pb, radius, pa, half_extents).map(|(n, p)| ([-n[0], -n[1], -n[2]], p))
        }
        (
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
            ColliderShape::Sphere { radius: rs },
        ) => {
            let closest_seg_y = pb[1].clamp(pa[1] - hh, pa[1] + hh);
            collide_spheres([pa[0], closest_seg_y, pa[2]], r, pb, rs)
        }
        (
            ColliderShape::Sphere { radius: rs },
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
        ) => {
            let closest_seg_y = pa[1].clamp(pb[1] - hh, pb[1] + hh);
            collide_spheres(pa, rs, [pb[0], closest_seg_y, pb[2]], r)
        }
        (
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
            ColliderShape::Box { half_extents: h },
        ) => {
            let closest_seg_y = pb[1].clamp(pa[1] - hh, pa[1] + hh);
            collide_sphere_box([pa[0], closest_seg_y, pa[2]], r, pb, h)
        }
        (
            ColliderShape::Box { half_extents: h },
            ColliderShape::Capsule {
                radius: r,
                half_height: hh,
            },
        ) => {
            let closest_seg_y = pa[1].clamp(pb[1] - hh, pb[1] + hh);
            collide_sphere_box([pb[0], closest_seg_y, pb[2]], r, pa, h)
                .map(|(n, p)| ([-n[0], -n[1], -n[2]], p))
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
            let ya = pb[1].clamp(pa[1] - ha, pa[1] + ha);
            let yb = ya.clamp(pb[1] - hb, pb[1] + hb);
            collide_spheres([pa[0], ya, pa[2]], ra, [pb[0], yb, pb[2]], rb)
        }
    }
}

fn collide_spheres(pa: [f32; 3], ra: f32, pb: [f32; 3], rb: f32) -> Option<([f32; 3], f32)> {
    let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
    let dist_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let r_sum = ra + rb;
    if dist_sq >= r_sum * r_sum {
        return None;
    }
    let dist = sqrtf(dist_sq);
    if dist > 1e-6 {
        Some(([d[0] / dist, d[1] / dist, d[2] / dist], r_sum - dist))
    } else {
        Some(([0.0, 1.0, 0.0], r_sum))
    }
}

fn collide_boxes(
    pa: [f32; 3],
    ha: [f32; 3],
    pb: [f32; 3],
    hb: [f32; 3],
) -> Option<([f32; 3], f32)> {
    let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
    let ox = (ha[0] + hb[0]) - d[0].abs();
    let oy = (ha[1] + hb[1]) - d[1].abs();
    let oz = (ha[2] + hb[2]) - d[2].abs();
    if ox <= 0.0 || oy <= 0.0 || oz <= 0.0 {
        return None;
    }
    if ox <= oy && ox <= oz {
        let sx = if d[0] >= 0.0 { 1.0 } else { -1.0 };
        Some(([sx, 0.0, 0.0], ox))
    } else if oy <= oz {
        let sy = if d[1] >= 0.0 { 1.0 } else { -1.0 };
        Some(([0.0, sy, 0.0], oy))
    } else {
        let sz = if d[2] >= 0.0 { 1.0 } else { -1.0 };
        Some(([0.0, 0.0, sz], oz))
    }
}

fn collide_sphere_box(
    ps: [f32; 3],
    r: f32,
    pb: [f32; 3],
    hb: [f32; 3],
) -> Option<([f32; 3], f32)> {
    let closest = [
        ps[0].clamp(pb[0] - hb[0], pb[0] + hb[0]),
        ps[1].clamp(pb[1] - hb[1], pb[1] + hb[1]),
        ps[2].clamp(pb[2] - hb[2], pb[2] + hb[2]),
    ];
    let diff = [closest[0] - ps[0], closest[1] - ps[1], closest[2] - ps[2]];
    let dist_sq = diff[0] * diff[0] + diff[1] * diff[1] + diff[2] * diff[2];
    if dist_sq > r * r {
        return None;
    }
    let dist = sqrtf(dist_sq);
    if dist > 1e-6 {
        Some(([diff[0] / dist, diff[1] / dist, diff[2] / dist], r - dist))
    } else {
        let rel = [ps[0] - pb[0], ps[1] - pb[1], ps[2] - pb[2]];
        let px = hb[0] - rel[0].abs();
        let py = hb[1] - rel[1].abs();
        let pz = hb[2] - rel[2].abs();
        if px <= py && px <= pz {
            let sx = if rel[0] >= 0.0 { -1.0 } else { 1.0 };
            Some(([sx, 0.0, 0.0], r + px))
        } else if py <= pz {
            let sy = if rel[1] >= 0.0 { -1.0 } else { 1.0 };
            Some(([0.0, sy, 0.0], r + py))
        } else {
            let sz = if rel[2] >= 0.0 { -1.0 } else { 1.0 };
            Some(([0.0, 0.0, sz], r + pz))
        }
    }
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
