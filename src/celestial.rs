//! The Lantern Sky: Solenne is a warm sun; Aster and copper Vey share a
//! resonant nightly procession. All visible bodies and directional lighting
//! derive from this single clock. The 24-hour orbit repeats intentionally;
//! this prototype does not yet simulate lunar months or seasons.
use glam::Vec3;

pub const ASTER_RADIUS: f32 = 0.029;
pub const VEY_RADIUS: f32 = 0.016;

#[derive(Clone, Copy, Debug)]
pub struct CelestialState {
    pub sun: Vec3,
    pub aster: Vec3,
    pub vey: Vec3,
    pub primary: Vec3,
    pub direct_color: Vec3,
    pub direct_strength: f32,
    pub ambient_color: Vec3,
    pub exposure: f32,
    pub daylight: f32,
    pub stars: f32,
    pub shadow_strength: f32,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn state(hour: f32) -> CelestialState {
    let hour = if hour.is_finite() {
        hour.rem_euclid(24.0)
    } else {
        12.0
    };
    let angle = (hour - 6.0) * std::f32::consts::PI / 12.0;
    let sun = Vec3::new(angle.cos(), angle.sin(), -0.25).normalize();
    let moon_angle = (hour - 17.0) * std::f32::consts::PI / 12.0;
    let aster = Vec3::new(moon_angle.cos() * 0.90, moon_angle.sin() * 0.85, 0.42).normalize();
    let vey = Vec3::new(
        (moon_angle + 0.40).cos() * 0.65 - 0.24,
        (moon_angle + 0.40).sin() * 0.81,
        0.66,
    )
    .normalize();
    let daylight = smooth(-0.13, 0.22, sun.y);
    let stars = 1.0 - smooth(-0.18, 0.02, sun.y);
    let solar_power = smooth(-0.015, 0.18, sun.y);
    let lunar_phase = (0.5 - 0.5 * aster.dot(sun)).clamp(0.0, 1.0);
    let lunar_power = smooth(0.0, 0.18, aster.y)
        * (0.10 + lunar_phase * 0.24)
        * (1.0 - smooth(-0.10, -0.015, sun.y));
    // Both directional terms vanish at the handoff; the independent twilight
    // ambient bridges it. The shadow camera therefore never flips between two
    // strong competing lights. The weaker secondary moon is ambient-only.
    let solar_primary = solar_power >= lunar_power;
    let primary = if solar_primary { sun } else { aster };
    let direct_strength = if solar_primary {
        solar_power
    } else {
        lunar_power
    };
    let direct_color = if solar_primary {
        Vec3::new(1.28, 0.64, 0.32).lerp(Vec3::new(1.04, 0.985, 0.86), smooth(0.04, 0.55, sun.y))
    } else {
        Vec3::new(0.59, 0.73, 1.06)
    };
    // Minimum blue skylight is an intentional art-direction allowance. It
    // preserves a readable walking surface without making midnight look like day.
    let ambient_color = Vec3::new(0.28, 0.38, 0.57).lerp(Vec3::new(0.97, 1.0, 1.035), daylight);
    let exposure = 0.24 + daylight * 0.88;
    let shadow_strength = smooth(0.035, 0.14, primary.y) * smooth(0.018, 0.13, direct_strength);
    CelestialState {
        sun,
        aster,
        vey,
        primary,
        direct_color,
        direct_strength,
        ambient_color,
        exposure,
        daylight,
        stars,
        shadow_strength,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn celestial_clock_wraps_and_keeps_all_sources_finite() {
        for step in -288..577 {
            let hour = step as f32 / 12.0;
            let sky = state(hour);
            for source in [sky.sun, sky.aster, sky.vey, sky.primary] {
                assert!(source.is_finite());
                assert!((source.length() - 1.0).abs() < 0.00001);
            }
            assert!(sky.direct_color.is_finite() && sky.ambient_color.is_finite());
            assert!((0.0..=1.0).contains(&sky.shadow_strength));
            assert!((sky.sun - state(hour + 24.0).sun).length() < 0.00001);
        }
        assert!(state(f32::NAN).sun.is_finite());
    }
    #[test]
    fn midday_is_warm_and_midnight_has_real_moonlight_and_shadows() {
        let day = state(12.0);
        let night = state(23.0);
        assert!(day.daylight > 0.99 && day.stars == 0.0);
        assert_eq!(day.primary, day.sun);
        assert!(day.direct_color.x > day.direct_color.z);
        assert_eq!(night.primary, night.aster);
        assert!(night.aster.y > 0.2 && night.vey.y > 0.2);
        assert!(night.shadow_strength > 0.9 && night.stars > 0.99);
        assert!(night.direct_color.z > night.direct_color.x);
        assert!(night.exposure > 0.20 && night.exposure < 0.4);
    }
    #[test]
    fn daylight_is_continuous_across_the_clock_boundary_and_twilight() {
        for hour in [0.0, 5.7, 6.0, 6.3, 17.7, 18.0, 18.3, 24.0] {
            let a = state(hour - 0.0001);
            let b = state(hour + 0.0001);
            assert!((a.exposure - b.exposure).abs() < 0.001);
            assert!((a.ambient_color - b.ambient_color).length() < 0.002);
        }
    }
}
