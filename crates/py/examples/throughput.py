"""Quick sanity + throughput check for the vectorised env.

Run:  .venv/bin/python crates/py/examples/throughput.py
"""
import time
import numpy as np
from rocket_lander import VecSim, Sim

# --- single-env smoke ---
sim = Sim(randomize=True)
sim.reset(seed=0)
for _ in range(100):
    sim.step(np.random.randint(0, 4))
print(f"single-env ok  (final status={sim.status}, y={sim.y:.2f})")

# --- vectorised throughput ---
N = 4096
vec = VecSim(N, randomize=True, seed=0)
vec.reset()
STEPS = 1000
actions = np.random.randint(0, 4, size=N, dtype=np.int64)

t0 = time.perf_counter()
for _ in range(STEPS):
    obs, rew, term, trunc = vec.step(actions)
dt = time.perf_counter() - t0

total = N * STEPS
print(f"vectorised: {N} envs x {STEPS} steps = {total:,} env-steps in {dt:.2f}s")
print(f"throughput: {total / dt / 1e6:.1f} M env-steps/s")
print(f"obs shape {obs.shape}, all finite: {np.isfinite(obs).all()}")
