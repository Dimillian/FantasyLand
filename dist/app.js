import { compassReading } from './hud-navigation.js';
import { descriptor, worldKey, validSeed, validateSave, WorldStore } from './world-store.mjs';
import { AtlasCache } from './atlas-cache.mjs';
import { Soundscape } from './soundscape.mjs?v=combat-foley-5';
import { paintPortrait } from './portrait.js?v=encounters-1';
import { equipmentRecipe } from './equipment-items.mjs';
import { CombatView } from './combat-view.mjs?v=encounters-1';
import { AdaptiveResolution } from './adaptive-resolution.js';
// Authored interface for the Rust world engine. All terrain, movement, collision,
// and world rendering belong to Game; JavaScript only coordinates input and UI.
const $ = (id) => document.getElementById(id);
const canvas = $('world');
const combatView = new CombatView($('combat-view'));
let combatActive=false, weaponDrawn=false, combatAttack=false, combatBlock=false;
const STORAGE_KEY = 'fantasyland.preferences.v1';
const DEFAULT_SEED = 1337;
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const fmtDistance = (m) => m >= 1000 ? `${(m / 1000).toFixed(m >= 10000 ? 0 : 1)} km` : `${Math.round(m)} m`;
const niceName = (s = '') => String(s).replace(/[_-]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
const wrapAngle = (a) => ((a + Math.PI) % (Math.PI * 2) + Math.PI * 2) % (Math.PI * 2) - Math.PI;
let saved = {};
try {
  const current = localStorage.getItem(STORAGE_KEY);
  if(current)saved=JSON.parse(current) || {};
} catch (_) { /* Storage is optional. */ }
const soundscape = new Soundscape(saved.audio);
const urlSeed = new URL(location.href).searchParams.get('seed');
const seed = validSeed(urlSeed, validSeed(saved.seed, DEFAULT_SEED));
const worldIdentity = descriptor(seed);
const worldStore = new WorldStore(localStorage);
let playedSave = null, importingSave = false;
try { playedSave = worldStore.load(worldIdentity); } catch (error) { console.warn('Local saves unavailable; starting a fresh world',error); }
// World progress comes only from a validated current-version snapshot.
saved = {...saved, x:undefined, z:undefined, worldClock:undefined, waypoint:null, atlas:null,
  ...(playedSave ? {seed, waypoint:playedSave.waypoint, atlas:playedSave.atlas} : {})};
const atlasCache = new AtlasCache(worldKey(worldIdentity)+':atlas2');
let atlasBegan = 0;
let atlasRequest = 0, atlasPending = null, atlasBusy = false;
let otherViewActive = false, renderChannel = null, renderOwner = '', renderClaim = 0, renderSeen = 0;
const renderId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
let giOutstanding = false;
let streamWorker = null, streamReady = false, streamOutstanding = 0, streamResults = [], streamDeadline = 0;
let game, state = {}, started = false, locked = false, modal = null;
const menuScreens = ['map', 'settings', 'bag', 'character', 'equipment', 'skills', 'pause', 'options', 'loot'];
let modalReturnFocus = null;
let optionsPage = 'graphics';
let optionsReturnMenu = 'pause';
let quickMenuOpen = false;
let uiTextSize = saved.uiTextSize === 'large' ? 'large' : 'standard';
let uiHudDetail = saved.uiHudDetail === 'minimal' ? 'minimal' : 'full';
let uiContrast = saved.uiContrast === 'high' ? 'high' : 'standard';
let focusedLook = false, lockPending = false, lockTimer = null, lastMouse = null;
let pointerLockFallback = false, lockEpoch = 0;
let quality = clamp(Number(saved.quality ?? 1), 0, 2), sensitivity = clamp(Number(saved.sensitivity ?? 1), .35, 2);
let antialiasing = [0, 1, 2].includes(Number(saved.antialiasing ?? 1)) ? Number(saved.antialiasing ?? 1) : 1;
let adaptiveResolution = saved.adaptiveResolution === true;
const adaptive = new AdaptiveResolution();
let adaptivePosition = null;
let filterMode = [0, 1, 2].includes(Number(saved.filterMode)) ? Number(saved.filterMode) : 1;
let filterStrength = Number.isFinite(Number(saved.filterStrength ?? 1)) ? clamp(Number(saved.filterStrength ?? 1), 0, 1.5) : 1;
const RESOLUTION_OPTIONS = [0, 1, 120, 180, 240, 360, 420, 450, 540, 720, 1080];
let renderResolution = RESOLUTION_OPTIONS.includes(Number(saved.renderResolution ?? 1)) ? Number(saved.renderResolution ?? 1) : 1;
let sunShadows = saved.sunShadows !== false;
const requestedLighting = Number(new URL(location.href).searchParams.get("lighting") ?? saved.lightingMode ?? 7);
let lightingMode = [0,1,3,7].includes(requestedLighting) ? requestedLighting : 7;
let meadowCarpet = saved.meadowCarpet !== false;
let weatherMode = [0, 1, 2, 3, 4, 5, 6, 7, 8].includes(Number(saved.weatherMode ?? 0)) ? Number(saved.weatherMode ?? 0) : 0;
let weatherSpeed = Number.isFinite(Number(saved.weatherSpeed ?? 1)) ? clamp(Number(saved.weatherSpeed ?? 1), .25, 20) : 1;
let weatherPaused = saved.weatherPaused === true;
let reflections = saved.reflections !== false, enclosure = saved.enclosure !== false;
let groundCoverDensity = Number.isFinite(Number(saved.groundCoverDensity ?? 4)) ? clamp(Number(saved.groundCoverDensity ?? 4), 0, 4) : 4;
let waypoint = saved.seed === seed && saved.waypoint ? saved.waypoint : null;
let keys = new Set(), touchMoves = new Set(), jumpQueued = false, dragLook = null;
let lastFrame = 0, lastHUD = 0, lastSaved = 0, frames = 0, fps = 0, fpsTime = 0;
let motionCapture = null, motionGeneration = 0;
let benchmark = null, benchmarkReport = [], adapterLabel = 'WebGPU';
let fatal = false, toastTimer, mapTimer, resizeTimer, initialReady = false;
let worldSize = 384000;
let landscapeDestinations = null;
let walkingJourneys = null, loadingWalks = false, activeJourney = null;
let naturalWonders = null, loadingWonders = false;
const savedAtlas = saved.seed === seed && saved.atlas && Number.isFinite(saved.atlas.span) ? saved.atlas : null;
const map = { initialized: !!savedAtlas, x: savedAtlas?.x || 0, z: savedAtlas?.z || 0, span: savedAtlas?.span || 6000, selected: null, image: null, imageBounds: null, features: { sites: [], landmarks: [], roads: [], routes: null }, visibleFeatures: [], dragging: null, dirty: true };
const mapCanvas = $('map-canvas');
const mapContext = mapCanvas.getContext('2d');
$('atlas-layer').value=String([0,1,2].includes(Number(saved.atlasLayer))?Number(saved.atlasLayer):0);
document.body.classList.add('intro-open');
$('intro-seed').textContent = seed;
$('seed-input').value = seed;
$('quality-select').value = quality;
$('sensitivity').value = sensitivity;
$('render-resolution').value = String(renderResolution);
$('sun-shadows').value = sunShadows ? 'on' : 'off';
$('lighting-mode').value = String(lightingMode);
updateWeatherControls();
updateGroundCoverControls();
updateFilterControls();
updateAaControls();
updateLookSummary();

for (const name of ['master','ambience','footsteps','wildlife','effects']) {
  const control=$(`audio-${name}`), output=$(`audio-${name}-value`);
  control.value=String(Math.round(soundscape.volumes[name]*100));
  output.textContent=`${control.value}%`;
  control.addEventListener('input',()=>{
    soundscape.setVolume(name,Number(control.value)/100);
    soundscape.setActive(started && !document.hidden && !otherViewActive && soundscape.volumes.master>0);
    if(started) soundscape.unlock();
    output.textContent=`${Math.round(soundscape.volumes[name]*100)}%`;
    saveProgress();
  });
}

function toast(message, duration = 3500) {
  $('toast').textContent = message;
  $('toast').classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').classList.remove('visible'), duration);
}

function saveProgress() {
  updateLookSummary();
  if (benchmark || motionCapture || otherViewActive || importingSave) return;
  if (!game || !initialReady) return;
  try {
    const snapshot=game.save_snapshot();
    const save={schema:1,updated:Date.now(),snapshot,waypoint,atlas:map.initialized?{x:map.x,z:map.z,span:map.span}:null};
    worldStore.write(save); playedSave=save;
    localStorage.setItem(STORAGE_KEY, JSON.stringify({uiTextSize,uiHudDetail,uiContrast,atlasLayer:Number($('atlas-layer').value),audio:soundscape.volumes,seed,quality,sensitivity,filterMode,filterStrength,renderResolution,antialiasing,adaptiveResolution,groundCoverDensity,meadowCarpet,sunShadows,lightingMode,weatherMode,weatherSpeed,weatherPaused,reflections,enclosure}));
    $('save-status').textContent=`World ${seed} · saved locally`;
    return save;
  } catch (error) {
    $('save-status').textContent='Save unavailable · export a backup';
    console.warn('World could not be saved',error);
    return null;
  }
}

function selectedLandscapeDestination() {
  const value = $('landscape-destination').value;
  const index = Number(value);
  return value !== '' && Number.isInteger(index) && index >= 0 ? landscapeDestinations?.[index] || null : null;
}

function loadLandscapeDestinations() {
  if (landscapeDestinations !== null || !game || !initialReady) return;
  const select = $('landscape-destination');
  try {
    // Query lazily when this control is used, not during boot or every developer-tools visit.
    const destinations = game.landscape_destinations();
    if (!Array.isArray(destinations)) throw new Error('Invalid landscape destinations');
    landscapeDestinations = destinations.filter((d) => d && typeof d.name === 'string' && d.name.trim() && Number.isFinite(d.x) && Number.isFinite(d.z) && Math.abs(d.x) < worldSize / 2 && Math.abs(d.z) < worldSize / 2).map((d) => ({ name: d.name.trim(), x: d.x, z: d.z, yaw: Number.isFinite(d.yaw) ? d.yaw : 0, pitch: Number.isFinite(d.pitch) ? d.pitch : -0.04, kind: 'landscape' }));
    for (const [index, destination] of landscapeDestinations.entries()) {
      const option = document.createElement('option');
      option.value = String(index); option.textContent = destination.name;
      select.append(option);
    }
    $('landscape-destination-help').textContent = landscapeDestinations.length ? 'Visit a generated location in this world.' : 'No landscape destinations found for this seed.';
  } catch (error) {
    console.error('Landscape destinations unavailable', error);
    $('landscape-destination-help').textContent = 'Could not find landscapes. Focus this list to retry.';
  }
  $('travel-landscape').disabled = !selectedLandscapeDestination();
}

async function loadNaturalWonders() {
  if (naturalWonders !== null || loadingWonders || !game || !initialReady) return;
  loadingWonders = true; $('wonders-help').textContent = 'Finding natural wonders…';
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  try {
    const found = game.natural_destinations();
    if (!Array.isArray(found)) throw new Error('Invalid natural wonders');
    naturalWonders = found.filter(d => d && typeof d.name === 'string' && [d.x,d.z,d.yaw,d.pitch].every(Number.isFinite));
    for (const [i,d] of naturalWonders.entries()) {
      const option = document.createElement('option'); option.value = String(i); option.textContent = d.name; $('wonders-select').append(option);
    }
    $('wonders-help').textContent = naturalWonders.length ? 'Travel to a viewpoint beside a real formation.' : 'No suitable viewpoints found in this seed.';
  } catch (error) { console.error(error); $('wonders-help').textContent = 'Could not find wonders. Focus the list to retry.'; }
  finally { loadingWonders = false; }
}
function selectedWonder() {
  const value = $('wonders-select').value, index = Number(value);
  return value !== '' && Number.isInteger(index) && index >= 0 ? naturalWonders?.[index] || null : null;
}

async function loadWalkingJourneys() {
  if (walkingJourneys !== null || loadingWalks || !game || !initialReady) return;
  loadingWalks = true; $('walking-help').textContent = 'Finding a path through the terrain…';
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  try {
    const found = game.walking_journeys();
    if (!Array.isArray(found)) throw new Error('Invalid journeys');
    walkingJourneys = found.filter(j => j && typeof j.name === 'string' && Number.isFinite(j.x) && Number.isFinite(j.z) && Number.isFinite(j.yaw) && Number.isFinite(j.minutes) && Array.isArray(j.points) && j.points.length >= 2 && j.points.every(p => Array.isArray(p) && p.length === 2 && p.every(Number.isFinite)));
    for (const [i, j] of walkingJourneys.entries()) {
      const option = document.createElement('option'); option.value = String(i);
      option.textContent = `${j.name} · ${Math.round(j.minutes)} min`; $('walking-select').append(option);
    }
    $('walking-help').textContent = walkingJourneys.length ? 'Follow the route in the atlas. Trees may call for small detours.' : 'No suitable walks found for this seed.';
  } catch (error) { console.error(error); $('walking-help').textContent = 'Could not find walks. Focus the list to retry.'; }
  finally { loadingWalks = false; }
}
function selectedWalk() {
  const value = $('walking-select').value; const index = Number(value);
  return value !== '' && Number.isInteger(index) && index >= 0 ? walkingJourneys?.[index] || null : null;
}
function updateWalk() {
  if (!activeJourney) return;
  const j = activeJourney;
  while (j.next < j.points.length && Math.hypot(j.points[j.next][0] - state.x, j.points[j.next][1] - state.z) < 38) j.next++;
  if (j.next >= j.points.length) { toast(`${j.name} completed.`); activeJourney = null; waypoint = null; saveProgress(); return; }
  const point = j.points[j.next];
  waypoint = { name: j.name, x: point[0], z: point[1], kind: 'walk' };
}

function updateWeatherControls() {
  $('weather-mode').value = String(weatherMode);
  $('weather-speed').value = String(weatherSpeed);
  const speed = Number(weatherSpeed.toFixed(2));
  $('weather-speed-value').textContent = `${speed}×${weatherPaused ? ' · paused' : ''}`;
  $('weather-speed').setAttribute('aria-valuetext', `${speed} times speed${weatherPaused ? ', paused' : ''}`);
  $('weather-paused').value = weatherPaused ? 'paused' : 'running';
  $('water-reflections').value = reflections ? 'on' : 'off';
  $('enclosure-shading').value = enclosure ? 'on' : 'off';
  $('weather-mode-help').textContent = weatherMode === 0
    ? 'Moving fronts follow the local climate.'
    : 'A gradual override. Choose Automatic to return to local weather.';
}

function applyWeatherPreferences() {
  updateWeatherControls();
  if (!game) return;
  game.set_weather_mode(weatherMode);
  game.set_weather_speed(weatherSpeed);
  game.set_weather_paused(weatherPaused);
  game.set_reflections(reflections);
  game.set_enclosure(enclosure);
}

function updateWeatherStatus() {
  const weather = state.weather;
  if (!weather || typeof weather.label !== 'string') return;
  const temperature = Number.isFinite(weather.temperature) ? `${Math.round(weather.temperature)}°C` : '—';
  const summary = `${weather.label} · ${temperature}`;
  $('weather-hud').textContent = temperature;
  $('weather-hud').setAttribute('aria-label', summary);
  if (modal !== 'settings') return;
  const percent = value => Number.isFinite(value) ? `${Math.round(clamp(value, 0, 1) * 100)}%` : '—';
  const wind = Math.hypot(Number(weather.windX), Number(weather.windZ));
  const windLabel = !Number.isFinite(wind) ? '—' : wind < 2 ? 'calm' : wind < 6 ? 'light' : wind < 12 ? 'brisk' : wind < 22 ? 'strong' : 'fierce';
  $('weather-current').textContent = `${summary} · ${weather.modeLabel || (weatherMode === 0 ? 'Automatic' : 'Override')}${weatherPaused ? ' · paused' : ''}`;
  $('weather-air').textContent = `Wind ${windLabel} · cloud ${percent(weather.cloudCover)} · rain ${percent(weather.rain)} · snow ${percent(weather.snow)}`;
  $('weather-ground').textContent = `Ground wetness ${percent(weather.wetness)} · snow cover ${percent(weather.snowCover)}`;
}

function updateGroundCoverControls() {
  const percent = Math.round(groundCoverDensity * 100);
  const label = groundCoverDensity === 0 ? 'Off' : `${percent}% · ${Number(groundCoverDensity.toFixed(2))}×`;
  $('meadow-carpet').value = meadowCarpet ? 'on' : 'off';
  $('ground-cover-density').value = String(percent);
  $('ground-cover-density-value').textContent = label;
  $('ground-cover-density').setAttribute('aria-valuetext', groundCoverDensity === 0 ? 'Off' : `${percent} percent, ${Number(groundCoverDensity.toFixed(2))} times density`);
}

function applyGroundCoverDensity() {
  updateGroundCoverControls();
  if (game) {game.set_ground_cover_density(groundCoverDensity);game.set_meadow(meadowCarpet);}
}

function updateFilterControls() {
  $('filter-select').value = String(filterMode);
  $('filter-strength').value = String(Math.round(filterStrength * 100));
  $('filter-strength-value').textContent = `${Math.round(filterStrength * 100)}%`;
  $('filter-strength').disabled = filterMode === 0;
  $('filter-strength-row').classList.toggle('setting-inactive', filterMode === 0);
  $('filter-description').textContent = [
    'Unfiltered, crisp scene colors.',
    'Soft light around bright surfaces.',
    'Classic monitor texture and soft glow.',
  ][filterMode];
}

function applyFilter() {
  updateFilterControls();
  if (game) game.set_filter(filterMode, filterStrength);
}

function updateAaControls() {
  $('antialiasing').value = String(antialiasing);
  $('antialiasing-help').textContent = ['Sharp pixels, no edge smoothing.', 'Softens jagged edges with a light touch.', 'Sharper edge smoothing, with a higher GPU cost.'][antialiasing];
  // Keep old custom resolutions visible when selected, without crowding the menu.
  const legacy = $('legacy-resolution');
  legacy.value = String(renderResolution);
  legacy.textContent = renderResolution === 0 ? 'Quality scaled' : `${renderResolution}p · custom`;
  legacy.hidden = [1, 420, 540, 720].includes(renderResolution);
  $('render-resolution').value = adaptiveResolution ? 'adaptive' : String(renderResolution);
}

function updateLookSummary() {
  const recommended = quality === 1 && antialiasing === 1 && filterMode === 1 && filterStrength === 1 && groundCoverDensity === 4 && meadowCarpet && sunShadows && lightingMode === 7 && reflections && enclosure;
  $('look-status').textContent = recommended ? 'Recommended look' : 'Custom look';
  $('look-description').textContent = recommended
    ? 'Lush ground cover, soft edges and cinematic light.'
    : 'Your saved graphics choices. Adjust them under Advanced graphics.';
}

function restoreVisualDefaults() {
  quality = 1; antialiasing = 1; filterMode = 1; filterStrength = 1;
  groundCoverDensity = 4; meadowCarpet = true;
  sunShadows = true; lightingMode = 7; reflections = true; enclosure = true;
  renderResolution = 1; adaptiveResolution = false;
  $('quality-select').value = '1'; $('sun-shadows').value = 'on'; $('lighting-mode').value = '7';
  game?.set_quality(quality);
  game?.set_antialiasing(antialiasing);
  game?.set_shadows(sunShadows);
  game?.set_lighting_mode(lightingMode);
  game?.set_reflections(reflections); game?.set_enclosure(enclosure);
  applyGroundCoverDensity(); applyFilter(); applyResolution(); updateWeatherControls();
  saveProgress(); toast('Recommended look restored · Native resolution');
}
function applyResolution() {
  game?.set_render_resolution(adaptiveResolution ? adaptive.height : renderResolution);
  adaptive.reset(performance.now());
  updateAaControls(); updateRenderDimensions();
}
function updateAdaptiveResolution(now, gap) {
  if (Number.isFinite(state.x) && Number.isFinite(state.z)) {
    if (adaptivePosition && Math.hypot(state.x-adaptivePosition.x,state.z-adaptivePosition.z)>96) adaptive.reset(now);
    adaptivePosition = {x:state.x,z:state.z};
  }
  const height = adaptive.sample(now, gap, adaptiveResolution && initialReady && started && !modal && !benchmark && !motionCapture && !document.hidden && !otherViewActive);
  if (height !== null) { game.set_render_resolution(height); updateRenderDimensions(); }
}

function updateRenderDimensions() {
  if (!game) return;
  const actual = game.render_resolution();
  if (actual && actual.length >= 2) $('render-dimensions').textContent = `Actual ${actual[0]} × ${actual[1]}${adaptiveResolution ? " · adaptive 420–720p" : ""}`;
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
  combatAttack=false;combatBlock=false;game?.combat_input?.(false,false,true);
  keys.clear(); touchMoves.clear(); jumpQueued = false; dragLook = null;
  document.querySelectorAll('[data-move]').forEach((button) => button.classList.remove('active'));
}

// Keep requestPointerLock on the original click/Enter user-gesture stack. No
// promise await, animation-frame callback, or pointer-capture competes with it.
function updateFocusHint() {
  const show = started && !modal && !quickMenuOpen && (!focusedLook || otherViewActive) && !fatal && !matchMedia('(pointer: coarse)').matches;
  $('focus-hint').classList.toggle('hidden', !show);
  document.body.classList.toggle('mouse-focused', focusedLook && !modal);
  $('focus-hint-text').textContent = otherViewActive ? 'Another view is active · click here to resume' : focusedLook ? 'Mouse look active · Esc releases' : 'Click the world to look around';
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
  if (started && !otherViewActive && !document.hidden) soundscape.unlock();
  if (!started || modal || fatal || benchmark || motionCapture || matchMedia('(pointer: coarse)').matches) return;
  setQuickMenu(false);
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

function setQuickMenu(open, restoreFocus = false) {
  if (open && (!started || modal)) return;
  quickMenuOpen = open;
  $('quick-menu').classList.toggle('hidden', !open);
  $('quick-menu-toggle').setAttribute('aria-expanded', String(open));
  document.body.classList.toggle('quick-menu-open', open);
  if (open) {
    releaseMouse();
    $('character-button').focus({ preventScroll: true });
  } else if (restoreFocus) {
    $('quick-menu-toggle').focus({ preventScroll: true });
  }
  updateFocusHint();
}

function quickMenuKeyboard(event) {
  if (!quickMenuOpen || !['Tab', 'ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.code)) return false;
  const controls = [...$('quick-menu').querySelectorAll('button'), $('quick-menu-toggle')];
  const index = controls.indexOf(document.activeElement);
  const backwards = event.code === 'ArrowUp' || (event.code === 'Tab' && event.shiftKey);
  const next = event.code === 'Home' ? 0 : event.code === 'End' ? controls.length - 1
    : (index + (backwards ? -1 : 1) + controls.length) % controls.length;
  controls[next].focus();
  event.preventDefault();
  return true;
}

function openModal(type) {
  if (!game || !initialReady) return;
  if (motionCapture) finishMotionCapture(true);
  if (benchmark) finishBenchmark(true);
  game?.end_dialogue();$('dialogue-modal').classList.add('hidden');
  if (type === 'options') optionsReturnMenu = quickMenuOpen ? 'quick' : 'pause';
  if (!modal) modalReturnFocus = quickMenuOpen ? $('quick-menu-toggle') : document.activeElement;
  setQuickMenu(false);
  modal = type;
  releaseMouse();
  for (const name of menuScreens) $(name + '-modal').classList.toggle('hidden', name !== type);
  for (const name of menuScreens) $(name + '-button')?.setAttribute('aria-expanded', String(name === type));
  document.body.classList.add('modal-open');
  if (type === 'settings') {
    $('time-setting').value = Number(state.dayTime ?? 9);
    $('time-setting-label').textContent = formatTime(state.dayTime);
    updateWeatherStatus();
  }
  if (['character','bag','equipment','skills'].includes(type)) syncCharacter();
  if (type === 'character') updateCharacter();
  if (type === 'pause') {
    $('pause-location').textContent = `${state.siteName || 'The wilderness'} · ${formatTime(state.dayTime)}`;
  }
  if (type === 'options') showOptionsPage(optionsPage);
  const initialFocus = {
    map: mapCanvas,
    settings: $('time-setting'),
    pause: $('resume-button'),
    options: $('option-' + optionsPage),
  };
  (initialFocus[type] || $(type + '-modal').querySelector('[data-close]')).focus({ preventScroll: true });
}

function closeModal() {
  game?.end_dialogue();$('dialogue-modal').classList.add('hidden');
  modal = null;
  for (const name of menuScreens) $(name + '-modal').classList.add('hidden');
  for (const name of menuScreens) $(name + '-button')?.setAttribute('aria-expanded', 'false');
  document.body.classList.remove('modal-open');
  clearMovement();
  const restore = modalReturnFocus && modalReturnFocus !== document.body && modalReturnFocus.getClientRects().length ? modalReturnFocus : canvas;
  restore.focus({ preventScroll: true }); modalReturnFocus = null;
  updateFocusHint();
  saveProgress();
}

function showOptionsPage(page) {
  optionsPage = page;
  for (const name of ['graphics', 'audio', 'controls', 'interface']) {
    $('options-' + name).classList.toggle('hidden', name !== page);
    $('option-' + name).setAttribute('aria-current', name === page ? 'page' : 'false');
  }
}

function applyInterfacePreferences() {
  document.body.classList.toggle('large-menu-text', uiTextSize === 'large');
  document.body.classList.toggle('minimal-hud', uiHudDetail === 'minimal');
  document.body.classList.toggle('high-contrast', uiContrast === 'high');
  $('ui-text-size').value = uiTextSize;
  $('ui-hud-detail').value = uiHudDetail;
  $('ui-contrast').value = uiContrast;
}

function backFromMenu() {
  if (modal === 'options') {
    if (optionsReturnMenu === 'quick') {
      closeModal();
      setQuickMenu(true);
      $('hud-options-button').focus({ preventScroll: true });
    } else {
      openModal('pause');
      $('options-button').focus({ preventScroll: true });
    }
  } else closeModal();
}

function resumeJourney(event) {
  closeModal();
  captureMouse(event);
}

function returnToTitle() {
  closeModal();
  started = false;
  releaseMouse();
  $('intro').classList.remove('hidden');
  document.body.classList.add('intro-open');
  $('start-button').textContent = 'CONTINUE YOUR JOURNEY →';
  $('start-button').focus({ preventScroll: true });
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

function applyAtlas(record, q, request) {
  if(request!==atlasRequest || modal!=='map')return;
  const image=document.createElement('canvas'); image.width=q.res;image.height=q.res;
  image.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(record.pixels),q.res,q.res),0,0);
  map.image=image;map.imageBounds={x:q.x,z:q.z,span:q.span};
  const f=record.features || {};
  map.features={sites:(f.sites||[]).map(v=>normalizeFeature(v,'settlement')),landmarks:(f.landmarks||[]).map(v=>normalizeFeature(v,'landmark')),roads:f.roads||[],routes:Array.isArray(f.routes)?f.routes:null,buildings:f.buildings||[],streets:f.streets||[],geography:f.geography||[]};
  mapCanvas.setAttribute('data-cache-source',record.source||'generated');
  mapCanvas.setAttribute('data-update-ms',(performance.now()-atlasBegan).toFixed(1));
  map.dirty=true;$('map-updating').classList.add('hidden');
}
function pumpAtlas() {
  if(atlasBusy || !atlasPending || modal!=='map')return;
  if(streamWorker && !streamReady)return;
  const pending=atlasPending;atlasPending=null;
  if(streamWorker && streamReady){atlasBusy=true;streamWorker.postMessage({type:'atlas',...pending});}
  else {
    try {
      const q=pending.q;
      const record=atlasCache.put(q,game.map_layer_data(q.x,q.z,q.span,q.res,q.layer),game.features(q.x,q.z,q.span));
      applyAtlas(record,q,pending.request);
    } catch(error){console.error(error);$('map-updating').classList.add('hidden');toast('Atlas generation failed. Reopen to retry.');}
  }
}
function scheduleMapData(delay = 100) {
  clearTimeout(mapTimer);map.dirty=true;atlasPending=null;
  const request=++atlasRequest;
  $('map-updating').classList.remove('hidden');
  mapTimer=setTimeout(async()=>{
    if(modal!=='map'||!game)return;
    atlasBegan=performance.now();
    const q={x:map.x,z:map.z,span:map.span,res:map.span>25000?384:320,layer:Number($('atlas-layer').value)};
    const record=await atlasCache.get(q);
    if(request!==atlasRequest||modal!=='map')return;
    if(record){applyAtlas(record,q,request);return;}
    atlasPending={q,request};pumpAtlas();
  },delay);
}

// All widths are CSS pixels: zoom changes geography, not road thickness.
// Draw the classified routes supplied by the current engine.
function drawMapRoutes(ctx, features, span) {
  const routes = features.routes || [];
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
  if(map.span<5000){
    ctx.save();ctx.strokeStyle='#baa77d';ctx.lineWidth=1;
    for(const street of map.features.streets||[]){ctx.beginPath();street.points.forEach((p,i)=>{const q=worldToScreen(p[0],p[1]);if(i)ctx.lineTo(q.x,q.y);else ctx.moveTo(q.x,q.y);});ctx.stroke();}
    for(const b of map.features.buildings||[]){ctx.fillStyle=b.usage==='home'?'#b5a17d':'#d4bd84';ctx.strokeStyle='#524c37';ctx.beginPath();const cs=Math.cos(b.yaw),sn=Math.sin(b.yaw);[[-1,-1],[1,-1],[1,1],[-1,1]].forEach(([x,z],i)=>{const p=worldToScreen(b.x+x*b.half[0]*cs+z*b.half[1]*sn,b.z-x*b.half[0]*sn+z*b.half[1]*cs);if(i)ctx.lineTo(p.x,p.y);else ctx.moveTo(p.x,p.y);});ctx.closePath();ctx.fill();ctx.stroke();}
    ctx.restore();
  }
  if (activeJourney) {
    ctx.save(); ctx.beginPath(); ctx.strokeStyle = '#f4d695'; ctx.lineWidth = 2; ctx.setLineDash([6, 4]);
    activeJourney.points.forEach((point, i) => { const p = worldToScreen(point[0], point[1]); if (i) ctx.lineTo(p.x, p.y); else ctx.moveTo(p.x, p.y); });
    ctx.stroke(); ctx.restore();
  }
  const visible = [];
  const occupied = [];
  if ($('atlas-names').checked) {
    ctx.font = `${map.span>170000?10:12}px "Marches Pixel", monospace`; ctx.textAlign='center';
    const markers=[...map.features.sites,...map.features.landmarks].map(f=>worldToScreen(f.x,f.z));
    const you=worldToScreen(state.x||0,state.z||0);
    for (const f of map.features.geography || []) {
      const anchor=worldToScreen(f.x,f.z), width=ctx.measureText(f.name).width;
      const p=[0,-24,24,-48,48].map(dy=>({x:anchor.x,y:anchor.y+dy})).find(p=>
        p.x>width/2+12 && p.x<w-width/2-12 && p.y>35 && p.y<h-40 &&
        !(Math.abs(you.x-p.x)<width/2+20 && Math.abs(you.y-p.y)<32) &&
        !markers.some(q=>Math.abs(q.x-p.x)<width/2+7 && Math.abs(q.y-(p.y-4))<13) &&
        !occupied.some(q=>Math.abs(q.x-p.x)<Math.max(120,(width+(q.width||130))/2+10)&&Math.abs(q.y-p.y)<34));
      if(!p)continue;
      ctx.lineWidth=3;ctx.strokeStyle='#172c2ce6';ctx.strokeText(f.name,p.x,p.y);
      ctx.fillStyle=(f.kind==='river'||f.kind==='lake')?'#acd7dd':'#e5d5ac';ctx.fillText(f.name,p.x,p.y);
      occupied.push({...p,width});
    }
  }
  const features = [...map.features.sites.map((f) => ({ ...f, isSite: true })), ...map.features.landmarks];
  for (const feature of features) {
    const p = worldToScreen(feature.x, feature.z);
    if (p.x < -10 || p.y < -10 || p.x > w + 10 || p.y > h + 10) continue;
    visible.push({ ...feature, px: p.x, py: p.y });
    const selected = map.selected && Math.hypot(feature.x - map.selected.x, feature.z - map.selected.z) < 1;
    const hostile=['enemy_camp','crypt','cemetery','mini_dungeon','haunted_grove'].includes(feature.kind);
    ctx.fillStyle = selected ? '#fff2ac' : hostile ? '#d28b7f' : feature.isSite ? '#efdc9f' : '#d8dfbb';
    ctx.strokeStyle = '#263320'; ctx.lineWidth = 2;
    ctx.beginPath();
    if (feature.isSite) ctx.rect(p.x - 3, p.y - 3, 6, 6);
    else { ctx.moveTo(p.x, p.y - 3.5); ctx.lineTo(p.x + 3.5, p.y); ctx.lineTo(p.x, p.y + 3.5); ctx.lineTo(p.x - 3.5, p.y); ctx.closePath(); }
    ctx.stroke(); ctx.fill();
    const labelAllowed = selected || (feature.isSite || map.span < 15000) && !occupied.some((q) => Math.abs(q.x - p.x) < 105 && Math.abs(q.y - p.y) < 26);
    if (labelAllowed && occupied.length < 65) {
      ctx.font = '12px "Marches Pixel", monospace';
      ctx.textAlign = 'center'; ctx.lineWidth = 3.5;
      ctx.strokeStyle = '#20321fe6'; ctx.strokeText(feature.name, p.x, p.y - 9);
      ctx.fillStyle = selected ? '#fff1ad' : '#f0ebc9'; ctx.fillText(feature.name, p.x, p.y - 9);
      occupied.push(p);
    }
  }
  map.visibleFeatures = visible;
  if (document.activeElement === mapCanvas) {
    ctx.strokeStyle = '#fff2bb'; ctx.lineWidth = 1;
    ctx.strokeRect(Math.round(w / 2) - 5.5, Math.round(h / 2) - 5.5, 11, 11);
  }
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
    ctx.font = '12px "Marches Pixel", monospace'; ctx.textAlign = 'center'; ctx.lineWidth = 3; ctx.strokeStyle = '#22341e';
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

function resourceMax(name) { return characterData?.derived[{health:'maxHealth',mana:'maxMana',stamina:'maxStamina'}[name]]??100; }
function resource(name) { return clamp(Number(state[name] ?? 100), 0, resourceMax(name)); }

let characterData=null, equipmentSignature='', trialActive=false, encounterOptions=[];
const starterRecipes={mainHand:equipmentRecipe(41,'sword'),offHand:equipmentRecipe(19,'shield')};
function syncCharacter(){
  if(!game?.character_state)return;
  characterData=game.character_state();const {character:c,derived:d}=characterData;
  const signature=`${c.sword_equipped}:${c.shield_equipped}`;
  if(signature!==equipmentSignature){equipmentSignature=signature;combatView.setEquipment({mainHand:c.sword_equipped?starterRecipes.mainHand:null,offHand:c.shield_equipped?starterRecipes.offHand:null});}
  document.dispatchEvent(new CustomEvent('character-progress',{detail:characterData}));
  const sword=`${Math.round(d.damage)} damage · ${d.attackCost} stamina per swing`;
  const shield=`${d.blockCost} stamina per block · 2 protection`;
  for(const [id,equipped,title,slot] of [['equipped-sword',c.sword_equipped,'Iron arming sword','Main hand'],['equipped-shield',c.shield_equipped,'Oak round shield','Off hand']]){
    const button=$(id);button.setAttribute('aria-label',`${slot}: ${equipped?title:'empty'}`);button.classList.toggle('slot-filled',equipped);button.querySelector('strong').textContent=equipped?title:`${slot} · Empty`;button.dataset.equipped=String(equipped);if(equipped)button.dataset.item=id==='equipped-sword'?'iron-sword':'oak-shield';else delete button.dataset.item;
  }
  $('live-attributes').innerHTML=[['Strength',c.strength,'Weapon damage'],['Endurance',c.endurance,'Health and stamina'],['Agility',c.agility,'Attack stamina cost'],['Intellect',c.intellect,'Mana capacity'],['Willpower',c.willpower,'Stamina recovery']].map(([name,value,use])=>`<div><dt>${name}<small>${use}</small></dt><dd>${value}</dd></div>`).join('');
}
function activateItem(button){
 const item=characterData?.items.find(i=>i.id===button?.dataset.item);if(!item)return;
 if(item.group!=='equipment')return;
 document.dispatchEvent(new CustomEvent('equip-item',{detail:{id:item.id}}));document.dispatchEvent(new Event('dismiss-item-tooltip'));
}
for(const button of document.querySelectorAll('#equipment-modal [data-item]'))button.addEventListener('click',()=>activateItem(button));
const inventoryGrid=document.querySelector('.inventory-grid');
inventoryGrid.addEventListener('click',e=>activateItem(e.target.closest('[data-item]')));
inventoryGrid.addEventListener('keydown',e=>{if(e.code==='Enter'||e.code==='Space'){e.preventDefault();activateItem(e.target.closest('[data-item]'));}});
function equipmentMessage(message){$('equipment-message').textContent=message;$('gear-message').textContent=message;}
document.addEventListener('equip-item',({detail:{id}})=>{
 if(!game||!characterData)return;
 const on=!(id==='iron-sword'?characterData.character.sword_equipped:characterData.character.shield_equipped);
 if(!game.equip_item(id,on)){equipmentMessage('Finish the encounter before changing equipment.');return;}
 equipmentMessage(on?'Item equipped.':'Item unequipped.');syncCharacter();saveProgress();
});
const skillNotices=new Map();
function notifySkill(event){
  const key=event.kind.startsWith('blades')?'blades':'blocking',ranked=event.kind.endsWith('-rank');
  let notice=skillNotices.get(key);if(!notice){const node=document.createElement('div');node.className='skill-notice';$('skill-notifications').append(node);notice={node,amount:0,timer:null};skillNotices.set(key,notice);}
  clearTimeout(notice.timer);notice.amount+=ranked?0:event.amount;const name=key==='blades'?'Blades':'Blocking',v=characterData.character[key],next=key==='blades'?characterData.bladesNext:characterData.blockingNext;
  notice.node.classList.toggle('rank-up',ranked);notice.node.innerHTML=`<span class="skill-notice-glyph">${key==='blades'?'⚔':'◇'}</span><div><small>${ranked?'SKILL IMPROVED':'PRACTICE'}</small><strong>${name} <b>${ranked?`Rank ${v.rank}`:`+${notice.amount} XP`}</b></strong><div class="skill-notice-track"><i style="width:${v.xp/next*100}%"></i></div><span>Rank ${v.rank} · ${v.xp} / ${next} XP</span></div>`;
  notice.timer=setTimeout(()=>{notice.node.remove();skillNotices.delete(key);},ranked?5500:2800);
}
$('find-encounters').addEventListener('click',()=>{
  encounterOptions=game?.encounter_locations?.()||[];const select=$('encounter-destination');select.replaceChildren(new Option(encounterOptions.length?'Choose a haunt…':'No uncleared haunts nearby',''));
  encounterOptions.forEach((e,i)=>select.add(new Option(e.name,String(i))));$('visit-encounter').disabled=true;
});
$('encounter-destination').addEventListener('change',()=>{$('visit-encounter').disabled=$('encounter-destination').value==='';});
$('visit-encounter').addEventListener('click',()=>{const e=encounterOptions[Number($('encounter-destination').value)];if(e&&game.visit_encounter(e.x,e.z)){state=game.state();closeModal();saveProgress();}});
$('revive-button').addEventListener('click',()=>{game.revive();state=game.state();combatView.reset();saveProgress();});

function updateCharacter() {
  $('character-health').textContent = `${Math.round(state.health??100)} / ${characterData?.derived.maxHealth??100}`;
  $('character-mana').textContent = `${Math.round(state.mana??100)} / ${characterData?.derived.maxMana??100}`;
  $('character-stamina').textContent = `${Math.round(state.stamina??100)} / ${characterData?.derived.maxStamina??100}`;
  $('character-place').textContent = state.siteName || 'The Wilds';
  $('character-biome').textContent = niceName(state.forest || state.landscape || state.biome || 'Wilderness');
  $('character-position').textContent = `${Math.round(Math.abs(state.x || 0))} ${state.x >= 0 ? 'E' : 'W'} · ${Math.round(Math.abs(state.z || 0))} ${state.z >= 0 ? 'S' : 'N'}`;
  $('character-altitude').textContent = `${Math.round(state.altitude ?? state.y ?? 0)} m`;
  $('character-walked').textContent = fmtDistance(Number(state.walked || 0));
  $('character-time').textContent = formatTime(state.dayTime);
  $('character-seed').textContent = seed;
}

const compassTicks = compassReading(0, { x: 0, z: 0 }, null).ticks.map(mark => {
  const tick = document.createElement('span');
  const label = document.createElement('span');
  tick.className = `compass-tick${mark.label ? ' major' : ''}${mark.cardinal ? ' cardinal' : ''}`;
  label.textContent = mark.label;
  tick.append(label);
  $('compass-track').append(tick);
  return tick;
});
let previousCompassPose = null;
let previousCompassLabel = '';

function updateCompass(x, z, yaw) {
  const previous = previousCompassPose;
  if (previous && previous.x === x && previous.z === z && previous.yaw === yaw
    && previous.targetX === waypoint?.x && previous.targetZ === waypoint?.z) return;
  previousCompassPose = { x, z, yaw, targetX: waypoint?.x, targetZ: waypoint?.z };
  const navigation = compassReading(yaw, { x, z }, waypoint);
  const label = `Compass, facing ${navigation.direction}, ${Math.round(navigation.degrees) % 360} degrees`;
  if (label !== previousCompassLabel) {
    $('compass').setAttribute('aria-label', label);
    previousCompassLabel = label;
  }
  navigation.ticks.forEach((mark, index) => {
    compassTicks[index].style.left = `${mark.offset}%`;
  });
  const pin = navigation.pin;
  $('compass-pin').classList.toggle('hidden', !pin || pin.arrived);
  if (pin) {
    $('compass-pin').style.left = `${pin.offset}%`;
    $('compass-pin').textContent = pin.edge === 'left' ? '‹' : pin.edge === 'right' ? '›' : '◆';
    $('compass-pin').classList.toggle('at-edge', !!pin.edge);
  }
}

function updateHUD(now) {
  const prompt=state.interaction || ''; $('interaction-prompt').textContent=prompt;$('interaction-prompt').classList.toggle('hidden',!prompt||!!modal||!started);

  updateWalk();
  const navigation = compassReading(Number(state.yaw || 0), state, waypoint);
  const biome = niceName(state.forest || state.landscape || state.biome || 'Wilderness');
  $('place-name').textContent = state.siteName && state.siteName !== 'The Wilds' ? state.siteName : biome;
  $('clock').textContent = formatTime(state.dayTime);
  $('navigation-clock').textContent = formatTime(state.dayTime);
  updateWeatherStatus();
  for (const name of ['health', 'mana', 'stamina']) {
    const amount = Math.round(resource(name));
    $(name + '-fill').style.width = `${amount/resourceMax(name)*100}%`;
    $(name + '-meter').setAttribute('aria-valuemax',resourceMax(name));
    $(name + '-value').textContent = amount;
    $(name + '-meter').setAttribute('aria-valuenow', amount);
    $(name + '-stat').classList.toggle('is-depleted', amount < resourceMax(name));
  }
  const pin = navigation.pin;
  $('pinned-location').classList.toggle('hidden', !pin);
  if (pin) {
    $('journey-target').textContent = waypoint.name;
    $('journey-distance').textContent = pin.arrived ? 'Arrived' : fmtDistance(pin.distance);
    const direction = pin.arrived ? 'arrived' : pin.edge ? `turn ${pin.edge}` : 'ahead';
    $('pinned-location').setAttribute('aria-label', `Pinned: ${waypoint.name}, ${fmtDistance(pin.distance)}, ${direction}`);
  }
  if (modal === 'character') updateCharacter();
  if (!$('diagnostics').classList.contains('hidden')) {
    $('diagnostics').textContent = `FANTASYLAND / RUST + WASM + WGPU\n${adapterLabel}\n${streamReady ? 'Background streaming' : 'Local streaming'} · ${state.streamingPending ?? 0} pending\nLighting ${['Classic','Ambient depth','','Detailed shadows','','','','Full indirect'][lightingMode] ?? 'Custom'}${lightingMode === 7 ? (streamReady ? ` · probes ${Math.round((state.giReadyFraction ?? 0)*100)}% ready` : ' · ambient fallback while generator is unavailable') : ''}\nSampled GPU draw span ${state.gpuRenderMs == null ? 'unavailable' : `${state.gpuRenderMs.toFixed(2)} ms`}\n${fps} FPS · ${Math.round(1000 / Math.max(fps, 1))} ms\n${state.chunkCount ?? '—'} chunks · ${Number(state.triangleCount || 0).toLocaleString()} loaded triangles\nCover ${Math.round(Number(state.groundCoverDensity ?? groundCoverDensity) * 100)}% · ${Number(state.coverInstances || 0).toLocaleString()} accent plants submitted\nAA ${["Off","FXAA","SMAA"][antialiasing]} · ${adaptiveResolution ? `Adaptive ${adaptive.height}p` : "Fixed resolution"}\nMeadow carpet ${meadowCarpet ? 'On · GPU culled' : 'Off'}\n${Number(state.meshMegabytes || 0).toFixed(1)} MB mesh buffers · Shadows ${sunShadows ? 'On' : 'Off'}\nReflections ${reflections ? quality > 0 ? 'On' : 'Off at Low quality' : 'Off'} · ${Number(state.reflectionDraws || 0)} reflection draws · Enclosure ${enclosure ? 'On' : 'Off'}\nAudio ${soundscape.status} · ${state.audio?.floor || 'unknown'} footsteps · ${soundscape.voices.size} voices · ${soundscape.thunder.pending.length} thunder queued\nX ${Math.round(state.x || 0)}  Z ${Math.round(state.z || 0)}\nAltitude ${Math.round(state.altitude ?? state.y ?? 0)} m\n${biome} · Seed ${seed}\n${locked ? 'Pointer captured' : focusedLook ? 'Focused mouse look' : 'Mouse released'} · ${state.grounded ? 'Grounded' : 'Airborne'}`;
  }
  if (now - lastSaved > 5000) { saveProgress(); lastSaved = now; }
}

// Embedded previews can report every tab as visible. Elect one rendering
// view per origin; input immediately takes ownership, without closing any tab.
function claimRenderer() {
  if(!renderChannel || document.hidden || (renderOwner===renderId && !otherViewActive))return;
  renderClaim=Math.max(Date.now(),renderClaim+1);
  renderOwner=renderId;renderSeen=performance.now();otherViewActive=false;lastFrame=0;
  if(streamWorker && streamDeadline)streamDeadline=performance.now()+120000;
  canvas.setAttribute('data-render-active','true');
  updateFocusHint();
  renderChannel.postMessage({type:'claim',id:renderId,stamp:renderClaim});
}
function initRenderCoordination() {
  if(typeof BroadcastChannel==='undefined')return;
  try {
    renderChannel=new BroadcastChannel('fantasyland.render-owner.v1');
    renderChannel.onmessage=({data})=>{
      if(!data || typeof data.id!=='string' || data.id===renderId)return;
      if(data.type==='claim' && Number.isFinite(data.stamp)
        && (data.stamp>renderClaim || (data.stamp===renderClaim && data.id>renderOwner))) {
        renderClaim=data.stamp;renderOwner=data.id;renderSeen=performance.now();
        otherViewActive=true;lastFrame=0;
        soundscape.setActive(false);
        canvas.setAttribute('data-render-active','false');
        releaseMouse();
        if(benchmark)finishBenchmark(true);
        if(motionCapture)finishMotionCapture(true);
      } else if(data.type==='heartbeat' && data.id===renderOwner)renderSeen=performance.now();
      else if(data.type==='release' && data.id===renderOwner)claimRenderer();
    };
    claimRenderer();
    setInterval(()=>{
      if(document.hidden)return;
      if(!otherViewActive)renderChannel.postMessage({type:'heartbeat',id:renderId});
      else if(performance.now()-renderSeen>30000)claimRenderer();
    },1000);
    window.addEventListener('focus',claimRenderer);
    window.addEventListener('pagehide',()=>{
      soundscape.setActive(false);
      if(!otherViewActive)renderChannel?.postMessage({type:'release',id:renderId});

    });
    document.addEventListener('pointerdown',claimRenderer);
    document.addEventListener('keydown',claimRenderer);
    document.addEventListener('visibilitychange',()=>{
      if(!document.hidden)claimRenderer();
      else if(!otherViewActive)renderChannel?.postMessage({type:'release',id:renderId});
    });
  } catch(_) {renderChannel=null;otherViewActive=false;}
}

function stopStreamingWorker(error) {
  streamWorker?.terminate(); streamWorker=null;streamReady=false;
  streamOutstanding=0;giOutstanding=false;streamResults=[];streamDeadline=0;
  game?.set_async_streaming(false);
  atlasBusy=false;
  if(modal==='map')scheduleMapData(0);
  if(error) console.warn('Background generation unavailable; using bounded local streaming.',String(error));
}
function startStreamingWorker() {
  if(typeof Worker==='undefined') return;
  try {
    streamWorker=new Worker(new URL('./world-worker.js?v=encounters-1',location.href),{type:'module',name:'FantasyLand world generation'});
    streamDeadline=performance.now()+120000;
    streamWorker.onmessage=({data})=>{
      if(data.type==='ready') {
        if(!data.identity || worldKey(data.identity)!==worldKey(worldIdentity)){stopStreamingWorker('Worker world version mismatch');return;}
        game.set_async_streaming(true);streamReady=true;streamDeadline=0;pumpAtlas();
      }
      else if(data.type==='atlas') {atlasBusy=false;const record=atlasCache.put(data.q,data.pixels,data.features);applyAtlas(record,data.q,data.request);pumpAtlas();}
      else if(data.type==='mesh' || data.type==='gi') {streamResults.push(data);}
      else if(data.type==='error') stopStreamingWorker(data.message);
    };
    streamWorker.onerror=(event)=>{event.preventDefault();stopStreamingWorker(event.message);};
    streamWorker.postMessage({type:'init',seed,identity:worldIdentity});
  } catch(error) {stopStreamingWorker(error);}
}
function pumpStreaming() {
  if(!streamWorker)return;
  const began=performance.now();
  // Small ready packets may share a frame; expensive packing is already done.
  // At most two packets exist, so uploads cannot accumulate an unbounded queue.
  while(streamResults.length && performance.now()-began<2) {
    const result=streamResults.shift();
    if(result.type==='gi') {giOutstanding=false;if(!game.accept_gi_result(result.ticket,result.bytes)){stopStreamingWorker('Invalid lighting packet');return;}continue;}
    streamOutstanding--;
    if(!game.accept_stream_result(result.ticket,result.bytes)){stopStreamingWorker('Invalid mesh packet');return;}
  }
  if(streamReady && !streamOutstanding && !giOutstanding)streamDeadline=0;
  // Completed packets can span multiple upload frames after an inactive view
  // resumes. A queued result is evidence of progress, not a stalled worker.
  if(!streamResults.length && streamDeadline && performance.now()>streamDeadline){stopStreamingWorker('Worker timed out');return;}
  if(!streamReady)return;
  while(streamOutstanding<2) {
    const job=game.next_stream_job();if(!job.length)break;
    streamWorker.postMessage({type:'generate',job});streamOutstanding++;
    streamDeadline=performance.now()+120000;
  }
  if(!giOutstanding && game.next_gi_job) {const job=game.next_gi_job();if(job){giOutstanding=true;streamWorker.postMessage({type:'generateGi',...job});streamDeadline=performance.now()+120000;}}
  if(!streamOutstanding && !giOutstanding)streamDeadline=0;
}

function renderFrame(now) {
  soundscape.setActive(started && !document.hidden && !otherViewActive && !fatal && !benchmark && !motionCapture && soundscape.volumes.master>0);
  if (fatal) return;
  // Hidden previews must not keep submitting GPU work or advancing the world.
  // Keep one RAF chain: the browser resumes it when this tab becomes visible.
  if (document.hidden || otherViewActive) { adaptive.sample(now, 0, false); lastFrame = 0; requestAnimationFrame(renderFrame); return; }
  if (!lastFrame) adaptive.reset(now);
  const frameGap = lastFrame ? now-lastFrame : 0;
  const frameStart = performance.now();
  const dt = lastFrame ? Math.min((now - lastFrame) / 1000, .05) : 1 / 60;
  lastFrame = now;
  try {
    if (benchmark) updateBenchmark(now, frameGap);
    if (motionCapture) updateMotionCapture(now);
    updateAdaptiveResolution(now, frameGap);
    const moving = started && !modal && !quickMenuOpen && !document.hidden && !benchmark && !motionCapture;
    let forward = moving ? Number(keys.has('KeyW') || keys.has('ArrowUp') || touchMoves.has('forward')) - Number(keys.has('KeyS') || keys.has('ArrowDown') || touchMoves.has('back')) : 0;
    let strafe = moving ? Number(keys.has('KeyD') || keys.has('ArrowRight') || touchMoves.has('right')) - Number(keys.has('KeyA') || keys.has('ArrowLeft') || touchMoves.has('left')) : 0;
    // Avoid diagonal movement being faster than walking straight.
    const length = Math.hypot(forward, strafe); if (length > 1) { forward /= length; strafe /= length; }
    const sprint = moving && (keys.has('ShiftLeft') || keys.has('ShiftRight') || touchMoves.has('sprint'));
    if (benchmark?.walking && benchmark.phase === "sample") { forward=1;strafe=0; }
    if (motionCapture?.phase === "record") {forward=1;strafe=0;}
    pumpStreaming();
    game.combat_input?.(moving && combatAttack,moving && (combatBlock || keys.has('KeyV')), !moving);
    combatAttack=false;
    game.tick((modal && modal !== 'settings') || !started ? 0 : dt, forward, strafe, sprint || !!(benchmark?.walking && benchmark.phase === "sample"), moving && jumpQueued);
    jumpQueued = false;
    if (game.combat_frame) {
      const packet=game.combat_frame();combatActive=!!packet[23];weaponDrawn=!!packet[0];trialActive=!!packet[26];
      $('defeat-actions').classList.toggle('hidden',packet[17]>0||trialActive||!!modal||!started);
      for(const event of game.combat_events()) {
        if(event.kind.endsWith('-xp')||event.kind.endsWith('-rank')){syncCharacter();notifySkill(event);}
        else {combatView.event(event);soundscape.combat?.(event,state);if(event.kind==='victory'||event.kind==='defeat')saveProgress();}
      }
      combatView.draw(packet,moving?dt:0,{visible:started&&!modal&&!quickMenuOpen,moving:Math.abs(forward)+Math.abs(strafe)>.01});
      const combatStatus=combatActive?(packet[10]===1?'Victory — T to rematch':packet[10]===2?'Defeated — T to rematch':(trialActive?'Sparring trial active':'Hostile encounter')):'No active encounter';
      if($('combat-status').textContent!==combatStatus)$('combat-status').textContent=combatStatus;
    }
    const compassVisible = started && !modal && !document.body.classList.contains('photo-mode');
    const audioNeedsFrame = soundscape.ready && soundscape.active;
    if (game.lightning_audio_frame && (compassVisible || audioNeedsFrame)) {
      // This small engine packet already carries rendered camera x/y/z/yaw at
      // indices 7–10. Share one read with audio instead of probing game.state()
      // (terrain, settlements, weather, acoustics) at the rendering frequency.
      const frame = game.lightning_audio_frame();
      if (audioNeedsFrame) soundscape.updateLightningFrame(frame);
      if (compassVisible) updateCompass(frame[7], frame[9], frame[10]);
    }
    if (now-lastHUD > 100) {
      state = game.state();
      soundscape.update(state,{moving,dialogue:modal==='dialogue',lightningPerFrame:true});
      $('sound-status').textContent = soundscape.failed ? 'Audio unavailable' : soundscape.loading ? 'Loading audio…' : !soundscape.ctx ? '' : soundscape.volumes.master===0 ? 'Muted' : '';
    }
    if (benchmark?.phase === "sample") benchmark.cpu.push(performance.now()-frameStart);
    if (!initialReady && (!game.is_ready || game.is_ready())) {
      initialReady = true;syncCharacter();
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
  soundscape.setActive(false);
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
    initRenderCoordination();
    $('loading-label').textContent = 'Preparing the world engine…';
    if (navigator.gpu.requestAdapter) {
      const adapter = await navigator.gpu.requestAdapter({powerPreference:'high-performance'});
      const info = adapter?.info;
      if (info) adapterLabel = [info.vendor,info.architecture,info.description].filter(Boolean).join(' · ') || 'WebGPU';
    }
    const { default: init, Game } = await import('./pkg/fantasy_land.js?v=encounters-1');
    await init({ module_or_path: new URL('./pkg/fantasy_land_bg.wasm?v=encounters-1', location.href) });
    $('loading-label').textContent = 'Carving rivers, raising hills, finding a road…';
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    game = await Game.create(canvas, seed);
    if(worldKey(game.world_identity())!==worldKey(worldIdentity))throw new Error("Engine version mismatch. Reload to update the engine.");
    startStreamingWorker();
    worldSize = Number(game.world_size());
    game.set_quality(quality);
    applyResolution();
    game.set_antialiasing(antialiasing);
    applyFilter();
    applyGroundCoverDensity();
    game.set_shadows(sunShadows);
    game.set_lighting_mode(lightingMode);
    applyWeatherPreferences();
    resize();
    if(playedSave)game.restore_snapshot(playedSave.snapshot);
    const destinations=game.settlement_destinations();
    for(const d of destinations){const option=document.createElement('option');option.value=d.id;option.textContent=`${d.kind[0].toUpperCase()+d.kind.slice(1)} · ${d.name} · ${d.region}`;$('settlement-select').append(option);}
    state = game.state();
    // Exposed intentionally for integration checks and world-generation inspection.
    window.fantasyDebug = { game, atlasCache, worldIdentity, get state() { return state; }, get map() { return map; }, get waypoint() { return waypoint; }, openMap, closeModal, saveProgress, get input() { return { started, locked, focusedLook, pointerLockFallback, lockPending, modal }; }, captureMouse, get renderActive() {return !otherViewActive && !document.hidden;}, version: 'encounters-1' };
    requestAnimationFrame(renderFrame);
  } catch (error) { showFatal(error); }
}

function startExploring(event) {
  soundscape.setActive(!document.hidden && !otherViewActive && soundscape.volumes.master>0);
  soundscape.unlock();
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
for (const type of ['map', 'bag', 'character', 'equipment', 'skills', 'settings', 'pause']) {
  $(type + '-button').setAttribute('aria-expanded', 'false');
  $(type + '-button').addEventListener('click', () => {
    if (!started) return;
    if (modal === type) closeModal(); else if (type === 'map') openMap(); else openModal(type);
  });
}
for (const button of document.querySelectorAll('[data-close]')) button.addEventListener('click', closeModal);
for (const overlay of document.querySelectorAll('.overlay')) {
  overlay.addEventListener('click', (event) => {
    if (event.target === overlay && modal !== 'pause') backFromMenu();
  });
}
for (const button of document.querySelectorAll('[data-screen]')) {
  button.addEventListener('click', () => {
    if (button.dataset.screen === 'map') openMap();
    else openModal(button.dataset.screen);
  });
}
for (const button of document.querySelectorAll('[data-option]')) {
  button.addEventListener('click', () => showOptionsPage(button.dataset.option));
}
for (const button of document.querySelectorAll('[data-back]')) button.addEventListener('click', backFromMenu);
$('options-button').addEventListener('click', () => openModal('options'));
$('resume-button').addEventListener('click', resumeJourney);
$('title-button').addEventListener('click', returnToTitle);
$('ui-text-size').addEventListener('change', (event) => {
  uiTextSize = event.target.value;
  applyInterfacePreferences();
  saveProgress();
});
$('ui-hud-detail').addEventListener('change', (event) => {
  uiHudDetail = event.target.value;
  applyInterfacePreferences();
  saveProgress();
});
$('ui-contrast').addEventListener('change', (event) => {
  uiContrast = event.target.value;
  applyInterfacePreferences();
  saveProgress();
});
applyInterfacePreferences();
$('quick-menu-toggle').addEventListener('click', () => setQuickMenu(!quickMenuOpen, true));
$('hud-options-button').addEventListener('click', () => openModal('options'));
document.addEventListener('pointerdown', event => {
  if (quickMenuOpen && !$('quick-menu').contains(event.target) && !$('quick-menu-toggle').contains(event.target)) setQuickMenu(false);
});

$('skills-open-map').addEventListener('click', openMap);
$('zoom-in').addEventListener('click', () => zoomMap(.70));
$('zoom-out').addEventListener('click', () => zoomMap(1.43));
$('center-map').addEventListener('click', () => { map.x = state.x; map.z = state.z; scheduleMapData(0); });
$('fit-map').addEventListener('click', fitWorld);
$('set-waypoint').addEventListener('click', () => {
  if (!map.selected) return;
  activeJourney = null; waypoint = { ...map.selected };
  saveProgress(); updateSelection();
  toast(`Waypoint set: ${waypoint.name}`);
  closeModal();
});
$('fast-travel').addEventListener('click', () => {
  if (!map.selected) return;
  const destination = { ...map.selected };
  game.teleport(destination.x, destination.z);
  activeJourney = null; state = game.state(); waypoint = destination;
  saveProgress(); closeModal();
  toast(`Arrived at ${destination.name}.`);
});
$('clear-waypoint').addEventListener('click', () => { activeJourney = null; waypoint = null; saveProgress(); updateSelection(); toast('Waypoint cleared.'); });
$('quality-select').addEventListener('change', (event) => { quality = Number(event.target.value); game?.set_quality(quality); updateRenderDimensions(); saveProgress(); });
$('ground-cover-density').addEventListener('input', (event) => {
  const percent = Number(event.target.value);
  groundCoverDensity = Number.isFinite(percent) ? clamp(percent / 100, 0, 4) : 4;
  // The engine updates a GPU density setting immediately; no terrain regeneration.
  applyGroundCoverDensity();
  saveProgress();
});
$('meadow-carpet').addEventListener('change', event => {
  meadowCarpet = event.target.value !== 'off'; game?.set_meadow(meadowCarpet); saveProgress();
});
$('lighting-mode').addEventListener('change',event=>{lightingMode=Number(event.target.value);game?.set_lighting_mode(lightingMode);saveProgress();});
$('sun-shadows').addEventListener('change', (event) => {
  sunShadows = event.target.value !== 'off';
  game?.set_shadows(sunShadows);
  saveProgress();
});
$('weather-mode').addEventListener('change', (event) => {
  const mode = Number(event.target.value);
  weatherMode = Number.isInteger(mode) && mode >= 0 && mode <= 8 ? mode : 0;
  soundscape.cancelThunder();
  game?.set_weather_mode(weatherMode);
  updateWeatherControls(); saveProgress();
});
$('weather-speed').addEventListener('input', (event) => {
  const speed = Number(event.target.value);
  weatherSpeed = Number.isFinite(speed) ? clamp(speed, .25, 20) : 1;
  game?.set_weather_speed(weatherSpeed);
  updateWeatherControls(); saveProgress();
});
$('weather-paused').addEventListener('change', (event) => {
  weatherPaused = event.target.value === 'paused';
  game?.set_weather_paused(weatherPaused);
  updateWeatherControls(); saveProgress();
});
$('water-reflections').addEventListener('change', (event) => {
  reflections = event.target.value !== 'off';
  game?.set_reflections(reflections);
  saveProgress();
});
$('enclosure-shading').addEventListener('change', (event) => {
  enclosure = event.target.value !== 'off';
  game?.set_enclosure(enclosure);
  saveProgress();
});
$('antialiasing').addEventListener('change', event => {
  const selected = Number(event.target.value);
  antialiasing = [0,1,2].includes(selected) ? selected : 1;
  game?.set_antialiasing(antialiasing); adaptive.reset(performance.now()); updateAaControls(); saveProgress();
});
$('restore-visual-defaults').addEventListener('click', restoreVisualDefaults);
$('filter-select').addEventListener('change', (event) => {
  const selected = Number(event.target.value);
  filterMode = [0, 1, 2].includes(selected) ? selected : 1;
  applyFilter();
  saveProgress();
});
$('filter-strength').addEventListener('input', (event) => {
  if (filterMode === 0) return;
  const amount = Number(event.target.value);
  filterStrength = Number.isFinite(amount) ? clamp(amount / 100, 0, 1.5) : 1;
  applyFilter();
  saveProgress();
});
$('render-resolution').addEventListener('change', (event) => {
  if (event.target.value === 'adaptive') {
    adaptiveResolution = true; applyResolution(); saveProgress(); return;
  }
  const selected = Number(event.target.value);
  renderResolution = RESOLUTION_OPTIONS.includes(selected) ? selected : 1;
  $('render-resolution').value = String(renderResolution);
  adaptiveResolution = false;
  applyResolution();
  saveProgress();
});
$('sensitivity').addEventListener('input', (event) => { sensitivity = Number(event.target.value); saveProgress(); });
$('time-setting').addEventListener('input', (event) => { game?.set_time(Number(event.target.value)); $('time-setting-label').textContent = formatTime(Number(event.target.value)); });
for (const [id, hour] of [['sky-dawn', 6.4], ['sky-day', 12], ['sky-dusk', 17.7], ['sky-night', 22]]) {
  $(id).addEventListener('click', () => {
    game?.set_time(hour);
    $('time-setting').value = String(hour);
    $('time-setting-label').textContent = formatTime(hour);
  });
}
$('export-world').addEventListener('click',()=>{
  // Export works even when browser storage is full.
  saveProgress();
  const data={schema:1,updated:Date.now(),snapshot:game.save_snapshot(),waypoint,atlas:map.initialized?{x:map.x,z:map.z,span:map.span}:null};
  const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:'application/json'}));
  const link=document.createElement('a');link.href=url;link.download=`fantasyland-${worldKey(worldIdentity)}.json`;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('import-world').addEventListener('change',async event=>{
  try {
    const file=event.target.files?.[0];if(!file)return;
    if(file.size>4*1024*1024)throw new Error('Save file exceeds 4 MB.');
    const data=validateSave(JSON.parse(await file.text()));
    saveProgress();worldStore.write(data);importingSave=true;
    const url=new URL(location.href);url.searchParams.set('seed',data.snapshot.world.seed);location.href=url.href;
  }catch(error){toast(String(error.message||error),7000);}finally{event.target.value='';}
});
$('seed-form').addEventListener('submit', (event) => {
  event.preventDefault();
  saveProgress();
  const nextSeed = validSeed($('seed-input').value, DEFAULT_SEED);
  const url = new URL(location.href); url.searchParams.set('seed', nextSeed);
  location.href = url.href;
});
$('landscape-destination').addEventListener('focus', loadLandscapeDestinations);
$('landscape-destination').addEventListener('pointerdown', loadLandscapeDestinations);
$('landscape-destination').addEventListener('change', () => {
  $('travel-landscape').disabled = !selectedLandscapeDestination();
});
$('landscape-travel-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const destination = selectedLandscapeDestination();
  if (!destination || !game || !initialReady) return;
  game.teleport(destination.x, destination.z);
  game.face?.(destination.yaw, destination.pitch);
  activeJourney = null; state = game.state(); waypoint = { ...destination };
  saveProgress(); closeModal();
  toast(`Arrived at ${destination.name}.`);
});
$('wonders-select').addEventListener('focus', loadNaturalWonders);
$('wonders-select').addEventListener('pointerdown', loadNaturalWonders);
$('wonders-select').addEventListener('change', () => { $('wonders-travel').disabled = !selectedWonder(); });
$('wonders-form').addEventListener('submit', event => {
  event.preventDefault(); const d = selectedWonder(); if (!d || !game || !initialReady) return;
  activeJourney = null; game.teleport(d.x, d.z); game.face?.(d.yaw,d.pitch); state = game.state();
  waypoint = {name:d.name,x:d.landmark_x ?? d.x,z:d.landmark_z ?? d.z,kind:'natural'};
  saveProgress(); closeModal(); toast(`Arrived beside ${d.name}.`);
});
$('walking-select').addEventListener('focus', loadWalkingJourneys);
$('walking-select').addEventListener('pointerdown', loadWalkingJourneys);
$('walking-select').addEventListener('change', () => { $('walking-start').disabled = !selectedWalk(); });
$('walking-form').addEventListener('submit', event => {
  event.preventDefault(); const walk = selectedWalk(); if (!walk || !game || !initialReady) return;
  game.teleport(walk.x, walk.z); game.face?.(walk.yaw, walk.pitch); state = game.state();
  activeJourney = { ...walk, next: 1 }; updateWalk(); map.dirty = true;
  saveProgress(); closeModal(); toast(`${walk.name} · follow the route in your atlas.`);
});
$('return-to-spawn').addEventListener('click', () => {
  activeJourney = null; game.return_to_spawn(); state = game.state(); saveProgress(); closeModal(); toast('Back on the starting road.');
});

const TOPIC_LABELS = {work:'Your work', life:'Your story', home:'Your home', area:'This settlement', road:'Your destination', weather:'Local news', inn:'Nearest inn', smith:'Blacksmith', guild:'Fighters’ guild', arcane:'Arcane guild', temple:'Temple', market:'Market'};
// Conversations keep a short transcript; every topic is a native, focusable button.
function conversationLine(question, text) {
  const log = $('dialogue-text');
  const entry = document.createElement('div'); entry.className = 'conversation-entry';
  const speaker = document.createElement('span'); speaker.className = 'conversation-speaker';
  speaker.textContent = question ? `> ${question}` : $('dialogue-name').textContent;
  const answer = document.createElement('p'); answer.textContent = text;
  entry.append(speaker, answer); log.append(entry);
  while (log.children.length > 12) log.firstElementChild.remove();
  log.scrollTop = log.scrollHeight;
}
function renderConversation(data) {
  if (!data) return;
  paintPortrait($('dialogue-portrait'), data);
  $('dialogue-name').textContent = data.name;
  $('dialogue-role').textContent = data.role.toUpperCase();
  $('dialogue-detail').textContent = data.detail;
  $('dialogue-text').replaceChildren(); conversationLine(null, data.text);
  const topics = $('dialogue-topics'); topics.replaceChildren();
  data.topics.forEach((topic, index) => {
    const button = document.createElement('button'); button.type = 'button';
    const number = document.createElement('kbd'); number.textContent = index < 9 ? `${index + 1}` : String.fromCharCode(65 + index - 9);
    const label = document.createElement('span'); label.textContent = TOPIC_LABELS[topic.id] || topic.label;
    button.setAttribute('aria-label', `${index + 1}. ${topic.label}`);
    button.append(number, label);
    button.addEventListener('click', () => {
      const answer = game.dialogue(topic.id);
      if (answer) conversationLine(topic.label, answer.text);
      for (const item of topics.children) item.removeAttribute('aria-current');
      button.setAttribute('aria-current', 'true');
    });
    topics.append(button);
  });
}
function interact() {
  const result = game?.interact(); if (!result) return;
  if(result.kind==='loot'){showLoot(result);return;}
  if (typeof result === 'string') { toast(result); return; }
  modalReturnFocus = document.activeElement;
  renderConversation(result); modal = 'dialogue'; releaseMouse();
  $('dialogue-modal').classList.remove('hidden'); document.body.classList.add('modal-open');
  ($('dialogue-topics').firstElementChild || $('dialogue-close')).focus();
}
$('dialogue-close').addEventListener('click', closeModal);
function modalKeyboard(event) {
  if (!modal || event.altKey || event.ctrlKey || event.metaKey) return false;
  const panel = $(modal + '-modal');
  if (event.code === 'Tab') {
    const controls = [...panel.querySelectorAll('button, input, select, textarea, summary, a[href], [tabindex="0"]')]
      .filter(el => !el.disabled && el.getClientRects().length);
    const current = controls.indexOf(document.activeElement);
    if (controls.length) controls[current < 0 ? (event.shiftKey ? controls.length - 1 : 0) : (current + (event.shiftKey ? -1 : 1) + controls.length) % controls.length].focus();
    event.preventDefault(); return true;
  }
  if (modal !== 'dialogue') return false;
  if (event.target === $('dialogue-text')) return true;
  const topics = [...$('dialogue-topics').children];
  const digit = /^(?:Digit|Numpad)([1-9])$/.exec(event.code);
  const choice = digit ? Number(digit[1]) - 1 : ({KeyA:9,KeyB:10,KeyC:11})[event.code];
  if (choice !== undefined && topics[choice]) {
    const button = topics[choice]; button.focus(); button.click(); event.preventDefault();
  } else if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.code) && topics.length) {
    const index = topics.indexOf(document.activeElement);
    const next = event.code === 'Home' ? 0 : event.code === 'End' ? topics.length - 1 : index < 0 ? (event.code === 'ArrowUp' ? topics.length - 1 : 0) : (index + (event.code === 'ArrowUp' ? -1 : 1) + topics.length) % topics.length;
    topics[next].focus(); event.preventDefault();
  }
  // Enter/Space retain native button activation, all game shortcuts stay outside.
  return true;
}
$('settlement-form').addEventListener('submit',event=>{event.preventDefault();const id=Number($('settlement-select').value);if(!id||!game)return;game.visit_settlement(id);state=game.state();closeModal();toast('Arrived. Follow the lanes; press E to talk or open a door.');});

function startCombatTrial() {
  if(!game||!initialReady)return;
  if(!(game.start_combat_kind?game.start_combat_kind(Number($('trial-foe').value)):game.start_combat())){toast('No safe sparring space here. Move to a clear road or open ground.');return;}
  combatActive=true;trialActive=true;combatView.reset();clearMovement();state=game.state();closeModal();
  $('toast').classList.remove('visible');$('toast').textContent='';
}
function leaveCombatTrial(){game?.stop_combat();combatActive=false;trialActive=false;clearMovement();combatView.reset();state=game.state();toast('Trial ended. Your original position and health are restored.');}
$('combat-start').addEventListener('click',startCombatTrial);
$('combat-stop').addEventListener('click',()=>{leaveCombatTrial();closeModal();});
const menuKeys = { KeyM: 'map', Tab: 'map', KeyI: 'bag', KeyC: 'character', KeyK: 'skills', KeyO: 'settings' };
document.addEventListener('keydown', (event) => {
  const editing = ['INPUT', 'SELECT', 'TEXTAREA'].includes(event.target.tagName);
  if (event.code === 'Escape') {
    event.preventDefault();
    if (motionCapture) {finishMotionCapture(true);toast('Recording cancelled.');}
    if (benchmark) {finishBenchmark(true);toast('Performance check cancelled.');}
    if (event.repeat) return;
    if (quickMenuOpen) { setQuickMenu(false, true); return; }
    if (modal) backFromMenu();
    else if (started) openModal('pause');
    releaseMouse();
    return;
  }
  if (quickMenuKeyboard(event) || modalKeyboard(event)) return;
  if (editing) return;
  if (event.code === 'F4') { event.preventDefault(); document.body.classList.toggle('photo-mode'); return; }
  if (event.code === 'F3') { event.preventDefault(); $('diagnostics').classList.toggle('hidden'); return; }
  if (!started) {
    if (event.code === 'Enter' && initialReady) { event.preventDefault(); startExploring(event); }
    return;
  }
  if(event.code==='F6'&&!event.repeat&&!modal){event.preventDefault();if(trialActive)leaveCombatTrial();else startCombatTrial();return;}
  if (event.code === 'KeyQ' && !modal && !event.altKey && !event.ctrlKey && !event.metaKey) {
    event.preventDefault();
    if (event.repeat) return;
    setQuickMenu(!quickMenuOpen);
    if (!quickMenuOpen) captureMouse(event);
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
  if (quickMenuOpen) return;
  if(!modal){
    if(event.code==='KeyR'&&!event.repeat){event.preventDefault();game.toggle_weapon?.();return;}
    if(event.code==='KeyF'&&!event.repeat){event.preventDefault();combatAttack=true;return;}
    if(event.code==='KeyV'){event.preventDefault();keys.add('KeyV');return;}
    if(trialActive&&event.code==='KeyT'&&!event.repeat){event.preventDefault();startCombatTrial();return;}

  }
  if(event.code==='KeyE'&&!event.repeat&&!modal){event.preventDefault();interact();return;}
  if (modal) {
    if (modal === 'map'  && event.target === mapCanvas) {
      const pan = map.span * (event.shiftKey ? .2 : .08);
      if (event.code === 'Enter') { event.preventDefault(); const {w,h} = mapGeometry(); selectMapPoint(w/2,h/2); }
      if (event.code === 'Home') { event.preventDefault(); map.x = state.x; map.z = state.z; scheduleMapData(0); }
      if (event.code === 'End') { event.preventDefault(); fitWorld(); }
      if (event.code === 'BracketLeft' || event.code === 'BracketRight') {
        event.preventDefault();
        const places = [...map.visibleFeatures].sort((a,b) => a.py - b.py || a.px - b.px);
        if (places.length) {
          const index = places.findIndex(p => map.selected && p.x === map.selected.x && p.z === map.selected.z);
          map.selected = {...places[index < 0 ? (event.code === 'BracketLeft' ? places.length - 1 : 0) : (index + (event.code === 'BracketLeft' ? -1 : 1) + places.length) % places.length]};
          updateSelection();
        }
      }
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
  const browserReleasedMouse = locked && !hasLock && focusedLook && !modal;
  locked = hasLock;
  clearTimeout(lockTimer); lockPending = false;
  if (locked) { focusedLook = true; pointerLockFallback = false; dragLook = null; }
  else { focusedLook = false; lastMouse = null; clearMovement(); }
  if (browserReleasedMouse && started && !document.hidden) openModal('pause');
  updateFocusHint();
});
document.addEventListener('pointerlockerror', () => lockFailed());
document.addEventListener('mousemove', (event) => {
  if (!started || modal || !game || benchmark || motionCapture) return;
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
  if(started&&!modal&&!quickMenuOpen&&(locked||focusedLook)&&!benchmark&&!motionCapture&&event.pointerType!=='touch'){
    if(event.button===0){combatAttack=true;event.preventDefault();}
    if(event.button===2){combatBlock=true;event.preventDefault();}
  }
  if (!started || modal || !game || locked || benchmark || motionCapture) return;
  canvas.focus({ preventScroll: true });
  // Desktop lock uses click; mouse drag fallback is document-level, so there is
  // no setPointerCapture call racing requestPointerLock. Touch keeps capture.
  dragLook = { id: event.pointerId, x: event.clientX, y: event.clientY, touch: event.pointerType !== 'mouse' };
  if (dragLook.touch) canvas.setPointerCapture(event.pointerId);
});
document.addEventListener('pointermove', (event) => {
  if (benchmark || motionCapture || dragLook?.id !== event.pointerId || locked || modal || !started || (focusedLook && !dragLook.touch)) return;
  game.look((event.clientX - dragLook.x) * sensitivity, (event.clientY - dragLook.y) * sensitivity);
  dragLook.x = event.clientX; dragLook.y = event.clientY;
});
document.addEventListener('pointerup', (event) => { if(event.button===2)combatBlock=false; if (dragLook?.id === event.pointerId) dragLook = null; });
document.addEventListener('pointercancel', (event) => { combatBlock=false; if (dragLook?.id === event.pointerId) dragLook = null; });
canvas.addEventListener('contextmenu', (event) => event.preventDefault());
for (const button of document.querySelectorAll('[data-move]')) {
  const move = button.dataset.move;
  button.addEventListener('pointerdown', (event) => { event.preventDefault(); touchMoves.add(move); button.classList.add('active'); button.setPointerCapture(event.pointerId); });
  const release = () => { touchMoves.delete(move); button.classList.remove('active'); };
  button.addEventListener('pointerup', release); button.addEventListener('pointercancel', release); button.addEventListener('lostpointercapture', release);
}
$('touch-jump').addEventListener('pointerdown', (event) => { event.preventDefault(); if (started && !modal) jumpQueued = true; });
mapCanvas.addEventListener('focus', () => { map.dirty = true; });
mapCanvas.addEventListener('blur', () => { map.dirty = true; });
document.fonts?.ready.then(() => { map.dirty = true; });
mapCanvas.addEventListener('pointerdown', (event) => {
  mapCanvas.focus();
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
  selectMapPoint(px, py);
});
function selectMapPoint(px, py) {
  const nearest = map.visibleFeatures.map((feature) => ({ feature, distance: Math.hypot(feature.px - px, feature.py - py) })).sort((a, b) => a.distance - b.distance)[0];
  if (nearest && nearest.distance < 18) map.selected = { ...nearest.feature };
  else {
    const point = screenToWorld(px, py);
    if (Math.abs(point.x) > worldSize / 2 || Math.abs(point.z) > worldSize / 2) { toast('This point lies beyond the surveyed world.'); return; }
    map.selected = { ...point, name: 'Uncharted wilderness', kind: 'point' };
  }
  updateSelection();
}
mapCanvas.addEventListener('pointercancel', () => { map.dragging = null; mapCanvas.style.cursor = 'crosshair'; scheduleMapData(); });
mapCanvas.addEventListener('wheel', (event) => {
  event.preventDefault();
  const rect = mapCanvas.getBoundingClientRect();
  zoomMap(Math.exp(clamp(event.deltaY, -150, 150) * .002), event.clientX - rect.left, event.clientY - rect.top);
}, { passive: false });
window.addEventListener('resize', () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(() => { resize(); if (modal === 'map') scheduleMapData(); }, 100); });
window.addEventListener('blur', releaseMouse);
document.addEventListener('visibilitychange', () => { if (!document.hidden && streamWorker && streamDeadline) streamDeadline=performance.now()+120000; if (document.hidden) { soundscape.setActive(false); if (motionCapture) finishMotionCapture(true); if (benchmark) finishBenchmark(true); adaptive.reset(performance.now()); releaseMouse(); saveProgress(); } lastFrame = 0; });
window.addEventListener('pagehide', saveProgress);

// Short real-browser motion evidence. Encoding never runs during FPS checks.
function finishMotionCapture(cancelled = false) {
  const capture = motionCapture; if (!capture) return;
  motionCapture = null;
  if (cancelled) motionGeneration++;
  if (capture.recorder.state !== 'inactive') capture.recorder.stop();
  for (const track of capture.stream.getTracks()) track.stop();
  game.set_time(capture.hour); game.teleport(capture.x,capture.z); game.face(capture.yaw,capture.pitch);
  state=game.state();
  adaptive.reset(performance.now());
  $('motion-record').disabled = false;
  $('motion-status').textContent = cancelled ? 'Recording interrupted.' : 'Preparing walking clip…';
  openModal('settings');
}
function updateMotionCapture(now) {
  const capture = motionCapture;
  game.set_time(capture.hour);
  if (capture.phase === 'warm') {
    if (now-capture.since<2500 || game.pending_chunks()>0) return;
    capture.phase='record';capture.since=now;capture.recorder.start();
    toast('Recording 8-second walk · current AA setting',8000);
  }
  game.face(capture.yaw + Math.sin((now-capture.since)/8000*Math.PI*2)*0.07,capture.pitch);
  if (capture.phase==='record' && now-capture.since>=8000) finishMotionCapture();
}
$('motion-record').addEventListener('click', () => {
  if (!game || benchmark || motionCapture) return;
  if (!canvas.captureStream || typeof MediaRecorder==='undefined') {toast('Canvas recording is unavailable in this browser.');return;}
  const mime=['video/mp4','video/webm;codecs=vp9','video/webm'].find(t=>MediaRecorder.isTypeSupported(t));
  let stream, recorder;
  try {stream=canvas.captureStream(30);recorder=new MediaRecorder(stream,mime?{mimeType:mime,videoBitsPerSecond:10000000}:{videoBitsPerSecond:10000000});}
  catch(error) {stream?.getTracks().forEach(t=>t.stop());toast('This browser could not start canvas recording.');return;}
  $('motion-download').classList.add('hidden');
  $('motion-status').textContent='Warming up the current view…';
  const token=++motionGeneration;
  const chunks=[];const mode=['off','fxaa','smaa'][antialiasing];const height=game.render_resolution()[1];
  recorder.ondataavailable=event=>{if(event.data.size)chunks.push(event.data);};
  recorder.onstop=()=>{
    if (token!==motionGeneration) return;
    const blob=new Blob(chunks,{type:recorder.mimeType});
    if (!blob.size) {$('motion-status').textContent='The browser returned an empty recording.';return;}
    const reader=new FileReader();reader.onload=()=>{
      if (token!==motionGeneration) return;
      $('motion-download').href=reader.result;
      $('motion-download').download=`fantasyland-${mode}-${height}p.${recorder.mimeType.includes('mp4')?'mp4':'webm'}`;
      $('motion-download').classList.remove('hidden');
      $('motion-status').textContent=`${mode.toUpperCase()} · ${height}p · 8-second browser capture ready.`;
    };reader.readAsDataURL(blob);
  };
  recorder.onerror=()=>{if(token===motionGeneration && motionCapture?.recorder===recorder){finishMotionCapture(true);toast('Canvas recording was interrupted.');}};
  state=game.state();motionCapture={recorder,stream,phase:'warm',since:performance.now(),x:state.x,z:state.z,yaw:state.yaw,pitch:state.pitch,hour:state.dayTime};
  $('motion-record').disabled=true;closeModal();clearMovement();
});

// Repeatable frame-pacing comparison through the real browser animation loop.
// Initialization/streaming warm-up is shown separately, never included as steady FPS.
function finishBenchmark(cancelled = false) {
  if (!benchmark) return;
  applyResolution();
  const restored=benchmark.restore;
  game.set_time(restored.hour);
  game.set_antialiasing(antialiasing);
  $('diagnostics').classList.toggle('hidden',benchmark.diagnosticsHidden);
  if (benchmark.walking) {game.teleport(restored.x,restored.z);game.face(restored.yaw,restored.pitch);}
  else game.face(benchmark.yaw, benchmark.pitch);
  benchmark = null;
  state=game.state();saveProgress();
  $('benchmark-start').disabled = false;
  $('benchmark-aa').disabled = false;
  $('benchmark-walk').disabled = false;
  $('benchmark-result').textContent = (cancelled ? 'Interrupted; keep this tab visible to measure.\n' : '') + benchmarkReport.map(r => `${["Off","FXAA","SMAA"][r.antialiasing]} · ${r.width} × ${r.actualHeight} · ${r.fps.toFixed(1)} FPS · p95 ${r.p95.toFixed(1)} ms · p99 ${r.p99.toFixed(1)} ms · CPU ${r.cpu.toFixed(1)} ms · ${r.hitches} frames >33 ms${r.walking ? ` · walked ${r.distance.toFixed(0)} m / ${r.chunks} chunks` : ""}`).join('\n');
  $('benchmark-result').setAttribute('data-report',JSON.stringify(benchmarkReport));
  if (!cancelled) openModal('settings');
}
function beginBenchmark(walking = false, compareAa = false) {
  if (!game || benchmark || motionCapture) return;
  state = game.state();
  benchmarkReport = [];
  $('benchmark-result').textContent = ''; $('benchmark-result').setAttribute('data-report', '');
  benchmark = {compareAa,aa:compareAa?0:antialiasing,height:420,phase:'warm',since:performance.now(),samples:[],cpu:[],yaw:Number(state.yaw),pitch:Number(state.pitch),size:null,walking,hour:Number(state.dayTime ?? 9),diagnosticsHidden:$('diagnostics').classList.contains('hidden'),restore:{x:state.x,z:state.z,yaw:state.yaw,pitch:state.pitch,hour:Number(state.dayTime ?? 9)},chunks:new Set(),distance:0};
  if (walking) {game.return_to_spawn();state=game.state();benchmark.yaw=state.yaw;benchmark.pitch=state.pitch;benchmark.hour=9;}
  $('diagnostics').classList.add('hidden');
  $('benchmark-start').disabled = true;
  $('benchmark-aa').disabled = true;
  game.set_antialiasing(benchmark.aa);
  $('benchmark-walk').disabled = true;
  closeModal(); clearMovement();
  game.set_render_resolution(420);
  toast('420p warm-up · keep the game visible',3500);
}
function updateBenchmark(now,gap) {
  const b = benchmark;
  game.set_time(b.hour);
  if (document.hidden) { finishBenchmark(true); return; }
  if (b.phase === 'warm') {
    if (now-b.since < 3000 || (game.pending_chunks ? game.pending_chunks()>0 : (game.is_ready && !game.is_ready()))) return;
    b.phase='sample'; b.since=now; b.size=Array.from(game.render_resolution());b.walkStart=Number(state.walked || 0);
    toast(`${['Off','FXAA','SMAA'][b.aa]} · ${b.height}p · ${b.walking ? 'walking across streaming boundaries' : 'measuring a slow camera sweep'}`,b.walking?30000:15000);
    return;
  }
  if (!b.walking) game.face(b.yaw + Math.sin((now-b.since)/15000*Math.PI*2)*0.20,b.pitch);
  b.chunks.add(`${Math.floor(state.x/192)},${Math.floor(state.z/192)}`);
  if (gap>0) b.samples.push(gap);
  if (now-b.since<(b.walking?30000:15000)) return;
  const values=[...b.samples].sort((a,b)=>a-b);
  const percentile=q=>values[Math.min(values.length-1,Math.floor(values.length*q))] || 0;
  benchmarkReport.push({hour:b.hour,height:b.height,width:b.size[0],actualHeight:b.size[1],frames:values.length,fps:1000/(b.samples.reduce((a,b)=>a+b,0)/values.length),p95:percentile(.95),p99:percentile(.99),hitches:values.filter(t=>t>33.4).length,cpu:b.cpu.reduce((a,b)=>a+b,0)/Math.max(b.cpu.length,1),cover:Number(state.coverInstances),meshMB:Number(state.meshMegabytes),weather:state.weather?.kind||state.weather?.name||weatherMode,adapter:adapterLabel,gpuIntervals:state.gpuTimings,gpuDrawSpan:state.gpuRenderMs,worker:streamReady,pending:game.pending_chunks?.() ?? null,quality,groundCoverDensity,meadowCarpet,antialiasing:b.aa,adaptiveSuspended:adaptiveResolution,shadows:sunShadows,reflections,enclosure,walking:b.walking,distance:Number(state.walked || 0)-b.walkStart,chunks:b.chunks.size});
  if (b.height===420) {b.height=720;b.phase='warm';b.since=now;b.samples=[];b.cpu=[];b.chunks=new Set();if(b.walking){game.return_to_spawn();game.face(b.yaw,b.pitch);}game.set_render_resolution(720);toast('720p warm-up',3000);}
  else if (b.compareAa && b.aa<2) {
    b.aa++;b.height=420;b.phase='warm';b.since=now;b.samples=[];b.cpu=[];b.chunks=new Set();
    game.set_antialiasing(b.aa);game.set_render_resolution(420);
    toast(`${['Off','FXAA','SMAA'][b.aa]} · 420p warm-up`,3000);
  } else finishBenchmark();
}
$('benchmark-start').addEventListener('click',()=>beginBenchmark(false));
$('benchmark-aa').addEventListener('click',()=>beginBenchmark(false,true));
$('benchmark-walk').addEventListener('click',()=>beginBenchmark(true));
$('study-form').addEventListener('submit',event=>{
  event.preventDefault(); if(!game || benchmark)return;
  const choice=$('study-select').value;
  if(choice.startsWith('interior:')) {
    const name=game.visit_interior(choice.slice(9));
    if(!name){toast('No matching room found nearby.');return;}
    activeJourney=null;closeModal();clearMovement();state=game.state();saveProgress();
    toast(`${name} · walk around, or press E to talk`);return;
  }
  if(seed!==1337){toast('These studies use seed 1337. Landscape travel works with every seed.');return;}
  const studies={meadow:[-57269,20719,0,-.20,9], 'meadow-dawn':[-57269,20719,1.7,-.10,6.5], 'meadow-dusk':[-57269,20719,1.7,-.10,17.25],hearth:[-16211.261,-12684.719,-1.4056476,-.08339161,22],forest:[-10879,58547,1.4,.08,7.5],lake:[-8909.148,-77660.938,-.2618,-.0438,9.3],moon:[-8909.148,-77660.938,-.2618,.12,23],stone:[23512.3,63468.41,-1.9067289,.27,16]};
  const v=studies[$('study-select').value]; if(!v)return;
  game.teleport(v[0],v[1]); game.face(v[2],v[3]); game.set_time(v[4]);
  weatherMode=1;game.set_weather_mode(1);updateWeatherControls();
  closeModal(); clearMovement(); state=game.state(); saveProgress();
  toast('Light study · walk anywhere from here');
});


$('atlas-layer').addEventListener('change',()=>{
  $('atlas-legend').classList.toggle('hidden',Number($('atlas-layer').value)!==0);
  $('atlas-layer-key').textContent=['Habitats, shores & farmland','Green lowlands → ochre heights → white summits','Olive loam · blue-grey granite · red sandstone · ivory chalk · violet basalt'][Number($('atlas-layer').value)];
  scheduleMapData(0);
});
$('atlas-names').addEventListener('change',()=>{map.dirty=true;});


let lootedBody=null;
function showLoot(data){lootedBody=data.id;openModal('loot');document.dispatchEvent(new CustomEvent('loot-content',{detail:data}));}
function takeLoot(id){if(!game?.take_loot(lootedBody,id))return;syncCharacter();saveProgress();const data=game.corpse_loot();if(data){document.dispatchEvent(new CustomEvent('loot-content',{detail:data}));($('loot-items').querySelector('button')||$('loot-modal').querySelector('[data-close]')).focus();}else closeModal();}
document.addEventListener('loot-take',({detail})=>takeLoot(detail));
$('loot-all').addEventListener('click',()=>takeLoot('all'));

boot();
