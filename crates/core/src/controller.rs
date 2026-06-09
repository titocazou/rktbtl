//! Hand-written heuristic controllers. Two uses:
//!   1. the system test flies a rocket around for thousands of steps to prove
//!      the sim stays finite and responsive;
//!   2. the placeholder AI opponent in the game (to be swapped for an RL policy).
//!
//! These are deliberately simple PD laws over the binary boosters — not optimal,
//! just stable and "goes roughly where you point it".

use crate::config::Cfg;
use crate::world::{Action, Pad, Rocket};

/// A cascaded guidance law (position → velocity → attitude → torque). It holds
/// altitude above the pad while traversing horizontally, then settles straight
/// down once lined up. Returns continuous throttles in `[0,1]`; callers wanting
/// binary boosters can threshold at 0.5.
pub fn guide_to_pad(cfg: &Cfg, r: &Rocket, pad: &Pad) -> Action {
    let aligned = (r.x - pad.cx).abs() < 0.8 && r.vx.abs() < 1.0;

    // --- horizontal: position → desired velocity (capped) → desired tilt. ---
    let ex = pad.cx - r.x;
    let vx_des = (0.6 * ex).clamp(-4.0, 4.0); // approach speed cap, m/s
    // To accelerate toward vx_des the body must tilt: +x accel needs th<0
    // (thrust Fx = −F·sinθ). Tilt ∝ velocity error, gently bounded.
    let desired_th = (-0.22 * (vx_des - r.vx)).clamp(-0.4, 0.4);

    // --- attitude PD → torque demand → differential booster bias. ---
    let eth = desired_th - r.th;
    let torque_cmd = 6.0 * eth - 2.0 * r.om; // >0 ⇒ favour right booster

    // --- vertical: hold high while traversing, descend slowly once aligned. ---
    // The legs hang `foot_drop` below the CoM, so to set the FEET on the deck we
    // aim the CoM that much higher (with a touch of penetration to settle).
    let foot_drop = cfg.h * 0.5 + cfg.leg_len * cfg.leg_splay.cos();
    let aim_y = if aligned { pad.top() + foot_drop - 0.05 } else { pad.y + 5.0 };
    let ey = aim_y - r.y;
    let vy_des = (0.6 * ey).clamp(-1.5, 2.0); // limit descent rate for control authority
    let hover = (cfg.m * cfg.g.abs()) / (2.0 * cfg.tmax); // throttle to hover
    let base = (hover + 0.3 * (vy_des - r.vy)).clamp(0.0, 1.0);

    // Split common throttle by the torque demand.
    let bias = (0.5 * torque_cmd).clamp(-base, 1.0 - base);
    let tl = (base - bias).clamp(0.0, 1.0);
    let tr = (base + bias).clamp(0.0, 1.0);
    Action::new(tl, tr)
}

/// Binary version of [`guide_to_pad`] for the on/off booster model.
pub fn guide_to_pad_binary(cfg: &Cfg, r: &Rocket, pad: &Pad) -> Action {
    let a = guide_to_pad(cfg, r, pad);
    Action::binary(a.tl > 0.5, a.tr > 0.5)
}
