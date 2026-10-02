# Project workflow

- Use local development previews for changes and open the local URL for the user, reusing the existing preview tab when possible.
- Do not deploy or publish to Sites automatically. Deploy only when the user explicitly requests it.
- Do not opt into automatic publication on Git push.

This is the user's standing preference, recorded on 2026-09-12.

# World compatibility during development

- Support only the current world generator, recipes, and save format. We are building the game; world changes may be destructive.
- Do not add backports, historical generator implementations, save migrations, automatic backup/recovery snapshots, or compatibility paths for older worlds unless the user explicitly requests them.
- Keep current-version saves and seed-based generation working. Missing, incompatible, or corrupt local saves should start a fresh current-version world instead of blocking play or attempting recovery.
- Keep validation and world/cache version identifiers so obsolete state is never applied to changed geography. Bump the relevant version when generation or save semantics change; rebuilding an old world is not required.

This is the user's standing preference, recorded on 2026-10-02.
