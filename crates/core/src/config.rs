//! Simulation configuration. Every tunable from the original HTML prototype is
//! here. The `Default` impl reproduces the hand-tuned values the user settled on
//! (mass 1.8, inertia scale 3.3, soft legs, etc.) so the sim "feels" identical
//! out of the box; the frontend hides these behind a toggle.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Cfg {
    // --- integration ---
    pub dt: f64, // fixed timestep (s)
    pub g: f64,  // gravity (m/s^2), y-up so this is negative

    // --- rigid body ---
    pub m: f64,       // mass (kg)
    pub w: f64,       // body width (m)
    pub h: f64,       // body height (m)
    pub d: f64,       // booster lateral offset from centerline (m)
    pub tmax: f64,    // max thrust per booster (N)
    pub i_scale: f64, // multiplier on the rectangular inertia (twitchiness knob)
    pub com: f64,     // CoM position along the body, 0 = engine end, 1 = nose, 0.5 = center

    // --- fuel ---
    pub fuel_max: f64,
    pub fuel_rate: f64,       // fuel/sec at full thrust, per booster
    pub fuel_idle_rate: f64,  // fuel/sec drained constantly while flying (match clock)
    pub infinite_fuel: bool,  // RL / sandbox toggle: never run dry

    // --- world bounds (m), origin bottom-left ---
    pub world_w: f64,
    pub world_h: f64,

    // --- landing legs (kickstands) ---
    pub deploy_r: f64,    // legs deploy when CoM within this radius of target pad (m)
    pub stow_hysteresis: f64, // legs re-stow past deploy_r * this factor (avoids flicker)
    pub leg_len: f64,     // leg length (m)
    pub leg_splay: f64,   // outward angle of each leg from the body's down axis (rad)
    pub leg_k: f64,       // contact spring stiffness (N/m of penetration)
    pub leg_c: f64,       // contact damper (N per m/s)
    pub leg_mu: f64,      // Coulomb friction coefficient at the foot

    // --- rocket-vs-rocket collision / destruction ---
    pub hull_r: f64,              // capsule radius of the rocket hull (m)
    pub rocket_restitution: f64,  // bounce on rocket-rocket contact: 0 = inelastic, 1 = elastic
    pub rocket_crush_speed: f64,  // min normal impact speed (m/s) to destroy a bottom-half hit

    // --- stable-landing win condition ---
    pub stable_need: f64, // seconds of stability required to win
    pub v_stable: f64,    // max speed to count as stable (m/s)
    pub ang_stable: f64,  // max |angle| to count as stable (rad)
    pub om_stable: f64,   // max |ang vel| to count as stable (rad/s)
    pub v_explode: f64,   // foot/body impact speed above which the rocket explodes (m/s)
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            dt: 1.0 / 60.0,
            g: -9.0,

            m: 1.40,
            w: 0.6,
            h: 1.4,
            d: 0.28,
            tmax: 29.0,
            i_scale: 2.1,
            com: 0.2,

            fuel_max: 100.0,
            fuel_rate: 6.15,
            fuel_idle_rate: 0.375,
            infinite_fuel: false,

            world_w: 32.0,
            world_h: 28.0,

            deploy_r: 5.0,
            stow_hysteresis: 1.25,
            leg_len: 0.7,
            leg_splay: 0.6,
            leg_k: 1000.0,
            leg_c: 50.0,
            leg_mu: 0.39,

            hull_r: 0.30,
            rocket_restitution: 0.35,
            rocket_crush_speed: 3.0,

            stable_need: 1.0,
            v_stable: 0.4,
            ang_stable: 0.20,
            om_stable: 0.4,
            v_explode: 5.0,
        }
    }
}

impl Cfg {
    /// Moment of inertia of the body about its CoM (rectangular plate × scale).
    /// When the CoM is off-center (`com != 0.5`) the parallel-axis term `m·e²`
    /// shifts the inertia to the actual CoM, `e` being the offset from the
    /// geometric center.
    #[inline]
    pub fn inertia(&self) -> f64 {
        let e = (self.com - 0.5) * self.h;
        self.i_scale * self.m * ((self.w * self.w + self.h * self.h) / 12.0 + e * e)
    }

    /// Body-frame y of the engine (bottom) end relative to the CoM (negative).
    #[inline]
    pub fn bottom_off(&self) -> f64 {
        -self.com * self.h
    }

    /// Body-frame y of the nose (top) end relative to the CoM (positive).
    #[inline]
    pub fn top_off(&self) -> f64 {
        (1.0 - self.com) * self.h
    }

    /// Thrust-to-weight ratio using both boosters. >1 means it can hover.
    #[inline]
    pub fn thrust_to_weight(&self) -> f64 {
        (2.0 * self.tmax) / (self.m * self.g.abs())
    }

    /// Angular authority T·d / I (rad/s²) — the "twitchiness" number.
    #[inline]
    pub fn angular_authority(&self) -> f64 {
        (self.tmax * self.d) / self.inertia()
    }
}
