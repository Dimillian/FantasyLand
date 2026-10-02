import assert from 'node:assert/strict';
import fs from 'node:fs';
import { tooltipPosition, createTooltips } from '../dist/ui-tooltips.js';
import { skillGroups } from '../dist/skill-catalog.js';

const sprite = ['icons.svg', 'equipment-icons.svg']
  .map((file) => fs.readFileSync(new URL(`../dist/ui/${file}`, import.meta.url), 'utf8'))
  .join('');
const icons = new Set([...sprite.matchAll(/<symbol id="([^"]+)"/g)].map((match) => match[1]));
const seen = new Set();
for (const group of skillGroups)
  for (const [name, icon, description, level, xp, next] of group.skills) {
    assert(!seen.has(name));
    seen.add(name);
    assert(icons.has(icon), `${name} must have a real icon`);
    assert(description.length && Number.isInteger(level) && level >= 1);
    assert(next > 0 && xp >= 0 && xp < next, `${name} has invalid XP`);
  }
assert(seen.has('Woodworking'));
assert.equal(seen.size, 26);
assert.equal(skillGroups.find((group) => group.name === 'Magic').skills.length, 8);
const html = fs.readFileSync(new URL('../dist/index.html', import.meta.url), 'utf8');
for (const [, name] of html.matchAll(/data-icon="([^"]+)"/g))
  assert(icons.has(name), `Missing ${name}`);
// Viewport bounds: near every corner and with a 320px-wide mobile viewport.
for (const viewport of [
  { width: 1280, height: 720 },
  { width: 320, height: 568 },
]) {
  for (const anchor of [
    { left: 8, right: 58, top: 8, bottom: 40 },
    { left: viewport.width - 58, right: viewport.width - 8, top: 8, bottom: 40 },
    { left: 8, right: 58, top: viewport.height - 40, bottom: viewport.height - 8 },
    {
      left: viewport.width - 58,
      right: viewport.width - 8,
      top: viewport.height - 40,
      bottom: viewport.height - 8,
    },
  ]) {
    const result = tooltipPosition(anchor, { width: 264, height: 185 }, viewport);
    assert(result.x >= 8 && result.y >= 8);
    assert(result.x + 264 <= viewport.width - 8 && result.y + 185 <= viewport.height - 8);
  }
}
// Lifecycle tests cover the input boundary without starting the WASM world.
class Node {
  constructor() {
    this.attrs = {};
    this.events = {};
    this.style = {};
    this.classes = new Set();
    this.classList = {
      add: (c) => this.classes.add(c),
      remove: (c) => this.classes.delete(c),
      contains: (c) => this.classes.has(c),
    };
    this.visible = true;
  }
  addEventListener(type, callback) {
    (this.events[type] ??= []).push(callback);
  }
  fire(type, data = {}) {
    const event = {
      target: this,
      preventDefault() {
        this.prevented = true;
      },
      stopImmediatePropagation() {
        this.stopped = true;
      },
      ...data,
    };
    for (const callback of this.events[type] || []) callback(event);
    return event;
  }
  getAttribute(name) {
    return this.attrs[name] ?? null;
  }
  setAttribute(name, value) {
    this.attrs[name] = value;
  }
  removeAttribute(name) {
    delete this.attrs[name];
  }
  getBoundingClientRect() {
    return { left: 100, right: 300, top: 100, bottom: 140, width: 264, height: 185 };
  }
  getClientRects() {
    return this.visible ? [this.getBoundingClientRect()] : [];
  }
  append(node) {
    node.parent = this;
  }
  closest() {
    return this.parent;
  }
  contains(node) {
    return node === this;
  }
  matches() {
    return false;
  }
}
const document = new Node(),
  window = new Node(),
  overlay = new Node(),
  trigger = new Node(),
  other = new Node();
trigger.parent = overlay;
other.parent = overlay;
trigger.setAttribute('aria-describedby', 'original-help');
let tooltip, observe;
const pendingTimers = new Map();
let timerId = 0;
function flushTimers() {
  const callbacks = [...pendingTimers.values()];
  pendingTimers.clear();
  for (const callback of callbacks) callback();
}
document.body = new Node();
document.createElement = () => (tooltip = new Node());
document.querySelectorAll = () => [overlay];
Object.assign(globalThis, {
  document,
  window,
  setTimeout: (callback) => {
    pendingTimers.set(++timerId, callback);
    return timerId;
  },
  clearTimeout: (id) => pendingTimers.delete(id),
  innerWidth: 1280,
  innerHeight: 720,
  getComputedStyle: (node) => ({ visibility: node.cssHidden ? 'hidden' : 'visible' }),
  MutationObserver: class {
    constructor(fn) {
      observe = fn;
    }
    observe() {}
  },
});
let rendered = null;
const tips = createTooltips((popup, source) => {
  rendered = source;
});
tips.bind(trigger);
tips.bind(other);
trigger.fire('focus');
assert.equal(rendered, trigger);
assert.equal(tooltip.parent, overlay);
assert.equal(trigger.getAttribute('aria-describedby'), 'original-help game-tooltip');
trigger.fire('pointerenter', { pointerType: 'mouse' });
assert.equal(trigger.getAttribute('aria-describedby'), 'original-help game-tooltip');
const escape = document.fire('keydown', { code: 'Escape' });
assert(escape.prevented && escape.stopped);
assert(tooltip.classList.contains('hidden'));
assert.equal(trigger.getAttribute('aria-describedby'), 'original-help');
other.fire('pointerenter', { pointerType: 'mouse' });
assert(
  tooltip.classList.contains('hidden'),
  'Escape must not reopen a tooltip under a stationary pointer',
);
other.fire('focus');
assert.equal(rendered, other);
assert(!tooltip.classList.contains('hidden'));
other.visible = false;
observe();
assert(tooltip.classList.contains('hidden'), 'Closing a menu dismisses its tooltip');
assert.equal(other.getAttribute('aria-describedby'), null);
trigger.fire('focus');
trigger.cssHidden = true;
observe();
assert(tooltip.classList.contains('hidden'), 'HUD visibility changes also dismiss tooltips');
trigger.cssHidden = false;
trigger.fire('focus');
document.fire('pointerdown', { target: document.body });
assert(tooltip.classList.contains('hidden'));
// Moving from a focused control to another hovered control can deliver the old
// blur/leave after the new popup is open. Those events must not close it.
other.visible = true;
trigger.fire('focus');
other.fire('pointerenter', { pointerType: 'mouse' });
trigger.fire('blur');
trigger.fire('pointerleave');
flushTimers();
assert.equal(rendered, other);
assert(!tooltip.classList.contains('hidden'), 'Stale trigger events must not hide a new tooltip');
other.fire('pointerleave');
flushTimers();
assert(tooltip.classList.contains('hidden'), 'An unfocused tooltip closes when the pointer leaves');
document.activeElement = trigger;
trigger.fire('focus');
trigger.fire('pointerleave');
flushTimers();
assert(!tooltip.classList.contains('hidden'), 'Keyboard focus keeps the tooltip open');
document.activeElement = other;
trigger.fire('blur');
other.fire('focus');
flushTimers();
assert(!tooltip.classList.contains('hidden'), 'Changing focus cancels the previous hide timer');
tips.hide();
console.log(
  'PASS: skill/icon catalogue, valid preview XP, tooltip viewport clamping, keyboard focus, hover handoff, Escape suppression, ARIA restoration, outside click and hidden-menu dismissal.',
);
