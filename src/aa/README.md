# Spatial anti-aliasing

`smaa_*.wgsl` are the three SMAA 1x High passes from Bevy **v0.17.0**:
https://github.com/bevyengine/bevy/blob/v0.17.0/crates/bevy_anti_alias/src/smaa/smaa.wgsl

Only conditional preprocessing and removal of the introductory manual were performed.
The High preset retains diagonal detection, corner handling and local contrast adaptation.
Each module selects one stage. Bindings and equations are unchanged. Both MIT notices are included.

Lookup bytes come directly from the array bodies in upstream `AreaTex.h` and `SearchTex.h`:
https://github.com/iryoku/smaa/tree/master/Textures

- area.bin: 160 × 560 RG8, SHA256 `35065cef2a02cabcad711d6bf430239ae64e27d71c4e4fa06f29cce2c992f0d2`
- search.bin: 64 × 16 R8, SHA256 `3694eae5e9d44b8ebb4415a13f8c7b94dc08a2fc86658434d771c4610fe5744d`

Textures have one mip and linear clamp sampling; no flip, compression, sRGB conversion or resizing.
The internal-resolution input is tone-mapped, gamma-encoded RGBA8Unorm. Both spatial
methods blend in that display space; palette dithering, CRT masks and HTML HUD follow AA.
No jitter, history, motion vectors or multisampled depth are introduced.

FXAA is an independent WGSL implementation of the edge search and subpixel method
described in NVIDIA's FXAA whitepaper, with conservative subpixel blending (0.4 cap):
https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/FXAA_WhitePaper.pdf
