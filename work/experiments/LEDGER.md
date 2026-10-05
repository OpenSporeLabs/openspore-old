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
