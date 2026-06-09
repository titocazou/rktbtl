"""End-to-end tests for the Python bindings + Gymnasium env."""

import numpy as np
import pytest

import rocket_lander
from rocket_lander import Sim, VecSim, Config, OBS_DIM


def test_config_defaults_and_overrides():
    c = Config()
    # hand-tuned defaults from the prototype
    assert abs(c.thrust_to_weight - 2.72) < 0.05
    assert abs(c.angular_authority - 5.4) < 0.2
    c2 = Config(m=2.0, infinite_fuel=True)
    d = c2.to_dict()
    assert d["m"] == 2.0 and d["infinite_fuel"] is True


def test_sim_step_shapes_and_determinism():
    sim = Sim(randomize=False)
    obs = sim.reset(seed=0)
    assert obs.shape == (OBS_DIM,)
    obs, reward, terminated, truncated, info = sim.step(1)
    assert obs.shape == (OBS_DIM,)
    assert isinstance(reward, float)
    assert isinstance(terminated, bool) and isinstance(truncated, bool)
    assert info["status"] == "flying"

    # determinism: same seed + same actions => same trajectory
    def rollout():
        s = Sim(randomize=False)
        s.reset(seed=42)
        last = None
        for i in range(200):
            last, *_ = s.step(i % 4)
        return last

    np.testing.assert_array_equal(rollout(), rollout())


def test_sim_terminates():
    sim = Sim(randomize=False)
    sim.reset(seed=0)
    done = False
    for _ in range(2000):
        _, _, term, trunc, _ = sim.step(0)  # coast -> falls and crashes
        if term or trunc:
            done = True
            break
    assert done


def test_fuel_bug_fixed_with_infinite_fuel():
    # finite fuel: thrust eventually stops responding (the reported bug)
    finite = Sim(randomize=False)
    finite.reset(seed=0)
    for _ in range(2000):
        finite.step(3)
    # infinite fuel: never runs dry
    inf = Sim(randomize=False, infinite_fuel=True)
    inf.reset(seed=0)
    for _ in range(2000):
        _, _, term, trunc, info = inf.step(3)
        if term or trunc:
            inf.reset(seed=0)
    assert inf.fuel > 0.0


def test_vecsim_batch():
    n = 64
    vec = VecSim(n, randomize=True, seed=0)
    assert vec.num_envs == n
    obs = vec.reset()
    assert obs.shape == (n, OBS_DIM)
    actions = np.random.randint(0, 4, size=n, dtype=np.int64)
    for _ in range(500):
        obs, rew, term, trunc, = vec.step(actions)
        assert obs.shape == (n, OBS_DIM)
        assert rew.shape == (n,)
        assert term.shape == (n,) and trunc.shape == (n,)
        assert np.isfinite(obs).all()


def test_gymnasium_env():
    import gymnasium as gym

    env = gym.make("RocketLander-v0")
    obs, info = env.reset(seed=0)
    assert env.observation_space.shape == (OBS_DIM,)
    assert env.action_space.n == 4
    total = 0.0
    for _ in range(300):
        obs, r, term, trunc, info = env.step(env.action_space.sample())
        total += r
        if term or trunc:
            obs, info = env.reset()
    assert np.isfinite(total)


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
