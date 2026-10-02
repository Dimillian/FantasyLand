const HEADINGS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
const VISIBLE_ARC = 120;

export function relativeBearing(bearing, heading) {
  return ((((bearing - heading + 540) % 360) + 360) % 360) - 180;
}

export function compassReading(yaw, origin, target) {
  const heading = (((Number.isFinite(yaw) ? yaw : 0) * 180) / Math.PI + 360) % 360;
  const degrees = (heading + 360) % 360;
  const direction = HEADINGS[Math.round(degrees / 45) % HEADINGS.length];
  const ticks = Array.from({ length: 24 }, (_, index) => ({
    label: index % 3 === 0 ? HEADINGS[index / 3] : '',
    cardinal: index % 6 === 0,
    offset: 50 + (relativeBearing(index * 15, degrees) / VISIBLE_ARC) * 100,
  }));
  let pin = null;
  if (target && [origin.x, origin.z, target.x, target.z].every(Number.isFinite)) {
    const dx = target.x - origin.x;
    const dz = target.z - origin.z;
    const distance = Math.hypot(dx, dz);
    const bearing = ((Math.atan2(dx, -dz) * 180) / Math.PI + 360) % 360;
    const relative = relativeBearing(bearing, degrees);
    const edge = relative < -VISIBLE_ARC / 2 ? 'left' : relative > VISIBLE_ARC / 2 ? 'right' : null;
    pin = {
      distance,
      arrived: distance < 40,
      edge,
      offset: Math.max(3, Math.min(97, 50 + (relative / VISIBLE_ARC) * 100)),
    };
  }
  return { degrees, direction, ticks, pin };
}
