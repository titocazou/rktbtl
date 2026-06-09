//! The simulation world: one or more rockets, one or more floating landing pads,
//! and a single deterministic `step`. This is the *only* physics implementation
//! in the project — Python (PyO3) and the browser (WASM) both drive this code,
//! so they can never disagree.
//!
//! Coordinate frame: y-up, origin bottom-left, metres. Body angle `th` is the
//! tilt from vertical (0 = nose up). Body→world rotation is the standard
//! R(th) = [[cos,-sin],[sin,cos]], so the body's +y (up) axis maps to
//! world (-sin, cos) — matching the original prototype's thrust direction.

use crate::config::Cfg;
use crate::geom::{seg_seg_closest, Vec2};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Length of the observation vector produced by [`Rocket::observation`].
pub const OBS_DIM: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Status {
    Flying,
    Landed,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DeathCause {
    #[default]
    None,
    Ground,
    Wall,
    Ceiling,
    FootImpact,
    BodySlam,
    RocketHit, // bottom half touched by another rocket
    NonFinite, // numerical blow-up guard
}

/// Per-step control: throttle for each booster in `[0,1]` (fraction of `tmax`).
/// The keyboard/agent "binary" mode just feeds 0.0 / 1.0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Action {
    pub tl: f64,
    pub tr: f64,
}

impl Action {
    #[inline]
    pub const fn new(tl: f64, tr: f64) -> Self {
        Self { tl, tr }
    }
    /// Binary action from two booleans (held / not held).
    #[inline]
    pub fn binary(left: bool, right: bool) -> Self {
        Self {
            tl: if left { 1.0 } else { 0.0 },
            tr: if right { 1.0 } else { 0.0 },
        }
    }
    /// Decode a discrete action id in `0..4`: bit0=left, bit1=right.
    #[inline]
    pub fn from_discrete(a: u8) -> Self {
        Self::binary(a & 1 != 0, a & 2 != 0)
    }
}

/// A floating landing platform.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pad {
    pub cx: f64,
    pub y: f64,
    pub half_w: f64,
    pub thick: f64,
}

impl Pad {
    #[inline]
    pub fn top(&self) -> f64 {
        self.y + self.thick
    }
    #[inline]
    pub fn over(&self, x: f64) -> bool {
        (x - self.cx).abs() <= self.half_w
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rocket {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub th: f64,
    pub om: f64,
    pub fuel: f64,
    pub legs_out: bool,
    pub stable_time: f64,
    pub status: Status,
    pub death_cause: DeathCause,
    /// Index into `World::pads` — the pad this rocket must settle on to win.
    pub target_pad: usize,
}

impl Rocket {
    /// Default spawn pose (mirrors the original prototype's `initState`).
    pub fn spawn(x: f64, y: f64, fuel_max: f64, target_pad: usize) -> Self {
        Rocket {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            th: 0.0,
            om: 0.0,
            fuel: fuel_max,
            legs_out: false,
            stable_time: 0.0,
            status: Status::Flying,
            death_cause: DeathCause::None,
            target_pad,
        }
    }

    #[inline]
    pub fn alive(&self) -> bool {
        self.status == Status::Flying
    }

    #[inline]
    pub fn speed(&self) -> f64 {
        self.vx.hypot(self.vy)
    }

    /// Transform a point given in body frame (bx right, by up) into world space.
    #[inline]
    pub fn to_world(&self, bx: f64, by: f64) -> Vec2 {
        let (s, c) = self.th.sin_cos();
        Vec2::new(self.x + bx * c - by * s, self.y + bx * s + by * c)
    }

    /// Velocity of a point attached to the body at world offset `r` from the CoM.
    #[inline]
    fn point_vel(&self, r: Vec2) -> Vec2 {
        // v_point = v_com + omega × r   (2D cross with scalar omega)
        Vec2::new(self.vx - self.om * r.y, self.vy + self.om * r.x)
    }

    /// Hull as a capsule: bottom (engine end) and top (nose end) centreline pts.
    #[inline]
    pub fn hull_bottom(&self, cfg: &Cfg) -> Vec2 {
        self.to_world(0.0, -cfg.h * 0.5)
    }
    #[inline]
    pub fn hull_top(&self, cfg: &Cfg) -> Vec2 {
        self.to_world(0.0, cfg.h * 0.5)
    }

    /// The two kickstand foot positions in world space.
    ///
    /// Redesigned to match the user's sketch: each leg hangs from a *lower
    /// corner* of the body (not the centreline) and splays outward-and-down.
    /// The renderer calls the same function so visuals and contact never drift.
    pub fn feet(&self, cfg: &Cfg) -> [Vec2; 2] {
        let mut out = [Vec2::default(); 2];
        for (i, sign) in [-1.0f64, 1.0].into_iter().enumerate() {
            // attach at the lower corner
            let cx = sign * cfg.w * 0.5;
            let cy = -cfg.h * 0.5;
            // leg direction in body frame: down (−y) tilted outward by splay
            let (ss, cs) = cfg.leg_splay.sin_cos();
            let fx = cx + cfg.leg_len * sign * ss;
            let fy = cy - cfg.leg_len * cs;
            out[i] = self.to_world(fx, fy);
        }
        out
    }

    /// RL observation vector (normalised-ish). Stable layout — see OBS_DIM.
    pub fn observation(&self, cfg: &Cfg, pad: &Pad) -> [f64; OBS_DIM] {
        let (s, c) = self.th.sin_cos();
        [
            self.x / cfg.world_w,
            self.y / cfg.world_h,
            self.vx / 10.0,
            self.vy / 10.0,
            s,
            c,
            self.om / 5.0,
            self.fuel / cfg.fuel_max,
            (pad.cx - self.x) / cfg.world_w,
            (pad.y - self.y) / cfg.world_h,
            if self.legs_out { 1.0 } else { 0.0 },
            self.stable_time / cfg.stable_need,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub cfg: Cfg,
    pub rockets: Vec<Rocket>,
    pub pads: Vec<Pad>,
    pub time: f64,
    pub steps: u64,
}

impl World {
    /// Single-rocket world: the classic RL task. One rocket up top, one pad.
    pub fn single(cfg: Cfg) -> Self {
        let pad = Pad {
            cx: 16.0,
            y: 7.0,
            half_w: 2.6,
            thick: 0.4,
        };
        let rocket = Rocket::spawn(6.0, 22.0, cfg.fuel_max, 0);
        World {
            cfg,
            rockets: vec![rocket],
            pads: vec![pad],
            time: 0.0,
            steps: 0,
        }
    }

    /// Reset every rocket to its spawn pose without reallocating.
    pub fn reset_single(&mut self) {
        self.time = 0.0;
        self.steps = 0;
        self.rockets[0] = Rocket::spawn(6.0, 22.0, self.cfg.fuel_max, 0);
    }

    /// Advance the whole world one fixed timestep.
    ///
    /// `actions[i]` drives `rockets[i]`. Extra/short slices are tolerated
    /// (missing actions coast). Integration is per-rocket and independent;
    /// rocket–rocket destruction is resolved afterwards so order can't bias it.
    pub fn step(&mut self, actions: &[Action]) {
        let cfg = self.cfg;
        for i in 0..self.rockets.len() {
            let act = actions.get(i).copied().unwrap_or_default();
            integrate(&cfg, &self.pads, &mut self.rockets[i], act);
        }
        resolve_rocket_collisions(&cfg, &mut self.rockets);
        self.time += cfg.dt;
        self.steps += 1;
    }
}

/// Allocation-free single-rocket step against one static pad + world bounds.
///
/// This is the hot path for vectorised RL (`VecSim`): no `Vec`, no collision
/// pass, just the per-rocket numerics. Equivalent to `World::step` on a
/// one-rocket world whose only pad is `pad`.
#[inline]
pub fn step_isolated(cfg: &Cfg, pad: &Pad, r: &mut Rocket, action: Action) {
    // integrate() indexes pads[r.target_pad]; present a 1-element view.
    let saved_target = r.target_pad;
    r.target_pad = 0;
    let pads = core::slice::from_ref(pad);
    integrate(cfg, pads, r, action);
    r.target_pad = saved_target;
}

/// Integrate a single rocket one step against the static environment (its target
/// pad + world bounds). Rocket–rocket contact is handled separately.
fn integrate(cfg: &Cfg, pads: &[Pad], r: &mut Rocket, action: Action) {
    match r.status {
        // A landed rocket has settled on a pad; leave it put.
        Status::Landed => return,
        // A wreck keeps tumbling under gravity instead of freezing: no thrust,
        // no legs, no control, just the linear and angular momentum it died with.
        // It grinds to rest once it reaches the ground.
        Status::Dead => {
            r.vy += cfg.g * cfg.dt;
            r.x += r.vx * cfg.dt;
            r.y += r.vy * cfg.dt;
            r.th += r.om * cfg.dt;
            let floor = cfg.hull_r;
            if r.y <= floor {
                r.y = floor;
                r.vy = 0.0;
                r.vx *= 0.7; // skid to rest
                r.om *= 0.6; // tumble winds down
            }
            r.x = r.x.clamp(cfg.hull_r, cfg.world_w - cfg.hull_r);
            return;
        }
        Status::Flying => {}
    }

    let mut tl = action.tl.clamp(0.0, 1.0);
    let mut tr = action.tr.clamp(0.0, 1.0);

    // Fuel. The original prototype silently zeroed thrust here — which is the
    // "controls stop answering after flying a while" bug the user hit. We keep
    // the behaviour but (a) make it explicit and (b) allow an infinite-fuel
    // toggle for RL/sandbox so a depleted tank never looks like a frozen input.
    if !cfg.infinite_fuel && r.fuel <= 0.0 {
        tl = 0.0;
        tr = 0.0;
        r.fuel = 0.0;
    }

    let big_t = (tl + tr) * cfg.tmax; // total thrust magnitude
    let (s, c) = r.th.sin_cos();
    let fx_thrust = -big_t * s; // up-along-body, rotated into world
    let fy_thrust = big_t * c;
    let tau_thrust = (tr - tl) * cfg.tmax * cfg.d; // differential torque

    // --- landing legs: deploy near the NEAREST pad, RE-STOW when far away. ---
    // Legs come out for any pad you might set down on (your own or the
    // opponent's). Hysteresis band avoids flicker right at the boundary.
    let mut min_pad_dist = f64::INFINITY;
    for p in pads {
        min_pad_dist = min_pad_dist.min((r.x - p.cx).hypot(r.y - p.y));
    }
    if min_pad_dist <= cfg.deploy_r {
        r.legs_out = true;
    } else if min_pad_dist > cfg.deploy_r * cfg.stow_hysteresis {
        r.legs_out = false;
    }

    // --- leg contact: spring-damper feet against the pads. ---
    // Each pad is a THIN FLOATING SLAB, not a solid column: a foot only makes
    // contact when it sits within the slab band [pad.y, pad_top] (i.e. coming
    // down onto the top surface). Below the slab there is no force, so you can
    // fly underneath. Contact is tested against EVERY pad — you can rest on your
    // own pad too — but only feet on the *target* pad count toward the win.
    let mut c_fx = 0.0;
    let mut c_fy = 0.0;
    let mut c_tau = 0.0;
    let mut feet_down_target = 0u8;
    let mut max_foot_impact = 0.0f64;
    if r.legs_out {
        for f in r.feet(cfg) {
            let rel = Vec2::new(f.x - r.x, f.y - r.y);
            let vf = r.point_vel(rel);
            for (pi, p) in pads.iter().enumerate() {
                let pen = p.top() - f.y; // penetration below the top surface
                // thin slab: only the top face is solid (foot still above slab bottom)
                if p.over(f.x) && pen > 0.0 && f.y >= p.y {
                    max_foot_impact = max_foot_impact.max(vf.len());

                    // normal (vertical) spring + damper, clamped non-negative
                    let mut fn_ = cfg.leg_k * pen - cfg.leg_c * vf.y;
                    if fn_ < 0.0 {
                        fn_ = 0.0;
                    }
                    // lateral friction, opposing foot x-velocity, capped at mu*Fn
                    let cap = cfg.leg_mu * fn_;
                    let ff = (-cfg.leg_c * 0.5 * vf.x).clamp(-cap, cap);

                    c_fx += ff;
                    c_fy += fn_;
                    c_tau += rel.x * fn_ - rel.y * ff; // r × F (2D scalar)
                    if pi == r.target_pad {
                        feet_down_target += 1;
                    }
                }
            }
        }
    }

    // --- integrate (semi-implicit Euler) ---
    let inv_m = 1.0 / cfg.m;
    let inv_i = 1.0 / cfg.inertia();
    let ax = (fx_thrust + c_fx) * inv_m;
    let ay = (fy_thrust + c_fy) * inv_m + cfg.g;
    let al = (tau_thrust + c_tau) * inv_i;

    r.vx += ax * cfg.dt;
    r.vy += ay * cfg.dt;
    r.om += al * cfg.dt;
    r.x += r.vx * cfg.dt;
    r.y += r.vy * cfg.dt;
    r.th += r.om * cfg.dt;

    // fuel burn
    if !cfg.infinite_fuel {
        r.fuel -= (tl + tr) * cfg.fuel_rate * cfg.dt;
        if r.fuel < 0.0 {
            r.fuel = 0.0;
        }
    }

    // --- numerical guard: stiff contact + tiny inertia can blow up. Rather than
    // let the state go NaN (which "freezes" the rocket and looks like dead
    // controls), terminate cleanly. ---
    if !(r.x.is_finite() && r.y.is_finite() && r.vx.is_finite() && r.vy.is_finite()
        && r.th.is_finite() && r.om.is_finite())
    {
        die(r, DeathCause::NonFinite);
        return;
    }

    // --- termination: hard foot impact ---
    if max_foot_impact > cfg.v_explode {
        die(r, DeathCause::FootImpact);
        return;
    }

    // --- environment collision, with the half rule. ---
    // The BOTTOM (engine) end touching the environment destroys the rocket; the
    // TOP (nose) end is indestructible and merely gets clamped back in-bounds.
    let bottom = r.hull_bottom(cfg);
    if bottom.x < 0.0 || bottom.x > cfg.world_w || bottom.y < 0.0 || bottom.y > cfg.world_h {
        let cause = if bottom.y < 0.0 {
            DeathCause::Ground
        } else if bottom.y > cfg.world_h {
            DeathCause::Ceiling
        } else {
            DeathCause::Wall
        };
        die(r, cause);
        return;
    }
    // top pokes out → clamp CoM, kill the offending velocity component, survive.
    let top = r.hull_top(cfg);
    if top.x < 0.0 {
        r.x -= top.x;
        r.vx = r.vx.max(0.0);
    } else if top.x > cfg.world_w {
        r.x -= top.x - cfg.world_w;
        r.vx = r.vx.min(0.0);
    }
    if top.y > cfg.world_h {
        r.y -= top.y - cfg.world_h;
        r.vy = r.vy.min(0.0);
    }

    // body (not feet) slamming into a pad's top face at speed → explode. Only
    // the slab band counts, so flying *under* a pad at speed is fine.
    for p in pads {
        if p.over(bottom.x) && bottom.y <= p.top() && bottom.y >= p.y && r.speed() > cfg.v_explode {
            die(r, DeathCause::BodySlam);
            return;
        }
    }

    // --- stable-landing win timer (only the target pad wins) ---
    let stable = feet_down_target >= 2
        && r.speed() <= cfg.v_stable
        && r.th.abs() <= cfg.ang_stable
        && r.om.abs() <= cfg.om_stable;
    if stable {
        r.stable_time += cfg.dt;
        if r.stable_time >= cfg.stable_need {
            r.status = Status::Landed;
        }
    } else {
        r.stable_time = 0.0;
    }
}

#[inline]
fn die(r: &mut Rocket, cause: DeathCause) {
    r.status = Status::Dead;
    r.death_cause = cause;
}

/// Resolve rocket-vs-rocket contact. Each rocket is a solid capsule (hull
/// centreline + `hull_r`):
///
///   * **Solid everywhere** — a rigid-body impulse (with restitution
///     `rocket_restitution`) plus positional correction stops them from passing
///     through each other, including the indestructible gold nose.
///   * **Destruction is a separate, gated event** — a rocket is destroyed only
///     if the contact point on its OWN body is on its bottom (engine) half AND
///     the normal impact speed meets `rocket_crush_speed`. A gentle tap won't
///     kill; a fast ram into the engines will. The top half is never destroyed.
fn resolve_rocket_collisions(cfg: &Cfg, rockets: &mut [Rocket]) {
    let n = rockets.len();
    if n < 2 {
        return;
    }
    let contact = 2.0 * cfg.hull_r;
    let contact_sq = contact * contact;
    let inv_m = 1.0 / cfg.m;
    let inv_i = 1.0 / cfg.inertia();

    for i in 0..n {
        for j in (i + 1)..n {
            if !rockets[i].alive() || !rockets[j].alive() {
                continue;
            }

            // --- geometry & kinematics from immutable borrows ---
            let (ai0, ai1) = (rockets[i].hull_bottom(cfg), rockets[i].hull_top(cfg));
            let (bj0, bj1) = (rockets[j].hull_bottom(cfg), rockets[j].hull_top(cfg));
            let (d_sq, cp_i) = seg_seg_closest(ai0, ai1, bj0, bj1);
            if d_sq > contact_sq {
                continue;
            }
            let (_, cp_j) = seg_seg_closest(bj0, bj1, ai0, ai1);

            // contact normal: from j's surface toward i. Fall back to the
            // centre-to-centre direction if the centrelines coincide.
            let mut nrm = cp_i.sub(cp_j);
            if nrm.len_sq() < 1e-12 {
                nrm = Vec2::new(rockets[i].x - rockets[j].x, rockets[i].y - rockets[j].y);
            }
            let nlen = nrm.len();
            if nlen < 1e-9 {
                continue; // perfectly coincident; skip this pathological step
            }
            let nrm = nrm.scale(1.0 / nlen);

            // contact point (midway between the closest points) and the lever
            // arms from each CoM to it.
            let cp = cp_i.add(cp_j).scale(0.5);
            let ra = Vec2::new(cp.x - rockets[i].x, cp.y - rockets[i].y);
            let rb = Vec2::new(cp.x - rockets[j].x, cp.y - rockets[j].y);

            // relative velocity at the contact point, projected on the normal
            let vi = rockets[i].point_vel(ra);
            let vj = rockets[j].point_vel(rb);
            let vrel_n = vi.sub(vj).dot(nrm);
            let impact = (-vrel_n).max(0.0); // closing speed (>0 if approaching)

            // which halves were involved (for the destruction gate)
            let i_bottom = in_bottom_half(cp_i, ai0, ai1);
            let j_bottom = in_bottom_half(cp_j, bj0, bj1);
            let hard = impact >= cfg.rocket_crush_speed;

            // 2D cross products r × n (scalar)
            let ran = ra.x * nrm.y - ra.y * nrm.x;
            let rbn = rb.x * nrm.y - rb.y * nrm.x;

            // --- normal impulse (only while approaching) ---
            if vrel_n < 0.0 {
                let denom = 2.0 * inv_m + ran * ran * inv_i + rbn * rbn * inv_i;
                let jimp = -(1.0 + cfg.rocket_restitution) * vrel_n / denom;
                let (ix, iy) = (jimp * nrm.x, jimp * nrm.y);
                let (a, b) = mut_pair(rockets, i, j);
                a.vx += ix * inv_m;
                a.vy += iy * inv_m;
                a.om += ran * jimp * inv_i;
                b.vx -= ix * inv_m;
                b.vy -= iy * inv_m;
                b.om -= rbn * jimp * inv_i;
            }

            // --- positional correction: shove the overlap apart so the hulls
            // (gold nose included) can never interpenetrate. ---
            let pen = contact - nlen;
            if pen > 0.0 {
                let corr = pen * 0.5;
                let (a, b) = mut_pair(rockets, i, j);
                a.x += nrm.x * corr;
                a.y += nrm.y * corr;
                b.x -= nrm.x * corr;
                b.y -= nrm.y * corr;
            }

            // --- gated destruction (bottom half + hard enough) ---
            if hard {
                if i_bottom {
                    die(&mut rockets[i], DeathCause::RocketHit);
                }
                if j_bottom {
                    die(&mut rockets[j], DeathCause::RocketHit);
                }
            }
        }
    }
}

/// Borrow two distinct elements of a slice mutably (`i < j`).
#[inline]
fn mut_pair<T>(s: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    let (lo, hi) = s.split_at_mut(j);
    (&mut lo[i], &mut hi[0])
}

/// Is point `p` (closest point on a hull) on the bottom half of segment
/// `[bottom, top]`? Parametrise t∈[0,1] from bottom→top; bottom half is t<0.5.
#[inline]
fn in_bottom_half(p: Vec2, bottom: Vec2, top: Vec2) -> bool {
    let axis = top.sub(bottom);
    let len_sq = axis.len_sq();
    if len_sq <= 1e-12 {
        return true;
    }
    let t = p.sub(bottom).dot(axis) / len_sq;
    t < 0.5
}
