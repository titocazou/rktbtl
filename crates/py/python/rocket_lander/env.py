"""Gymnasium bindings on top of the Rust core.

`RocketLanderEnv` is a thin, hackable wrapper around `rocket_lander.Sim`; the
physics, reward and termination all live in Rust, so this file is just the Gym
plumbing (spaces, reset/step signatures, info dict).
"""

from __future__ import annotations

import numpy as np
import gymnasium as gym
from gymnasium import spaces

from ._core import Sim, VecSim, OBS_DIM


class RocketLanderEnv(gym.Env):
    """Single-rocket landing task.

    Action space (Discrete(4), binary boosters):
        0 = coast, 1 = left booster, 2 = right booster, 3 = both.
    Observation: float32 vector of length OBS_DIM (see Rocket::observation).
    """

    metadata = {"render_modes": []}

    def __init__(self, randomize: bool = True, max_steps: int = 1000, **cfg):
        super().__init__()
        self._kwargs = dict(randomize=randomize, max_steps=max_steps, **cfg)
        self.sim = Sim(**self._kwargs)
        self.action_space = spaces.Discrete(4)
        high = np.full(OBS_DIM, np.inf, dtype=np.float32)
        self.observation_space = spaces.Box(-high, high, dtype=np.float32)

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        obs = self.sim.reset(seed=seed)
        return obs.astype(np.float32), {}

    def step(self, action):
        obs, reward, terminated, truncated, info = self.sim.step(int(action))
        return obs.astype(np.float32), float(reward), bool(terminated), bool(truncated), info


def make_vec(num_envs: int, **kwargs) -> VecSim:
    """Convenience constructor for the fast vectorised batch env (no Gym
    overhead). Returns a `VecSim`; `step` takes an int array of shape [N]."""
    return VecSim(num_envs, **kwargs)
