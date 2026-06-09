"""rocket_lander — deterministic 2D rocket-lander sim with a Rust core.

The fast path is the compiled extension (`rocket_lander._core`):

    from rocket_lander import Sim, VecSim, Config

    sim = Sim(randomize=True)
    obs = sim.reset(seed=0)
    obs, reward, terminated, truncated, info = sim.step(1)   # 1 = left booster

For RL, use the Gymnasium env (requires `gymnasium`):

    import gymnasium as gym
    import rocket_lander            # registers "RocketLander-v0"
    env = gym.make("RocketLander-v0")
"""

from ._core import Sim, VecSim, Config, OBS_DIM

__all__ = ["Sim", "VecSim", "Config", "OBS_DIM", "RocketLanderEnv", "make_vec"]

# Gymnasium is optional: importing rocket_lander must work without it (e.g. for
# pure-Rust-core inference). The env + registration only load if it's present.
try:
    from .env import RocketLanderEnv, make_vec
    import gymnasium as _gym

    _gym.register(
        id="RocketLander-v0",
        entry_point="rocket_lander.env:RocketLanderEnv",
        max_episode_steps=1000,
    )
except ImportError:  # pragma: no cover - gymnasium not installed
    RocketLanderEnv = None
    make_vec = None
