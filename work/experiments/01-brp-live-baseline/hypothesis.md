# Experiment 01 — the live application as a laboratory

## Hypothesis
The running `openspore` binary exposes a real, inspectable application state over
BRP: diagnostics, frame timing, entity count, resources, and a screenshot.

## Why this had to be measured rather than assumed
`docs/BRP.md` and `AGENTS.md` both record a specific trap: in Bevy 0.19 only ASSET
types register for reflection, so `Transform`, `Name` and `Visibility` are invisible
to BRP and a query for them fails *silently* with `strict: false`. A connected bridge
and an inspectable application are different claims, and only the second one is at
risk.

## Inputs
- binary: `target/debug/openspore`, built with `--features brp`
- `--preset documented-asset --package SPORE/Data/Spore_Content.package`
- display `:0`, AMD Radeon RX 7900 XTX, Vulkan/RADV

## Runtime setup
`setsid`, window 800x600, BRP on 127.0.0.1:15702. Launched as a tracked PID.

## Operations
1. `brp_status` — is the bridge alive and which app does it own
2. `brp_extras_get_diagnostics` — frame time, entity count, uptime
3. `world_get_resources` on `spore_engine::brp::BrpProbe` — read an OpenSpore-owned resource
4. `brp_extras_screenshot` — capture the real framebuffer

## Stopping condition
Port released and PID gone.
