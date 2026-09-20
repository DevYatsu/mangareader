# Vendored assets

## bulma.min.css

- Version: **1.0.4** (published 2025-04-19)
- Source: `https://cdn.jsdelivr.net/npm/bulma@1.0.4/css/bulma.min.css`
- License: MIT
- SHA-256: `67fa26df1ca9e95d8f2adc7c04fa1b15fa3d24257470ebc10cc68b9aab914bee`

Vendored rather than loaded from a CDN so the app renders fully offline on
Raspberry Pi class hardware with no external network dependency.

To upgrade: download the new pinned version, re-run the verification greps in
`docs/superpowers/plans/2026-09-20-bulma-slice-a-foundation.md` (Task 1, Step 2),
and re-run `cargo test` to confirm the Ink & Ember token overrides still land.
