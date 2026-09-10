// Authored interface for the Rust world engine. All terrain, movement, collision,
// and world rendering belong to Game; JavaScript only coordinates input and UI.
const $ = (id) => document.getElementById(id);
const canvas = $('world');
const STORAGE_KEY = 'wayfarer.exploration.v3';
const DEFAULT_SEED = 1337;
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const fmtDistance = (m) => m >= 1000 ? `${(m / 1000).toFixed(m >= 10000 ? 0 : 1)} km` : `${Math.round(m)} m`;
const niceName = (s = '') => String(s).replace(/[_-]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
const wrapAngle = (a) => ((a + Math.PI) % (Math.PI * 2) + Math.PI * 2) % (Math.PI * 2) - Math.PI;
let saved = {};
try { saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}'); } catch (_) { /* Storage is optional. */ }
const urlSeed = new URL(location.href).searchParams.get('seed');
const seed = clamp(Math.floor(Number(urlSeed || saved.seed) || DEFAULT_SEED), 1, 4294967295);
let game, state = {}, started = false, locked = false, modal = null;
let quality = clamp(Number(saved.quality ?? 1), 0, 2), sensitivity = clamp(Number(saved.sensitivity ?? 1), .35, 2);
let waypoint = saved.seed === seed && saved.waypoint ? saved.waypoint : null;
let keys = new Set(), touchMoves = new Set(), jumpQueued = false, dragLook = null;
let lastFrame = 0, lastHUD = 0, lastSaved = 0, frames = 0, fps = 0, fpsTime = 0;
let fatal = false, toastTimer, mapTimer, resizeTimer, initialReady = false;
let worldSize = 256000;
const map = { mode: 'local', x: 0, z: 0, span: 6000, selected: null, image: null, imageBounds: null, features: { sites: [], landmarks: [], roads: [] }, visibleFeatures: [], dragging: null, dirty: true };
const mapCanvas = $('map-canvas');
const mapContext = mapCanvas.getContext('2d');
document.body.classList.add('intro-open');
$('intro-seed').textContent = seed;
$('seed-input').value = seed;
$('quality-select').value = quality;
$('sensitivity').value = sensitivity;

function toast(message, duration = 3500) {
  $('toast').textContent = message;
  $('toast').classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').classList.remove('visible'), duration);
}

function saveProgress() {
  if (!game || !initialReady) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ seed, x: state.x, z: state.z, waypoint, quality, sensitivity }));
  } catch (_) { /* Private browsing can disable storage; the world still works. */ }
}

function resize() {
  if (!game) return;
  // wgpu owns the internal scene resolution; use CSS pixels to keep this cheap
  // on high-density displays and keep the pixel treatment intentional.
  const width = Math.max(1, Math.round(canvas.clientWidth));
  const height = Math.max(1, Math.round(canvas.clientHeight));
  game.resize(width, height);
  if (modal === 'map') resizeMap();
}

function clearMovement() {
  keys.clear(); touchMoves.clear(); jumpQueued = false; dragLook = null;
  document.querySelectorAll('[data-move]').forEach((button) => button.classList.remove('active'));
}

async function captureMouse() {
  if (!started || modal || matchMedia('(pointer: coarse)').matches) return;
  try {
    const pending = canvas.requestPointerLock?.();
    if (pending?.catch) await pending;
    if (!canvas.requestPointerLock) toast('Click and drag the view to look around.');
  } catch (_) {
    toast('Click and drag to look around. WASD still moves you.');
  }
}

function openModal(type) {
  clearMovement();
  if (document.pointerLockElement) document.exitPointerLock();
  modal = type;
  $('map-modal').classList.toggle('hidden', type !== 'map');
  $('settings-modal').classList.toggle('hidden', type !== 'settings');
  document.body.classList.add('modal-open');
  if (type === 'settings') $('close-settings').focus();
}

function closeModal() {
  modal = null;
  $('map-modal').classList.add('hidden');
  $('settings-modal').classList.add('hidden');
  document.body.classList.remove('modal-open');
  clearMovement();
  canvas.focus();
}

function openMap(mode) {
  if (!game || !initialReady) return;
  const changeMode = modal !== 'map' || map.mode !== mode;
  openModal('map');
  map.mode = mode;
  if (changeMode) {
    map.x = mode === 'local' ? state.x : 0;
    map.z = mode === 'local' ? state.z : 0;
    map.span = mode === 'local' ? 6000 : worldSize * 1.08;
    map.selected = waypoint ? { ...waypoint } : null;
  }
  $('map-title').textContent = mode === 'local' ? 'Local map' : 'World map';
  $('map-view-label').textContent = mode === 'local' ? 'LOCAL SURVEY' : 'THE ENDLESS MARCHES';
  $('local-tab').setAttribute('aria-selected', String(mode === 'local'));
  $('world-tab').setAttribute('aria-selected', String(mode === 'world'));
  $('map-seed').textContent = `SEED ${seed} · ${Math.round(worldSize / 1000)} × ${Math.round(worldSize / 1000)} KM`;
  resizeMap();
  if (changeMode && mode === 'world') {
    const w = mapCanvas.clientWidth, h = mapCanvas.clientHeight;
    map.span = worldSize * Math.max(w, h) / Math.min(w, h) * 1.04;
  }
  updateSelection();
  scheduleMapData(0);
  $('close-map').focus();
}

function resizeMap() {
  const rect = $('map-canvas-wrap').getBoundingClientRect();
  const dpr = Math.min(devicePixelRatio || 1, 2);
  mapCanvas.width = Math.max(1, Math.round(rect.width * dpr));
  mapCanvas.height = Math.max(1, Math.round(rect.height * dpr));
  map.dirty = true;
}

function mapGeometry() {
  const w = mapCanvas.clientWidth, h = mapCanvas.clientHeight;
  return { w, h, ppm: Math.max(w, h) / map.span };
}

function screenToWorld(px, py) {
  const { w, h, ppm } = mapGeometry();
  return { x: map.x + (px - w / 2) / ppm, z: map.z + (py - h / 2) / ppm };
}

function worldToScreen(x, z) {
  const { w, h, ppm } = mapGeometry();
  return { x: w / 2 + (x - map.x) * ppm, y: h / 2 + (z - map.z) * ppm };
}

function normalizeFeature(feature, kind) {
  return { ...feature, x: Number(feature.x), z: Number(feature.z), name: feature.name || niceName(feature.kind || kind), kind: feature.kind || kind };
}

function scheduleMapData(delay = 100) {
  clearTimeout(mapTimer);
  map.dirty = true;
  $('map-updating').classList.remove('hidden');
  mapTimer = setTimeout(() => {
    if (modal !== 'map' || !game) return;
    try {
      const res = map.mode === 'world' ? 384 : 320;
      // The engine returns RGBA for a north-up square of the requested span.
      const pixels = game.map_data(map.x, map.z, map.span, res);
      const image = document.createElement('canvas');
      image.width = res; image.height = res;
      image.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(pixels), res, res), 0, 0);
      map.image = image;
      map.imageBounds = { x: map.x, z: map.z, span: map.span };
      const features = game.features(map.x, map.z, map.span) || {};
      map.features = {
        sites: (features.sites || []).map((f) => normalizeFeature(f, 'settlement')),
        landmarks: (features.landmarks || []).map((f) => normalizeFeature(f, 'landmark')),
        roads: features.roads || [],
      };
      map.dirty = true;
    } catch (error) {
      console.error('Map generation failed', error);
      toast('The atlas could not be drawn. Close and reopen it to retry.');
    }
    $('map-updating').classList.add('hidden');
  }, delay);
}

function drawMap(now) {
  if (modal !== 'map' || (!map.dirty && now - (map.lastDraw || 0) < 100)) return;
  map.lastDraw = now; map.dirty = false;
  const ctx = mapContext;
  const { w, h, ppm } = mapGeometry();
  if (!w || !h) return;
  const dpr = mapCanvas.width / w;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = '#283629'; ctx.fillRect(0, 0, w, h);
  ctx.imageSmoothingEnabled = false;
  if (map.image && map.imageBounds) {
    const b = map.imageBounds;
    const p = worldToScreen(b.x - b.span / 2, b.z - b.span / 2);
    ctx.drawImage(map.image, p.x, p.y, b.span * ppm, b.span * ppm);
    ctx.fillStyle = '#d9c9950d'; ctx.fillRect(0, 0, w, h);
  }
  // A subtle survey grid gives scale without overwhelming the terrain colors.
  const gridStep = map.span > 80000 ? 20000 : map.span > 15000 ? 5000 : map.span > 4000 ? 1000 : 250;
  const left = screenToWorld(0, 0), right = screenToWorld(w, h);
  ctx.strokeStyle = '#edebc217'; ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = Math.ceil(left.x / gridStep) * gridStep; x < right.x; x += gridStep) { const p = worldToScreen(x, 0); ctx.moveTo(p.x, 0); ctx.lineTo(p.x, h); }
  for (let z = Math.ceil(left.z / gridStep) * gridStep; z < right.z; z += gridStep) { const p = worldToScreen(0, z); ctx.moveTo(0, p.y); ctx.lineTo(w, p.y); }
  ctx.stroke();
  ctx.strokeStyle = '#d9c99177'; ctx.lineWidth = map.mode === 'local' ? 1.8 : .75;
  ctx.beginPath();
  for (const road of map.features.roads) {
    const points = road.points || road;
    if (!Array.isArray(points)) continue;
    points.forEach((point, i) => {
      const p = worldToScreen(point.x ?? point[0], point.z ?? point[1]);
      if (i === 0) ctx.moveTo(p.x, p.y); else ctx.lineTo(p.x, p.y);
    });
  }
  ctx.stroke();
  const half = worldSize / 2;
  const worldCorner = worldToScreen(-half, -half);
  ctx.strokeStyle = '#e7deaa55'; ctx.lineWidth = 1; ctx.strokeRect(worldCorner.x, worldCorner.y, worldSize * ppm, worldSize * ppm);
  const visible = [];
  const occupied = [];
  const features = [...map.features.sites.map((f) => ({ ...f, isSite: true })), ...map.features.landmarks];
  for (const feature of features) {
    const p = worldToScreen(feature.x, feature.z);
    if (p.x < -10 || p.y < -10 || p.x > w + 10 || p.y > h + 10) continue;
    visible.push({ ...feature, px: p.x, py: p.y });
    const selected = map.selected && Math.hypot(feature.x - map.selected.x, feature.z - map.selected.z) < 1;
    ctx.fillStyle = selected ? '#fff2ac' : feature.isSite ? '#efdc9f' : '#d8dfbb';
    ctx.strokeStyle = '#263320'; ctx.lineWidth = 2;
    ctx.beginPath();
    if (feature.isSite) ctx.rect(p.x - 3, p.y - 3, 6, 6);
    else { ctx.moveTo(p.x, p.y - 3.5); ctx.lineTo(p.x + 3.5, p.y); ctx.lineTo(p.x, p.y + 3.5); ctx.lineTo(p.x - 3.5, p.y); ctx.closePath(); }
    ctx.stroke(); ctx.fill();
    const labelAllowed = selected || (feature.isSite || map.span < 15000) && !occupied.some((q) => Math.abs(q.x - p.x) < 105 && Math.abs(q.y - p.y) < 26);
    if (labelAllowed && occupied.length < 65) {
      ctx.font = `${selected ? 'bold ' : ''}11px ui-monospace, monospace`;
      ctx.textAlign = 'center'; ctx.lineWidth = 3.5;
      ctx.strokeStyle = '#20321fe6'; ctx.strokeText(feature.name, p.x, p.y - 9);
      ctx.fillStyle = selected ? '#fff1ad' : '#f0ebc9'; ctx.fillText(feature.name, p.x, p.y - 9);
      occupied.push(p);
    }
  }
  map.visibleFeatures = visible;
  if (waypoint) {
    const p = worldToScreen(waypoint.x, waypoint.z), player = worldToScreen(state.x || 0, state.z || 0);
    ctx.setLineDash([4, 5]); ctx.strokeStyle = '#f7dfa0aa'; ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(player.x, player.y); ctx.lineTo(p.x, p.y); ctx.stroke(); ctx.setLineDash([]);
    ctx.strokeStyle = '#fff0a9'; ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.arc(p.x, p.y, 10, 0, Math.PI * 2); ctx.stroke();
  }
  if (map.selected) {
    const p = worldToScreen(map.selected.x, map.selected.z);
    ctx.strokeStyle = '#fbebb4'; ctx.lineWidth = 1;
    ctx.strokeRect(p.x - 8, p.y - 8, 16, 16);
  }
  const player = worldToScreen(state.x || 0, state.z || 0);
  ctx.save(); ctx.translate(player.x, player.y); ctx.rotate(state.yaw || 0);
  ctx.beginPath(); ctx.moveTo(0, -9); ctx.lineTo(6, 6); ctx.lineTo(0, 3); ctx.lineTo(-6, 6); ctx.closePath();
  ctx.lineWidth = 3; ctx.strokeStyle = '#21381b'; ctx.stroke(); ctx.fillStyle = '#fff5c0'; ctx.fill(); ctx.restore();
  if (player.x > 15 && player.x < w - 15 && player.y > 15 && player.y < h - 15) {
    ctx.font = 'bold 11px ui-monospace, monospace'; ctx.textAlign = 'center'; ctx.lineWidth = 3; ctx.strokeStyle = '#22341e';
    ctx.strokeText('YOU', player.x, player.y + 20); ctx.fillStyle = '#f8efba'; ctx.fillText('YOU', player.x, player.y + 20);
  }
  const scaleOptions = [50, 100, 250, 500, 1000, 2500, 5000, 10000, 25000, 50000, 100000];
  const scale = scaleOptions.reduce((best, n) => Math.abs(n * ppm - 90) < Math.abs(best * ppm - 90) ? n : best, 1000);
  $('map-scale-line').style.width = `${scale * ppm}px`;
  $('map-scale-text').textContent = fmtDistance(scale);
  $('map-coordinates').textContent = `${fmtDistance(Math.abs(map.x))} ${map.x >= 0 ? 'E' : 'W'} · ${fmtDistance(Math.abs(map.z))} ${map.z >= 0 ? 'S' : 'N'}`;
}

function updateSelection() {
  const selected = map.selected;
  $('selection-name').textContent = selected?.name || 'The open road';
  $('selection-symbol').textContent = selected ? selected.kind === 'point' ? '⌖' : selected.isSite || ['city', 'town', 'village', 'settlement'].includes(String(selected.kind).toLowerCase()) ? '♜' : '◇' : '⌖';
  $('selection-detail').textContent = selected ? `${niceName(selected.kind || 'landmark')} · ${fmtDistance(Math.abs(selected.x))} ${selected.x >= 0 ? 'E' : 'W'}, ${fmtDistance(Math.abs(selected.z))} ${selected.z >= 0 ? 'S' : 'N'}` : 'Select a settlement, landmark, or point on the map to plan your journey.';
  if (selected) {
    const dist = Math.hypot(selected.x - state.x, selected.z - state.z);
    const minutes = Math.max(1, Math.round(dist / (5.5 * 60)));
    $('selection-distance').textContent = `${fmtDistance(dist)} away · ~${minutes} min on foot`;
  } else $('selection-distance').textContent = '';
  $('set-waypoint').disabled = !selected;
  $('fast-travel').disabled = !selected;
  $('fast-travel').innerHTML = selected?.kind === 'point' ? 'TRAVEL HERE <span>→</span>' : 'FAST TRAVEL <span>→</span>';
  $('clear-waypoint').classList.toggle('hidden', !waypoint);
  map.dirty = true;
}

function zoomMap(factor, px = mapCanvas.clientWidth / 2, py = mapCanvas.clientHeight / 2) {
  const before = screenToWorld(px, py);
  map.span = clamp(map.span * factor, 500, worldSize * 2.5);
  const after = screenToWorld(px, py);
  map.x = clamp(map.x + before.x - after.x, -worldSize / 2, worldSize / 2);
  map.z = clamp(map.z + before.z - after.z, -worldSize / 2, worldSize / 2);
  scheduleMapData(140);
}

function updateHUD(now) {
  const radians = Number(state.yaw || 0);
  const degrees = (radians * 180 / Math.PI % 360 + 360) % 360;
  const headings = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  $('heading-label').textContent = `${headings[Math.round(degrees / 45) % 8]} · ${String(Math.round(degrees) % 360).padStart(3, '0')}°`;
  const compassWidth = $('compass-track').parentElement.clientWidth;
  $('compass-track').innerHTML = Array.from({ length: 24 }, (_, i) => {
    const angle = i * 15;
    let diff = (angle - degrees + 540) % 360 - 180;
    if (Math.abs(diff) > 80) return '';
    const major = i % 3 === 0;
    return `<span class="${major ? '' : 'minor'}" style="left:${diff * compassWidth / 120}px">${major ? headings[i / 3] : '·'}</span>`;
  }).join('');
  const biome = niceName(state.biome || 'Wilderness');
  $('biome-label').textContent = biome;
  $('place-name').textContent = state.siteName || 'The open wilderness';
  const time = Number(state.dayTime ?? 9);
  $('clock').textContent = `${String(Math.floor(time) % 24).padStart(2, '0')}:${String(Math.floor(time % 1 * 60)).padStart(2, '0')}`;
  const stamina = Number(state.stamina ?? 100);
  $('stamina-fill').style.width = `${clamp(stamina, 0, 100)}%`;
  if (waypoint) {
    const distance = Math.hypot(waypoint.x - state.x, waypoint.z - state.z);
    $('journey-target').textContent = waypoint.name;
    $('journey-distance').textContent = distance < 40 ? 'You have reached your destination.' : `${fmtDistance(distance)} away · ${Math.max(1, Math.round(distance / 330))} min on foot`;
    $('target-icon').textContent = '◇';
    const bearing = Math.atan2(waypoint.x - state.x, -(waypoint.z - state.z));
    const angle = wrapAngle(bearing - radians);
    const markerVisible = Math.abs(angle) < .8 && distance >= 40 && !modal;
    $('destination-marker').classList.toggle('hidden', !markerVisible);
    if (markerVisible) {
      $('destination-marker').style.left = `${50 + Math.tan(angle) * 43}%`;
      $('marker-distance').textContent = fmtDistance(distance);
    }
  } else {
    $('journey-target').textContent = 'Follow your own path';
    $('journey-distance').textContent = `${fmtDistance(Number(state.walked || 0))} explored · Every horizon is within reach.`;
    $('target-icon').textContent = '↟';
    $('destination-marker').classList.add('hidden');
  }
  if (!$('diagnostics').classList.contains('hidden')) {
    $('diagnostics').textContent = `FANTASYLAND / RUST + WASM + WGPU\n${fps} FPS · ${Math.round(1000 / Math.max(fps, 1))} ms\n${state.chunkCount ?? '—'} chunks · ${Number(state.triangleCount || 0).toLocaleString()} triangles\nX ${Math.round(state.x || 0)}  Z ${Math.round(state.z || 0)}\nAltitude ${Math.round(state.altitude ?? state.y ?? 0)} m\n${biome} · Seed ${seed}\n${locked ? 'Pointer captured' : 'Drag to look'} · ${state.grounded ? 'Grounded' : 'Airborne'}`;
  }
  if (now - lastSaved > 5000) { saveProgress(); lastSaved = now; }
}

function renderFrame(now) {
  if (fatal) return;
  const dt = lastFrame ? Math.min((now - lastFrame) / 1000, .05) : 1 / 60;
  lastFrame = now;
  try {
    const moving = started && !modal && !document.hidden;
    let forward = moving ? Number(keys.has('KeyW') || keys.has('ArrowUp') || touchMoves.has('forward')) - Number(keys.has('KeyS') || keys.has('ArrowDown') || touchMoves.has('back')) : 0;
    let strafe = moving ? Number(keys.has('KeyD') || keys.has('ArrowRight') || touchMoves.has('right')) - Number(keys.has('KeyA') || keys.has('ArrowLeft') || touchMoves.has('left')) : 0;
    // Avoid diagonal movement being faster than walking straight.
    const length = Math.hypot(forward, strafe); if (length > 1) { forward /= length; strafe /= length; }
    const sprint = moving && (keys.has('ShiftLeft') || keys.has('ShiftRight') || touchMoves.has('sprint'));
    game.tick(dt, forward, strafe, sprint, moving && jumpQueued);
    jumpQueued = false;
    state = game.state();
    if (!initialReady && (!game.is_ready || game.is_ready())) {
      initialReady = true;
      $('loading-track').classList.add('hidden');
      $('loading-label').classList.add('hidden');
      $('start-button').classList.remove('hidden');
    }
    frames++;
    if (now - fpsTime >= 1000) { fps = Math.round(frames * 1000 / (now - fpsTime)); frames = 0; fpsTime = now; }
    if (now - lastHUD > 100) { updateHUD(now); lastHUD = now; }
    drawMap(now);
  } catch (error) { showFatal(error); return; }
  requestAnimationFrame(renderFrame);
}

function showFatal(error) {
  fatal = true;
  closeModal();
  console.error(error);
  clearMovement();
  if (document.pointerLockElement) document.exitPointerLock();
  $('intro').classList.remove('hidden');
  $('loading-track').classList.add('hidden');
  $('loading-label').classList.add('hidden');
  $('start-button').classList.add('hidden');
  $('load-error').classList.remove('hidden');
  $('load-error-message').textContent = `The wilderness could not be opened.\n${String(error?.message || error)}\n\nThis prototype needs a browser with WebGPU enabled. Try an up-to-date Chrome or Edge browser.`;
}

async function boot() {
  try {
    if (!navigator.gpu) throw new Error('WebGPU is unavailable in this browser.');
    $('loading-label').textContent = 'Preparing the world engine…';
    const { default: init, Game } = await import('./pkg/fantasy_land.js');
    await init();
    $('loading-label').textContent = 'Carving rivers, raising hills, finding a road…';
    game = await Game.create(canvas, seed);
    worldSize = Number(game.world_size());
    game.set_quality(quality);
    resize();
    if (saved.seed === seed && Number.isFinite(saved.x) && Number.isFinite(saved.z) && Math.abs(saved.x) < worldSize / 2 && Math.abs(saved.z) < worldSize / 2) game.teleport(saved.x, saved.z);
    state = game.state();
    // Exposed intentionally for integration checks and world-generation inspection.
    window.fantasyDebug = { game, get state() { return state; }, get map() { return map; }, get waypoint() { return waypoint; }, openMap, closeModal, saveProgress, version: 'wilderness-3' };
    requestAnimationFrame(renderFrame);
  } catch (error) { showFatal(error); }
}

$('start-button').addEventListener('click', () => {
  started = true;
  $('intro').classList.add('hidden');
  document.body.classList.remove('intro-open');
  canvas.focus();
  captureMouse();
  setTimeout(() => $('explore-hint').style.opacity = '.45', 15000);
});
$('retry-button').addEventListener('click', () => location.reload());
$('local-map-button').addEventListener('click', () => openMap('local'));
$('world-map-button').addEventListener('click', () => openMap('world'));
$('local-tab').addEventListener('click', () => openMap('local'));
$('world-tab').addEventListener('click', () => openMap('world'));
$('close-map').addEventListener('click', closeModal);
$('settings-button').addEventListener('click', () => { if (game && initialReady) openModal('settings'); });
$('close-settings').addEventListener('click', closeModal);
$('zoom-in').addEventListener('click', () => zoomMap(.65));
$('zoom-out').addEventListener('click', () => zoomMap(1.5));
$('center-map').addEventListener('click', () => { map.x = state.x; map.z = state.z; scheduleMapData(0); });
$('set-waypoint').addEventListener('click', () => {
  if (!map.selected) return;
  waypoint = { ...map.selected };
  saveProgress(); updateSelection();
  toast(`Waypoint set: ${waypoint.name}`);
  closeModal();
});
$('fast-travel').addEventListener('click', () => {
  if (!map.selected) return;
  const destination = { ...map.selected };
  game.teleport(destination.x, destination.z);
  state = game.state();
  waypoint = destination;
  saveProgress(); closeModal();
  toast(`Arrived at ${destination.name}. Your journey continues.`);
});
$('clear-waypoint').addEventListener('click', () => { waypoint = null; saveProgress(); updateSelection(); toast('Waypoint cleared.'); });
$('quality-select').addEventListener('change', (event) => { quality = Number(event.target.value); game?.set_quality(quality); saveProgress(); });
$('sensitivity').addEventListener('input', (event) => { sensitivity = Number(event.target.value); saveProgress(); });
$('time-setting').addEventListener('input', (event) => game?.set_time(Number(event.target.value)));
$('seed-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const nextSeed = clamp(Math.floor(Number($('seed-input').value) || DEFAULT_SEED), 1, 4294967295);
  const url = new URL(location.href); url.searchParams.set('seed', nextSeed);
  location.href = url.href;
});
$('return-to-spawn').addEventListener('click', () => {
  game.return_to_spawn(); state = game.state(); saveProgress(); closeModal(); toast('Back on the starting road.');
});

document.addEventListener('keydown', (event) => {
  if (['INPUT', 'SELECT', 'TEXTAREA'].includes(event.target.tagName)) {
    if (event.code === 'Escape') { closeModal(); event.preventDefault(); }
    return;
  }
  if (event.code === 'F3') { event.preventDefault(); $('diagnostics').classList.toggle('hidden'); return; }
  if (!started) return;
  if (event.code === 'Tab' || event.code === 'KeyM') {
    event.preventDefault();
    if (event.repeat) return;
    const mode = event.code === 'Tab' ? 'world' : 'local';
    if (modal === 'map' && map.mode === mode) closeModal(); else openMap(mode);
    return;
  }
  if (event.code === 'Escape') { if (modal) closeModal(); clearMovement(); return; }
  if (modal) return;
  if (['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Space', 'ShiftLeft', 'ShiftRight'].includes(event.code)) {
    event.preventDefault(); keys.add(event.code);
    if (event.code === 'Space' && !event.repeat) jumpQueued = true;
  }
});
document.addEventListener('keyup', (event) => keys.delete(event.code));
document.addEventListener('pointerlockchange', () => { locked = document.pointerLockElement === canvas; if (!locked) clearMovement(); });
document.addEventListener('pointerlockerror', () => { if (started && !modal) toast('Click and drag to look around. WASD moves you.'); });
document.addEventListener('mousemove', (event) => { if (locked && started && !modal && game) game.look(event.movementX * sensitivity, event.movementY * sensitivity); });
canvas.addEventListener('pointerdown', (event) => {
  if (!started || modal || !game) return;
  if (event.pointerType === 'mouse') captureMouse();
  if (!locked) { dragLook = { id: event.pointerId, x: event.clientX, y: event.clientY }; canvas.setPointerCapture(event.pointerId); }
});
canvas.addEventListener('pointermove', (event) => {
  if (dragLook?.id !== event.pointerId || locked || modal || !started) return;
  game.look((event.clientX - dragLook.x) * sensitivity, (event.clientY - dragLook.y) * sensitivity);
  dragLook.x = event.clientX; dragLook.y = event.clientY;
});
canvas.addEventListener('pointerup', () => dragLook = null);
canvas.addEventListener('pointercancel', () => dragLook = null);
canvas.addEventListener('contextmenu', (event) => event.preventDefault());
for (const button of document.querySelectorAll('[data-move]')) {
  const move = button.dataset.move;
  button.addEventListener('pointerdown', (event) => { event.preventDefault(); touchMoves.add(move); button.classList.add('active'); button.setPointerCapture(event.pointerId); });
  const release = () => { touchMoves.delete(move); button.classList.remove('active'); };
  button.addEventListener('pointerup', release); button.addEventListener('pointercancel', release); button.addEventListener('lostpointercapture', release);
}
mapCanvas.addEventListener('pointerdown', (event) => {
  mapCanvas.setPointerCapture(event.pointerId);
  map.dragging = { id: event.pointerId, x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, moved: false };
  mapCanvas.style.cursor = 'grabbing';
});
mapCanvas.addEventListener('pointermove', (event) => {
  const drag = map.dragging;
  if (!drag || drag.id !== event.pointerId) return;
  const { ppm } = mapGeometry();
  if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) > 5) drag.moved = true;
  map.x = clamp(map.x - (event.clientX - drag.x) / ppm, -worldSize / 2, worldSize / 2);
  map.z = clamp(map.z - (event.clientY - drag.y) / ppm, -worldSize / 2, worldSize / 2);
  drag.x = event.clientX; drag.y = event.clientY;
  map.dirty = true;
});
mapCanvas.addEventListener('pointerup', (event) => {
  const drag = map.dragging;
  if (!drag) return;
  map.dragging = null; mapCanvas.style.cursor = 'crosshair';
  if (drag.moved) { scheduleMapData(0); return; }
  const rect = mapCanvas.getBoundingClientRect();
  const px = event.clientX - rect.left, py = event.clientY - rect.top;
  const nearest = map.visibleFeatures.map((feature) => ({ feature, distance: Math.hypot(feature.px - px, feature.py - py) })).sort((a, b) => a.distance - b.distance)[0];
  if (nearest && nearest.distance < 18) map.selected = { ...nearest.feature };
  else {
    const point = screenToWorld(px, py);
    if (Math.abs(point.x) > worldSize / 2 || Math.abs(point.z) > worldSize / 2) { toast('This point lies beyond the surveyed world.'); return; }
    map.selected = { ...point, name: 'Uncharted wilderness', kind: 'point' };
  }
  updateSelection();
});
mapCanvas.addEventListener('pointercancel', () => { map.dragging = null; mapCanvas.style.cursor = 'crosshair'; scheduleMapData(); });
mapCanvas.addEventListener('wheel', (event) => {
  event.preventDefault();
  const rect = mapCanvas.getBoundingClientRect();
  zoomMap(Math.exp(clamp(event.deltaY, -150, 150) * .002), event.clientX - rect.left, event.clientY - rect.top);
}, { passive: false });
window.addEventListener('resize', () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(() => { resize(); if (modal === 'map') scheduleMapData(); }, 100); });
window.addEventListener('blur', clearMovement);
document.addEventListener('visibilitychange', () => { if (document.hidden) { clearMovement(); saveProgress(); } lastFrame = 0; });
window.addEventListener('pagehide', saveProgress);
boot();
