# Current game captures

Captured on October 2, 2026 from engine revision `2b60e38`, seed **1337**. These PNGs are unretouched output from FantasyLand's native wgpu renderer, which shares the world generation, geometry, materials, and shaders with the WASM game. They contain no generated promotional artwork or browser interface overlays.

All output images are 1280 × 720. The landscape `shot` mode uses Balanced quality, 720p internal rendering, 400% ground cover, Bloom at 85%, and Clear weather with 30 simulated seconds to settle the override. The `settlement-art` mode uses Low quality, 540p internal rendering, FXAA, and Clear weather, and includes the live citizen simulation. It outputs at 1280 × 720 after scaling.

| Image | View |
| --- | --- |
| [amberwood-morning.png](amberwood-morning.png) | Forest morning, 07:30 |
| [meadowlands.png](meadowlands.png) | Generated meadowland viewpoint, 09:30 |
| [aster-and-vey.png](aster-and-vey.png) | Two moons above a forest, 19:24 |
| [sunstone-evening.png](sunstone-evening.png) | Natural stone arches, 16:00 |
| [capital.png](capital.png) | Generated capital streets and residents, 09:00 |
| [inn-night.png](inn-night.png) | Inn interior with hearth and leaded glass, 22:00 |

Camera coordinates, angles in radians, times, and mode selection are recorded in [captures.json](captures.json). Landscape eye height comes from the actual walking surface plus 1.72 m. The meadow and moon cameras use viewpoints discovered from the current seeded world. The settlement cameras are derived from generated building records.

Run these commands from the repository root to reproduce the selected captures:

```sh
cargo build --release --bin verify --locked
target/release/verify output/readme/forest 1337 shot -10879 58547 1.4 0.08 7.5 1 30
target/release/verify output/readme/meadow 1337 shot 87851 55519 1.1780972 -0.035 9.5 1 30
target/release/verify output/readme/moons 1337 shot 30438.885 105463.4 2.493867 0.36 19.4 1 30
target/release/verify output/readme/arches 1337 shot 23512.3 63468.41 -1.9067289 0.27 16 1 30
target/release/verify output/readme/settlements 1337 settlement-art
```

Each `shot` directory contains `shot.png`. The settlement run supplies `capital.png` and `inn-night.png`, along with additional art checks. Native GPU captures verify appearance; they do not establish browser frame rates. Future world-generation changes can move scenery away from old coordinates, so visually inspect regenerated screenshots before replacing the gallery.
