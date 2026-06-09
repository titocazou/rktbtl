//! System test: fly a rocket around for a long episode with a simple closed-loop
//! controller and assert the simulation stays healthy and *responsive* the whole
//! time. This is the regression guard for the "controls stop answering after
//! flying for a while" report.

use rocket_core::{guide_to_pad, Action, Cfg, Status, World};

#[test]
fn long_guided_flight_stays_healthy() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true; // we're testing dynamics, not fuel economy
    let mut w = World::single(cfg);
    let pad = w.pads[0];

    let mut got_close = false;
    let mut max_speed = 0.0f64;

    for step in 0..6000 {
        let a = guide_to_pad(&cfg, &w.rockets[0], &pad);
        w.step(&[a]);
        let r = &w.rockets[0];

        // 1. never non-finite
        assert!(
            r.x.is_finite() && r.y.is_finite() && r.vx.is_finite() && r.vy.is_finite()
                && r.th.is_finite() && r.om.is_finite(),
            "state went non-finite at step {step}"
        );
        // 2. speed never explodes (numerical sanity)
        max_speed = max_speed.max(r.speed());
        assert!(max_speed < 1e3, "speed blew up to {max_speed} at step {step}");

        if (r.x - pad.cx).hypot(r.y - pad.y) < cfg.deploy_r {
            got_close = true;
        }
        if r.status != Status::Flying {
            break; // landed or crashed — either is a clean terminal state
        }
    }

    assert!(got_close, "guided rocket should at least reach the pad's vicinity");
}

#[test]
fn controls_remain_responsive_throughout() {
    // Directly probe responsiveness: at many points during a long flight, firing
    // a single booster must measurably change angular velocity.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut w = World::single(cfg);

    for step in 0..3000 {
        // keep it loitering mid-arena so it never terminates on bounds
        w.rockets[0].x = 16.0;
        w.rockets[0].y = 16.0;
        w.rockets[0].vx = 0.0;
        w.rockets[0].vy = 0.0;
        w.rockets[0].th = 0.0;
        w.rockets[0].om = 0.0;

        w.step(&[Action::binary(true, false)]); // left booster only
        assert!(
            w.rockets[0].om.abs() > 1e-6,
            "booster input produced no response at step {step}"
        );
        assert_eq!(w.rockets[0].status, Status::Flying);
    }
}
