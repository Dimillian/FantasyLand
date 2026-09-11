// Authored interface for the Rust world engine. All terrain, movement, collision,
// and world rendering belong to Game; JavaScript only coordinates input and UI.
const $ = (id) => document.getElementById(id);
const canvas = $('world');
const STORAGE_KEY = 'wayfarer.exploration.v4';
const PREVIOUS_STORAGE_KEY = 'wayfarer.exploration.v3';
const DEFAULT_SEED = 1337;
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const fmtDistance = (m) => m >= 1000 ? `${(m / 1000).toFixed(m >= 10000 ? 0 : 1)} km` : `${Math.round(m)} m`;
const niceName = (s = '') => String(s).replace(/[_-]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
const wrapAngle = (a) => ((a + Math.PI) % (Math.PI * 2) + Math.PI * 2) % (Math.PI * 2) - Math.PI;
let saved = {};
try {
  const current = localStorage.getItem(STORAGE_KEY);
  if (current) saved = JSON.parse(current);
  else {
    // Terrain/hydrology changed in v4. Keep preferences, never old coordinates.
    const previous = JSON.parse(localStorage.getItem(PREVIOUS_STORAGE_KEY) || '{}');
    saved = { seed: previous.seed, quality: previous.quality, sensitivity: previous.sensitivity };
  }
} catch (_) { /* Storage is optional. */ }
const urlSeed = new URL(location.href).searchParams.get('seed');
const seed = clamp(Math.floor(Number(urlSeed || saved.seed) || DEFAULT_SEED), 1, 4294967295);
let game, state = {}, started = false, locked = false, modal = null;
let focusedLook = false, lockPending = false, lockTimer = null, lastMouse = null;
let pointerLockFallback = false, lockEpoch = 0;
let quality = clamp(Number(saved.quality ?? 1), 0, 2), sensitivity = clamp(Number(saved.sensitivity ?? 1), .35, 2);
// Existing v4 saves acquire the new visual preferences without moving the player.
let filterMode = [0, 1, 2, 3].includes(Number(saved.filterMode)) ? Number(saved.filterMode) : 1;
let filterStrength = Number.isFinite(Number(saved.filterStrength ?? 1)) ? clamp(Number(saved.filterStrength ?? 1), 0, 1.5) : 1;
const RESOLUTION_OPTIONS = [0, 1, 120, 180, 240, 360, 450, 720, 1080];
let renderResolution = RESOLUTION_OPTIONS.includes(Number(saved.renderResolution ?? 0)) ? Number(saved.renderResolution ?? 0) : 0;
let asciiScale = [1, 2, 3].includes(Number(saved.asciiScale ?? 2)) ? Number(saved.asciiScale ?? 2) : 2;
let asciiPalette = [0, 1, 2].includes(Number(saved.asciiPalette ?? 0)) ? Number(saved.asciiPalette ?? 0) : 0;
// Density is a renderer preference: preserve existing v4 world progress.
let groundCoverDensity = Number.isFinite(Number(saved.groundCoverDensity ?? 4)) ? clamp(Number(saved.groundCoverDensity ?? 4), 0, 4) : 4;
let waypoint = saved.seed === seed && saved.waypoint ? saved.waypoint : null;
let keys = new Set(), touchMoves = new Set(), jumpQueued = false, dragLook = null;
let lastFrame = 0, lastHUD = 0, lastSaved = 0, frames = 0, fps = 0, fpsTime = 0;
let fatal = false, toastTimer, mapTimer, resizeTimer, initialReady = false;
let worldSize = 384000;
const savedAtlas = saved.seed === seed && saved.atlas && Number.isFinite(saved.atlas.span) ? saved.atlas : null;
const map = { initialized: !!savedAtlas, x: savedAtlas?.x || 0, z: savedAtlas?.z || 0, span: savedAtlas?.span || 6000, selected: null, image: null, imageBounds: null, features: { sites: [], landmarks: [], roads: [], routes: null }, visibleFeatures: [], dragging: null, dirty: true };
const mapCanvas = $('map-canvas');
const mapContext = mapCanvas.getContext('2d');
document.body.classList.add('intro-open');
$('intro-seed').textContent = seed;
$('seed-input').value = seed;
$('quality-select').value = quality;
$('sensitivity').value = sensitivity;
$('render-resolution').value = String(renderResolution);
updateGroundCoverControls();
updateFilterControls();
updateAsciiControls();

function toast(message, duration = 3500) {
  $('toast').textContent = message;
  $('toast').classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').classList.remove('visible'), duration);
}

function saveProgress() {
  if (!game || !initialReady) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ seed, x: state.x, z: state.z, waypoint, quality, sensitivity, filterMode, filterStrength, renderResolution, asciiScale, asciiPalette, groundCoverDensity, atlas: map.initialized ? { x: map.x, z: map.z, span: map.span } : null }));
  } catch (_) { /* Private browsing can disable storage; the world still works. */ }
}

function updateGroundCoverControls() {
  const percent = Math.round(groundCoverDensity * 100);
  const label = groundCoverDensity === 0 ? 'Off' : `${percent}% · ${Number(groundCoverDensity.toFixed(2))}×`;
  $('ground-cover-density').value = String(percent);
  $('ground-cover-density-value').textContent = label;
  $('ground-cover-density').setAttribute('aria-valuetext', groundCoverDensity === 0 ? 'Off' : `${percent} percent, ${Number(groundCoverDensity.toFixed(2))} times density`);
}

function applyGroundCoverDensity() {
  updateGroundCoverControls();
  if (game) game.set_ground_cover_density(groundCoverDensity);
}

function updateFilterControls() {
  $('filter-select').value = String(filterMode);
  $('filter-strength').value = String(Math.round(filterStrength * 100));
  $('filter-strength-value').textContent = `${Math.round(filterStrength * 100)}%`;
  $('filter-strength').disabled = filterMode === 0 || filterMode === 3;
  $('filter-strength-row').classList.toggle('setting-inactive', filterMode === 0 || filterMode === 3);
  $('ascii-options').classList.toggle('hidden', filterMode !== 3);
  $('filter-description').textContent = [
    'Unfiltered, crisp scene colors.',
    'Soft light around bright surfaces.',
    'Classic monitor texture and soft glow.',
    'The world drawn entirely with colored glyphs.',
  ][filterMode];
}

function applyFilter() {
  updateFilterControls();
  if (game) game.set_filter(filterMode, filterStrength);
}

function updateAsciiControls() {
  $('ascii-scale').value = String(asciiScale);
  $('ascii-palette').value = String(asciiPalette);
}

function applyAscii() {
  updateAsciiControls();
  if (game) game.set_ascii(asciiScale, asciiPalette);
}

function updateRenderDimensions() {
  if (!game) return;
  const actual = game.render_resolution();
  if (actual && actual.length >= 2) $('render-dimensions').textContent = `Actual ${actual[0]} × ${actual[1]}`;
}

function resize() {
  if (!game) return;
  // wgpu owns the internal scene resolution; use CSS pixels to keep this cheap
  // on high-density displays and keep the pixel treatment intentional.
  const width = Math.max(1, Math.round(canvas.clientWidth));
  const height = Math.max(1, Math.round(canvas.clientHeight));
  game.resize(width, height);
  updateRenderDimensions();
  if (modal === 'map') resizeMap();
}

function clearMovement() {
  keys.clear(); touchMoves.clear(); jumpQueued = false; dragLook = null;
  document.querySelectorAll('[data-move]').forEach((button) => button.classList.remove('active'));
}

// Keep requestPointerLock on the original click/Enter user-gesture stack. No
// promise await, animation-frame callback, or pointer-capture competes with it.
function updateFocusHint() {
  const show = started && !modal && !locked && !fatal && !matchMedia('(pointer: coarse)').matches;
  $('focus-hint').classList.toggle('hidden', !show);
  document.body.classList.toggle('mouse-focused', focusedLook && !modal);
  $('focus-hint-text').textContent = focusedLook ? 'Mouse look active · Esc releases' : 'Click the world to look around';
  $('focus-hint').querySelector('small').textContent = focusedLook && pointerLockFallback
    ? 'Window edges limit turning. Click to try full capture.'
    : focusedLook ? 'WASD move · Shift run · Space jump' : 'WASD move · Shift run · Space jump';
}

function lockFailed(epoch = lockEpoch) {
  if (epoch !== lockEpoch || !lockPending || !focusedLook) return;
  clearTimeout(lockTimer);
  lockPending = false;
  if (!started || modal || fatal || document.pointerLockElement === canvas) return;
  // Browsers that block true pointer lock still provide click-to-focus look.
  // It uses ordinary mouse coordinates and necessarily stops at window edges.
  pointerLockFallback = true;
  focusedLook = true;
  updateFocusHint();
}

function captureMouse(event) {
  if (!started || modal || fatal || matchMedia('(pointer: coarse)').matches) return;
  canvas.focus({ preventScroll: true });
  focusedLook = true;
  lastMouse = event && Number.isFinite(event.clientX) ? { x: event.clientX, y: event.clientY } : null;
  updateFocusHint();
  if (document.pointerLockElement === canvas || lockPending) return;
  lockPending = true;
  const epoch = ++lockEpoch;
  if (typeof canvas.requestPointerLock !== 'function') { lockFailed(); return; }
  try {
    const result = canvas.requestPointerLock();
    // Older implementations return void; newer implementations return a promise.
    if (result && typeof result.catch === 'function') result.catch(() => lockFailed(epoch));
    lockTimer = setTimeout(() => {
      if (document.pointerLockElement !== canvas) lockFailed(epoch);
    }, 1200);
  } catch (_) { lockFailed(epoch); }
}

function releaseMouse() {
  lockEpoch++;
  focusedLook = false; lockPending = false; lastMouse = null;
  clearTimeout(lockTimer);
  clearMovement();
  if (document.pointerLockElement) document.exitPointerLock();
  updateFocusHint();
}

function openModal(type) {
  if (!game || !initialReady) return;
  modal = type;
  releaseMouse();
  for (const name of ['map', 'settings', 'bag', 'character', 'skills']) $(name + '-modal').classList.toggle('hidden', name !== type);
  for (const name of ['map', 'settings', 'bag', 'character', 'skills']) $(name + '-button').setAttribute('aria-expanded', String(name === type));
  document.body.classList.add('modal-open');
  if (type === 'settings') {
    $('time-setting').value = Number(state.dayTime ?? 9);
    $('time-setting-label').textContent = formatTime(state.dayTime);
  }
  if (type === 'character') updateCharacter();
  $(type + '-modal').querySelector('[data-close]').focus({ preventScroll: true });
}

function closeModal() {
  modal = null;
  for (const name of ['map', 'settings', 'bag', 'character', 'skills']) $(name + '-modal').classList.add('hidden');
  for (const name of ['map', 'settings', 'bag', 'character', 'skills']) $(name + '-button').setAttribute('aria-expanded', 'false');
  document.body.classList.remove('modal-open');
  clearMovement();
  canvas.focus({ preventScroll: true });
  updateFocusHint();
  saveProgress();
}

function openMap() {
  if (!game || !initialReady) return;
  openModal('map');
  // One atlas: opening it or pressing M/Tab never resets the zoom or location.
  if (!map.initialized) {
    map.x = state.x; map.z = state.z; map.span = 6000;
    map.initialized = true;
  }
  if (!map.selected && waypoint) map.selected = { ...waypoint };
  $('map-seed').textContent = `SEED ${seed} · ${Math.round(worldSize / 1000)} × ${Math.round(worldSize / 1000)} KM`;
  resizeMap();
  updateSelection();
  scheduleMapData(0);
}

function fitWorld() {
  const { w, h } = mapGeometry();
  map.x = 0; map.z = 0;
  map.span = worldSize * Math.max(w, h) / Math.max(Math.min(w, h), 1) * 1.04;
  scheduleMapData(0);
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
      const res = map.span > 25000 ? 384 : 320;
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
        routes: Array.isArray(features.routes) ? features.routes : null,
      };
      map.dirty = true;
    } catch (error) {
      console.error('Map generation failed', error);
      toast('The atlas could not be drawn. Close and reopen it to retry.');
    }
    $('map-updating').classList.add('hidden');
  }, delay);
}

// All widths are CSS pixels: zoom changes geography, not road thickness.
// An explicitly empty routes array is authoritative; only older engines fall
// back to roads, whose unclassified lines are treated as main roads.
function drawMapRoutes(ctx, features, span) {
  const routes = Array.isArray(features.routes)
    ? features.routes
    : (features.roads || []).map((road) => ({ kind: 'main', points: road.points || road }));
  const scale = clamp(15000 / Math.max(span, 1), .65, 1);
  const styles = [
    { kind: 'trail', color: '#a8ad8770', width: .7, dash: [2, 4] },
    { kind: 'lane', color: '#b5a27b85', width: Math.max(.8, scale), dash: [] },
    { kind: 'main', color: '#c5a36eaa', width: 1.5 * scale, dash: [] },
  ];
  ctx.save();
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  for (const style of styles) {
    if (style.kind === 'trail' && span > 18000) continue;
    ctx.strokeStyle = style.color;
    ctx.lineWidth = style.width;
    ctx.setLineDash(style.dash);
    ctx.beginPath();
    let hasSegments = false;
    for (const route of routes) {
      if (route.kind !== style.kind || !Array.isArray(route.points) || route.points.length < 2) continue;
      route.points.forEach((point, i) => {
        const position = worldToScreen(point.x ?? point[0], point.z ?? point[1]);
        if (i === 0) ctx.moveTo(position.x, position.y);
        else ctx.lineTo(position.x, position.y);
      });
      hasSegments = true;
    }
    if (hasSegments) ctx.stroke();
  }
  ctx.restore();
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
  ctx.fillStyle = '#173b55'; ctx.fillRect(0, 0, w, h);
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
  drawMapRoutes(ctx, map.features, map.span);
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
  $('map-view-label').textContent = map.span < 12000 ? 'THE SURROUNDING WILDS' : map.span < 90000 ? 'THE MARCHES' : 'THE KNOWN WORLD';
  $('map-coordinates').textContent = `${fmtDistance(Math.abs(map.x))} ${map.x >= 0 ? 'E' : 'W'} · ${fmtDistance(Math.abs(map.z))} ${map.z >= 0 ? 'S' : 'N'}`;
}

function updateSelection() {
  const selected = map.selected;
  $('selection-name').textContent = selected?.name || 'The open road';
    $('selection-detail').textContent = selected ? `${niceName(selected.kind || 'landmark')} · ${fmtDistance(Math.abs(selected.x))} ${selected.x >= 0 ? 'E' : 'W'}, ${fmtDistance(Math.abs(selected.z))} ${selected.z >= 0 ? 'S' : 'N'}` : 'Select a settlement, landmark, or point on the map to plan your journey.';
  if (selected) {
    const dist = Math.hypot(selected.x - state.x, selected.z - state.z);
    const minutes = Math.max(1, Math.round(dist / (5.5 * 60)));
    $('selection-distance').textContent = `${fmtDistance(dist)} away · ~${minutes} min on foot`;
  } else $('selection-distance').textContent = '';
  $('set-waypoint').disabled = !selected;
  $('fast-travel').disabled = !selected;
  $('fast-travel').innerHTML = selected?.kind === 'point' ? 'Travel here <span>→</span>' : 'Fast travel <span>→</span>';
  $('clear-waypoint').classList.toggle('hidden', !waypoint);
  map.dirty = true;
}

function zoomMap(factor, px = mapCanvas.clientWidth / 2, py = mapCanvas.clientHeight / 2) {
  const before = screenToWorld(px, py);
  map.span = clamp(map.span * factor, 300, worldSize * 3.5);
  const after = screenToWorld(px, py);
  map.x = clamp(map.x + before.x - after.x, -worldSize / 2, worldSize / 2);
  map.z = clamp(map.z + before.z - after.z, -worldSize / 2, worldSize / 2);
  scheduleMapData(140);
}

function formatTime(value = 9) {
  const time = (Number(value) % 24 + 24) % 24;
  return `${String(Math.floor(time)).padStart(2, '0')}:${String(Math.floor(time % 1 * 60)).padStart(2, '0')}`;
}

function resource(name) { return clamp(Number(state[name] ?? 100), 0, 100); }

function updateCharacter() {
  $('character-health').textContent = `${Math.round(resource('health'))} / 100`;
  $('character-mana').textContent = `${Math.round(resource('mana'))} / 100`;
  $('character-stamina').textContent = `${Math.round(resource('stamina'))} / 100`;
  $('character-place').textContent = state.siteName || 'The Wilds';
  $('character-biome').textContent = niceName(state.landscape || state.biome || 'Wilderness');
  $('character-position').textContent = `${Math.round(Math.abs(state.x || 0))} ${state.x >= 0 ? 'E' : 'W'} · ${Math.round(Math.abs(state.z || 0))} ${state.z >= 0 ? 'S' : 'N'}`;
  $('character-altitude').textContent = `${Math.round(state.altitude ?? state.y ?? 0)} m`;
  $('character-walked').textContent = fmtDistance(Number(state.walked || 0));
  $('character-time').textContent = formatTime(state.dayTime);
  $('character-seed').textContent = seed;
}

function updateHUD(now) {
  const radians = Number(state.yaw || 0);
  const degrees = (radians * 180 / Math.PI % 360 + 360) % 360;
  const headings = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  $('heading-label').textContent = `${headings[Math.round(degrees / 45) % 8]} · ${String(Math.round(degrees) % 360).padStart(3, '0')}°`;
  const compassWidth = $('compass-track').parentElement.clientWidth;
  $('compass-track').innerHTML = Array.from({ length: 24 }, (_, i) => {
    const angle = i * 15;
    const diff = (angle - degrees + 540) % 360 - 180;
    if (Math.abs(diff) > 80) return '';
    const major = i % 3 === 0;
    return `<span class="${major ? '' : 'minor'}" style="left:${diff * compassWidth / 120}px">${major ? headings[i / 3] : '·'}</span>`;
  }).join('');
  const biome = niceName(state.landscape || state.biome || 'Wilderness');
  $('biome-label').textContent = biome;
  $('place-name').textContent = state.siteName || 'The Wilds';
  $('clock').textContent = formatTime(state.dayTime);
  for (const name of ['health', 'mana', 'stamina']) {
    const amount = Math.round(resource(name));
    $(name + '-fill').style.width = `${amount}%`;
    $(name + '-value').textContent = amount;
    $(name + '-meter').setAttribute('aria-valuenow', amount);
  }
  if (waypoint) {
    const distance = Math.hypot(waypoint.x - state.x, waypoint.z - state.z);
    $('journey-target').textContent = `◇ ${waypoint.name}`;
    $('journey-distance').textContent = distance < 40 ? 'Destination reached.' : `${fmtDistance(distance)} · ~${Math.max(1, Math.round(distance / 330))} min on foot`;
    const bearing = Math.atan2(waypoint.x - state.x, -(waypoint.z - state.z));
    const angle = wrapAngle(bearing - radians);
    const markerVisible = Math.abs(angle) < .8 && distance >= 40 && !modal;
    $('destination-marker').classList.toggle('hidden', !markerVisible);
    if (markerVisible) {
      $('destination-marker').style.left = `${50 + Math.tan(angle) * 43}%`;
      $('marker-distance').textContent = fmtDistance(distance);
    }
  } else {
    $('journey-target').textContent = 'A road of your own';
    $('journey-distance').textContent = `${fmtDistance(Number(state.walked || 0))} explored`;
    $('destination-marker').classList.add('hidden');
  }
  if (modal === 'character') updateCharacter();
  if (!$('diagnostics').classList.contains('hidden')) {
    $('diagnostics').textContent = `FANTASYLAND / RUST + WASM + WGPU\n${fps} FPS · ${Math.round(1000 / Math.max(fps, 1))} ms\n${state.chunkCount ?? '—'} chunks · ${Number(state.triangleCount || 0).toLocaleString()} loaded triangles\nCover ${Math.round(Number(state.groundCoverDensity ?? groundCoverDensity) * 100)}% · ${Number(state.coverInstances || 0).toLocaleString()} plants submitted\n${Number(state.meshMegabytes || 0).toFixed(1)} MB mesh buffers\nX ${Math.round(state.x || 0)}  Z ${Math.round(state.z || 0)}\nAltitude ${Math.round(state.altitude ?? state.y ?? 0)} m\n${biome} · Seed ${seed}\n${locked ? 'Pointer captured' : focusedLook ? 'Focused mouse look' : 'Mouse released'} · ${state.grounded ? 'Grounded' : 'Airborne'}`;
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
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    game = await Game.create(canvas, seed);
    worldSize = Number(game.world_size());
    game.set_quality(quality);
    game.set_render_resolution(renderResolution);
    applyAscii();
    applyFilter();
    applyGroundCoverDensity();
    resize();
    if (saved.seed === seed && Number.isFinite(saved.x) && Number.isFinite(saved.z) && Math.abs(saved.x) < worldSize / 2 && Math.abs(saved.z) < worldSize / 2) game.teleport(saved.x, saved.z);
    state = game.state();
    // Exposed intentionally for integration checks and world-generation inspection.
    window.fantasyDebug = { game, get state() { return state; }, get map() { return map; }, get waypoint() { return waypoint; }, openMap, closeModal, saveProgress, get input() { return { started, locked, focusedLook, pointerLockFallback, lockPending, modal }; }, captureMouse, version: 'wilderness-4' };
    requestAnimationFrame(renderFrame);
  } catch (error) { showFatal(error); }
}

function startExploring(event) {
  if (!game || !initialReady || fatal) return;
  started = true;
  $('intro').classList.add('hidden');
  document.body.classList.remove('intro-open');
  canvas.focus({ preventScroll: true });
  captureMouse(event);
  updateFocusHint();
}

$('start-button').addEventListener('click', startExploring);
$('focus-hint').addEventListener('click', captureMouse);
$('retry-button').addEventListener('click', () => location.reload());
for (const type of ['map', 'bag', 'character', 'skills', 'settings']) {
  $(type + '-button').setAttribute('aria-expanded', 'false');
  $(type + '-button').addEventListener('click', () => {
    if (!started) return;
    if (modal === type) closeModal(); else if (type === 'map') openMap(); else openModal(type);
  });
}
for (const button of document.querySelectorAll('[data-close]')) button.addEventListener('click', closeModal);
for (const overlay of document.querySelectorAll('.overlay')) overlay.addEventListener('click', (event) => { if (event.target === overlay) closeModal(); });
$('bag-open-map').addEventListener('click', openMap);
$('skills-open-map').addEventListener('click', openMap);
$('zoom-in').addEventListener('click', () => zoomMap(.70));
$('zoom-out').addEventListener('click', () => zoomMap(1.43));
$('center-map').addEventListener('click', () => { map.x = state.x; map.z = state.z; scheduleMapData(0); });
$('fit-map').addEventListener('click', fitWorld);
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
  state = game.state(); waypoint = destination;
  saveProgress(); closeModal();
  toast(`Arrived at ${destination.name}.`);
});
$('clear-waypoint').addEventListener('click', () => { waypoint = null; saveProgress(); updateSelection(); toast('Waypoint cleared.'); });
$('quality-select').addEventListener('change', (event) => { quality = Number(event.target.value); game?.set_quality(quality); updateRenderDimensions(); saveProgress(); });
$('ground-cover-density').addEventListener('input', (event) => {
  const percent = Number(event.target.value);
  groundCoverDensity = Number.isFinite(percent) ? clamp(percent / 100, 0, 4) : 4;
  // The engine updates a GPU density setting immediately; no terrain regeneration.
  applyGroundCoverDensity();
  saveProgress();
});
$('filter-select').addEventListener('change', (event) => {
  const selected = Number(event.target.value);
  filterMode = [0, 1, 2, 3].includes(selected) ? selected : 1;
  applyFilter();
  saveProgress();
});
$('filter-strength').addEventListener('input', (event) => {
  if (filterMode === 0 || filterMode === 3) return;
  const amount = Number(event.target.value);
  filterStrength = Number.isFinite(amount) ? clamp(amount / 100, 0, 1.5) : 1;
  applyFilter();
  saveProgress();
});
$('render-resolution').addEventListener('change', (event) => {
  const selected = Number(event.target.value);
  renderResolution = RESOLUTION_OPTIONS.includes(selected) ? selected : 0;
  $('render-resolution').value = String(renderResolution);
  game?.set_render_resolution(renderResolution);
  updateRenderDimensions();
  saveProgress();
});
$('ascii-scale').addEventListener('change', (event) => {
  const selected = Number(event.target.value);
  asciiScale = [1, 2, 3].includes(selected) ? selected : 2;
  applyAscii();
  saveProgress();
});
$('ascii-palette').addEventListener('change', (event) => {
  const selected = Number(event.target.value);
  asciiPalette = [0, 1, 2].includes(selected) ? selected : 0;
  applyAscii();
  saveProgress();
});
$('sensitivity').addEventListener('input', (event) => { sensitivity = Number(event.target.value); saveProgress(); });
$('time-setting').addEventListener('input', (event) => { game?.set_time(Number(event.target.value)); $('time-setting-label').textContent = formatTime(Number(event.target.value)); });
$('seed-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const nextSeed = clamp(Math.floor(Number($('seed-input').value) || DEFAULT_SEED), 1, 4294967295);
  const url = new URL(location.href); url.searchParams.set('seed', nextSeed);
  location.href = url.href;
});
$('return-to-spawn').addEventListener('click', () => {
  game.return_to_spawn(); state = game.state(); saveProgress(); closeModal(); toast('Back on the starting road.');
});

const menuKeys = { KeyM: 'map', Tab: 'map', KeyI: 'bag', KeyC: 'character', KeyK: 'skills', KeyO: 'settings' };
document.addEventListener('keydown', (event) => {
  const editing = ['INPUT', 'SELECT', 'TEXTAREA'].includes(event.target.tagName);
  if (event.code === 'Escape') {
    event.preventDefault();
    if (modal) closeModal();
    releaseMouse();
    return;
  }
  if (editing) return;
  if (event.code === 'F3') { event.preventDefault(); $('diagnostics').classList.toggle('hidden'); return; }
  if (!started) {
    if (event.code === 'Enter' && initialReady) { event.preventDefault(); startExploring(event); }
    return;
  }
  // Within a modal, Tab keeps normal keyboard focus navigation; M is the atlas
  // toggle. From the game canvas, M and Tab open the exact same retained view.
  if (event.code === 'Tab' && modal) return;
  const menu = menuKeys[event.code];
  if (menu && !event.altKey && !event.ctrlKey && !event.metaKey) {
    event.preventDefault();
    if (event.repeat) return;
    if (modal === menu) closeModal(); else if (menu === 'map') openMap(); else openModal(menu);
    return;
  }
  if (modal) {
    if (modal === 'map' && event.target === mapCanvas) {
      const pan = map.span * .08;
      if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.code)) {
        event.preventDefault();
        if (event.code === 'ArrowLeft') map.x -= pan;
        if (event.code === 'ArrowRight') map.x += pan;
        if (event.code === 'ArrowUp') map.z -= pan;
        if (event.code === 'ArrowDown') map.z += pan;
        map.x = clamp(map.x, -worldSize / 2, worldSize / 2); map.z = clamp(map.z, -worldSize / 2, worldSize / 2);
        scheduleMapData();
      }
      if (event.code === 'Equal' || event.code === 'NumpadAdd') { event.preventDefault(); zoomMap(.7); }
      if (event.code === 'Minus' || event.code === 'NumpadSubtract') { event.preventDefault(); zoomMap(1.43); }
    }
    return;
  }
  if (['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Space', 'ShiftLeft', 'ShiftRight'].includes(event.code)) {
    event.preventDefault(); keys.add(event.code);
    if (event.code === 'Space' && !event.repeat) jumpQueued = true;
  }
});
document.addEventListener('keyup', (event) => keys.delete(event.code));
document.addEventListener('pointerlockchange', () => {
  const hasLock = document.pointerLockElement === canvas;
  if (hasLock && (!focusedLook || modal || fatal)) {
    document.exitPointerLock();
    return;
  }
  locked = hasLock;
  clearTimeout(lockTimer); lockPending = false;
  if (locked) { focusedLook = true; pointerLockFallback = false; dragLook = null; }
  else { focusedLook = false; lastMouse = null; clearMovement(); }
  updateFocusHint();
});
document.addEventListener('pointerlockerror', () => lockFailed());
document.addEventListener('mousemove', (event) => {
  if (!started || modal || !game) return;
  if (locked) {
    game.look(event.movementX * sensitivity, event.movementY * sensitivity);
  } else if (focusedLook && !matchMedia('(pointer: coarse)').matches) {
    if (lastMouse) {
      const dx = event.clientX - lastMouse.x, dy = event.clientY - lastMouse.y;
      if (Math.abs(dx) < 200 && Math.abs(dy) < 200) game.look(dx * sensitivity, dy * sensitivity);
    }
    lastMouse = { x: event.clientX, y: event.clientY };
  }
});
canvas.addEventListener('click', (event) => {
  if (!started) startExploring(event);
  else if (!modal && !locked) captureMouse(event);
});
canvas.addEventListener('pointerdown', (event) => {
  if (!started || modal || !game || locked) return;
  canvas.focus({ preventScroll: true });
  // Desktop lock uses click; mouse drag fallback is document-level, so there is
  // no setPointerCapture call racing requestPointerLock. Touch keeps capture.
  dragLook = { id: event.pointerId, x: event.clientX, y: event.clientY, touch: event.pointerType !== 'mouse' };
  if (dragLook.touch) canvas.setPointerCapture(event.pointerId);
});
document.addEventListener('pointermove', (event) => {
  if (dragLook?.id !== event.pointerId || locked || modal || !started || (focusedLook && !dragLook.touch)) return;
  game.look((event.clientX - dragLook.x) * sensitivity, (event.clientY - dragLook.y) * sensitivity);
  dragLook.x = event.clientX; dragLook.y = event.clientY;
});
document.addEventListener('pointerup', (event) => { if (dragLook?.id === event.pointerId) dragLook = null; });
document.addEventListener('pointercancel', (event) => { if (dragLook?.id === event.pointerId) dragLook = null; });
canvas.addEventListener('contextmenu', (event) => event.preventDefault());
for (const button of document.querySelectorAll('[data-move]')) {
  const move = button.dataset.move;
  button.addEventListener('pointerdown', (event) => { event.preventDefault(); touchMoves.add(move); button.classList.add('active'); button.setPointerCapture(event.pointerId); });
  const release = () => { touchMoves.delete(move); button.classList.remove('active'); };
  button.addEventListener('pointerup', release); button.addEventListener('pointercancel', release); button.addEventListener('lostpointercapture', release);
}
$('touch-jump').addEventListener('pointerdown', (event) => { event.preventDefault(); if (started && !modal) jumpQueued = true; });
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
window.addEventListener('blur', releaseMouse);
document.addEventListener('visibilitychange', () => { if (document.hidden) { releaseMouse(); saveProgress(); } lastFrame = 0; });
window.addEventListener('pagehide', saveProgress);
boot();
