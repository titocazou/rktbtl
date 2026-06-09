// Frontend: rendering + input only. All physics is in the Rust/WASM core.
import init, { Sim } from './pkg/rocket_wasm.js';

const DT = 1 / 60;

await init();
const sim = new Sim();

const cv = document.getElementById('cv');
const ctx = cv.getContext('2d');
const W = cv.width, H = cv.height;

let snap = sim.snapshot();
let SCALE = W / snap.world_w;
const toPx = (x, y) => [x * SCALE, H - y * SCALE];
// body→world transform for a point given in body frame (bx right, by up)
function b2w(r, bx, by) {
  const s = Math.sin(r.th), c = Math.cos(r.th);
  return [r.x + bx * c - by * s, r.y + bx * s + by * c];
}

// --- input ---
const keys = { left: false, right: false };
addEventListener('keydown', e => {
  if (e.key === 'ArrowLeft') keys.left = true;
  if (e.key === 'ArrowRight') keys.right = true;
  if (e.key === 'r' || e.key === 'R') sim.reset();
});
addEventListener('keyup', e => {
  if (e.key === 'ArrowLeft') keys.left = false;
  if (e.key === 'ArrowRight') keys.right = false;
});
document.getElementById('reset').onclick = () => sim.reset();

// --- tuning (hidden behind a toggle; defaults are the locked-in values) ---
const tuneToggle = document.getElementById('tuneToggle');
const tunePanel = document.getElementById('tunePanel');
tuneToggle.onclick = () => {
  tunePanel.classList.toggle('open');
  tuneToggle.textContent = tunePanel.classList.contains('open')
    ? '▾ Tuning' : '▸ Tuning (defaults locked)';
};
function bindSlider(id, labId, param, fmt) {
  const el = document.getElementById(id), lab = document.getElementById(labId);
  el.oninput = () => {
    const v = parseFloat(el.value);
    sim.set_param(param, v);
    lab.textContent = fmt(v);
    updateRatios();
  };
}
bindSlider('sm', 'lm', 'm', v => v.toFixed(2));
bindSlider('st', 'lt', 'tmax', v => v.toFixed(0));
bindSlider('sd', 'ld', 'd', v => v.toFixed(2));
bindSlider('si', 'li', 'i_scale', v => v.toFixed(2));
bindSlider('sk', 'lk', 'leg_k', v => v.toFixed(0));
bindSlider('sc', 'lc', 'leg_c', v => v.toFixed(0));
bindSlider('smu', 'lmu', 'leg_mu', v => v.toFixed(2));
document.getElementById('sif').oninput = e => {
  const on = e.target.value === '1';
  sim.set_param('infinite_fuel', on ? 1 : 0);
  document.getElementById('lif').textContent = on ? 'ON' : 'off';
};
function updateRatios() {
  const r = sim.ratios();
  document.getElementById('rtw').textContent = r.thrust_to_weight.toFixed(2);
  document.getElementById('raa').textContent = r.angular_authority.toFixed(1) + ' rad/s²';
}
updateRatios();

// --- rendering ---
let legAnim = 0;
function drawRocket(r) {
  const [cx, cy] = toPx(r.x, r.y);
  const w = snap.body_w * SCALE, h = snap.body_h * SCALE;
  const dead = r.status === 'dead', landed = r.status === 'landed';
  // indestructible TOP half is gold; destructible bottom half is the hull colour.
  const topCol = dead ? '#ff5e3a' : landed ? '#3ddc97' : '#f2c14e';
  const botCol = dead ? '#ff5e3a' : landed ? '#3ddc97' : '#c9d6e3';

  // legs first (behind body), eased deploy/stow toward the actual physics feet
  const target = r.legs_out ? 1 : 0;
  legAnim += (target - legAnim) * 0.25;
  if (legAnim > 0.02) drawLegs(r);

  ctx.save();
  ctx.translate(cx, cy);
  ctx.rotate(-r.th);
  ctx.fillStyle = topCol; ctx.fillRect(-w / 2, -h / 2, w, h / 2); // top (indestructible)
  ctx.fillStyle = botCol; ctx.fillRect(-w / 2, 0, w, h / 2);      // bottom (destructible)
  ctx.fillStyle = topCol;                                          // nose = part of the gold top
  ctx.beginPath();
  ctx.moveTo(-w / 2, -h / 2);
  ctx.lineTo(0, -h / 2 - w * 0.7);
  ctx.lineTo(w / 2, -h / 2);
  ctx.closePath();
  ctx.fill();
  // flames from booster nozzles, scaled by applied throttle
  const thr = sim.throttle();
  const dpx = sim.get_param('d') * SCALE;
  ctx.fillStyle = '#ffb13a';
  if (thr[0] > 0.02) flame(-dpx, h / 2, thr[0]);
  if (thr[1] > 0.02) flame(dpx, h / 2, thr[1]);
  ctx.restore();
}

// Simple kickstands: one thin rod from each lower corner out to the foot,
// eased from stowed (at the corner) to the actual physics foot position.
function drawLegs(r) {
  ctx.strokeStyle = r.status === 'dead' ? '#ff5e3a' : '#8aa0b8';
  ctx.lineCap = 'round';
  ctx.lineWidth = 2;
  for (let i = 0; i < 2; i++) {
    const sign = i === 0 ? -1 : 1;
    const corner = b2w(r, sign * snap.body_w / 2, -snap.body_h / 2);
    const footFull = r.feet[i];
    const foot = [corner[0] + (footFull[0] - corner[0]) * legAnim,
                  corner[1] + (footFull[1] - corner[1]) * legAnim];
    const [c0, c1] = toPx(corner[0], corner[1]);
    const [f0, f1] = toPx(foot[0], foot[1]);
    ctx.beginPath(); ctx.moveTo(c0, c1); ctx.lineTo(f0, f1); ctx.stroke();
  }
}

function flame(bx, by, mag) {
  const len = mag * 26 + 4;
  ctx.beginPath();
  ctx.moveTo(bx - 4, by); ctx.lineTo(bx + 4, by); ctx.lineTo(bx, by + len);
  ctx.closePath(); ctx.fill();
}

function render() {
  ctx.clearRect(0, 0, W, H);
  // grid
  ctx.strokeStyle = '#1a222e'; ctx.lineWidth = 1;
  for (let gx = 0; gx <= snap.world_w; gx += 4) { const [px] = toPx(gx, 0); ctx.beginPath(); ctx.moveTo(px, 0); ctx.lineTo(px, H); ctx.stroke(); }
  for (let gy = 0; gy <= snap.world_h; gy += 4) { const [, py] = toPx(0, gy); ctx.beginPath(); ctx.moveTo(0, py); ctx.lineTo(W, py); ctx.stroke(); }

  for (const p of snap.pads) {
    const [plx, pty] = toPx(p.cx - p.half_w, p.y + p.thick);
    ctx.fillStyle = '#3ddc97';
    ctx.fillRect(plx, pty, p.half_w * 2 * SCALE, p.thick * SCALE);
    // pad support legs
    ctx.strokeStyle = '#1f3a30'; ctx.lineWidth = 2;
    const [l1] = toPx(p.cx - p.half_w * 0.6, 0);
    const [l2] = toPx(p.cx + p.half_w * 0.6, 0);
    const [, padBottom] = toPx(0, p.y);
    ctx.beginPath(); ctx.moveTo(l1, padBottom); ctx.lineTo(l1, H); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(l2, padBottom); ctx.lineTo(l2, H); ctx.stroke();
    // deploy ring
    const [dcx, dcy] = toPx(p.cx, p.y);
    ctx.strokeStyle = snap.rockets[0].legs_out ? 'rgba(61,220,151,.25)' : 'rgba(90,107,128,.3)';
    ctx.setLineDash([4, 6]); ctx.beginPath(); ctx.arc(dcx, dcy, snap.deploy_r * SCALE, 0, Math.PI * 2); ctx.stroke(); ctx.setLineDash([]);
  }
  for (const r of snap.rockets) drawRocket(r);
}

function updateHUD() {
  const r = snap.rockets[0];
  const $ = id => document.getElementById(id);
  $('px').textContent = r.x.toFixed(2);
  $('py').textContent = r.y.toFixed(2);
  $('vel').textContent = Math.hypot(r.vx, r.vy).toFixed(2);
  $('ang').textContent = (r.th * 180 / Math.PI).toFixed(1) + '°';
  $('om').textContent = r.om.toFixed(2);
  $('fu').textContent = r.fuel.toFixed(0);
  const thr = sim.throttle();
  $('tlv').textContent = (thr[0] * 100).toFixed(0) + '%'; $('tlb').style.width = (thr[0] * 100) + '%';
  $('trv').textContent = (thr[1] * 100).toFixed(0) + '%'; $('trb').style.width = (thr[1] * 100) + '%';
  $('lg').textContent = r.legs_out ? 'DEPLOYED' : 'stowed';
  $('stv').textContent = r.stable_time.toFixed(2) + ' s';
  $('stb').style.width = Math.min(100, r.stable_time * 100) + '%';
  const st = $('status'); st.className = 'status ' + r.status; st.textContent = r.status.toUpperCase();
}

// advance the sim by `n` fixed steps, then redraw once
function tick(n) {
  sim.set_input(keys.left, keys.right);
  for (let i = 0; i < n; i++) sim.step();
  snap = sim.snapshot();
  render();
  updateHUD();
}

// fixed-timestep loop (decouple sim from render rate)
let last = performance.now(), acc = 0;
function loop(now) {
  let frame = (now - last) / 1000; last = now;
  if (frame > 0.1) frame = 0.1;
  acc += frame;
  let steps = 0;
  while (acc >= DT) { acc -= DT; steps++; }
  tick(steps);
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);

// debug hook (lets headless tooling drive frames when rAF is throttled)
window.__rkt = { sim, tick, press: (l, r) => { keys.left = l; keys.right = r; } };
