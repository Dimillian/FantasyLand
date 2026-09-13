//! Stable long-term climate. Weather fronts use these continuous fields; biome
//! labels never select a weather preset. Elevation is metres, temperature is a
//! normalized climate index (weather converts it to degrees C).
use crate::world::{noise, smooth};
pub fn temperature(seed: u32, x: f32, z: f32, height: f32) -> f32 {
    (0.61 + z / 150000. * 0.34 + (noise(seed ^ 0x3101, x / 55000., z / 55000.) - 0.5) * 0.16
        - (height - 80.).max(0.) * 0.00018)
        .clamp(0., 1.)
}
pub fn rainfall(seed: u32, x: f32, z: f32, windward: f32, shadow: f32, lift: f32) -> f32 {
    let regional = noise(seed ^ 0x7315, x / 46000., z / 46000.);
    let continental = smooth(-15000., 95000., x);
    let monsoon = smooth(10000., 90000., z) * (1. - continental * 0.7);
    (0.22 + regional * 0.62 + windward * 0.24 - shadow * 0.40 + lift * 0.72 - continental * 0.18
        + monsoon * 0.22)
        .clamp(0.08, 0.96)
}
/// Climatic snowline varies with latitude. Permanent snow is attached to each
/// piece of ground; camera-local weather adds transient snowfall on top.
pub fn snow_cover(temperature: f32, slope: f32) -> f32 {
    (1. - smooth(0.16, 0.29, temperature)) * (1. - smooth(0.65, 1.4, slope))
}
