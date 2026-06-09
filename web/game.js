// Versus mode frontend: rendering + input only. Game logic + AI run in WASM.
import init, { GameSim } from './pkg/rocket_wasm.js';

const DT = 1 / 60;
const COL = { player: '#5ec8ff', opp: '#ff5e3a', good: '#3ddc97', dim: '#8aa0b8', gold: '#f2c14e' };

await init();
const game = new GameSim();

const cv = document.getElementById('cv');
const ctx = cv.getContext('2d');
const W = cv.width, H = cv.height;
let snap = game.snapshot();
const SCALE = W / snap.world_w;
const FUEL_MAX = (snap.rockets[0] && snap.rockets[0].fuel) || 100;
const toPx = (x, y) => [x * SCALE, H - y * SCALE];
function b2w(r, bx, by) {
  const s = Math.sin(r.th), c = Math.cos(r.th);
  return [r.x + bx * c - by * s, r.y + bx * s + by * c];
}

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
document.getElementById('reset').onclick = rematch;

// "Display AI controls" toggle: show the AI rocket's engine flames (in 2D) and its L/R thrust bars.
const showAIEl = document.getElementById('showAI');
const bThrustBlock = document.getElementById('bThrustBlock');
const showAI = () => showAIEl.checked;
showAIEl.addEventListener('change', () => { bThrustBlock.style.display = showAIEl.checked ? '' : 'none'; });
function rematch() { game.reset(); document.getElementById('banner').classList.remove('show'); applyParams(); }

// collision tuning (persists across rematches)
const restEl = document.getElementById('rest'), crushEl = document.getElementById('crush');
function applyParams() {
  game.set_param('rocket_restitution', parseFloat(restEl.value));
  game.set_param('rocket_crush_speed', parseFloat(crushEl.value));
}
restEl.oninput = () => { document.getElementById('restVal').textContent = parseFloat(restEl.value).toFixed(2); applyParams(); };
crushEl.oninput = () => { document.getElementById('crushVal').textContent = parseFloat(crushEl.value).toFixed(1); applyParams(); };
applyParams();

const legAnim = [0, 0];

function drawRocket(r, i) {
  const dead = r.status === 'dead', landed = r.status === 'landed';
  // indestructible TOP half is gold; destructible bottom half keeps the side colour.
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

  // flames: player always; AI only when "Display AI controls" is on.
  if (r.side === 0 || showAI()) {
    const thr = r.side === 0 ? game.player_throttle() : game.opp_throttle();
    const dpx = 0.28 * SCALE;
    ctx.fillStyle = '#ffb13a';
    if (thr[0] > 0.02) flame(-dpx, h / 2, thr[0]);
    if (thr[1] > 0.02) flame(dpx, h / 2, thr[1]);
  }
  ctx.restore();

  // fuel bar floating above the rocket (horizontal in screen space, green -> orange -> red as it drains)
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
  // deploy ring
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

function setBar(id, frac, max = 100) { document.getElementById(id).style.width = Math.min(100, frac / max * 100) + '%'; }
function updateHUD() {
  const a = snap.rockets[0], b = snap.rockets[1];
  const $ = id => document.getElementById(id);
  $('aStatus').textContent = a.status; $('bStatus').textContent = b.status;
  $('aFuel').textContent = a.fuel.toFixed(0); $('bFuel').textContent = b.fuel.toFixed(0);
  $('aStable').textContent = a.stable_time.toFixed(2) + ' s'; $('bStable').textContent = b.stable_time.toFixed(2) + ' s';
  setBar('aStb', a.stable_time, 1); setBar('bStb', b.stable_time, 1);
  const thr = game.player_throttle();
  setBar('aTl', thr[0] * 100); setBar('aTr', thr[1] * 100);
  if (showAI()) {
    const othr = game.opp_throttle();
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

let last = performance.now(), acc = 0;
function tick(n) {
  game.set_input(keys.left, keys.right);
  for (let i = 0; i < n; i++) game.step();
  snap = game.snapshot();
  render(); updateHUD();
}
function loop(now) {
  let frame = (now - last) / 1000; last = now;
  if (frame > 0.1) frame = 0.1;
  acc += frame;
  let steps = 0; while (acc >= DT) { acc -= DT; steps++; }
  tick(steps);
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);

// debug hook for headless tooling (rAF is throttled when the tab is hidden)
window.__rkt = { game, tick, press: (l, r) => { keys.left = l; keys.right = r; } };
