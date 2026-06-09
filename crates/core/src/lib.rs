//! # rocket-core
//!
//! Pure, deterministic 2D rocket-lander physics and game logic. No rendering,
//! no threads, no I/O, no allocation in the per-step hot path. This crate is the
//! single source of truth driven by both the Python RL bindings (`rocket-py`)
//! and the browser build (`rocket-wasm`).
//!
//! Quick start:
//! ```
//! use rocket_core::{Cfg, World, Action};
//! let mut world = World::single(Cfg::default());
//! world.step(&[Action::binary(true, false)]); // fire left booster
//! assert_eq!(world.rockets.len(), 1);
//! ```

mod config;
mod controller;
mod game;
mod geom;
mod world;

pub use config::Cfg;
pub use controller::{guide_to_pad, guide_to_pad_binary};
pub use game::{Game, Outcome, Side};
pub use geom::{point_seg_dist_sq, seg_seg_closest, Vec2};
pub use world::{step_isolated, Action, DeathCause, Pad, Rocket, Status, World, OBS_DIM};

/// Reward shaping for the RL task (single rocket, single pad). Pure function of
/// the previous and current rocket pose. Mirrors the prototype's `reward()`.
pub fn reward(_cfg: &Cfg, pad: &Pad, prev: &Rocket, cur: &Rocket) -> f64 {
    match cur.status {
        Status::Landed => 100.0,
        Status::Dead => -100.0,
        Status::Flying => {
            let aim_y = pad.y + 1.0;
            let d_prev = (prev.x - pad.cx).hypot(prev.y - aim_y);
            let d_now = (cur.x - pad.cx).hypot(cur.y - aim_y);
            (d_prev - d_now) * 2.0 - cur.th.abs() * 0.1 - 0.01
        }
    }
}
