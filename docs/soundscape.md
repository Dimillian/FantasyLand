# Environmental soundscape

The Rust engine reports acoustic facts through `GameState.audio`. The browser mixes them using Web Audio, independently of the rendering resolution. This first sound palette consists of original synthesized effects, not field recordings or voiced characters.

## Coverage

- Broadleaf leaf rustle, conifer needle hiss and open-country wind respond to the shared weather wind and gust strength.
- Daytime forest/meadow/tropical/coastal bird calls, nighttime owls, marsh frogs and insect beds. Calls vary in position, timing and pitch; rain, snow and gales suppress wildlife activity.
- Rain outside, muffled rain on roofs inside, alpine/blizzard wind, river flow, quiet lake water, ocean surf and nearby campfire/hearth crackle.
- Nine footstep surfaces: grass, leaves, mud, gravel, stone, wood, sand, snow and water. Actual travelled distance drives alternating footsteps; idle movement against obstacles and teleports do not produce steps. Landings have a separate accent. Rain-wet roads become muddy, snowy ground becomes snow, and interiors use wood/stone.
- Lightning events queue delayed thunder on the real audio clock, using distant world-space storm anchors. Return strokes do not enqueue duplicate thunder. Teleports clear distant queued events. This approximates the nearest discharge cell rather than tracing the rendered lightning channel.
- Interior transitions filter exterior sound and add a shared short room reverb, hearths and occasional wood creaks. Dialogue ducks ambience.

## Playback and budgets

Audio unlocks on an existing enter-world or mouse-focus gesture. Hidden tabs, inactive game views and benchmarks suspend playback. Volume controls for master, ambience, footsteps and wildlife persist with the save. Footsteps default to 45%; their soft-surface impacts are deliberately subdued.

`node scripts/build-audio.mjs` deterministically creates 73 mono 22.05 kHz / 16-bit WAV assets and a manifest, approximately 7 MiB downloaded. The browser check decoded these to 30.4 MiB at its output sample rate. There are ten looping sources and at most eighteen short voices, with four concurrent asset decodes. No audio synthesis runs in the animation loop. Mixer targets update with the existing 10 Hz HUD sample; acoustic water/fire/interior probes cache their results for 0.8 seconds or until significant movement.

The initial sources are procedural textures of sound: wildlife is suggestive rather than species-accurate field audio. Dedicated waterfall emitters, building-specific acoustic geometry, NPC voices/footsteps and music are future extensions. Current interior occlusion is a room envelope rather than per-door ray tracing. Audio is implemented in the browser frontend; the native render-verification executable remains silent.

## Local verification

- `node scripts/verify-audio.mjs`: mix policy, step cadence, thunder deduplication/delay, graph lifecycle, voice cap, mute/resume races, asset levels and loop seams.
- `node scripts/verify-ui.cjs`: saved volume controls and existing game UI/input regressions.
- `cargo test --release --lib --locked`: engine checks, including acoustic floor priority and thunder anchor stability.
- `/audio-review.html`: lightweight listening room using the actual sound player and staged environment facts. Includes every surface, ten environments, a three-second thunder preview and decoded-memory/signal meters. Its walking and event triggers are audition controls, not a world simulation.
