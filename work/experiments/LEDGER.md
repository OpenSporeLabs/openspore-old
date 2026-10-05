# Experiment ledger

Every experiment that tests a hypothesis against the real corpus or the running
application. `result` is one of PROVEN / OBSERVED / REFUTED / UNKNOWN.

| id | hypothesis | result | evidence | next |
|---|---|---|---|---|
| 01 | the running `openspore` exposes real, inspectable state over BRP | **PROVEN** | 62.8 FPS, 16.7 ms avg frame, 439 KB non-blank 800×600 screenshot inspected, `BrpProbe` read back, `decode=complete triangles=20` in the log | wire the creator projection into the engine |
| 01a | `--sporemodel-package` yields a real creature colour | **REFUTED** | `state=unavailable reason=model_group_unnamed link=UNKNOWN`; the preset's model group `0x40637e03` matches no creator block group | find a model whose group corresponds to a `sporemodel` group |

## Conventions

* Screenshots are inspected, not assumed. A screenshot proves visual presence, broad
  geometry, visual colour and texture presence. It does **not** prove internal
  identity, exact resource linkage, or exact codec semantics — those need the
  structured evidence beside it.
* A negative result is a result. 01a is recorded as REFUTED rather than dropped.
* Process ownership is part of the evidence: every launched PID is tracked and every
  experiment ends with the port released.

## Experiment 02 — a real creator document, loaded and rendered

| id | hypothesis | result | evidence |
|---|---|---|---|
| 02 | a real `.sporemodel` document loads and renders with its real hierarchy | **PROVEN** | `crt 0x2b978c46:0x40626200:0x05e3df13`: 6 blocks, 6 resolved, 0 unresolved, 6 entities, 384 triangles, 540 vertices, 6 bones, depth 5, 0 misses, 0 read errors. Screenshot inspected. Parentage read back over BRP matches the document exactly: `1→root, 0→1, 2→0, 3→2, 4→3, 5→4` |
| 02a | the camera frames the creature | **REFUTED** | screenshot shows the creature at ~2% of frame height. Real cause NOT isolated — see below |

### 02a: what is known, and what is not

**Known.** The part's `gmsh` geometry spans 0.41 units on its largest axis and the
document's authored `<position>` values span 0.36, so a correctly framed creature is
about **0.7 units** tall. `ScenePlugin` places the camera at a hardcoded `(0, 6, 34)`,
authored for "geometry of roughly unit-to-10-unit size" by its own comment. At distance
34 a 0.7-unit creature occupies ~2% of the view, which is exactly what the screenshot
shows.

**Fixed along the way.** `Projection::bounds()` was built from each part's **rigblock**,
which is a *bounds box* — a tight box marking where a part may be placed — not the part's
extent. Measured over the 1320 instances carrying both a rigblock and a `gmsh`, geometry
spans a median **2.01x** the rigblock half-extent, worst case **13.2x** among the 678
composed parts. It now uses the decoded geometry, which is what actually gets drawn.

**Not isolated.** Making `bounds()` correct did **not** fix the framing, so the camera
still appears not to be moving. The remaining suspect is
`creator::frame_creator_camera`, which takes `Option<Res<CameraFraming>>`: an absent
resource is not an error anywhere, it just returns, leaving the default camera. Whether
the resource is absent or the `Query<&mut Transform, With<MainCamera>>` does not match is
**UNKNOWN** — both produce this identical picture.

**A wrong inference worth recording.** I read `CameraFraming` over BRP, got `null`, and
concluded the resource had never been inserted — then nearly went to "fix" a framing
system that may be entirely correct. `CameraFraming` derives `Resource` but not
`Reflect`, so a `null` from BRP means *not registered for reflection*, never *absent
from the world*. This is the same trap as Bevy's `Transform` being invisible to BRP, and
it is now documented at the type. The lesson generalises: **a probe that returns nothing
is evidence about the probe until proven otherwise.**

## Experiment 02 — the creator camera, and a misdiagnosis closed

| id | hypothesis | result | evidence |
|---|---|---|---|
| 02 | the camera does not frame the creator (`CameraFraming` is absent, or the camera query does not match) | **REFUTED** | headless probe: resource present in the world, 1 camera matched, camera at `(0.0, 0.5258, 1.8056)` — exactly the closed form for that creature's measured extent |
| 02a | framing holds across real geometry, not just a tidy fixture | **PROVEN** | 47/47 real creature documents frame; distances span **0.43 .. 130.45**, a 300x range, each from its own measured extent |
| 02b | the creature in the session's screenshot was a representative creature | **REFUTED** | `0x40626200:0x05e3df13` is six copies of ONE part (`0x40626200:0xd05ff5c3`) in a sparse line. 46 of the 47 creature documents are single-part chains |

### What actually happened, and why it is worth the space

The previous session concluded the camera was parked at `ScenePlugin`'s
`(0, 6, 34)`. That conclusion came from a BRP read of the camera transform
returning `null`. But **Bevy 0.19 registers no reflection for `Transform`** —
`bevy_transform`, `bevy_camera` and `bevy_mesh` contain zero `register_type`
calls — so `null` means *not registered for reflection*, never *absent from the
world*. The read was correct and the interpretation was wrong.

This is the second time this exact trap produced a confident wrong answer, and
the second time it was in this repository's own documentation. Two more things
made it stick:

1. **The nearest plausible bug got the credit.** The rigblock-as-bounds-box bug
   was REAL and worth fixing (geometry spans a median 2.01x the rigblock
   half-extent), but for this creature the world-position spread dominates the
   box, so fixing it moved the camera distance by about 6%. It was not the cause
   of the symptom that prompted the search. Attributing a symptom to the nearest
   plausible cause is how a real bug hides an unrelated one.
2. **The 2% estimate was arithmetic on the wrong premise.** `0.7 / 34` is only
   meaningful if the camera is at 34. At the real distance of 1.21 the same
   creature occupies roughly 30% of frame height.

So the real cause of "tiny" was never a bug: the document is six identical
sub-0.1-unit parts spread over 0.87 units, framed correctly, each blob
legitimately small in frame. The only genuinely wrong thing was the **diagnosis**,
and the reason it was wrong is that the camera's position was not observable.

### The fix is observability, not arithmetic

`CreatorRuntimeState` now publishes `camera_framed`, `framed_extent`,
`camera_position`, `camera_distance_to_centre` and
`world_transform_applies_orientation`, read out of the world by
`tick_creator_runtime` rather than assumed from the projection. "Did the camera
move" is now a question with an answer, and answering it needs no screenshot.

`world_transform_applies_orientation` is deliberately published as `false`.
`Projection::bounds` adds a part's world position to its **untransformed**
geometry positions, so a rotated or non-unit-scaled part contributes a box that
does not match what is drawn. That is correct for the corpus, which is authored
at small near-axis scales, and wrong in general. Recording it as a `false` field
is the honest reading and is what a future fix must flip.

Also recorded: 46 of 47 real creature documents are single-part chains, so
`0x40626200:0xde727fa9` (5 distinct parts, 10 blocks, 1896 triangles) is the only
multi-part creature in `Spore_Game.package` and the honest demo subject.
