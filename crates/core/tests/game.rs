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
    let foot_drop = cfg.h * 0.5 + cfg.leg_len * cfg.leg_splay.cos();
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
    let mut outcome = Outcome::Playing;
    for _ in 0..3000 {
        // player does nothing and just crashes; B (AI) should win by landing
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
    // also gone — landing is the only win. Both dead => Draw.
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
