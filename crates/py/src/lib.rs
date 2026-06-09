//! PyO3 bindings for `rocket-core`.
//!
//! Exposes three things to Python (module name: `rocket_lander`):
//!   * `Config`     — all physics tunables (kwargs-constructed; defaults match
//!                    the hand-tuned prototype).
//!   * `Sim`        — one rocket / one pad, a Gym-style `reset`/`step` loop.
//!   * `VecSim`     — N independent envs stepped in a single Rust call (no
//!                    per-env Python overhead) with auto-reset, for fast PPO.
//!
//! The Gymnasium `Env` wrapper lives in pure Python on top of `Sim`
//! (`python/rocket_lander/env.py`) so it stays easy to tweak.

use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use rocket_core::{reward, Action, Cfg, DeathCause, Rocket, Status, World, OBS_DIM};

/// Tiny deterministic xorshift RNG (no external dep) for spawn randomisation.
#[derive(Clone)]
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in [lo, hi).
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * u
    }
}

fn death_str(c: DeathCause) -> &'static str {
    match c {
        DeathCause::None => "none",
        DeathCause::Ground => "ground",
        DeathCause::Wall => "wall",
        DeathCause::Ceiling => "ceiling",
        DeathCause::FootImpact => "foot_impact",
        DeathCause::BodySlam => "body_slam",
        DeathCause::RocketHit => "rocket_hit",
        DeathCause::NonFinite => "non_finite",
    }
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Flying => "flying",
        Status::Landed => "landed",
        Status::Dead => "dead",
    }
}

/// Apply kwargs (`{field: value}`) onto a `Cfg`. Unknown keys raise.
fn apply_kwargs(cfg: &mut Cfg, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let Some(kw) = kwargs else { return Ok(()) };
    for (k, v) in kw.iter() {
        let key: String = k.extract()?;
        macro_rules! f {
            ($field:ident) => {{
                cfg.$field = v.extract()?;
                continue;
            }};
        }
        match key.as_str() {
            "dt" => f!(dt),
            "g" => f!(g),
            "m" => f!(m),
            "w" => f!(w),
            "h" => f!(h),
            "d" => f!(d),
            "tmax" => f!(tmax),
            "i_scale" => f!(i_scale),
            "fuel_max" => f!(fuel_max),
            "fuel_rate" => f!(fuel_rate),
            "infinite_fuel" => f!(infinite_fuel),
            "world_w" => f!(world_w),
            "world_h" => f!(world_h),
            "deploy_r" => f!(deploy_r),
            "stow_hysteresis" => f!(stow_hysteresis),
            "leg_len" => f!(leg_len),
            "leg_splay" => f!(leg_splay),
            "leg_k" => f!(leg_k),
            "leg_c" => f!(leg_c),
            "leg_mu" => f!(leg_mu),
            "hull_r" => f!(hull_r),
            "rocket_restitution" => f!(rocket_restitution),
            "rocket_crush_speed" => f!(rocket_crush_speed),
            "stable_need" => f!(stable_need),
            "v_stable" => f!(v_stable),
            "ang_stable" => f!(ang_stable),
            "om_stable" => f!(om_stable),
            "v_explode" => f!(v_explode),
            other => {
                return Err(PyTypeError::new_err(format!("unknown Config field: {other}")))
            }
        }
    }
    Ok(())
}

/// Physics configuration. Construct with keyword overrides:
/// `Config(m=2.0, infinite_fuel=True)`.
#[pyclass]
#[derive(Clone)]
struct Config {
    inner: Cfg,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (**kwargs))]
    fn new(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut inner = Cfg::default();
        apply_kwargs(&mut inner, kwargs)?;
        Ok(Config { inner })
    }

    /// Read-only derived quantities (handy for tuning UIs / sanity checks).
    #[getter]
    fn inertia(&self) -> f64 {
        self.inner.inertia()
    }
    #[getter]
    fn thrust_to_weight(&self) -> f64 {
        self.inner.thrust_to_weight()
    }
    #[getter]
    fn angular_authority(&self) -> f64 {
        self.inner.angular_authority()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        let c = &self.inner;
        d.set_item("dt", c.dt)?;
        d.set_item("g", c.g)?;
        d.set_item("m", c.m)?;
        d.set_item("w", c.w)?;
        d.set_item("h", c.h)?;
        d.set_item("d", c.d)?;
        d.set_item("tmax", c.tmax)?;
        d.set_item("i_scale", c.i_scale)?;
        d.set_item("fuel_max", c.fuel_max)?;
        d.set_item("fuel_rate", c.fuel_rate)?;
        d.set_item("infinite_fuel", c.infinite_fuel)?;
        d.set_item("world_w", c.world_w)?;
        d.set_item("world_h", c.world_h)?;
        d.set_item("deploy_r", c.deploy_r)?;
        d.set_item("leg_k", c.leg_k)?;
        d.set_item("leg_c", c.leg_c)?;
        d.set_item("leg_mu", c.leg_mu)?;
        d.set_item("hull_r", c.hull_r)?;
        d.set_item("rocket_restitution", c.rocket_restitution)?;
        d.set_item("rocket_crush_speed", c.rocket_crush_speed)?;
        Ok(d)
    }
}

fn parse_action(obj: &Bound<'_, PyAny>) -> PyResult<Action> {
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Action::from_discrete(i.rem_euclid(4) as u8));
    }
    if let Ok((tl, tr)) = obj.extract::<(f64, f64)>() {
        return Ok(Action::new(tl, tr));
    }
    Err(PyTypeError::new_err(
        "action must be an int in 0..4 (bit0=left, bit1=right) or a (tl, tr) tuple",
    ))
}

/// Spawn pose for a fresh episode, optionally randomised for RL exploration.
fn fresh_rocket(cfg: &Cfg, rng: &mut Rng, randomize: bool) -> Rocket {
    if !randomize {
        return Rocket::spawn(6.0, 22.0, cfg.fuel_max, 0);
    }
    let mut r = Rocket::spawn(
        rng.uniform(cfg.world_w * 0.15, cfg.world_w * 0.85),
        rng.uniform(cfg.world_h * 0.6, cfg.world_h * 0.85),
        cfg.fuel_max,
        0,
    );
    r.vx = rng.uniform(-1.5, 1.5);
    r.vy = rng.uniform(-1.0, 0.0);
    r.th = rng.uniform(-0.1, 0.1);
    r
}

/// Single-rocket RL environment.
#[pyclass]
struct Sim {
    world: World,
    prev: Rocket,
    steps: u32,
    max_steps: u32,
    randomize: bool,
    rng: Rng,
}

#[pymethods]
impl Sim {
    #[new]
    #[pyo3(signature = (config=None, max_steps=1000, randomize=false, seed=0, **kwargs))]
    fn new(
        config: Option<Config>,
        max_steps: u32,
        randomize: bool,
        seed: u64,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let mut cfg = config.map(|c| c.inner).unwrap_or_default();
        apply_kwargs(&mut cfg, kwargs)?;
        let mut rng = Rng::new(seed);
        let world = World::single(cfg);
        let mut sim = Sim {
            world,
            prev: fresh_rocket(&cfg, &mut rng, false),
            steps: 0,
            max_steps,
            randomize,
            rng,
        };
        sim.world.rockets[0] = sim.prev;
        Ok(sim)
    }

    /// Reset to a new episode. Returns the initial observation (shape [OBS_DIM]).
    #[pyo3(signature = (seed=None))]
    fn reset<'py>(&mut self, py: Python<'py>, seed: Option<u64>) -> Bound<'py, PyArray1<f64>> {
        if let Some(s) = seed {
            self.rng = Rng::new(s);
        }
        let cfg = self.world.cfg;
        let r = fresh_rocket(&cfg, &mut self.rng, self.randomize);
        self.world.rockets[0] = r;
        self.world.time = 0.0;
        self.world.steps = 0;
        self.prev = r;
        self.steps = 0;
        self.obs(py)
    }

    /// Advance one step. Returns `(obs, reward, terminated, truncated, info)`.
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        action: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let act = parse_action(action)?;
        self.prev = self.world.rockets[0];
        self.world.step(&[act]);
        self.steps += 1;

        let cfg = self.world.cfg;
        let pad = self.world.pads[0];
        let r = self.world.rockets[0];
        let rew = reward(&cfg, &pad, &self.prev, &r);
        let terminated = r.status != Status::Flying;
        let truncated = !terminated && self.steps >= self.max_steps;

        let info = PyDict::new_bound(py);
        info.set_item("status", status_str(r.status))?;
        info.set_item("death_cause", death_str(r.death_cause))?;
        info.set_item("landed", r.status == Status::Landed)?;
        info.set_item("fuel", r.fuel)?;
        info.set_item("steps", self.steps)?;

        let obs = self.obs(py);
        Ok(PyTuple::new_bound(
            py,
            &[
                obs.into_any(),
                rew.into_py(py).into_bound(py),
                terminated.into_py(py).into_bound(py),
                truncated.into_py(py).into_bound(py),
                info.into_any(),
            ],
        ))
    }

    // --- read-only state accessors (handy for rendering / debugging) ---
    #[getter]
    fn x(&self) -> f64 { self.world.rockets[0].x }
    #[getter]
    fn y(&self) -> f64 { self.world.rockets[0].y }
    #[getter]
    fn vx(&self) -> f64 { self.world.rockets[0].vx }
    #[getter]
    fn vy(&self) -> f64 { self.world.rockets[0].vy }
    #[getter]
    fn th(&self) -> f64 { self.world.rockets[0].th }
    #[getter]
    fn om(&self) -> f64 { self.world.rockets[0].om }
    #[getter]
    fn fuel(&self) -> f64 { self.world.rockets[0].fuel }
    #[getter]
    fn legs_out(&self) -> bool { self.world.rockets[0].legs_out }
    #[getter]
    fn status(&self) -> &'static str { status_str(self.world.rockets[0].status) }

    fn observation<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        self.obs(py)
    }
}

impl Sim {
    fn obs<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        let cfg = self.world.cfg;
        let o = self.world.rockets[0].observation(&cfg, &self.world.pads[0]);
        o.to_vec().into_pyarray_bound(py)
    }
}

/// Vectorised batch of N independent single-rocket envs, stepped in one Rust
/// call. Auto-resets terminated/truncated envs and returns the fresh observation
/// for them (SB3-style), so a training loop never stalls.
#[pyclass]
struct VecSim {
    cfg: Cfg,
    rockets: Vec<Rocket>,
    prev: Vec<Rocket>,
    pad: rocket_core::Pad,
    steps: Vec<u32>,
    max_steps: u32,
    randomize: bool,
    rngs: Vec<Rng>,
}

#[pymethods]
impl VecSim {
    #[new]
    #[pyo3(signature = (n, config=None, max_steps=1000, randomize=true, seed=0, **kwargs))]
    fn new(
        n: usize,
        config: Option<Config>,
        max_steps: u32,
        randomize: bool,
        seed: u64,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let mut cfg = config.map(|c| c.inner).unwrap_or_default();
        apply_kwargs(&mut cfg, kwargs)?;
        let template = World::single(cfg);
        let pad = template.pads[0];
        let mut rngs: Vec<Rng> = (0..n).map(|i| Rng::new(seed ^ (i as u64 + 1))).collect();
        let rockets: Vec<Rocket> = rngs
            .iter_mut()
            .map(|rng| fresh_rocket(&cfg, rng, randomize))
            .collect();
        Ok(VecSim {
            cfg,
            prev: rockets.clone(),
            rockets,
            pad,
            steps: vec![0; n],
            max_steps,
            randomize,
            rngs,
        })
    }

    #[getter]
    fn num_envs(&self) -> usize {
        self.rockets.len()
    }

    /// Reset all envs. Returns observations of shape `[N, OBS_DIM]`.
    fn reset<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        for i in 0..self.rockets.len() {
            self.rockets[i] = fresh_rocket(&self.cfg, &mut self.rngs[i], self.randomize);
            self.prev[i] = self.rockets[i];
            self.steps[i] = 0;
        }
        self.obs_batch(py)
    }

    /// Step every env with `actions` (int array, shape `[N]`, values 0..4).
    /// Returns `(obs[N,OBS_DIM], reward[N], terminated[N], truncated[N])`.
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray1<'py, i64>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let a = actions.as_slice()?;
        let n = self.rockets.len();
        if a.len() != n {
            return Err(PyTypeError::new_err(format!(
                "expected {n} actions, got {}",
                a.len()
            )));
        }

        let mut rewards = vec![0.0f64; n];
        let mut terminated = vec![false; n];
        let mut truncated = vec![false; n];

        for i in 0..n {
            let act = Action::from_discrete(a[i].rem_euclid(4) as u8);
            self.prev[i] = self.rockets[i];
            // step this env in isolation (one rocket, the shared static pad)
            rocket_core::step_isolated(&self.cfg, &self.pad, &mut self.rockets[i], act);
            self.steps[i] += 1;

            rewards[i] = reward(&self.cfg, &self.pad, &self.prev[i], &self.rockets[i]);
            terminated[i] = self.rockets[i].status != Status::Flying;
            truncated[i] = !terminated[i] && self.steps[i] >= self.max_steps;

            if terminated[i] || truncated[i] {
                // auto-reset; returned obs is the fresh episode's first obs
                self.rockets[i] = fresh_rocket(&self.cfg, &mut self.rngs[i], self.randomize);
                self.prev[i] = self.rockets[i];
                self.steps[i] = 0;
            }
        }

        let obs = self.obs_batch(py);
        Ok(PyTuple::new_bound(
            py,
            &[
                obs.into_any(),
                rewards.into_pyarray_bound(py).into_any(),
                PyArray1::from_vec_bound(py, terminated).into_any(),
                PyArray1::from_vec_bound(py, truncated).into_any(),
            ],
        ))
    }
}

impl VecSim {
    fn obs_batch<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        let n = self.rockets.len();
        let mut data = Vec::with_capacity(n * OBS_DIM);
        for r in &self.rockets {
            data.extend_from_slice(&r.observation(&self.cfg, &self.pad));
        }
        Array2::from_shape_vec((n, OBS_DIM), data)
            .expect("shape matches")
            .into_pyarray_bound(py)
    }
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Config>()?;
    m.add_class::<Sim>()?;
    m.add_class::<VecSim>()?;
    m.add("OBS_DIM", OBS_DIM)?;
    Ok(())
}
