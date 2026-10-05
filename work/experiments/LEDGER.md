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
