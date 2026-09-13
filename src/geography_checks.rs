//! Cross-system acceptance checks for the continental climate generation.
use crate::{
    climate, geometry,
    weather::{sample_automatic, WeatherClimate},
    world::{Biome, World},
};
#[test]
fn continent_is_dry_diverse_and_has_prominent_localized_peaks() {
    for seed in [1337, 42, 2026] {
        let w = World::new(seed);
        let mut kinds = std::collections::BTreeMap::new();
        let (mut land, mut water, mut high, mut peak) = (0, 0, 0, 0f32);
        for z in (-170000..170000).step_by(1800) {
            for x in (-170000..170000).step_by(1800) {
                let s = w.natural_sample(x as f32, z as f32);
                if s.ocean {
                    continue;
                }
                land += 1;
                water += usize::from(s.water_height > s.height);
                high += usize::from(s.height > 2000.);
                peak = peak.max(s.height);
                *kinds.entry(s.biome.name()).or_insert(0) += 1;
            }
        }
        for b in [
            Biome::Grassland,
            Biome::Forest,
            Biome::Swamp,
            Biome::Desert,
            Biome::Savanna,
            Biome::Jungle,
            Biome::TropicalCoast,
            Biome::Alpine,
        ] {
            assert!(
                kinds.get(b.name()).copied().unwrap_or(0) > 8,
                "seed{seed} missing {b:?}: {kinds:?}"
            );
        }
        assert!(
            (water as f32 / land as f32) < 0.028,
            "too much inland water seed{seed}"
        );
        assert!(
            peak > 3400. && (high as f32 / land as f32) < 0.12,
            "peaks={peak} high={high}/{land}"
        );
        assert!(w.hydrology_stats().headwaters < 1000);
    }
}
#[test]
fn weather_climate_weights_fronts_without_permanent_rain_or_boundary_jumps() {
    let mut rain = [0f32; 3];
    let mut fair = [0; 3];
    let mut blizzards = 0;
    let climates = [
        WeatherClimate {
            temperature: 0.78,
            moisture: 0.18,
            elevation: 150.,
        },
        WeatherClimate {
            temperature: 0.62,
            moisture: 0.48,
            elevation: 150.,
        },
        WeatherClimate {
            temperature: 0.82,
            moisture: 0.83,
            elevation: 150.,
        },
    ];
    for i in 0..1200 {
        let seconds = i as f64 * 90.;
        for (j, &c) in climates.iter().enumerate() {
            let a = sample_automatic(1337, 5000., c.elevation, 8000., 12., seconds, c);
            let b = sample_automatic(1337, 5001., c.elevation, 8000., 12., seconds, c);
            rain[j] += a.rain;
            fair[j] += usize::from(a.rain < 0.01 && a.cloud_cover < 0.5);
            assert!((a.rain - b.rain).abs() < 0.003 && (a.wind_x - b.wind_x).abs() < 0.03);
        }
        let c = WeatherClimate {
            temperature: 0.08,
            moisture: 0.70,
            elevation: 3200.,
        };
        let snow = sample_automatic(1337, 5000., 3200., 8000., 12., seconds, c);
        blizzards += usize::from(snow.label == "Blizzard");
    }
    assert!(
        rain[2] > rain[1] * 1.5 && rain[1] > rain[0] * 2.,
        "rain={rain:?}"
    );
    assert!(fair.iter().all(|&n| n > 80), "no fair breaks {fair:?}");
    assert!(blizzards > 0, "mountains never receive automatic blizzards");
}
#[test]
fn frozen_tarn_mesh_and_walking_surface_agree_including_outlet() {
    let w = World::new(1337);
    let lake = w
        .lakes()
        .iter()
        .find(|l| w.lake_ice(l.center[0], l.center[1]) > 0.5)
        .expect("cold retained tarn");
    let h = geometry::walk_height(&w, lake.center[0], lake.center[1]);
    assert!((h - lake.surface - 0.025).abs() < 0.015);
    let mut tested = 0;
    for center in [lake.center, lake.outlet] {
        let cx = (center[0] / geometry::CHUNK_SIZE).floor() as i32;
        let cz = (center[1] / geometry::CHUNK_SIZE).floor() as i32;
        for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (1, 0), (0, 1)] {
            let mesh = geometry::water_chunk(&w, cx + dx, cz + dz, 0);
            for tri in mesh.vertices.chunks_exact(3).step_by(7) {
                if tri[0].color[0].hypot(tri[0].color[1]) > 5.5 {
                    continue;
                }
                let p: [f32; 3] =
                    std::array::from_fn(|i| tri.iter().map(|v| v.position[i]).sum::<f32>() / 3.);
                let height = geometry::ice_surface_height(&w, p[0], p[2]);
                if tri[0].uv[0] > 0.5 {
                    assert!(
                        height.is_some_and(|h| (h - p[1]).abs() < 0.03),
                        "ice collision mismatch {p:?}"
                    );
                    tested += 1;
                } else if p[1] < lake.surface - 0.10 {
                    assert!(height.is_none(), "ice over sloped outlet");
                }
            }
        }
    }
    assert!(tested > 20);
    let mut player = crate::player::Player::new(&w, lake.center[0], lake.center[1]);
    let start = player.position;
    for _ in 0..120 {
        player.update(&w, 1. / 60., 1., 0., false, false);
    }
    assert!(
        (player.position - start).length() > 9.,
        "player swimming through ice"
    );
    assert!((player.position.y - lake.surface - 0.025).abs() < 0.04);
    assert!(climate::snow_cover(0.1, 0.1) > 0.9);
}
#[test]
fn mountain_trails_are_connected_dry_and_follow_a_bounded_grade() {
    let w = World::new(1337);
    let trails = w.mountain_trails();
    assert!(trails.len() >= 3, "only{} trails", trails.len());
    let mut gain = 0f32;
    let mut reaches_snowy_summit = false;
    let summits: Vec<_> = crate::geography::landmarks(1337)
        .into_iter()
        .filter(|l| l.kind == crate::geography::GeoLandmarkKind::RidgeSummit)
        .collect();
    for r in trails {
        let a = r.points[0];
        let b = *r.points.last().unwrap();
        reaches_snowy_summit |= w.natural_sample(b[0], b[1]).height > 2500.
            && summits
                .iter()
                .any(|s| (s.position[0] - b[0]).hypot(s.position[1] - b[1]) < 30.);
        gain = gain.max(w.natural_sample(b[0], b[1]).height - w.natural_sample(a[0], a[1]).height);
        for pair in r.points.windows(2) {
            let len = (pair[0][0] - pair[1][0]).hypot(pair[0][1] - pair[1][1]);
            let n = (len / 8.).ceil().max(1.) as usize;
            let mut h = w.natural_sample(pair[0][0], pair[0][1]).height;
            for i in 1..=n {
                let t = i as f32 / n as f32;
                let p = [
                    pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                    pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                ];
                let s = w.natural_sample(p[0], p[1]);
                assert!(s.height > s.water_height + 0.34 && !s.ocean);
                assert!((s.height - h).abs() <= 0.36 * len / n as f32 + 0.045);
                h = s.height;
            }
        }
        for &p in r.points.iter().step_by(3) {
            assert!(w.sample(p[0], p[1]).road > 0.99);
            assert!(
                geometry::walk_height(&w, p[0], p[1])
                    > w.natural_sample(p[0], p[1]).water_height + 0.34
            );
            assert!(
                !geometry::blocks_player(&w, p[0], p[1]),
                "obstructed mountain trail"
            );
        }
    }
    assert!(gain > 700.);
    assert!(reaches_snowy_summit, "no route reaches a snowy summit");
}
