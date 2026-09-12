//! Moving weather systems and a bounded local surface-history cache.
//!
//! Automatic fronts are deterministic continuous functions of world position,
//! simulation seconds and climate. They are not dice rolls on a global preset.
//! Manual modes are gradual showcase overrides. Surface water/snow integrate
//! precipitation and temperature, remember recently visited areas, and never
//! travel with the camera. This is an art-directed weather model, not CFD.
use crate::world::{hash, rand01, World};
use serde::Serialize;

const CELL_SIZE: f32 = 1600.0;
const CACHE_LIMIT: usize = 192;
const HISTORY_SECONDS: f64 = 900.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherMode {
    Auto = 0,
    Clear = 1,
    Cloudy = 2,
    Rain = 3,
    Storm = 4,
    Tempest = 5,
    Snow = 6,
    Blizzard = 7,
    Overcast = 8,
}
impl WeatherMode {
    pub fn from_index(value: u32) -> Self {
        match value {
            1 => Self::Clear,
            2 => Self::Cloudy,
            3 => Self::Rain,
            4 => Self::Storm,
            5 => Self::Tempest,
            6 => Self::Snow,
            7 => Self::Blizzard,
            8 => Self::Overcast,
            _ => Self::Auto,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Clear => "Fair",
            Self::Cloudy => "Cloudy",
            Self::Rain => "Rain",
            Self::Storm => "Storm",
            Self::Tempest => "Tempest",
            Self::Snow => "Snow",
            Self::Blizzard => "Blizzard",
            Self::Overcast => "Overcast",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WeatherClimate {
    /// World::Sample.temperature is a regional normalized climate that already
    /// includes terrain elevation. Do not apply a second ground-level lapse rate.
    pub temperature: f32,
    pub moisture: f32,
    pub elevation: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherState {
    pub mode: u32,
    pub mode_label: &'static str,
    pub label: &'static str,
    pub seconds: f32,
    pub speed: f32,
    pub paused: bool,
    pub cloud_cover: f32,
    pub rain: f32,
    pub snow: f32,
    pub fog: f32,
    pub wind_x: f32,
    pub wind_z: f32,
    pub gust: f32,
    pub lightning: f32,
    pub temperature: f32,
    pub wetness: f32,
    pub snow_cover: f32,
    pub storm_strength: f32,
    pub weather: [f32; 4],
    pub storm: [f32; 4],
    pub surface: [f32; 4],
}
impl Default for WeatherState {
    fn default() -> Self {
        let climate = WeatherClimate {
            temperature: 0.62,
            moisture: 0.5,
            elevation: 0.0,
        };
        let fair = forecast(0, 0.0, 0.0, 0.0, 12.0, 0.0, climate, WeatherMode::Clear);
        Self::from_forecast(fair, WeatherMode::Auto, 0.0, 1.0, false, 0.0, 0.0)
    }
}

impl WeatherState {
    fn from_forecast(
        f: Forecast,
        mode: WeatherMode,
        seconds: f64,
        speed: f32,
        paused: bool,
        wetness: f32,
        snow_cover: f32,
    ) -> Self {
        let label = if f.snow > 0.12 {
            if f.gust > 0.65 && f.snow > 0.55 {
                "Blizzard"
            } else {
                "Snow"
            }
        } else if f.rain > 0.12 {
            if f.severity > 0.75 {
                "Tempest"
            } else if f.severity > 0.30 {
                "Storm"
            } else {
                "Rain"
            }
        } else if f.cloud > 0.83 {
            "Overcast"
        } else if f.cloud > 0.37 {
            "Cloudy"
        } else {
            "Fair"
        };
        Self {
            mode: mode as u32,
            mode_label: mode.name(),
            label,
            seconds: seconds as f32,
            speed,
            paused,
            cloud_cover: f.cloud,
            rain: f.rain,
            snow: f.snow,
            fog: f.fog,
            wind_x: f.wind[0],
            wind_z: f.wind[1],
            gust: f.gust,
            lightning: f.lightning,
            temperature: f.temperature,
            wetness,
            snow_cover,
            storm_strength: f.severity,
            weather: [f.cloud, f.rain, f.snow, f.fog],
            storm: [f.wind[0], f.wind[1], f.gust, f.lightning],
            surface: [wetness, snow_cover, f.temperature, seconds as f32],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Forecast {
    cloud: f32,
    rain: f32,
    snow: f32,
    fog: f32,
    wind: [f32; 2],
    gust: f32,
    lightning: f32,
    temperature: f32,
    severity: f32,
}
impl Forecast {
    fn blend(self, b: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            cloud: mix(self.cloud, b.cloud),
            rain: mix(self.rain, b.rain),
            snow: mix(self.snow, b.snow),
            fog: mix(self.fog, b.fog),
            wind: [mix(self.wind[0], b.wind[0]), mix(self.wind[1], b.wind[1])],
            gust: mix(self.gust, b.gust),
            lightning: b.lightning,
            temperature: mix(self.temperature, b.temperature),
            severity: mix(self.severity, b.severity),
        }
    }
}

#[derive(Clone, Debug)]
struct SurfaceCell {
    x: i32,
    z: i32,
    climate: WeatherClimate,
    wetness: f32,
    snow: f32,
    last_seconds: f64,
    touched: u64,
}
#[derive(Clone, Copy, Debug)]
struct ClimateCache {
    x: f32,
    z: f32,
    climate: WeatherClimate,
}

pub struct WeatherSystem {
    seed: u32,
    seconds: f64,
    speed: f32,
    paused: bool,
    mode: WeatherMode,
    mode_since: f64,
    current: Option<Forecast>,
    state: WeatherState,
    cells: Vec<SurfaceCell>,
    climate: Option<ClimateCache>,
    touches: u64,
}
impl WeatherSystem {
    pub fn new(seed: u32) -> Self {
        let f = automatic(
            seed,
            0.0,
            0.0,
            0.0,
            9.0,
            0.0,
            WeatherClimate {
                temperature: 0.62,
                moisture: 0.5,
                elevation: 0.0,
            },
        );
        Self {
            seed,
            seconds: 0.0,
            speed: 1.0,
            paused: false,
            mode: WeatherMode::Auto,
            mode_since: 0.0,
            current: None,
            state: WeatherState::from_forecast(f, WeatherMode::Auto, 0.0, 1.0, false, 0.0, 0.0),
            cells: Vec::new(),
            climate: None,
            touches: 0,
        }
    }
    pub fn set_mode(&mut self, index: u32) {
        let mode = WeatherMode::from_index(index);
        if mode != self.mode {
            self.mode = mode;
            self.mode_since = self.seconds;
        }
    }
    /// 0 freezes the automatic clock and accumulation; [0.05, 60] speeds are
    /// useful for weather observation. Manual showcase transitions use real dt.
    pub fn set_speed(&mut self, speed: f32) {
        if speed.is_finite() {
            self.speed = speed.clamp(0.0, 60.0);
        }
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn state(&self) -> WeatherState {
        self.state
    }
    pub fn seconds(&self) -> f64 {
        self.seconds
    }
    pub fn mode(&self) -> WeatherMode {
        self.mode
    }
    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        x: f32,
        y: f32,
        z: f32,
        hour: f32,
    ) -> WeatherState {
        let x = finite(x, 0.0).clamp(-1_000_000.0, 1_000_000.0);
        let z = finite(z, 0.0).clamp(-1_000_000.0, 1_000_000.0);
        let y = finite(y, 0.0).clamp(-3000.0, 15000.0);
        let hour = finite(hour, 12.0).rem_euclid(24.0);
        let climate = if let Some(c) = self.climate.filter(|c| (c.x - x).hypot(c.z - z) < 40.0) {
            c.climate
        } else {
            let s = world.natural_sample(x, z);
            let c = WeatherClimate {
                temperature: s.temperature,
                moisture: s.moisture,
                elevation: s.height,
            };
            self.climate = Some(ClimateCache { x, z, climate: c });
            c
        };
        let dt = finite(dt, 0.0).clamp(0.0, 1.0);
        let elapsed = if self.paused { 0.0 } else { dt * self.speed };
        self.seconds += elapsed as f64;
        let target = forecast(self.seed, x, y, z, hour, self.seconds, climate, self.mode);
        let old = self.current.unwrap_or(target);
        // Spatial fronts remain smooth when walking, while fast travel moves the
        // weather toward the destination over a few seconds rather than a flash.
        let blend = 1.0 - (-dt / 2.6).exp();
        let mut f = old.blend(target, blend);
        // A flash follows this frame's deterministic envelope, with smooth storm
        // strength so switching to Tempest does not cause an immediate white flash.
        f.lightning *= smooth(0.12, 0.70, f.severity);
        self.current = Some(f);
        let (wetness, snow) = self.surface_at(world, x, z, hour);
        self.state = WeatherState::from_forecast(
            f,
            self.mode,
            self.seconds,
            self.speed,
            self.paused,
            wetness,
            snow,
        );
        self.state
    }

    fn surface_at(&mut self, world: &World, x: f32, z: f32, hour: f32) -> (f32, f32) {
        let qx = x / CELL_SIZE;
        let qz = z / CELL_SIZE;
        let ix = qx.floor() as i32;
        let iz = qz.floor() as i32;
        let tx = hermite(qx - ix as f32);
        let tz = hermite(qz - iz as f32);
        let mut wetness = 0.0;
        let mut snow = 0.0;
        for (dx, dz, weight) in [
            (0, 0, (1.0 - tx) * (1.0 - tz)),
            (1, 0, tx * (1.0 - tz)),
            (0, 1, (1.0 - tx) * tz),
            (1, 1, tx * tz),
        ] {
            let gx = ix + dx;
            let gz = iz + dz;
            let index = if let Some(i) = self.cells.iter().position(|c| c.x == gx && c.z == gz) {
                i
            } else {
                let s = world.natural_sample(gx as f32 * CELL_SIZE, gz as f32 * CELL_SIZE);
                let c = SurfaceCell {
                    x: gx,
                    z: gz,
                    climate: WeatherClimate {
                        temperature: s.temperature,
                        moisture: s.moisture,
                        elevation: s.height,
                    },
                    wetness: 0.0,
                    snow: 0.0,
                    last_seconds: self.seconds - HISTORY_SECONDS,
                    touched: 0,
                };
                if self.cells.len() >= CACHE_LIMIT {
                    let i = self
                        .cells
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, c)| c.touched)
                        .map(|(i, _)| i)
                        .unwrap();
                    self.cells[i] = c;
                    i
                } else {
                    self.cells.push(c);
                    self.cells.len() - 1
                }
            };
            self.touches = self.touches.wrapping_add(1);
            let c = &mut self.cells[index];
            // Reconstruct no more than 15 minutes in at most 45 steps after a
            // long absence. Cache memory and per-frame catch-up are both bounded.
            let start = c.last_seconds.max(self.seconds - HISTORY_SECONDS);
            let duration = (self.seconds - start).max(0.0);
            let steps = (duration / 20.0).ceil().clamp(1.0, 45.0) as u32;
            let step = (duration / steps as f64) as f32;
            for i in 0..steps {
                let time = start + duration * (i as f64 + 0.5) / steps as f64;
                let mode = if time >= self.mode_since {
                    self.mode
                } else {
                    WeatherMode::Auto
                };
                let f = forecast(
                    self.seed,
                    gx as f32 * CELL_SIZE,
                    c.climate.elevation,
                    gz as f32 * CELL_SIZE,
                    hour,
                    time,
                    c.climate,
                    mode,
                );
                integrate_surface(&mut c.wetness, &mut c.snow, step, f);
            }
            c.last_seconds = self.seconds;
            c.touched = self.touches;
            wetness += c.wetness * weight;
            snow += c.snow * weight;
        }
        (wetness.clamp(0.0, 1.0), snow.clamp(0.0, 1.0))
    }
}

/// Deterministic read-only weather at arbitrary position/time. Surface history
/// belongs to WeatherSystem and is deliberately absent from this pure sampler.
pub fn sample_automatic(
    seed: u32,
    x: f32,
    y: f32,
    z: f32,
    hour: f32,
    seconds: f64,
    climate: WeatherClimate,
) -> WeatherState {
    let x = finite(x, 0.0);
    let y = finite(y, climate.elevation);
    let z = finite(z, 0.0);
    let hour = finite(hour, 12.0).rem_euclid(24.0);
    let seconds = if seconds.is_finite() { seconds } else { 0.0 };
    let f = automatic(seed, x, y, z, hour, seconds, climate);
    WeatherState::from_forecast(f, WeatherMode::Auto, seconds, 1.0, false, 0.0, 0.0)
}

fn forecast(
    seed: u32,
    x: f32,
    y: f32,
    z: f32,
    hour: f32,
    seconds: f64,
    c: WeatherClimate,
    mode: WeatherMode,
) -> Forecast {
    let mut f = automatic(seed, x, y, z, hour, seconds, c);
    if mode == WeatherMode::Auto {
        return f;
    }
    // Presets retain coherent drifting billows and gust timing. Each numeric
    // control is blended by WeatherSystem rather than snapping at the button.
    let (cloud, rain, snow, fog, wind, gust, severity) = match mode {
        WeatherMode::Clear => (0.10, 0.0, 0.0, 0.025, 2.5, 0.13, 0.0),
        WeatherMode::Cloudy => (0.58, 0.0, 0.0, 0.08, 5.0, 0.24, 0.0),
        WeatherMode::Overcast => (0.98, 0.0, 0.0, 0.20, 7.0, 0.30, 0.0),
        WeatherMode::Rain => (0.92, 0.62, 0.0, 0.35, 8.0, 0.38, 0.10),
        WeatherMode::Storm => (0.98, 0.86, 0.0, 0.50, 14.0, 0.72, 0.64),
        WeatherMode::Tempest => (1.0, 1.0, 0.0, 0.78, 24.0, 1.0, 1.0),
        WeatherMode::Snow => (0.88, 0.0, 0.70, 0.35, 5.0, 0.26, 0.0),
        WeatherMode::Blizzard => (1.0, 0.0, 1.0, 0.94, 20.0, 0.94, 0.05),
        WeatherMode::Auto => unreachable!(),
    };
    let swirl = (seconds * 0.011 + x as f64 * 0.000025 + z as f64 * 0.000019).sin() as f32;
    let angle = -0.43 + swirl * 0.14;
    f.cloud = cloud;
    f.rain = rain;
    f.snow = snow;
    f.fog = fog;
    f.wind = [angle.cos() * wind, angle.sin() * wind];
    f.gust = (gust * (0.82 + swirl * 0.12)).clamp(0.0, 1.0);
    f.severity = severity;
    f.temperature = match mode {
        WeatherMode::Snow => f.temperature.min(-3.0),
        WeatherMode::Blizzard => f.temperature.min(-9.0),
        WeatherMode::Rain | WeatherMode::Storm | WeatherMode::Tempest => f.temperature.max(4.0),
        _ => f.temperature,
    };
    f.lightning = lightning(seed, seconds, severity);
    f
}

fn automatic(
    seed: u32,
    x: f32,
    y: f32,
    z: f32,
    hour: f32,
    seconds: f64,
    c: WeatherClimate,
) -> Forecast {
    // Continental fronts move ENE at an art-directed 18 m/s. Slow transverse
    // shear and distinct convection cells avoid repeating full-screen states.
    let px = (x as f64 - seconds * 18.0) / 17000.0;
    let pz = (z as f64 + seconds * 7.5) / 17000.0;
    let broad = noise(seed ^ 0xA617, px * 0.48, pz * 0.48);
    let warp = noise(seed ^ 0x923D, px * 0.64 + 17.0, pz * 0.64 - 8.0);
    let phase = rand01(hash(seed, 713, 19)) * std::f32::consts::TAU;
    let ribbon =
        ((px * 0.85 + pz * 0.31) as f32 * std::f32::consts::TAU + (warp - 0.5) * 3.6 + phase).sin()
            * 0.5
            + 0.5;
    let cells = noise(seed ^ 0x77A3, px * 2.3 - 9.0, pz * 2.3 + 14.0);
    let moist = finite(c.moisture, 0.5).clamp(0.0, 1.0);
    let pressure = (ribbon * 0.67 + broad * 0.25 + moist * 0.20 - 0.08).clamp(0.0, 1.0);
    let cloud = smooth(0.13, 0.83, pressure);
    let precipitation =
        smooth(0.53, 0.87, pressure) * smooth(0.30, 0.74, cells) * (0.24 + moist * 0.76);
    let warmth = finite(c.temperature, 0.5).clamp(0.0, 1.0);
    let diurnal = ((hour - 7.0) * std::f32::consts::PI / 12.0).sin() * 3.0;
    let temperature = warmth * 36.0 - 12.0 + diurnal + (broad - 0.5) * 5.0
        - pressure * 3.0
        - (y - finite(c.elevation, 0.0)).max(0.0) * 0.0065;
    let frozen = 1.0 - smooth(-1.2, 2.2, temperature);
    let snow = precipitation * frozen;
    let rain = precipitation * (1.0 - frozen);
    let severity = smooth(0.54, 0.89, cells) * precipitation * (0.50 + warmth * 0.50);
    let gust_field = (seconds * 0.021 + x as f64 * 0.000071 + z as f64 * 0.000037).sin() as f32;
    let gust = (0.12 + cloud * 0.16 + severity * 0.65 + gust_field * 0.08).clamp(0.0, 1.0);
    let wind = 2.2 + cloud * 5.8 + severity * 17.0;
    let angle = -0.43 + (broad - 0.5) * 0.55 + gust_field * 0.12;
    let fog = (cloud * 0.055
        + precipitation * 0.43
        + snow * gust * 0.30
        + smooth(0.6, 1.0, moist) * (1.0 - smooth(0.0, 0.55, diurnal / 3.0)) * 0.14)
        .clamp(0.0, 0.95);
    Forecast {
        cloud,
        rain,
        snow,
        fog,
        wind: [wind * angle.cos(), wind * angle.sin()],
        gust,
        lightning: lightning(seed, seconds, severity) * smooth(0.02, 0.30, rain),
        temperature,
        severity,
    }
}

fn lightning(seed: u32, seconds: f64, severity: f32) -> f32 {
    if severity < 0.28 {
        return 0.0;
    }
    let interval = 11.0;
    let id = (seconds / interval).floor() as i32;
    let r = rand01(hash(seed ^ 0xA51E, id, 23));
    if r > 0.18 + severity * 0.72 {
        return 0.0;
    }
    let phase = (seconds - id as f64 * interval) as f32;
    let strike = 1.3 + rand01(hash(seed ^ 0x52F1, id, 7)) * 7.0;
    let t = phase - strike;
    let first = smooth(0.0, 0.025, t) * (1.0 - smooth(0.08, 0.22, t));
    let after = smooth(0.23, 0.245, t) * (1.0 - smooth(0.29, 0.41, t)) * 0.55;
    (first + after).clamp(0.0, 1.0) * severity
}

fn integrate_surface(wetness: &mut f32, snow: &mut f32, dt: f32, f: Forecast) {
    if dt <= 0.0 {
        return;
    }
    let melt = smooth(-0.5, 7.0, f.temperature) * 0.0018;
    let snow_gain = f.snow * 0.0030;
    let snow_loss = melt + f.rain * 0.0009;
    *snow = equilibrium_step(*snow, snow_gain, snow_loss, dt);
    let wet_gain = f.rain * 0.009 + *snow * melt * 4.0;
    let dry_loss = 0.0008
        + smooth(0.0, 24.0, f.temperature) * 0.0011
        + (f.wind[0].hypot(f.wind[1]) / 24.0).clamp(0.0, 1.0) * 0.0006;
    *wetness = equilibrium_step(*wetness, wet_gain, dry_loss, dt);
}
fn equilibrium_step(value: f32, gain: f32, loss: f32, dt: f32) -> f32 {
    let rate = gain + loss;
    if rate <= 0.0 {
        return value.clamp(0.0, 1.0);
    }
    let equilibrium = gain / rate;
    (equilibrium + (value - equilibrium) * (-rate * dt).exp()).clamp(0.0, 1.0)
}
fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}
fn hermite(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    hermite(((x - a) / (b - a)).clamp(0.0, 1.0))
}
fn noise(seed: u32, x: f64, z: f64) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let tx = hermite((x - x.floor()) as f32);
    let tz = hermite((z - z.floor()) as f32);
    let a = rand01(hash(seed, ix, iz));
    let b = rand01(hash(seed, ix + 1, iz));
    let c = rand01(hash(seed, ix, iz + 1));
    let d = rand01(hash(seed, ix + 1, iz + 1));
    (a + (b - a) * tx) * (1.0 - tz) + (c + (d - c) * tx) * tz
}

#[cfg(test)]
mod tests {
    use super::*;
    fn climate() -> WeatherClimate {
        WeatherClimate {
            temperature: 0.70,
            moisture: 0.76,
            elevation: 100.0,
        }
    }
    #[test]
    fn automatic_fronts_are_seeded_continuous_and_not_global_presets() {
        let c = climate();
        let mut minimum = 1f32;
        let mut maximum = 0f32;
        let mut rain = 0f32;
        for i in 0..240 {
            let x = i as f32 * 811.0 - 92000.;
            let z = (i % 23) as f32 * 997. - 11000.;
            let a = sample_automatic(1337, x, 100., z, 11., 421.0, c);
            let b = sample_automatic(1337, x, 100., z, 11., 421.0, c);
            let adjacent = sample_automatic(1337, x + 0.1, 100., z - 0.1, 11., 421.001, c);
            assert_eq!(a.weather, b.weather);
            assert_eq!(a.storm, b.storm);
            for j in 0..4 {
                assert!((a.weather[j] - adjacent.weather[j]).abs() < 0.001);
            }
            minimum = minimum.min(a.cloud_cover);
            maximum = maximum.max(a.cloud_cover);
            rain = rain.max(a.rain);
        }
        assert!(maximum - minimum > 0.75);
        assert!(rain > 0.4);
        assert_ne!(
            sample_automatic(4, 0., 100., 0., 12., 0., c).weather,
            sample_automatic(5, 0., 100., 0., 12., 0., c).weather
        );
        let later = sample_automatic(1337, 0., 100., 0., 12., 1800., c);
        assert_ne!(
            later.weather,
            sample_automatic(1337, 0., 100., 0., 12., 0., c).weather
        );
    }
    #[test]
    fn precipitation_phase_uses_climate_and_real_altitude() {
        let mut tested = false;
        for i in 0..200 {
            let x = i as f32 * 2311.;
            let warm = automatic(42, x, 100., -18000., 11., 337., climate());
            if warm.rain > 0.15 {
                let cold = automatic(42, x, 4100., -18000., 11., 337., climate());
                assert!(cold.snow > warm.snow);
                assert!(cold.rain < warm.rain);
                tested = true;
                break;
            }
        }
        assert!(tested);
        let warm = WeatherClimate {
            temperature: 1.,
            moisture: 0.1,
            elevation: 20.,
        };
        let snow = forecast(42, 0., 20., 0., 12., 0., warm, WeatherMode::Snow);
        assert!(snow.snow > 0.6 && snow.rain == 0.0 && snow.temperature < 0.0);
    }
    #[test]
    fn surfaces_accumulate_and_melt_gradually_with_frame_independent_integrator() {
        let mut wet = 0.;
        let mut snow = 0.;
        let f = forecast(42, 0., 100., 0., 12., 20., climate(), WeatherMode::Snow);
        integrate_surface(&mut wet, &mut snow, 60., f);
        assert!(snow > 0.1 && snow < 0.25);
        let before = snow;
        let warm = forecast(42, 0., 100., 0., 12., 20., climate(), WeatherMode::Clear);
        integrate_surface(&mut wet, &mut snow, 120., warm);
        assert!(snow < before && wet > 0.0);
        let rain = forecast(42, 0., 100., 0., 12., 20., climate(), WeatherMode::Rain);
        let mut a = 0.;
        let mut b = 0.;
        let mut sa = 0.;
        let mut sb = 0.;
        integrate_surface(&mut a, &mut sa, 20., rain);
        for _ in 0..200 {
            integrate_surface(&mut b, &mut sb, 0.1, rain);
        }
        assert!((a - b).abs() < 0.0001);
    }
    #[test]
    fn manual_modes_are_distinct_and_flash_envelopes_are_finite() {
        let c = climate();
        let mut flashes = 0;
        for mode in 0..=8 {
            for i in 0..900 {
                let f = forecast(
                    67,
                    0.,
                    100.,
                    0.,
                    22.,
                    i as f64 / 30.,
                    c,
                    WeatherMode::from_index(mode),
                );
                for v in [f.cloud, f.rain, f.snow, f.fog, f.gust, f.lightning] {
                    assert!(v.is_finite() && (0.0..=1.0).contains(&v));
                }
                assert!(f.wind[0].is_finite() && f.temperature.is_finite());
                if mode == 5 && f.lightning > 0.1 {
                    flashes += 1;
                }
            }
        }
        assert!(flashes > 0);
        assert_eq!(WeatherMode::from_index(900), WeatherMode::Auto);
    }
    #[test]
    fn stateful_overrides_crossfade_pause_and_keep_surface_history_at_its_location() {
        let world = World::new(1337);
        let (p, _) = world.spawn_view();
        let y = world.natural_sample(p[0], p[1]).height + 1.72;
        let mut system = WeatherSystem::new(1337);
        system.set_mode(1);
        let clear = system.update(0.0, &world, p[0], y, p[1], 12.0);
        assert_eq!(clear.rain, 0.0);
        system.set_mode(6);
        let first = system.update(0.1, &world, p[0], y, p[1], 12.0);
        assert!(first.snow > 0.0 && first.snow < 0.10, "snow must crossfade");
        system.set_speed(60.0);
        for _ in 0..100 {
            system.update(0.1, &world, p[0], y, p[1], 12.0);
        }
        let snowy = system.state();
        assert!(snowy.snow_cover > 0.55 && snowy.temperature < 0.0);
        system.set_paused(true);
        let seconds = system.seconds();
        for _ in 0..20 {
            system.update(0.1, &world, p[0], y, p[1], 12.0);
        }
        assert_eq!(system.seconds(), seconds);
        assert_eq!(system.state().snow_cover, snowy.snow_cover);
        assert_eq!(system.state().wetness, snowy.wetness);
        system.update(0.0, &world, p[0] + 50000.0, y, p[1] + 50000.0, 12.0);
        let returned = system.update(0.0, &world, p[0], y, p[1], 12.0);
        assert_eq!(returned.snow_cover, snowy.snow_cover);
        assert_eq!(returned.wetness, snowy.wetness);
        assert!(system.cells.len() <= CACHE_LIMIT);
        system.set_paused(false);
        system.set_mode(1);
        for _ in 0..200 {
            system.update(0.1, &world, p[0], y, p[1], 12.0);
        }
        assert!(system.state().snow_cover < snowy.snow_cover * 0.4);
        system.set_speed(0.0);
        let stopped = system.seconds();
        system.update(1.0, &world, p[0], y, p[1], 12.0);
        assert_eq!(system.seconds(), stopped);
        system.set_speed(f32::NAN);
        assert_eq!(system.speed(), 0.0);
        assert_eq!(system.mode(), WeatherMode::Clear);
    }
}
