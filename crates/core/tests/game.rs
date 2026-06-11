//! Tests for the competitive 2-rocket game layer.

use rocket_core::{guide_to_pad_binary, Action, Cfg, Game, Outcome, Side, Status};

#[test]
fn own_pad_supports_but_does_not_win() {
    // You can set down on your OWN pad (it's solid — you don't fall through), but
    // resting there never wins; only the opponent's pad does.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    g.ai_enabled = false; // isolate rocket A

    let pad0 = g.world.pads[0]; // A's own home pad
    let foot_drop = cfg.com * cfg.h + cfg.leg_len * cfg.leg_splay.cos();
    g.world.rockets[0].x = pad0.cx;
    g.world.rockets[0].y = pad0.top() + foot_drop + 0.05; // feet just above own deck
    g.world.rockets[0].vy = -0.05;

    for _ in 0..400 {
        // hold a hover over the OWN pad
        let act = guide_to_pad_binary(&cfg, g.rocket(Side::A), &pad0);
        let outcome = g.step(act);
        assert_eq!(outcome, Outcome::Playing, "resting on your own pad is never a win");
    }
    let a = g.rocket(Side::A);
    assert_ne!(a.status, Status::Dead, "the own pad is solid — shouldn't crash/fall through");
    assert!(a.y > pad0.y, "rocket stays on top of its own pad, not through it");
}

#[test]
fn layout_targets_opponent_pad() {
    let g = Game::new(Cfg::default());
    // A is over the left pad, B over the right pad; each aims at the other's.
    assert!(g.rocket(Side::A).x < g.rocket(Side::B).x);
    assert_eq!(g.rocket(Side::A).target_pad, 1);
    assert_eq!(g.rocket(Side::B).target_pad, 0);
}

#[test]
fn ai_flies_toward_player_pad() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    let target = g.world.pads[0]; // B's target = A's home pad (left)
    let d0 = (g.rocket(Side::B).x - target.cx).hypot(g.rocket(Side::B).y - target.y);

    // player coasts; let the AI fly for a while
    for _ in 0..600 {
        if g.step(Action::default()) != Outcome::Playing {
            break;
        }
    }
    let b = g.rocket(Side::B);
    let d1 = (b.x - target.cx).hypot(b.y - target.y);
    // Either it already won by landing, or it has clearly closed the distance.
    assert!(
        g.outcome == Outcome::BWins || d1 < d0 - 5.0,
        "AI should approach the player's pad (d0={d0:.1} d1={d1:.1}, outcome={:?})",
        g.outcome
    );
}

#[test]
fn ai_eventually_lands_and_wins() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    // Player is already a wreck off to the side, so it doesn't block B's target
    // pad. B (AI) should fly over and win by landing on the now-clear pad.
    g.world.rockets[0].status = Status::Dead;
    g.world.rockets[0].x = 16.0;
    g.world.rockets[0].y = cfg.hull_r;
    g.world.rockets[0].vx = 0.0;
    g.world.rockets[0].vy = 0.0;
    let mut outcome = Outcome::Playing;
    for _ in 0..3000 {
        outcome = g.step(Action::default());
        if outcome != Outcome::Playing {
            break;
        }
    }
    assert_eq!(outcome, Outcome::BWins, "AI opponent should land and win");
}

#[test]
fn player_can_win_by_landing() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    g.ai_enabled = false; // isolate: opponent coasts
    // Opponent is already a wreck off to the side, clear of A's target pad, so
    // the guided player can set down on it and win.
    g.world.rockets[1].status = Status::Dead;
    g.world.rockets[1].x = 16.0;
    g.world.rockets[1].y = cfg.hull_r;
    g.world.rockets[1].vx = 0.0;
    g.world.rockets[1].vy = 0.0;
    let mut outcome = Outcome::Playing;
    for _ in 0..3000 {
        // drive A toward its target pad (B's home, index 1) with the guidance law
        let a = g.rocket(Side::A);
        let target = g.world.pads[a.target_pad];
        let act = guide_to_pad_binary(&cfg, a, &target);
        outcome = g.step(act);
        if outcome != Outcome::Playing {
            break;
        }
    }
    assert_eq!(outcome, Outcome::AWins, "guided player should land on opponent's pad");
}

#[test]
fn destroying_opponent_does_not_win() {
    // If the player is destroyed, the player cannot win even if the opponent is
    // also gone: a win still requires being landed on the opponent's pad. Both
    // dead => Draw.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    g.ai_enabled = false;

    // Force a hard mutual bottom-half collision mid-arena -> both destroyed.
    g.world.rockets[0].x = 16.0;
    g.world.rockets[0].y = 14.0;
    g.world.rockets[0].vx = 4.0; // closing fast (impact > crush speed)
    g.world.rockets[1].x = 16.5; // within 2*hull_r -> contact, parallel => both belly-hit
    g.world.rockets[1].y = 14.0;
    g.world.rockets[1].vx = -4.0;

    let outcome = g.step(Action::default());
    assert_eq!(g.rocket(Side::A).status, Status::Dead);
    assert_eq!(g.rocket(Side::B).status, Status::Dead);
    assert_eq!(outcome, Outcome::Draw);
}

#[test]
fn decided_game_is_frozen() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    g.ai_enabled = false;
    g.world.rockets[0].x = 16.0;
    g.world.rockets[0].y = 14.0;
    g.world.rockets[0].vx = 4.0;
    g.world.rockets[1].x = 16.5;
    g.world.rockets[1].y = 14.0;
    g.world.rockets[1].vx = -4.0;
    let first = g.step(Action::default());
    let steps_at_decision = g.world.steps;
    // further steps must not change anything once decided
    let second = g.step(Action::binary(true, true));
    assert_eq!(first, second);
    assert_eq!(g.world.steps, steps_at_decision, "no stepping after the game is decided");
}

#[test]
fn landing_needs_opponent_neutralized() {
    // Being stably landed on the opponent's pad is not enough on its own: while
    // the opponent is still alive and has fuel, the game keeps playing.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true; // B never runs dry
    let mut g = Game::new(cfg);
    g.ai_enabled = false;
    g.world.rockets[0].status = Status::Landed; // A parked on the opponent's pad
    g.world.rockets[1].x = 16.0; // B alive, high and clear of any contact
    g.world.rockets[1].y = 22.0;
    g.world.rockets[1].status = Status::Flying;
    let outcome = g.step(Action::default());
    assert_eq!(outcome, Outcome::Playing, "landing must not win while the opponent is alive and fueled");
}

#[test]
fn landing_wins_once_opponent_destroyed() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut g = Game::new(cfg);
    g.ai_enabled = false;
    g.world.rockets[0].status = Status::Landed;
    g.world.rockets[1].status = Status::Dead;
    assert_eq!(g.step(Action::default()), Outcome::AWins, "landed + opponent destroyed wins");
}

#[test]
fn landing_does_not_win_on_opponent_out_of_fuel() {
    // Out of fuel is no longer a win condition: the opponent has to be dead. A dry
    // but still-alive opponent keeps the game going.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = false;
    let mut g = Game::new(cfg);
    g.ai_enabled = false;
    g.world.rockets[0].status = Status::Landed;
    g.world.rockets[1].x = 16.0;
    g.world.rockets[1].y = 22.0;
    g.world.rockets[1].status = Status::Flying;
    g.world.rockets[1].fuel = 0.0; // opponent out of fuel but still alive
    assert_eq!(g.step(Action::default()), Outcome::Playing, "out of fuel alone must not hand over a win");
}

#[test]
fn idle_burn_drains_fuel_without_thrust() {
    // The constant idle drain means fuel falls even when no booster is firing.
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = false;
    let mut g = Game::new(cfg);
    g.ai_enabled = false;
    let f0 = g.world.rockets[0].fuel;
    g.step(Action::default()); // no thrust
    assert!(g.world.rockets[0].fuel < f0, "idle burn should drain fuel with zero thrust");
}

/// Reproduction sweep for the "landed dead-center but never goes stable" report.
/// The opponent is already a wreck parked on the floor mid-arena (out of the way
/// so it can't fall onto the player), and the player is dropped over the
/// opponent's pad across small height / vertical-speed / tilt / offset
/// perturbations, flying the same guidance autopilot the AI lands with. Every
/// case should close out as a win; any that doesn't is collected and reported.
#[test]
fn lands_on_opponent_pad_under_small_perturbations() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true; // isolate the stable-landing logic from fuel

    let heights = [0.0, 0.15, 0.4, 0.8]; // extra CoM height above a feet-on-deck pose
    let vys = [0.0, -0.3, -0.8]; // initial vertical speed (downward)
    let ths = [0.0, 0.08, -0.08, 0.15, -0.15]; // initial tilt (rad)
    let dxs = [0.0, 1.2, -1.2]; // horizontal offset from pad center (m)

    let foot_drop = cfg.com * cfg.h + cfg.leg_len * cfg.leg_splay.cos();
    let mut failures = Vec::new();

    for &dh in &heights {
        for &vy in &vys {
            for &th in &ths {
                for &dx in &dxs {
                    let mut g = Game::new(cfg);
                    g.ai_enabled = false;

                    // opponent: a wreck resting on the floor at mid-arena
                    let b = &mut g.world.rockets[1];
                    b.status = Status::Dead;
                    b.x = 16.0;
                    b.y = cfg.hull_r;
                    b.vx = 0.0;
                    b.vy = 0.0;
                    b.th = 0.0;
                    b.om = 0.0;

                    // player: dropped over its target pad (A targets the right pad)
                    let target = g.world.pads[g.world.rockets[0].target_pad];
                    let a = &mut g.world.rockets[0];
                    a.x = target.cx + dx;
                    a.y = target.top() + foot_drop + 0.05 + dh;
                    a.vx = 0.0;
                    a.vy = vy;
                    a.th = th;
                    a.om = 0.0;

                    // fly the guidance autopilot until the match is decided
                    let mut outcome = Outcome::Playing;
                    let mut steps = 0;
                    while steps < 1800 {
                        let a = g.rocket(Side::A);
                        let target = g.world.pads[a.target_pad];
                        let act = guide_to_pad_binary(&cfg, a, &target);
                        outcome = g.step(act);
                        steps += 1;
                        if outcome != Outcome::Playing {
                            break;
                        }
                    }
                    if outcome != Outcome::AWins {
                        let a = g.rocket(Side::A);
                        failures.push(format!(
                            "dh={dh} vy={vy} th={th} dx={dx} -> {outcome:?} after {steps} steps \
                             (status={:?}, stable={:.2}s, |th|={:.3}, |om|={:.3}, speed={:.3})",
                            a.status,
                            a.stable_time,
                            a.th.abs(),
                            a.om.abs(),
                            a.speed(),
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} / {} perturbations failed to land and win:\n{}",
        failures.len(),
        heights.len() * vys.len() * ths.len() * dxs.len(),
        failures.join("\n"),
    );
}

/// Closer to the user's report: the player starts with its feet a little ABOVE
/// the opponent's pad and free-falls onto it under gravity with ZERO control,
/// the way a human who has cut the thrusters would. Conditions are deliberately
/// mild (small gap, gentle touch, slight tilt/offset) so the touchdown is soft.
/// Each drop should settle and win on its own; this catches a "rests but never
/// goes stable" failure that an active autopilot could otherwise paper over.
#[test]
fn free_falls_onto_opponent_pad_and_settles() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;

    let gaps = [0.2, 0.5, 1.0]; // feet start this far above the deck (m)
    let vys = [0.0, -0.5]; // start from rest, or a gentle nudge downward
    let ths = [0.0, 0.03, -0.03]; // slight tilt (rad)
    let dxs = [0.0, 0.6, -0.6]; // mild horizontal offset (both feet stay on deck)

    let foot_drop = cfg.com * cfg.h + cfg.leg_len * cfg.leg_splay.cos();
    let mut failures = Vec::new();

    for &gap in &gaps {
        for &vy in &vys {
            for &th in &ths {
                for &dx in &dxs {
                    let mut g = Game::new(cfg);
                    g.ai_enabled = false;

                    let b = &mut g.world.rockets[1];
                    b.status = Status::Dead;
                    b.x = 16.0;
                    b.y = cfg.hull_r;
                    b.vx = 0.0;
                    b.vy = 0.0;
                    b.th = 0.0;
                    b.om = 0.0;

                    let target = g.world.pads[g.world.rockets[0].target_pad];
                    let a = &mut g.world.rockets[0];
                    a.x = target.cx + dx;
                    a.y = target.top() + foot_drop + gap; // feet `gap` above the deck
                    a.vx = 0.0;
                    a.vy = vy;
                    a.th = th;
                    a.om = 0.0;
                    a.legs_out = true;

                    let mut outcome = Outcome::Playing;
                    let mut steps = 0;
                    while steps < 600 {
                        // no input: pure free fall, then let it settle
                        outcome = g.step(Action::default());
                        steps += 1;
                        if outcome != Outcome::Playing {
                            break;
                        }
                    }
                    if outcome != Outcome::AWins {
                        let a = g.rocket(Side::A);
                        failures.push(format!(
                            "gap={gap} vy={vy} th={th} dx={dx} -> {outcome:?} after {steps} steps \
                             (status={:?}, stable={:.2}s, |th|={:.3}, |om|={:.3}, speed={:.3})",
                            a.status,
                            a.stable_time,
                            a.th.abs(),
                            a.om.abs(),
                            a.speed(),
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} / {} free-fall drops failed to settle and win:\n{}",
        failures.len(),
        gaps.len() * vys.len() * ths.len() * dxs.len(),
        failures.join("\n"),
    );
}
