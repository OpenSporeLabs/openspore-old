# Conclusion — experiment 01

**Result: PROVEN.** The running `openspore` is a usable experimental target, and the
BRP reflection trap is real and has the exact shape `AGENTS.md` documents.

## What was observed

| observation | value |
|---|---|
| `brp_status` | bridge live, app `openspore` |
| FPS | 62.8 current / 60.0 average |
| frame time | 15.9 ms current / **16.7 ms average** |
| frames advanced | 1159 → 1162 across two calls |
| `BrpProbe` resource | `{"frames":1162,"label":""}` — readable, and an OpenSpore-owned type |
| screenshot | 439 523 bytes, 800×600, **inspected and non-blank** |
| GPU | AMD Radeon RX 7900 XTX, Vulkan/RADV, clustering + GPU preprocessing supported |

The screenshot shows real decoded geometry with a real directional-light shadow on the
ground plane. That is visual presence and broad geometry only — per the standing rule,
a screenshot cannot prove internal identity, exact resource linkage, or exact codec
semantics. Structured evidence below is what carries those.

## Structured evidence from the launch log

```
OPENSPORE-STAGED v1 key=0x00e6bce5:0x40637e03:0x067a07f0 package=Spore_Content
                    format=gmdl meshes=1 triangles=20
                    bounds_min=-2.070057,0.139144,0.081329
                    bounds_max=-0.371524,2.164293,5.326164
                    normals=derived textures=0/0 decode=complete
OPENSPORE-SCENE v1 objects=1 models=1 parts=1 triangles=20
                    normal_maps_bound=0 mask_channels_bound=0 models_painted=0
                    packages=Spore_Content,Spore_Game
```

`decode=complete` and `triangles=20` are the claims that matter here: the record
decoded with no stage to stop at, and the triangle count is a real count rather than a
placeholder.

## Two findings that were not expected

### 1. `--sporemodel-package` did not produce a colour, and the reason is specific

`--sporemodel-package SPORE/Data/Spore_Game.package` was passed, and `Spore_Game`
appears in the scene's `packages=` list. The colour is still absent:

```
OPENSPORE-PAINT v1 record=0/1 key=0x00e6bce5:0x40637e03:0x067a07f0
                 state=unavailable reason=model_group_unnamed link=UNKNOWN base=absent
```

`reason=model_group_unnamed`, **not** "no package searched" and **not** "no record
found". The search ran and reached a step where the model's own group is not
something a `sporemodel` group can be matched against. So this is a genuine
`UNKNOWN` link rather than a missing-data case, and the report says so instead of
substituting a neutral tint and calling it the creature's colour. This is the
behaviour the evidence discipline asks for, observed live.

**It also means the vertical slice's "real creature colour" is not reachable through
this preset.** The preset's model group `0x40637e03` does not correspond to any of the
creator block groups (`0x40626000` and friends). Worth chasing separately; not
resolved here.

### 2. The reflection trap is confirmed, and it is silent

A connected bridge and an inspectable application are different claims. What is
observable here: an **OpenSpore-owned** type (`spore_engine::brp::BrpProbe`) reads
back fine, because it is registered for reflection. What is *not* observable is
anything about the 1 mesh entity's `Transform`, `Name` or `Visibility` — those live in
`bevy_transform` / `bevy_mesh` / `bevy_camera`, which contain **zero**
`register_type` calls in Bevy 0.19.1, so they are invisible to BRP and a query for
them fails as zero rows under `strict: false`.

Not measured in this experiment: a `world_get_entities` count of 1 mesh entity + the
known-unqueryable components. Deliberately not claimed.

## Process ownership

- Launched with `setsid`, PID tracked in `/tmp/opencode/openspore.pid`.
- Port 15702 was confirmed free before launch and is released via
  `brp_extras/shutdown` at session end.
- No unrelated process was touched. The user's `llama.cpp` server was not contacted,
  inspected, or signalled.

## Next action

The laboratory works. The next thing it should answer is the §47 milestone —
load a real `.sporemodel`, replace a block, observe, save, reload — which needs the
creator runtime projection wired into the engine before a screenshot can mean
anything beyond "some geometry is present".