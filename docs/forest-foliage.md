# Forest foliage — 2026-09-12

Checkpoint before this pass: `checkpoint/pre-forest-foliage-20260912` (`f52b1c4`).

## Observed Safari scene

Seed 1337, Ancient Woodland near Saint Orin's Waystone, X 109764 / Z -15885,
heading 306°. Safari was using saved **High** world quality, Native 1403 × 908,
FXAA, Bloom 150%, and 400% cover. This was not the newly recommended Balanced
profile. A settled diagnostic sample showed 37 FPS, 1,758 loaded chunks,
8,282,368 loaded triangles and 774.1 MB of mesh buffers. Loaded triangles are
not triangles submitted per frame. Reflections reported zero draws in this view.

This is a diagnostic observation, not a repeatable before/after benchmark.
Project Diablo II and its launcher were subsequently active; do not attribute
later timing differences solely to renderer changes. Native verification timings
are also not Safari FPS measurements.

## Change

- Broadleaf crowns use four rather than five cards; birch/willow use three
  rather than four. Nearby density of trees, terrain and the horizon are unchanged.
- Cards are offset through the crown volume and broadleaf widths shrink 6%,
  reducing overlap. Per-vertex crown normals replace flat card lighting.
- Conifers retain three cards, with an oblique third spray for ordinary pine/fir.
  Windswept coastal trees retain their broad upper fan.
- A tapered two-triangle conifer card removes 24% of the original rectangular
  area without stretching its UVs. The base-level needle alpha mask is entirely
  inside the new boundary. Coarse mip silhouettes need not be pixel-identical.
- Leaf materials perform alpha rejection before generic material projection.
  Surface-map detail fades between 16 and 36 metres; beyond that, foliage samples
  albedo only and uses shared roughness. Wet leaf sheen uses the lightweight
  vegetation highlight at all distances. Layers 5/6 are shared by some smaller
  plants, so they also use this material path.

Full and middle LODs generate identical retained cards, UVs, wind weights and
normals. Camera, reflected, shadow and shelter passes use the same trimmed mesh
and alpha cutoff. The godray pass, shadow settings and texture resolution are
unchanged. The atlas still has 17 shared 128 × 128 layers, authored procedurally
on a 64 × 64 grid with coverage-preserving mipmaps.

## Validation

- Rust foliage recipe test: finite bounded cards; middle LOD is an exact subset;
  distant meshes remain opaque.
- Four material tests: deterministic textures, cutout mip coverage, valid opaque
  materials and normals, and full-resolution needle containment.
- Release WASM build and deterministic streaming packet checks.
- UI / input / persistence / worker lifecycle checks.
- `verify … 1337 foliage` provides a repeatable native renderer capture at the
  user's forest coordinates. It exercises the real shader pipelines, with High
  quality, 720p, FXAA and Bloom 100%; it is not a browser performance benchmark.

The native GPU verification completed successfully and produced
`output/forest-foliage/native/orins-woodland.png`. Shader pipelines validated and
16 actual frames rendered at the captured position. The build's WASM SHA-256 is
`a9bfa0d5cb3edbe85716af4dd1bae2949aaa3f7fdd10152aa95e68c35000c34e`.

Safari subsequently entered the updated wilderness successfully. The user
confirmed the improvement while exploring; no controlled Safari timing was
recorded after the change. The next pass addresses forest composition and
plant art, separately from these shader and card optimizations.
