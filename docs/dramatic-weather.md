# Dramatic weather

2026-10-02. Checkpoint: `checkpoint/pre-dramatic-weather-20261002`.

Weather intensity now drives a continuous presentation rather than a switch to a
single grey filter. Cloudy and overcast lose warmth gradually; rain takes a
steel-blue grade, severe storms slightly reduce saturation and peripheral light,
and snow scatters a brighter pearl tone. Grading runs before tone mapping in the
existing presentation pass, once with or without FXAA, and works with Clean,
Bloom and CRT. Indoor attenuation fades over the first metre inside a room so
firelit interiors retain their warmth. There are no additional postprocess
textures or passes; the settings uniform grows from 16 to 48 bytes.

## Rain, snow and wind

The precipitation volume is 128 metres high instead of 176. It exchanges emitters
outside the 60-metre visibility fade. Rain has 8–40 layers of 1,024 emitters, snow
4–20, driven continuously by squared precipitation intensity. The fractional
last layer fades in rather than popping. Only active types are submitted (the
old path always submitted rain and snow together). A full tempest submits up to
40,960 rain quads; a blizzard up to 20,480 snow quads. Mixed fronts can use both
bounded budgets. This increases severe-weather geometry and translucent fragment
work, not persistent particle buffers or CPU particle updates.

Drops are brighter and have longer streaks. Falling speed remains fixed for each
particle. Wind is integrated in real seconds; changing gusts never multiply the
current velocity by application age. Normal rain below 8 m/s wind stays within
half a degree of vertical. Severe wind can tilt rain up to 20 degrees and drives
snow substantially sideways, with short spindrift trails. Roof/terrain/canopy
depth rejection remains active. Exact room bounds additionally reject indoor
fragments to close small leaks in the coarse overhead shelter map.

Storm and tempest manual winds peak at 20 and 32 m/s, with continuous gusts;
blizzards peak at 30. Existing shared vegetation wind moves grass, flowers,
branches and crowns, preserving roots, culling margins and shadow/reflection
consistency. Automatic fronts use the same precipitation, fog, grading and
lightning rendering while retaining climate-weighted forecasting.

## Atmosphere and lightning

Storm visibility now closes with distance through broad moving rain bands;
blizzards have a stronger spindrift extinction. The bands use world coordinates
and the same integrated precipitation offsets, with spatial periods matching
the offset wrap. Nearby terrain remains readable. Closed cloud decks have broad
soft shading and reduced white highlights, and the overcast zenith suppresses
fair-weather blue/sun glow. Reflections use the shared sky rendering.

Lightning uses seeded eight-second event windows, with a short primary stroke
and weaker return stroke followed by several dark seconds. CPU flash envelopes
and GPU bolt placement use the same event interval. The cloud underside and
exposed surfaces brighten together; there is no independent full-screen white
flash overlay.

## Verification

- Full release library suite: 164 passed after the main implementation.
- Six weather tests passed after adding severity/cadence checks (165 total tests
  now exist). New checks verify ordered rain/fog/wind severity and bounded flash
  occupancy; precipitation tests cover monotonic budgets and bounded drift.
- Release WASM build, UI and streaming checks.
- Real native wgpu captures across all eight manual weather modes, plus a
  lightning peak, in meadow, forest and inn fixtures. These validate shaders and
  rendering; capture timings are not browser FPS benchmarks.

Reproduce the same-camera studies with:

```sh
cargo run --release --bin verify -- output/weather-drama/forest 1337 weather-studies forest
cargo run --release --bin verify -- output/weather-drama/meadow 1337 weather-studies meadow
cargo run --release --bin verify -- output/weather-drama/inn 1337 weather-studies inn
```

Each writes PNGs and a manifest with actual weather state. Sequential fronts
retain wetness and snow history. Local preview remains at http://127.0.0.1:4173/.
