import assert from 'node:assert/strict';
import { compassReading, relativeBearing } from '../dist/hud-navigation.js';

const origin = { x: 0, z: 0 };
const rad = (degrees) => (degrees * Math.PI) / 180;
for (const [degrees, label, target] of [
  [0, 'N', { x: 0, z: -100 }],
  [90, 'E', { x: 100, z: 0 }],
  [180, 'S', { x: 0, z: 100 }],
  [270, 'W', { x: -100, z: 0 }],
]) {
  const reading = compassReading(rad(degrees), origin, target);
  assert.equal(reading.direction, label);
  assert.equal(reading.ticks.find((tick) => tick.label === label).offset, 50);
  assert.equal(reading.pin.offset, 50, `${label} destination must align with the heading needle`);
  assert.equal(reading.pin.edge, null);
  assert.equal(reading.pin.distance, 100);
}
assert.equal(relativeBearing(1, 359), 2);
assert.equal(relativeBearing(359, 1), -2);
assert.equal(compassReading(rad(-450), origin, null).direction, 'W');
assert.equal(compassReading(rad(1080), origin, null).direction, 'N');
const east = { x: 100, z: 0 };
assert.equal(compassReading(0, origin, east).pin.edge, 'right');
assert.equal(compassReading(rad(180), origin, east).pin.edge, 'left');
assert.equal(compassReading(rad(270), origin, east).pin.edge, 'left');
assert.equal(compassReading(0, origin, { x: 0, z: 100 }).pin.edge, 'left', 'Behind is not ahead');
assert.equal(compassReading(0, origin, { x: 5, z: 5 }).pin.arrived, true);
assert.equal(compassReading(0, origin, null).pin, null);
assert.equal(compassReading(0, origin, { x: NaN, z: 2 }).pin, null);
assert.equal(compassReading(NaN, origin, null).degrees, 0);
console.log(
  'PASS: compass cardinal alignment, heading wraparound, forward/behind pins, edge guidance, arrival, and missing destinations.',
);
