// RKT.BTL frontend: rendering + input only. Physics, AI, and game logic run in WASM.
// One menu picks between Solo (single-rocket sandbox, Sim) and Vs AI (GameSim).
import init, { Sim, GameSim } from './pkg/rocket_wasm.js';

const DT = 1 / 60;
const COL = { player: '#5ec8ff', opp: '#ff5e3a', good: '#3ddc97', dim: '#8aa0b8', gold: '#f2c14e' };

await init();

const cv = document.getElementById('cv');
const ctx = cv.getContext('2d');
const W = cv.width, H = cv.height;
const $ = id => document.getElementById(id);

let mode = null;     // 'solo' | 'vs'
let engine = null;   // Sim (solo) or GameSim (vs)
let snap = null;
let SCALE = 1;
let FUEL_MAX = 100;
let running = false;
const legAnim = [0, 0];

const toPx = (x, y) => [x * SCALE, H - y * SCALE];
function b2w(r, bx, by) {
  const s = Math.sin(r.th), c = Math.cos(r.th);
  return [r.x + bx * c - by * s, r.y + bx * s + by * c];
}

// ---- input ----
const keys = { left: false, right: false };
addEventListener('keydown', e => {
  if (e.key === 'ArrowLeft' || e.key === 'a' || e.key === 'A') keys.left = true;
  if (e.key === 'ArrowRight' || e.key === 'd' || e.key === 'D') keys.right = true;
  if (e.key === 'r' || e.key === 'R') rematch();
  if ([' ', 'ArrowLeft', 'ArrowRight'].includes(e.key)) e.preventDefault();
});
addEventListener('keyup', e => {
  if (e.key === 'ArrowLeft' || e.key === 'a' || e.key === 'A') keys.left = false;
  if (e.key === 'ArrowRight' || e.key === 'd' || e.key === 'D') keys.right = false;
});

// ---- "Display AI controls" toggle (vs only) ----
const showAIEl = $('showAI');
const bThrustBlock = $('bThrustBlock');
const showAI = () => showAIEl.checked;
showAIEl.addEventListener('change', () => { bThrustBlock.style.display = showAIEl.checked ? '' : 'none'; });

// "Display tuning" toggle: show/hide the bounce + crush-speed sliders (vs mode).
const showTuneEl = $('showTune');
const tuneCard = $('tuneCard');
showTuneEl.addEventListener('change', () => { tuneCard.style.display = showTuneEl.checked ? '' : 'none'; });

// ---- vs collision sliders ----
const restEl = $('rest'), crushEl = $('crush');
restEl.oninput = () => { $('restVal').textContent = parseFloat(restEl.value).toFixed(2); applyParams(); };
crushEl.oninput = () => { $('crushVal').textContent = parseFloat(crushEl.value).toFixed(1); applyParams(); };

// ---- solo tuning sliders ----
function bindSolo(id, labId, param, fmt) {
  const el = $(id), lab = $(labId);
  el.oninput = () => {
    const v = parseFloat(el.value);
    if (mode === 'solo' && engine) engine.set_param(param, v);
    lab.textContent = fmt(v);
    updateRatios();
  };
}
bindSolo('sm', 'lm', 'm', v => v.toFixed(2));
bindSolo('st', 'lt', 'tmax', v => v.toFixed(0));
bindSolo('sd', 'ld', 'd', v => v.toFixed(2));
bindSolo('si', 'li', 'i_scale', v => v.toFixed(2));
bindSolo('sk', 'lk', 'leg_k', v => v.toFixed(0));
bindSolo('sc', 'lc', 'leg_c', v => v.toFixed(0));
bindSolo('smu', 'lmu', 'leg_mu', v => v.toFixed(2));
$('sif').oninput = e => {
  const on = e.target.value === '1';
  if (mode === 'solo' && engine) engine.set_param('infinite_fuel', on ? 1 : 0);
  $('lif').textContent = on ? 'ON' : 'off';
};

// push the current UI values into a freshly created engine
function applyParams() {
  if (!engine) return;
  if (mode === 'vs') {
    engine.set_param('rocket_restitution', parseFloat(restEl.value));
    engine.set_param('rocket_crush_speed', parseFloat(crushEl.value));
  } else if (mode === 'solo') {
    engine.set_param('m', parseFloat($('sm').value));
    engine.set_param('tmax', parseFloat($('st').value));
    engine.set_param('d', parseFloat($('sd').value));
    engine.set_param('i_scale', parseFloat($('si').value));
    engine.set_param('leg_k', parseFloat($('sk').value));
    engine.set_param('leg_c', parseFloat($('sc').value));
    engine.set_param('leg_mu', parseFloat($('smu').value));
    engine.set_param('infinite_fuel', $('sif').value === '1' ? 1 : 0);
  }
}

function updateRatios() {
  if (mode !== 'solo' || !engine) return;
  const r = engine.ratios();
  $('rtw').textContent = r.thrust_to_weight.toFixed(2);
  $('raa').textContent = r.angular_authority.toFixed(1) + ' rad/s²';
}

function rematch() {
  if (!engine) return;
  engine.reset();
  $('banner').classList.remove('show');
  applyParams();
}
$('reset').onclick = rematch;

// ---- menu / mode switching ----
function startMode(m) {
  if (engine && engine.free) engine.free();
  mode = m;
  engine = m === 'solo' ? new Sim() : new GameSim();
  snap = engine.snapshot();
  SCALE = W / snap.world_w;
  FUEL_MAX = (snap.rockets[0] && snap.rockets[0].fuel) || 100;
  legAnim[0] = legAnim[1] = 0;
  $('menu').style.display = 'none';
  $('game').style.display = '';
  $('vsPanel').style.display = m === 'vs' ? '' : 'none';
  $('soloPanel').style.display = m === 'solo' ? '' : 'none';
  $('reset').textContent = m === 'solo' ? 'RESET ⟳' : 'REMATCH ⟳';
  $('banner').classList.remove('show');
  applyParams();
  updateRatios();
  if (!running) { running = true; last = performance.now(); requestAnimationFrame(loop); }
}
$('modeSolo').onclick = () => startMode('solo');
$('modeVs').onclick = () => startMode('vs');
$('backMenu').onclick = () => {
  $('game').style.display = 'none';
  $('menu').style.display = '';
  if (engine && engine.free) engine.free();
  mode = null; engine = null; snap = null;
};

// ---- rendering ----
function throttleFor(r) {
  if (mode === 'solo') return engine.throttle();
  return r.side === 0 ? engine.player_throttle() : engine.opp_throttle();
}

function drawRocket(r, i) {
  const dead = r.status === 'dead', landed = r.status === 'landed';
  const topCol = dead ? COL.opp : landed ? COL.good : COL.gold;
  const botCol = dead ? COL.opp : landed ? COL.good : (r.side === 0 ? COL.player : COL.opp);
  const target = r.legs_out ? 1 : 0;
  legAnim[i] += (target - legAnim[i]) * 0.25;
  if (legAnim[i] > 0.02) drawLegs(r, i);

  const [cx, cy] = toPx(r.x, r.y);
  const w = snap.body_w * SCALE, h = snap.body_h * SCALE;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.rotate(-r.th);
  ctx.fillStyle = topCol; ctx.fillRect(-w / 2, -h / 2, w, h / 2); // top (indestructible)
  ctx.fillStyle = botCol; ctx.fillRect(-w / 2, 0, w, h / 2);      // bottom (destructible)
  ctx.fillStyle = topCol;                                          // nose = part of the gold top
  ctx.beginPath();
  ctx.moveTo(-w / 2, -h / 2); ctx.lineTo(0, -h / 2 - w * 0.7); ctx.lineTo(w / 2, -h / 2);
  ctx.closePath(); ctx.fill();

  // flames: player/solo always; AI only when "Display AI controls" is on.
  if (mode === 'solo' || r.side === 0 || showAI()) {
    const thr = throttleFor(r);
    const dpx = (mode === 'solo' ? engine.get_param('d') : 0.28) * SCALE;
    ctx.fillStyle = '#ffb13a';
    if (thr[0] > 0.02) flame(-dpx, h / 2, thr[0]);
    if (thr[1] > 0.02) flame(dpx, h / 2, thr[1]);
  }
  ctx.restore();

  // fuel bar floating above the rocket (green -> orange -> red as it drains)
  if (!dead) {
    const f = Math.max(0, Math.min(1, r.fuel / FUEL_MAX));
    const bw = w * 1.4, bh = 5;
    const bx = cx - bw / 2, by = cy - h * 0.5 - w * 0.7 - 12;
    ctx.fillStyle = 'rgba(6,9,16,.55)'; ctx.fillRect(bx - 1, by - 1, bw + 2, bh + 2);
    ctx.fillStyle = '#1a222e'; ctx.fillRect(bx, by, bw, bh);
    ctx.fillStyle = f > 0.5 ? COL.good : f > 0.2 ? '#ffb13a' : COL.opp;
    ctx.fillRect(bx, by, bw * f, bh);
  }
}

// Simple kickstands: one thin rod from each lower corner out to the foot.
function drawLegs(r, i) {
  ctx.strokeStyle = r.status === 'dead' ? COL.opp : COL.dim;
  ctx.lineCap = 'round';
  ctx.lineWidth = 2;
  for (let k = 0; k < 2; k++) {
    const sign = k === 0 ? -1 : 1;
    const corner = b2w(r, sign * snap.body_w / 2, -snap.body_h / 2);
    const footFull = r.feet[k];
    const foot = [corner[0] + (footFull[0] - corner[0]) * legAnim[i],
                  corner[1] + (footFull[1] - corner[1]) * legAnim[i]];
    const [c0, c1] = toPx(corner[0], corner[1]);
    const [f0, f1] = toPx(foot[0], foot[1]);
    ctx.beginPath(); ctx.moveTo(c0, c1); ctx.lineTo(f0, f1); ctx.stroke();
  }
}

function flame(bx, by, mag) {
  const len = mag * 26 + 4;
  ctx.beginPath(); ctx.moveTo(bx - 4, by); ctx.lineTo(bx + 4, by); ctx.lineTo(bx, by + len);
  ctx.closePath(); ctx.fill();
}

function drawPad(p) {
  const col = p.owner === 0 ? COL.player : COL.opp;
  const [plx, pty] = toPx(p.cx - p.half_w, p.y + p.thick);
  ctx.fillStyle = col;
  ctx.fillRect(plx, pty, p.half_w * 2 * SCALE, p.thick * SCALE);
  ctx.strokeStyle = p.owner === 0 ? 'rgba(94,200,255,.25)' : 'rgba(255,94,58,.25)'; ctx.lineWidth = 2;
  const [l1] = toPx(p.cx - p.half_w * 0.6, 0);
  const [l2] = toPx(p.cx + p.half_w * 0.6, 0);
  const [, padBottom] = toPx(0, p.y);
  ctx.beginPath(); ctx.moveTo(l1, padBottom); ctx.lineTo(l1, H); ctx.stroke();
  ctx.beginPath(); ctx.moveTo(l2, padBottom); ctx.lineTo(l2, H); ctx.stroke();
  const [dcx, dcy] = toPx(p.cx, p.y);
  ctx.strokeStyle = p.owner === 0 ? 'rgba(94,200,255,.18)' : 'rgba(255,94,58,.18)';
  ctx.setLineDash([4, 6]); ctx.beginPath(); ctx.arc(dcx, dcy, snap.deploy_r * SCALE, 0, Math.PI * 2); ctx.stroke(); ctx.setLineDash([]);
}

function render() {
  ctx.clearRect(0, 0, W, H);
  ctx.strokeStyle = '#1a222e'; ctx.lineWidth = 1;
  for (let gx = 0; gx <= snap.world_w; gx += 4) { const [px] = toPx(gx, 0); ctx.beginPath(); ctx.moveTo(px, 0); ctx.lineTo(px, H); ctx.stroke(); }
  for (let gy = 0; gy <= snap.world_h; gy += 4) { const [, py] = toPx(0, gy); ctx.beginPath(); ctx.moveTo(0, py); ctx.lineTo(W, py); ctx.stroke(); }
  for (const p of snap.pads) drawPad(p);
  snap.rockets.forEach((r, i) => drawRocket(r, i));
}

function setBar(id, frac, max = 100) { $(id).style.width = Math.min(100, frac / max * 100) + '%'; }

function updateVsHUD() {
  const a = snap.rockets[0], b = snap.rockets[1];
  $('aStatus').textContent = a.status; $('bStatus').textContent = b.status;
  $('aFuel').textContent = a.fuel.toFixed(0); $('bFuel').textContent = b.fuel.toFixed(0);
  $('aStable').textContent = a.stable_time.toFixed(2) + ' s'; $('bStable').textContent = b.stable_time.toFixed(2) + ' s';
  setBar('aStb', a.stable_time, 1); setBar('bStb', b.stable_time, 1);
  const thr = engine.player_throttle();
  setBar('aTl', thr[0] * 100); setBar('aTr', thr[1] * 100);
  if (showAI()) {
    const othr = engine.opp_throttle();
    setBar('bTl', othr[0] * 100); setBar('bTr', othr[1] * 100);
  }
  if (snap.outcome !== 'playing' && snap.outcome !== '') {
    const banner = $('banner'), text = $('bannerText');
    const map = { a_wins: ['YOU WIN', COL.player], b_wins: ['AI WINS', COL.opp], draw: ['DRAW', COL.dim] };
    const [msg, col] = map[snap.outcome];
    text.textContent = msg; text.style.color = col;
    banner.classList.add('show');
  }
}

function updateSoloHUD() {
  const r = snap.rockets[0];
  $('px').textContent = r.x.toFixed(2);
  $('py').textContent = r.y.toFixed(2);
  $('vel').textContent = Math.hypot(r.vx, r.vy).toFixed(2);
  $('ang').textContent = (r.th * 180 / Math.PI).toFixed(1) + '°';
  $('om').textContent = r.om.toFixed(2);
  $('fu').textContent = r.fuel.toFixed(0);
  const thr = engine.throttle();
  setBar('tlb', thr[0] * 100); setBar('trb', thr[1] * 100);
  $('lg').textContent = r.legs_out ? 'DEPLOYED' : 'stowed';
  $('stv').textContent = r.stable_time.toFixed(2) + ' s';
  setBar('stb', r.stable_time, 1);
  const st = $('status'); st.className = 'status ' + r.status; st.textContent = r.status.toUpperCase();
}

// ---- fixed-timestep loop ----
let last = performance.now(), acc = 0;
function tick(n) {
  engine.set_input(keys.left, keys.right);
  for (let i = 0; i < n; i++) engine.step();
  snap = engine.snapshot();
  render();
  if (mode === 'solo') updateSoloHUD(); else updateVsHUD();
}
function loop(now) {
  let frame = (now - last) / 1000; last = now;
  if (frame > 0.1) frame = 0.1;
  acc += frame;
  let steps = 0; while (acc >= DT) { acc -= DT; steps++; }
  if (engine && mode) tick(steps); else acc = 0;
  requestAnimationFrame(loop);
}

// debug hook for headless tooling (rAF is throttled when the tab is hidden)
window.__rkt = { start: startMode, tick, press: (l, r) => { keys.left = l; keys.right = r; } };
