//! Browser frontend bindings. The JS layer does *only* rendering and input;
//! every bit of physics/game logic runs in `rocket-core` (the same code Python
//! drives), so the two can never diverge.

use rocket_core::{Action, Cfg, Game, Outcome, Rocket, Status, World};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct PadView {
    cx: f64,
    y: f64,
    half_w: f64,
    thick: f64,
    owner: u8, // which side this pad belongs to (0=A,1=B); 0 for the solo sandbox
}

#[derive(Serialize)]
struct RocketView {
    x: f64,
    y: f64,
    th: f64,
    vx: f64,
    vy: f64,
    om: f64,
    fuel: f64,
    status: String,
    death_cause: String,
    legs_out: bool,
    stable_time: f64,
    side: u8, // 0=A (player), 1=B (opponent); 0 in the solo sandbox
    // world-space geometry, precomputed in Rust so JS never recomputes the pose
    feet: [[f64; 2]; 2],
    top: [f64; 2],
    bottom: [f64; 2],
}

#[derive(Serialize)]
struct RenderState {
    world_w: f64,
    world_h: f64,
    deploy_r: f64,
    body_w: f64,
    body_h: f64,
    hull_r: f64,
    time: f64,
    outcome: String, // "playing" | "a_wins" | "b_wins" | "draw" | "" (sandbox)
    rockets: Vec<RocketView>,
    pads: Vec<PadView>,
}

fn status_str(s: Status) -> String {
    match s {
        Status::Flying => "flying",
        Status::Landed => "landed",
        Status::Dead => "dead",
    }
    .into()
}

fn rocket_view(cfg: &Cfg, r: &Rocket, side: u8) -> RocketView {
    let feet = r.feet(cfg);
    let top = r.hull_top(cfg);
    let bottom = r.hull_bottom(cfg);
    RocketView {
        x: r.x,
        y: r.y,
        th: r.th,
        vx: r.vx,
        vy: r.vy,
        om: r.om,
        fuel: r.fuel,
        status: status_str(r.status),
        death_cause: format!("{:?}", r.death_cause),
        legs_out: r.legs_out,
        stable_time: r.stable_time,
        side,
        feet: [[feet[0].x, feet[0].y], [feet[1].x, feet[1].y]],
        top: [top.x, top.y],
        bottom: [bottom.x, bottom.y],
    }
}

fn render_state(world: &World, outcome: &str, owners: &[u8]) -> RenderState {
    let cfg = &world.cfg;
    RenderState {
        world_w: cfg.world_w,
        world_h: cfg.world_h,
        deploy_r: cfg.deploy_r,
        body_w: cfg.w,
        body_h: cfg.h,
        hull_r: cfg.hull_r,
        time: world.time,
        outcome: outcome.into(),
        rockets: world
            .rockets
            .iter()
            .enumerate()
            .map(|(i, r)| rocket_view(cfg, r, i as u8))
            .collect(),
        pads: world
            .pads
            .iter()
            .enumerate()
            .map(|(i, p)| PadView {
                cx: p.cx,
                y: p.y,
                half_w: p.half_w,
                thick: p.thick,
                owner: owners.get(i).copied().unwrap_or(0),
            })
            .collect(),
    }
}

/// One interactive single-rocket sandbox, driven a fixed timestep at a time.
#[wasm_bindgen]
pub struct Sim {
    world: World,
    left: bool,
    right: bool,
}

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Sim {
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();
        Sim {
            world: World::single(Cfg::default()),
            left: false,
            right: false,
        }
    }

    pub fn reset(&mut self) {
        self.world.reset_single();
    }

    /// Debug/scenario helper: force the rocket pose (no effect on physics rules).
    pub fn set_pose(&mut self, x: f64, y: f64, th: f64) {
        let r = &mut self.world.rockets[0];
        r.x = x;
        r.y = y;
        r.th = th;
        r.vx = 0.0;
        r.vy = 0.0;
        r.om = 0.0;
    }

    /// Binary booster input from held keys.
    pub fn set_input(&mut self, left: bool, right: bool) {
        self.left = left;
        self.right = right;
    }

    /// Advance exactly one fixed physics step using the current input.
    pub fn step(&mut self) {
        self.world.step(&[Action::binary(self.left, self.right)]);
    }

    /// Throttle fractions actually applied (0/1 in binary mode) — for HUD bars.
    pub fn throttle(&self) -> Vec<f64> {
        let r = &self.world.rockets[0];
        let firing = r.status == Status::Flying && (self.world.cfg.infinite_fuel || r.fuel > 0.0);
        vec![
            if self.left && firing { 1.0 } else { 0.0 },
            if self.right && firing { 1.0 } else { 0.0 },
        ]
    }

    /// Full render snapshot as a JS object.
    pub fn snapshot(&self) -> JsValue {
        let state = render_state(&self.world, "", &[0]);
        serde_wasm_bindgen::to_value(&state).unwrap()
    }

    /// Derived ratios for the tuning panel.
    pub fn ratios(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&ratios_of(&self.world.cfg)).unwrap()
    }

    /// Set a tunable parameter by name (hidden behind the UI's "tune" toggle).
    pub fn set_param(&mut self, name: &str, value: f64) {
        let c = &mut self.world.cfg;
        match name {
            "m" => c.m = value,
            "tmax" => c.tmax = value,
            "d" => c.d = value,
            "i_scale" => c.i_scale = value,
            "leg_k" => c.leg_k = value,
            "leg_c" => c.leg_c = value,
            "leg_mu" => c.leg_mu = value,
            "infinite_fuel" => c.infinite_fuel = value != 0.0,
            _ => {}
        }
    }

    pub fn get_param(&self, name: &str) -> f64 {
        let c = &self.world.cfg;
        match name {
            "m" => c.m,
            "tmax" => c.tmax,
            "d" => c.d,
            "i_scale" => c.i_scale,
            "leg_k" => c.leg_k,
            "leg_c" => c.leg_c,
            "leg_mu" => c.leg_mu,
            "infinite_fuel" => if c.infinite_fuel { 1.0 } else { 0.0 },
            _ => 0.0,
        }
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Playing => "playing",
        Outcome::AWins => "a_wins",
        Outcome::BWins => "b_wins",
        Outcome::Draw => "draw",
    }
}

/// Competitive 2-rocket match: the player drives rocket A; rocket B is the AI.
#[wasm_bindgen]
pub struct GameSim {
    game: Game,
    left: bool,
    right: bool,
}

#[wasm_bindgen]
impl GameSim {
    #[wasm_bindgen(constructor)]
    pub fn new() -> GameSim {
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();
        GameSim {
            game: Game::new(Cfg::default()),
            left: false,
            right: false,
        }
    }

    pub fn reset(&mut self) {
        let cfg = self.game.world.cfg;
        self.game = Game::new(cfg);
    }

    pub fn set_input(&mut self, left: bool, right: bool) {
        self.left = left;
        self.right = right;
    }

    /// Debug/scenario helper: place a rocket (0=player A, 1=AI B) with a velocity.
    pub fn place(&mut self, side: usize, x: f64, y: f64, th: f64, vx: f64, vy: f64) {
        let r = &mut self.game.world.rockets[side.min(1)];
        r.x = x;
        r.y = y;
        r.th = th;
        r.vx = vx;
        r.vy = vy;
        r.om = 0.0;
    }

    /// Advance one fixed step (player drives A, AI drives B). Returns the outcome
    /// string.
    pub fn step(&mut self) -> String {
        let o = self.game.step(Action::binary(self.left, self.right));
        outcome_str(o).into()
    }

    pub fn outcome(&self) -> String {
        outcome_str(self.game.outcome).into()
    }

    /// Throttles applied to the player's rocket (A) — for the HUD bars.
    pub fn player_throttle(&self) -> Vec<f64> {
        let r = &self.game.world.rockets[0];
        let firing = r.status == Status::Flying && (self.game.world.cfg.infinite_fuel || r.fuel > 0.0);
        vec![
            if self.left && firing { 1.0 } else { 0.0 },
            if self.right && firing { 1.0 } else { 0.0 },
        ]
    }

    /// Throttles the AI applied to its rocket (B) on the last step — for HUD bars
    /// and engine flames, gated by the same firing condition as the player.
    pub fn opp_throttle(&self) -> Vec<f64> {
        let r = &self.game.world.rockets[1];
        let firing = r.status == Status::Flying && (self.game.world.cfg.infinite_fuel || r.fuel > 0.0);
        let a = self.game.last_ai_action;
        vec![
            if firing { a.tl } else { 0.0 },
            if firing { a.tr } else { 0.0 },
        ]
    }

    pub fn snapshot(&self) -> JsValue {
        // pad 0 belongs to A, pad 1 to B
        let state = render_state(&self.game.world, outcome_str(self.game.outcome), &[0, 1]);
        serde_wasm_bindgen::to_value(&state).unwrap()
    }

    /// Toggle infinite fuel for both rockets (sandbox/testing convenience).
    pub fn set_infinite_fuel(&mut self, on: bool) {
        self.game.world.cfg.infinite_fuel = on;
    }

    /// Tweak a collision parameter live: "rocket_restitution" (bounce, 0..1) or
    /// "rocket_crush_speed" (m/s impact needed to destroy an engine).
    pub fn set_param(&mut self, name: &str, value: f64) {
        let c = &mut self.game.world.cfg;
        match name {
            "rocket_restitution" => c.rocket_restitution = value,
            "rocket_crush_speed" => c.rocket_crush_speed = value,
            _ => {}
        }
    }

    pub fn get_param(&self, name: &str) -> f64 {
        let c = &self.game.world.cfg;
        match name {
            "rocket_restitution" => c.rocket_restitution,
            "rocket_crush_speed" => c.rocket_crush_speed,
            _ => 0.0,
        }
    }
}

impl Default for GameSim {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct Ratios {
    thrust_to_weight: f64,
    angular_authority: f64,
    inertia: f64,
}

fn ratios_of(c: &Cfg) -> Ratios {
    Ratios {
        thrust_to_weight: c.thrust_to_weight(),
        angular_authority: c.angular_authority(),
        inertia: c.inertia(),
    }
}
