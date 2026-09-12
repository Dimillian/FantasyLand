//! Integrated near-camera precipitation advection in real seconds.
//!
//! Positions must use an integral of wind, never the latest wind times the age
//! of the application. The GPU hashes the 4 m emitter lattice periodically at
//! 4096 m, so wrapping these offsets is an exact change of emitter indices.

const ADVECTION_PERIOD: f64 = 4096.0;

#[derive(Clone, Debug, Default)]
pub struct PrecipitationMotion {
    rain: [f64; 2],
    snow: [f64; 2],
}

/// Metres per real second. Keep this expression in sync with
/// `precipitation_drift` in environment.wgsl.
pub fn drift_velocity(wind: [f32; 2], snow: bool) -> [f32; 2] {
    if !wind[0].is_finite() || !wind[1].is_finite() {
        return [0.0; 2];
    }
    let speed = wind[0].hypot(wind[1]);
    let bounded_speed = speed.min(26.0);
    let t = ((bounded_speed - 8.0) / 14.0).clamp(0.0, 1.0);
    let storm = t * t * (3.0 - 2.0 * t);
    let response = if snow {
        0.07 + (0.16 - 0.07) * storm
    } else {
        0.025 + (0.14 - 0.025) * storm
    };
    let factor = response * bounded_speed / speed.max(0.00001);
    [wind[0] * factor, wind[1] * factor]
}

impl PrecipitationMotion {
    /// Call once from Renderer::advance_time with unscaled real dt and the
    /// current weather wind. Weather acceleration only changes the forecast.
    pub fn advance(&mut self, dt: f32, wind: [f32; 2]) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        for (offset, snow) in [(&mut self.rain, false), (&mut self.snow, true)] {
            let velocity = drift_velocity(wind, snow);
            for axis in 0..2 {
                offset[axis] =
                    (offset[axis] + velocity[axis] as f64 * dt as f64).rem_euclid(ADVECTION_PERIOD);
            }
        }
    }

    /// rain x/z, snow x/z: one vec4 in Globals.
    pub fn uniform(&self) -> [f32; 4] {
        [
            self.rain[0] as f32,
            self.rain[1] as f32,
            self.snow[0] as f32,
            self.snow[1] as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped_delta(a: f32, b: f32) -> f32 {
        (b - a + 2048.0).rem_euclid(4096.0) - 2048.0
    }

    #[test]
    fn ordinary_rain_is_nearly_vertical_and_extreme_wind_is_bounded() {
        for wind_speed in [0.0, 2.5, 5.0, 8.0, 14.0, 24.0, 26000.0] {
            let drift = drift_velocity([wind_speed, 0.0], false);
            let angle = (drift[0].abs() / 25.0).atan().to_degrees();
            assert!(angle < 8.3, "rain slant {angle} at wind {wind_speed}");
            if wind_speed <= 8.0 {
                assert!(angle < 0.46, "ordinary rain slant {angle}");
            }
        }
        assert_eq!(drift_velocity([f32::NAN, 0.0], false), [0.0; 2]);
    }

    #[test]
    fn changing_wind_does_not_reposition_the_accumulated_rainfall() {
        let mut motion = PrecipitationMotion::default();
        motion.advance(36000.0, [8.0, -1.0]);
        let before = motion.uniform();
        let wind = [-24.0, 8.0];
        let velocity = drift_velocity(wind, false);
        motion.advance(1.0 / 60.0, wind);
        let after = motion.uniform();
        for axis in 0..2 {
            assert!(
                (wrapped_delta(before[axis], after[axis]) - velocity[axis] / 60.0).abs() < 0.0005
            );
        }
    }

    #[test]
    fn motion_is_independent_of_frame_partition_and_stays_precise() {
        let mut one = PrecipitationMotion::default();
        let mut partitioned = PrecipitationMotion::default();
        one.advance(128.0, [24.0, -8.0]);
        for _ in 0..8192 {
            partitioned.advance(1.0 / 64.0, [24.0, -8.0]);
        }
        assert_eq!(one.uniform(), partitioned.uniform());
        one.advance(10_000_000.0, [24.0, -8.0]);
        assert!(one.uniform().iter().all(|v| (0.0..4096.0).contains(v)));
        let before = one.uniform();
        one.advance(1.0 / 60.0, [24.0, -8.0]);
        assert!(wrapped_delta(before[0], one.uniform()[0]) > 0.04);
    }

    #[test]
    fn offset_wrap_preserves_world_positions_and_periodic_emitter_keys() {
        for camera in [-120000.0_f64, -2000.0, -0.1, 0.1, 186000.0] {
            for offset in [0.0_f64, 0.1, 1023.75, 4095.9] {
                let cell = ((camera - offset) / 4.0).floor() as i64;
                let next_cell = ((camera - offset - ADVECTION_PERIOD) / 4.0).floor() as i64;
                assert_eq!(cell.rem_euclid(1024), next_cell.rem_euclid(1024));
                let at = cell as f64 * 4.0 + offset;
                let next_at = next_cell as f64 * 4.0 + offset + ADVECTION_PERIOD;
                assert!((at - next_at).abs() < 0.000001);
            }
        }
    }

    #[test]
    fn snow_keeps_gentle_fair_wind_drift_and_invalid_dt_does_not_move_it() {
        let snow = drift_velocity([5.0, 0.0], true);
        assert!((snow[0] - 0.35).abs() < 0.00001);
        let mut motion = PrecipitationMotion::default();
        for dt in [0.0, -1.0, f32::INFINITY, f32::NAN] {
            motion.advance(dt, [24.0, 5.0]);
        }
        assert_eq!(motion.uniform(), [0.0; 4]);
    }
}
