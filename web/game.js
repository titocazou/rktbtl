// RKT.BTL frontend: rendering + input only. Physics, AI, and game logic run in WASM.
// One menu picks between Solo (single-rocket sandbox, Sim) and Vs AI (GameSim).
import init, { Sim, GameSim } from './pkg/rocket_wasm.js?v=dev';

const DT = 1 / 60;
const COL = { player: '#5ec8ff', opp: '#ff5e3a', good: '#3ddc97', dim: '#8aa0b8', gold: '#f2c14e', dead: '#737b85' };

await init(new URL('./pkg/rocket_wasm_bg.wasm?v=dev', import.meta.url));

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
const TRAIL_MS = 3000;     // 3-second center-of-mass trail per rocket
let trails = [];           // trails[i] = [{x, y, t}], world coords + timestamp

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

// "Display tuning" toggle: show/hide the whole tuning panel (vs mode; solo shows it always).
const showTuneEl = $('showTune');
const tunePanel = $('tunePanel');
showTuneEl.addEventListener('change', () => { tunePanel.style.display = showTuneEl.checked ? '' : 'none'; });

// ---- collision sliders (vs only) ----
const restEl = $('rest'), crushEl = $('crush');
restEl.oninput = () => { const v = parseFloat(restEl.value); $('restVal').textContent = v.toFixed(2); if (engine) engine.set_param('rocket_restitution', v); };
crushEl.oninput = () => { const v = parseFloat(crushEl.value); $('crushVal').textContent = v.toFixed(1); if (engine) engine.set_param('rocket_crush_speed', v); };

// ---- body / leg tuning sliders (shared by solo and vs; set_param works on both engines) ----
function bindParam(id, labId, param, fmt) {
  const el = $(id), lab = $(labId);
  el.oninput = () => {
    const v = parseFloat(el.value);
    if (engine) engine.set_param(param, v);
    lab.textContent = fmt(v);
    updateRatios();
  };
}
bindParam('sm', 'lm', 'm', v => v.toFixed(2));
bindParam('st', 'lt', 'tmax', v => v.toFixed(0));
bindParam('sd', 'ld', 'd', v => v.toFixed(2));
bindParam('si', 'li', 'i_scale', v => v.toFixed(2));
bindParam('scom', 'lcom', 'com', v => v.toFixed(2));
bindParam('sk', 'lk', 'leg_k', v => v.toFixed(0));
bindParam('sc', 'lc', 'leg_c', v => v.toFixed(0));
bindParam('smu', 'lmu', 'leg_mu', v => v.toFixed(2));
bindParam('sve', 'lve', 'v_explode', v => v.toFixed(1));
$('sif').oninput = e => {
  const on = e.target.value === '1';
  if (engine) engine.set_param('infinite_fuel', on ? 1 : 0);
  $('lif').textContent = on ? 'ON' : 'off';
};

// Pull the engine's current parameters (the Rust Cfg::default, plus any live
// tweaks) into the sliders. config.rs is the single source of truth for the
// defaults; the UI just reflects whatever the engine reports.
function syncSlidersFromEngine() {
  if (!engine) return;
  const sync = (id, labId, param, fmt) => {
    const v = engine.get_param(param);
    $(id).value = v;
    $(labId).textContent = fmt(v);
  };
  sync('sm', 'lm', 'm', v => v.toFixed(2));
  sync('st', 'lt', 'tmax', v => v.toFixed(0));
  sync('sd', 'ld', 'd', v => v.toFixed(2));
  sync('si', 'li', 'i_scale', v => v.toFixed(2));
  sync('scom', 'lcom', 'com', v => v.toFixed(2));
  sync('sk', 'lk', 'leg_k', v => v.toFixed(0));
  sync('sc', 'lc', 'leg_c', v => v.toFixed(0));
  sync('smu', 'lmu', 'leg_mu', v => v.toFixed(2));
  sync('sve', 'lve', 'v_explode', v => v.toFixed(1));
  const inf = engine.get_param('infinite_fuel') !== 0;
  $('sif').value = inf ? '1' : '0';
  $('lif').textContent = inf ? 'ON' : 'off';
  if (mode === 'vs') {
    const e = engine.get_param('rocket_restitution');
    $('rest').value = e; $('restVal').textContent = e.toFixed(2);
    const cr = engine.get_param('rocket_crush_speed');
    $('crush').value = cr; $('crushVal').textContent = cr.toFixed(1);
  }
  updateRatios();
}

function updateRatios() {
  if (!engine || !engine.ratios) return;
  const r = engine.ratios();
  $('rtw').textContent = r.thrust_to_weight.toFixed(2);
  $('raa').textContent = r.angular_authority.toFixed(1) + ' rad/s²';
}

function rematch() {
  if (!engine) return;
  engine.reset(); // keeps the current cfg, so slider tweaks persist across a reset
  trails = [];
  $('banner').classList.remove('show');
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
  trails = [];
  $('menu').style.display = 'none';
  $('game').style.display = '';
  $('vsPanel').style.display = m === 'vs' ? '' : 'none';
  $('tunePanel').style.display = m === 'solo' ? '' : 'none'; // vs reveals it via "Display tuning"
  $('collisionTune').style.display = m === 'vs' ? '' : 'none'; // bounce/crush only matter in vs
  $('telemetryPanel').style.display = m === 'solo' ? '' : 'none';
  $('showTune').checked = false;
  $('showAI').checked = false;
  bThrustBlock.style.display = 'none';
  $('reset').textContent = m === 'solo' ? 'RESET ⟳' : 'REMATCH ⟳';
  $('banner').classList.remove('show');
  syncSlidersFromEngine();
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

// ---- CoM trails: a fading 3-second path of each rocket's center of mass ----
function recordTrails() {
  const now = performance.now();
  snap.rockets.forEach((r, i) => {
    const tr = trails[i] || (trails[i] = []);
    const prev = tr[tr.length - 1];
    if (prev && Math.hypot(r.x - prev.x, r.y - prev.y) > 4) tr.length = 0; // reset/respawn jump
    tr.push({ x: r.x, y: r.y, t: now });
    while (tr.length && tr[0].t < now - TRAIL_MS) tr.shift();
  });
}

function hexA(hex, a) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

function drawTrail(tr, side) {
  if (!tr || tr.length < 2) return;
  const col = side === 1 ? COL.opp : COL.player;
  ctx.lineCap = 'round'; ctx.lineJoin = 'round';
  for (let k = 1; k < tr.length; k++) {
    const a = k / tr.length;            // older samples fade out toward the tail
    ctx.strokeStyle = hexA(col, a * 0.55);
    ctx.lineWidth = 0.5 + a * 2;
    const [x0, y0] = toPx(tr[k - 1].x, tr[k - 1].y);
    const [x1, y1] = toPx(tr[k].x, tr[k].y);
    ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke();
  }
}

// ---- rendering ----
function throttleFor(r) {
  if (mode === 'solo') return engine.throttle();
  return r.side === 0 ? engine.player_throttle() : engine.opp_throttle();
}

function drawRocket(r, i) {
  const dead = r.status === 'dead', landed = r.status === 'landed';
  const topCol = dead ? COL.dead : landed ? COL.good : COL.gold;
  const botCol = dead ? COL.dead : landed ? COL.good : (r.side === 0 ? COL.player : COL.opp);
  const target = r.legs_out ? 1 : 0;
  legAnim[i] += (target - legAnim[i]) * 0.25;
  if (legAnim[i] > 0.02) drawLegs(r, i);

  const [cx, cy] = toPx(r.x, r.y);
  const w = snap.body_w * SCALE, h = snap.body_h * SCALE;
  const com = snap.com ?? 0.5;
  const noseY = -(1 - com) * h;   // nose (top) end relative to the CoM
  const engY = com * h;           // engine (bottom) end relative to the CoM
  const midY = (com - 0.5) * h;   // gold/engine split = the body's geometric midpoint
  ctx.save();
  ctx.translate(cx, cy);
  ctx.rotate(-r.th);
  ctx.fillStyle = topCol; ctx.fillRect(-w / 2, noseY, w, midY - noseY); // top (indestructible)
  ctx.fillStyle = botCol; ctx.fillRect(-w / 2, midY, w, engY - midY);   // bottom (destructible)
  ctx.fillStyle = topCol;                                                // nose = part of the gold top
  ctx.beginPath();
  ctx.moveTo(-w / 2, noseY); ctx.lineTo(0, noseY - w * 0.7); ctx.lineTo(w / 2, noseY);
  ctx.closePath(); ctx.fill();

  // flames: player/solo always; AI only when "Display AI controls" is on.
  if (mode === 'solo' || r.side === 0 || showAI()) {
    const thr = throttleFor(r);
    const dpx = (mode === 'solo' ? engine.get_param('d') : 0.28) * SCALE;
    ctx.fillStyle = '#ffb13a';
    if (thr[0] > 0.02) flame(-dpx, engY, thr[0]);
    if (thr[1] > 0.02) flame(dpx, engY, thr[1]);
  }
  ctx.restore();

  // fuel bar floating above the rocket (green -> orange -> red as it drains)
  if (!dead) {
    const f = Math.max(0, Math.min(1, r.fuel / FUEL_MAX));
    const bw = w * 1.4, bh = 5;
    const bx = cx - bw / 2, by = cy - (1 - com) * h - w * 0.7 - 12;
    ctx.fillStyle = 'rgba(6,9,16,.55)'; ctx.fillRect(bx - 1, by - 1, bw + 2, bh + 2);
    ctx.fillStyle = '#1a222e'; ctx.fillRect(bx, by, bw, bh);
    ctx.fillStyle = f > 0.5 ? COL.good : f > 0.2 ? '#ffb13a' : COL.opp;
    ctx.fillRect(bx, by, bw * f, bh);
  }
}

// Simple kickstands: one thin rod from each lower corner out to the foot.
function drawLegs(r, i) {
  ctx.strokeStyle = r.status === 'dead' ? COL.dead : COL.dim;
  ctx.lineCap = 'round';
  ctx.lineWidth = 2;
  for (let k = 0; k < 2; k++) {
    const sign = k === 0 ? -1 : 1;
    const corner = b2w(r, sign * snap.body_w / 2, -(snap.com ?? 0.5) * snap.body_h);
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
  // pad is a solid capsule (pill): full-width rounded slab, fully rounded ends
  const pw = p.half_w * 2 * SCALE, ph = p.thick * SCALE;
  ctx.fillStyle = col;
  ctx.beginPath();
  ctx.roundRect(plx, pty, pw, ph, ph / 2);
  ctx.fill();
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
  snap.rockets.forEach((r, i) => drawTrail(trails[i], r.side));
  snap.rockets.forEach((r, i) => drawRocket(r, i));
}

function setBar(id, frac, max = 100) { $(id).style.width = Math.min(100, frac / max * 100) + '%'; }

// TEMP diagnostic: log which of the four "stable" gates is blocking a landing.
let stableDbg = 0;
function logStableGates(a, pads) {
  if (a.status !== 'flying') return;
  const tp = pads.reduce((m, p) => (p.cx > m.cx ? p : m), pads[0]); // player A's target = right pad
  const padTop = tp.y + tp.thick;
  const footDown = f => a.legs_out && Math.abs(f[0] - tp.cx) <= tp.half_w && (padTop - f[1]) > 0 && f[1] >= tp.y;
  const feetN = (footDown(a.feet[0]) ? 1 : 0) + (footDown(a.feet[1]) ? 1 : 0);
  const speed = Math.hypot(a.vx, a.vy);
  if ((stableDbg = (stableDbg + 1) % 15) !== 0) return; // ~4 logs/sec
  console.log('stable gates →',
    'feet', feetN + '/2', feetN >= 2 ? 'ok' : 'FAIL',
    '| speed', speed.toFixed(3), speed <= 0.4 ? 'ok' : 'FAIL',
    '| |angle|', Math.abs(a.th).toFixed(3), Math.abs(a.th) <= 0.20 ? 'ok' : 'FAIL',
    '| |om|', Math.abs(a.om).toFixed(3), Math.abs(a.om) <= 0.4 ? 'ok' : 'FAIL');
}

function updateVsHUD() {
  const a = snap.rockets[0], b = snap.rockets[1];
  logStableGates(a, snap.pads);
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
  recordTrails();
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
