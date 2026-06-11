//! Competitive 2-rocket game on top of the same `World`.
//!
//! Rules (from the design):
//!   * Two rockets, two floating pads. Each rocket starts above its own home
//!     pad and aims to land on the opponent's pad.
//!   * Landing alone does not win. To win you must be stably landed on the
//!     opponent's pad AND have killed the opponent: ram their bottom/engine half
//!     with your indestructible top half. While the opponent is still in the
//!     fight you can throttle back up and lift off the pad again to go finish them.
//!   * A destroyed rocket is not removed: the wreck keeps tumbling and still
//!     blocks pads and other rockets, so you have to land around it.
//!   * Fuel drains at a small constant idle rate on top of thrust, so it doubles
//!     as a match clock: running dry can't win it for you, but it leaves you
//!     unable to maneuver.
//!   * If both are destroyed it's a draw; any other terminal state is a loss for
//!     the local player.
//!   * The opponent is driven by a simple AI for now (a placeholder for an RL
//!     policy): it navigates toward the player's home pad.

use crate::config::Cfg;
use crate::controller::guide_to_pad_binary;
use crate::world::{Action, Pad, Rocket, Status, World};

/// Which rocket. `A` is the local player (left), `B` the opponent (right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    AWins,
    BWins,
    Draw,
}

pub struct Game {
    pub world: World,
    pub outcome: Outcome,
    /// If false, rocket B coasts (no AI) — handy for tests / 2-human play.
    pub ai_enabled: bool,
    /// The action the AI applied on the most recent step (for HUD/flame display).
    pub last_ai_action: Action,
}

impl Game {
    /// Standard match layout: pads on the left (A's home) and right (B's home),
    /// each rocket spawned above its own pad and targeting the opponent's.
    pub fn new(cfg: Cfg) -> Self {
        let pad_a = Pad { cx: 6.0, y: 7.0, half_w: 2.6, thick: 0.4 }; // A's home
        let pad_b = Pad { cx: 26.0, y: 7.0, half_w: 2.6, thick: 0.4 }; // B's home

        // A spawns over its home pad (index 0) and must land on B's pad (index 1).
        let mut a = Rocket::spawn(pad_a.cx, 22.0, cfg.fuel_max, 1);
        a.x = pad_a.cx;
        // B spawns over its home pad (index 1) and must land on A's pad (index 0).
        let b = Rocket::spawn(pad_b.cx, 22.0, cfg.fuel_max, 0);

        Game {
            world: World {
                cfg,
                rockets: vec![a, b],
                pads: vec![pad_a, pad_b],
                time: 0.0,
                steps: 0,
            },
            outcome: Outcome::Playing,
            ai_enabled: true,
            last_ai_action: Action::default(),
        }
    }

    pub fn rocket(&self, side: Side) -> &Rocket {
        match side {
            Side::A => &self.world.rockets[0],
            Side::B => &self.world.rockets[1],
        }
    }

    /// Advance one step. `player` drives rocket A; rocket B is driven by the AI
    /// (or coasts if `ai_enabled` is false). Returns the (possibly updated)
    /// outcome. Once the game is decided, further steps are no-ops.
    pub fn step(&mut self, player: Action) -> Outcome {
        if self.outcome != Outcome::Playing {
            return self.outcome;
        }

        let ai = if self.ai_enabled && self.world.rockets[1].alive() {
            self.ai_action(Side::B)
        } else {
            Action::default()
        };
        self.last_ai_action = ai;

        self.world.step(&[player, ai]);
        self.outcome = self.evaluate();
        self.outcome
    }

    /// The opponent's control for this step: steer toward its target pad (the
    /// player's home pad). Swap this out for an RL policy later.
    pub fn ai_action(&self, side: Side) -> Action {
        let r = self.rocket(side);
        let pad = self.world.pads[r.target_pad];
        guide_to_pad_binary(&self.world.cfg, r, &pad)
    }

    fn evaluate(&self) -> Outcome {
        let a = &self.world.rockets[0];
        let b = &self.world.rockets[1];
        // Landing alone does not win: you must be stably landed on the opponent's
        // pad (Status::Landed is only ever set there) AND the opponent must be
        // dead. If both sides qualify on the same step, A is the local player and
        // takes the deterministic tiebreak.
        let a_wins = a.status == Status::Landed && b.status == Status::Dead;
        let b_wins = b.status == Status::Landed && a.status == Status::Dead;
        if a_wins {
            Outcome::AWins
        } else if b_wins {
            Outcome::BWins
        } else if a.status == Status::Dead && b.status == Status::Dead {
            Outcome::Draw
        } else {
            Outcome::Playing
        }
    }
}
