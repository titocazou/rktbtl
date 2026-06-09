//! Unit tests for the deterministic physics core.

use rocket_core::{Action, Cfg, DeathCause, Pad, Rocket, Status, World};

fn approx(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn falls_under_gravity() {
    let mut w = World::single(Cfg::default());
    let y0 = w.rockets[0].y;
    for _ in 0..30 {
        w.step(&[Action::new(0.0, 0.0)]);
    }
    let r = &w.rockets[0];
    assert!(r.vy < 0.0, "should accelerate downward, vy={}", r.vy);
    assert!(r.y < y0, "should lose altitude");
    assert_eq!(r.status, Status::Flying);
}

#[test]
fn both_boosters_beat_gravity() {
    // Default TWR is ~2.7 (>1), so full thrust must produce net upward accel.
    let mut w = World::single(Cfg::default());
    assert!(w.cfg.thrust_to_weight() > 1.0);
    for _ in 0..20 {
        w.step(&[Action::new(1.0, 1.0)]);
    }
    assert!(w.rockets[0].vy > 0.0, "full thrust should climb");
}

#[test]
fn differential_thrust_makes_torque() {
    let mut left = World::single(Cfg::default());
    let mut right = World::single(Cfg::default());
    for _ in 0..10 {
        left.step(&[Action::new(1.0, 0.0)]); // left booster only
        right.step(&[Action::new(0.0, 1.0)]); // right booster only
    }
    // opposite spin directions, both non-trivial
    assert!(left.rockets[0].om * right.rockets[0].om < 0.0);
    assert!(left.rockets[0].om.abs() > 0.1);
    // pure-both gives (almost) no spin
    let mut both = World::single(Cfg::default());
    for _ in 0..10 {
        both.step(&[Action::new(1.0, 1.0)]);
    }
    assert!(both.rockets[0].om.abs() < 1e-9, "balanced thrust = no torque");
}

#[test]
fn fuel_runs_out_and_kills_thrust() {
    // This is the reported "controls stop answering after a while" bug: once the
    // finite tank empties, thrust silently stops. We assert it happens...
    let mut w = World::single(Cfg::default());
    let burn = (w.cfg.fuel_max / (2.0 * w.cfg.fuel_rate * w.cfg.dt)).ceil() as i32 + 5;
    for _ in 0..burn {
        w.step(&[Action::new(1.0, 1.0)]);
    }
    assert_eq!(w.rockets[0].fuel, 0.0, "tank should be empty");
    let vy_before = w.rockets[0].vy;
    w.step(&[Action::new(1.0, 1.0)]); // command full thrust on empty tank
    let dvy = w.rockets[0].vy - vy_before;
    assert!(
        approx(dvy, w.cfg.g * w.cfg.dt, 1e-9),
        "empty tank => only gravity acts, dvy={dvy}"
    );
}

#[test]
fn infinite_fuel_fixes_frozen_controls() {
    // ...and the fix: with infinite fuel the controls never stop responding.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut w = World::single(cfg);
    for _ in 0..5000 {
        w.step(&[Action::new(1.0, 1.0)]);
        // keep it in the room so it doesn't terminate on bounds
        w.rockets[0].y = 14.0;
        w.rockets[0].vy = 0.0;
    }
    assert_eq!(w.rockets[0].status, Status::Flying);
    let vy_before = w.rockets[0].vy;
    w.step(&[Action::new(1.0, 1.0)]);
    assert!(
        w.rockets[0].vy - vy_before > 0.0,
        "thrust must still respond after long flight"
    );
}

#[test]
fn legs_deploy_near_pad_and_restow_far() {
    let cfg = Cfg::default();
    let mut w = World::single(cfg);
    let pad = w.pads[0];
    // place CoM right over the pad → must deploy
    w.rockets[0].x = pad.cx;
    w.rockets[0].y = pad.y + 3.0;
    w.step(&[Action::default()]);
    assert!(w.rockets[0].legs_out, "legs deploy inside deploy radius");
    // fly far away → must re-stow (the user's requested change)
    w.rockets[0].x = 2.0;
    w.rockets[0].y = 25.0;
    w.step(&[Action::default()]);
    assert!(!w.rockets[0].legs_out, "legs stow again when far from pad");
}

#[test]
fn feet_geometry_matches_sketch() {
    // Upright rocket: left foot to the left & below base, right foot to the right.
    let cfg = Cfg::default();
    let r = Rocket::spawn(10.0, 10.0, cfg.fuel_max, 0);
    let feet = r.feet(&cfg);
    let base = r.hull_bottom(&cfg);
    assert!(feet[0].x < r.x, "left foot is left of centreline");
    assert!(feet[1].x > r.x, "right foot is right of centreline");
    assert!(feet[0].y < base.y && feet[1].y < base.y, "feet hang below base");
    // symmetric splay
    assert!(approx(feet[0].x - r.x, -(feet[1].x - r.x), 1e-9));
}

#[test]
fn deterministic() {
    let run = || {
        let mut w = World::single(Cfg::default());
        for i in 0..600 {
            let a = Action::binary(i % 3 == 0, i % 5 == 0);
            w.step(&[a]);
        }
        w.rockets[0]
    };
    let a = run();
    let b = run();
    assert_eq!(a, b, "identical inputs must give identical state");
}

#[test]
fn falling_to_ground_is_fatal() {
    let mut w = World::single(Cfg::default());
    for _ in 0..2000 {
        w.step(&[Action::default()]);
        if w.rockets[0].status != Status::Flying {
            break;
        }
    }
    assert_eq!(w.rockets[0].status, Status::Dead);
    assert_eq!(w.rockets[0].death_cause, DeathCause::Ground);
}

#[test]
fn top_half_survives_ceiling_but_bottom_dies() {
    let cfg = Cfg::default();

    // Nose poking through the ceiling: indestructible, clamped back in.
    let mut top_world = World::single(cfg);
    top_world.rockets[0].x = 16.0;
    top_world.rockets[0].y = cfg.world_h - 0.1; // top (y+h/2) is above ceiling
    top_world.rockets[0].vy = 0.0;
    top_world.step(&[Action::default()]);
    assert_eq!(top_world.rockets[0].status, Status::Flying);
    assert!(top_world.rockets[0].hull_top(&cfg).y <= cfg.world_h + 1e-9);

    // Engine end through the ceiling: fatal. Place CoM high enough that the
    // hull bottom (y − h/2) is itself above the ceiling.
    let mut bot_world = World::single(cfg);
    bot_world.rockets[0].x = 16.0;
    bot_world.rockets[0].y = cfg.world_h + cfg.h * 0.5 + 0.2; // bottom above ceiling
    bot_world.step(&[Action::default()]);
    assert_eq!(bot_world.rockets[0].status, Status::Dead);
    assert_eq!(bot_world.rockets[0].death_cause, DeathCause::Ceiling);
}

#[test]
fn stays_finite_under_stiff_contact() {
    // Twitchy inertia + stiff legs is exactly the regime that blew up to NaN in
    // the prototype. After the guard, state must always remain finite.
    let mut cfg = Cfg::default();
    cfg.i_scale = 0.2; // twitchiest
    cfg.leg_k = 2500.0; // stiffest
    cfg.infinite_fuel = true;
    let mut w = World::single(cfg);
    w.rockets[0].x = w.pads[0].cx;
    w.rockets[0].y = w.pads[0].top() + 0.5;
    for i in 0..4000 {
        let a = Action::binary(i % 2 == 0, i % 3 == 0);
        w.step(&[a]);
        let r = &w.rockets[0];
        assert!(
            r.x.is_finite() && r.y.is_finite() && r.th.is_finite() && r.om.is_finite(),
            "state went non-finite at step {i}"
        );
        if r.status != Status::Flying {
            break;
        }
    }
}

#[test]
fn gentle_touchdown_lands() {
    // Park the rocket just above the pad, upright and slow, holding a hover with
    // the guidance law. It must accrue the stable timer and win.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut w = World::single(cfg);
    let pad = w.pads[0];
    let foot_drop = cfg.h * 0.5 + cfg.leg_len * cfg.leg_splay.cos();
    w.rockets[0].x = pad.cx;
    w.rockets[0].y = pad.top() + foot_drop + 0.05; // feet just above the deck
    w.rockets[0].vx = 0.0;
    w.rockets[0].vy = -0.05;

    let mut landed = false;
    for _ in 0..400 {
        let a = rocket_core::guide_to_pad(&cfg, &w.rockets[0], &pad);
        w.step(&[a]);
        if w.rockets[0].status == Status::Landed {
            landed = true;
            break;
        }
        assert_ne!(w.rockets[0].status, Status::Dead, "should not crash on a gentle set-down");
    }
    assert!(landed, "gentle approach should produce a stable landing");
}

#[test]
fn can_fly_under_floating_pad() {
    // The pad is a thin FLOATING slab: directly below it (over its x-range, below
    // pad.y) there must be no contact force — you can fly underneath.
    let cfg = Cfg::default();
    let mut w = World::single(cfg);
    let pad = w.pads[0];
    w.rockets[0].x = pad.cx; // directly under the pad
    w.rockets[0].y = pad.y - 1.5; // CoM (and feet) below the slab
    w.rockets[0].vx = 0.0;
    w.rockets[0].vy = 0.0;
    w.step(&[Action::default()]); // coast: only gravity should act
    let dvy = w.rockets[0].vy;
    assert!(
        approx(dvy, cfg.g * cfg.dt, 1e-9),
        "under the slab there is no upward contact force (dvy={dvy})"
    );
    assert_eq!(w.rockets[0].status, Status::Flying);
}

#[test]
fn pad_top_is_solid_no_sinking_through() {
    // Approaching the top face from above, the slab must push back (can't sink
    // straight through it).
    let cfg = Cfg::default();
    let mut w = World::single(cfg);
    let pad = w.pads[0];
    let foot_drop = cfg.h * 0.5 + cfg.leg_len * cfg.leg_splay.cos();
    w.rockets[0].x = pad.cx;
    w.rockets[0].y = pad.y + 0.2 + foot_drop; // feet at pad.y+0.2, inside the slab band
    w.rockets[0].vy = -1.0; // descending onto the deck
    w.step(&[Action::default()]);
    // contact opposes the descent: vy ends up above free-fall (more than gravity alone)
    assert!(
        w.rockets[0].vy > -1.0 + cfg.g * cfg.dt + 1e-6,
        "the pad's top face supports the feet (vy={})",
        w.rockets[0].vy
    );
}

fn ram_scene(attacker_vx: f64) -> World {
    // Attacker A is horizontal, nose pointing +x, just left of victim B's engine
    // (bottom) end, charging right at `attacker_vx`.
    let cfg = Cfg::default();
    let pad = Pad { cx: 16.0, y: 7.0, half_w: 2.6, thick: 0.4 };
    let mut attacker = Rocket::spawn(9.0, 14.3, cfg.fuel_max, 0);
    attacker.th = -std::f64::consts::FRAC_PI_2; // nose (top) points +x toward B
    attacker.vx = attacker_vx;
    let victim = Rocket::spawn(10.0, 15.0, cfg.fuel_max, 0); // upright, belly at (10,14.3)
    World {
        cfg,
        rockets: vec![attacker, victim],
        pads: vec![pad],
        time: 0.0,
        steps: 0,
    }
}

#[test]
fn fast_nose_ram_destroys_opponent_belly_only() {
    // Hard hit (> crush speed): B's belly is destroyed; A's nose (top half) lives.
    let mut w = ram_scene(5.0);
    w.step(&[Action::default(), Action::default()]);
    assert_eq!(w.rockets[0].status, Status::Flying, "nose (top half) is indestructible");
    assert_eq!(w.rockets[1].status, Status::Dead, "belly (bottom half) is destroyed");
    assert_eq!(w.rockets[1].death_cause, DeathCause::RocketHit);
}

#[test]
fn slow_belly_tap_survives_and_bounces() {
    // Below crush speed: nobody is destroyed, but the hulls are solid so they
    // exchange momentum (the victim gets shoved in the ram direction).
    let mut w = ram_scene(1.0); // 1 m/s < default crush speed 3 m/s
    w.step(&[Action::default(), Action::default()]);
    assert_eq!(w.rockets[0].status, Status::Flying);
    assert_eq!(w.rockets[1].status, Status::Flying, "a gentle tap must not destroy");
    assert!(w.rockets[1].vx > 0.0, "victim is pushed along the ram direction (momentum transfer)");
    assert!(w.rockets[0].vx < 1.0, "attacker is slowed / bounced by the impact");
}

#[test]
fn restitution_controls_bounce() {
    // More restitution ⇒ more separation velocity imparted to the victim.
    let sep = |e: f64| {
        let mut w = ram_scene(2.0);
        w.cfg.rocket_restitution = e;
        w.step(&[Action::default(), Action::default()]);
        w.rockets[1].vx
    };
    let inelastic = sep(0.0);
    let elastic = sep(1.0);
    assert!(
        elastic > inelastic + 1e-3,
        "elastic bounce transfers more momentum (e=1: {elastic:.3} vs e=0: {inelastic:.3})"
    );
}

#[test]
fn solid_hulls_cannot_interpenetrate() {
    // Two overlapping rockets get pushed apart so the gold nose (and the rest of
    // the hull) can't pass through — positional correction separates them.
    let cfg = Cfg::default();
    let pad = Pad { cx: 16.0, y: 7.0, half_w: 2.6, thick: 0.4 };
    let a = Rocket::spawn(10.0, 14.0, cfg.fuel_max, 0);
    let b = Rocket::spawn(10.3, 14.0, cfg.fuel_max, 0); // overlapping (< 2*hull_r apart)
    let mut w = World { cfg, rockets: vec![a, b], pads: vec![pad], time: 0.0, steps: 0 };
    w.step(&[Action::default(), Action::default()]);
    let (a, b) = (w.rockets[0], w.rockets[1]);
    let (d_sq, _) = rocket_core::seg_seg_closest(
        a.hull_bottom(&cfg), a.hull_top(&cfg), b.hull_bottom(&cfg), b.hull_top(&cfg),
    );
    assert!(
        d_sq.sqrt() >= 2.0 * cfg.hull_r - 1e-3,
        "hulls must be separated to at least the contact distance (got {:.3})",
        d_sq.sqrt()
    );
    assert_eq!(a.status, Status::Flying);
    assert_eq!(b.status, Status::Flying);
}

#[test]
fn observation_dim_is_stable() {
    let cfg = Cfg::default();
    let w = World::single(cfg);
    let obs = w.rockets[0].observation(&cfg, &w.pads[0]);
    assert_eq!(obs.len(), rocket_core::OBS_DIM);
    assert!(obs.iter().all(|v| v.is_finite()));
}
