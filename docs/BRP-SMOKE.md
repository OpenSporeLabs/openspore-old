# BRP smoke test — transcript

What was actually run, and what it actually returned. Produced against
`spore-engine` with `--features brp` on a real windowed run
(`--placeholder`, 640×480, Vulkan/RADV on an RX 7900 XTX).

`docs/BRP.md` has the design; this file is the evidence. Every result below is
copied from the run, not paraphrased.

---

## Environment

```
bevy                0.19.1   (Cargo.lock)
bevy_remote         0.19.1   (resolved by cargo tree)
bevy_brp_extras     0.22.8   (crates.io release)
bevy_brp_mcp        0.22.8   (crates.io release)
openspore           pid 2310296
BRP port            15702/TCP on 127.0.0.1
adapter             AMD Radeon RX 7900 XTX (RADV NAVI31), backend Vulkan
```

---

## Commands

```bash
# install the MCP server
cargo install bevy_brp_mcp --version 0.22.8 --locked --root ~/.local

# confirm one Bevy in the feature union
cargo tree -p spore-engine --features brp -e normal
#   ├── bevy v0.19.1
#   │   ├── bevy_remote v0.19.1
#   ├── bevy_brp_extras v0.22.8

# build with the bridge
cargo build -p spore-engine --features brp

# launch
./target/debug/openspore --placeholder --width 640 --height 480
```

---

## 1 — The listener comes up

```
INFO spore_engine::brp: openspore: BRP dev bridge enabled (bevy_brp_extras),
     listening on port 15702 unless BRP_EXTRAS_PORT overrides it
INFO bevy_brp_extras::plugin: BRP extras enabled on http://localhost:15702 (default)

$ ss -ltnp | grep 15702
LISTEN 0 128 127.0.0.1:15702 0.0.0.0:*  users:(("openspore",pid=2310296,fd=15))
```

The socket is owned by the `openspore` process. **PASS**

---

## 2 — The MCP server connects to the live app

`brp_status`:

```json
{"status":"success",
 "message":"Process 'openspore' (PID: 2310296) is running with BRP enabled on port 15702",
 "metadata":{"app_name":"openspore","pid":2310296,"port":15702}}
```

**PASS** — the PID matches the process this script started.

---

## 3 — Live inspection (not just "the port is open")

`brp_extras_get_diagnostics`:

```json
{"fps":{"current":59.972442662596535,"average":72.90240770315155,
        "smoothed":59.972442662596535,"history_len":72},
 "frame_time_ms":{"current":16.674325,"average":17.871446319444445},
 "frame_count":72.0}
```

60 FPS, 16.7 ms/frame, 72 frames counted. **PASS** — a static process could not
report a live frame counter.

`rpc_discover` → **39 methods** registered in the running app's
`RemoteMethods`, including all 15 `brp_extras/*` methods and Bevy's own
`world.*` / `schedule.*`.

---

## 4 — Query a real resource

`world_list_resources` → **5 resources**:

```json
["bevy_time::time::Time<()>",
 "bevy_time::time::Time<bevy_time::fixed::Fixed>",
 "bevy_time::time::Time<bevy_time::real::Real>",
 "bevy_time::time::Time<bevy_time::virt::Virtual>",
 "spore_engine::brp::BrpProbe"]
```

`world_get_resources(resource="spore_engine::brp::BrpProbe")`:

```json
{"value":{"frames":77,"label":""}}
```

**PASS** — real component data, read out of the live world.

---

## 5 — Mutate runtime state, and prove it

`world_mutate_resources(resource=…, path="label", value="mutated-by-smoke-test")`
→ `{"status":"success","message":"Mutated resource spore_engine::brp::BrpProbe"}`

Read back:

```json
{"value":{"frames":81,"label":"mutated-by-smoke-test"}}
```

**PASS**, and the read-back is worth two details:

* `label` changed from `""` to `"mutated-by-smoke-test"` — the write landed;
* `frames` advanced `77 → 81` between the two reads — the app was **still
  running and stepping** while the mutation happened. That is what separates
  "the RPC returned success" from "the value is in the world".

---

## 6 — Screenshot

`brp_extras_screenshot(path="/tmp/opencode/openspore-smoke.png")`:

```json
{"status":"success","message":"Screenshot saved to /tmp/opencode/openspore-smoke.png",
 "result":{"success":true,"path":"/tmp/opencode/openspore-smoke.png","status":"completed"}}
```

On disk:

```
-rw-------  190592 bytes
PNG magic ok: True
dimensions: 640 x 480
distinct byte values: 120  ->  NON-uniform (real content)
```

**PASS, with the caveat checked rather than assumed.** The upstream README warns
that a minimized/occluded surface can yield a black frame while still reporting
success, so "the file exists" is not enough. Decompressing the IDAT stream gives
**120 distinct byte values** — real rendered geometry, not a uniform rectangle.

Only the full-window path was exercised. Camera-viewport and entity-crop
captures were **not** tested.

---

## 7 — Keyboard input reaches the app

There is no `brp_extras_send_keys` MCP tool; `brp_extras/send_keys` is a BRP
method, reached via `brp_execute`.

Key names are **Rust variant spellings**, not uppercase scan codes.
`"ESCAPE"` is rejected:

```json
{"status":"error","message":"Invalid key code 'ESCAPE': Unknown key code",
 "metadata":{"stage":"execution","code":-32602}}
```

`"Escape"` is accepted:

```
brp_execute(port=15702, method="brp_extras/send_keys",
            params={"keys":["Escape"]})
```

```json
{"status":"success","message":"Executed method brp_extras/send_keys",
 "result":{"success":true,"keys_sent":["Escape"],"duration_ms":100}}
```

**PASS** — and this is a real end-to-end assertion, not just "the RPC returned
success". OpenSpore's *only* input handling is `exit_on_escape`
(`crates/spore-engine/src/lib.rs`), which writes an `AppExit` message on
`KeyCode::Escape`. The application **exited**, which can only happen if the
synthesised key press reached Bevy's `ButtonInput<KeyCode>` and ran the system:

```
RESULT: app EXITED -> send_keys drove exit_on_escape
```

---

## 8 — Clean shutdown

`brp_extras/shutdown` is likewise a BRP method rather than an MCP tool:

```
brp_execute(port=15702, method="brp_extras/shutdown", params={})
```

```json
{"status":"success",
 "message":"Executed method brp_extras/shutdown",
 "result":{"success":true,"message":"Shutdown initiated - will exit in 10 frames",
           "pid":2310296}}
```

Afterwards:

```
### 9. clean shutdown?
    OK process 2310296 exited, status 0
### 10. no openspore process remains
    OK none
    OK port 15702 released
```

**PASS** — exit status **0**, not a signal, and the port was released.

---

## 9 — Nothing else regressed

```
cargo fmt --all --check                -> clean
cargo clippy -p spore-engine            --all-targets -- -D warnings -> clean
cargo clippy -p spore-engine --features brp --all-targets -- -D warnings -> clean
cargo test  --workspace                 -> 1622 passed, 0 failed, 112 ignored
cargo build --workspace                 -> ok
```

Isolation, measured rather than asserted:

```
$ cargo tree -p spore-engine -e normal | grep -c brp
0
```

The default build contains **no** BRP crate at all.

The headless decode path is unchanged — `--info` still returns before an `App`
exists, so it never opens the listener:

```
$ ./target/debug/openspore --preset documented-asset \
      --package SPORE/Data/Spore_Content.package --info
OPENSPORE-STAGED v1 key=0x00e6bce5:0x40637e03:0x067a07f0 package=Spore_Content
     format=gmdl meshes=1 triangles=20 normals=derived decode=complete
OPENSPORE-SCENE  v1 objects=1 models=1 parts=1 triangles=20 mapped_bytes=995268083
exit=0

$ ss -ltn | grep 15702      # after --info
(nothing)
```

---

## Not verified

Stated plainly, because each of these is a capability the brief asks about and
none of them was exercised:

| capability | status |
|---|---|
| launch the app | **verified** (via shell, not via `brp_launch`) |
| detect a running app | **verified** (`brp_status`) |
| query entities | **partial** — 241 entities enumerated; component data only for registered types (see below) |
| query resources | **verified** |
| mutate runtime state | **verified** (resource) |
| mutate a **component** on an entity | **not verified** |
| keyboard input (`send_keys`) | **verified** — drove `exit_on_escape` to completion |
| `type_text` | **not tested** |
| mouse input (click / drag / scroll / move) | **not tested** |
| read logs (`brp_read_log`) | **not tested** — the app logs to a redirected file here, not to the MCP log store, because it was launched by shell rather than by `brp_launch` |
| `watch` component changes | **not tested** |
| `world_find_entities_by_name` | **ran, 0 matches** — OpenSpore sets no `Name` components. See below. |
| entity-crop / camera-viewport screenshot | **not tested** |
| `brp_launch` / `brp_list_bevy` discovery | **not tested** |

### The reflection gap — the most important finding

**In Bevy 0.19, only asset types register themselves for reflection.**
`bevy_transform`, `bevy_camera` and `bevy_mesh` contain **zero**
`register_type` calls in 0.19.1, so the everyday components an agent would want
are invisible to BRP:

```
$ curl -s -X POST localhost:15702/jsonrpc -d '{"jsonrpc":"2.0","id":1,
    "method":"world.query","params":{"data":{"components":
    ["bevy_transform::components::transform::Transform"]},"strict":true}}'

{"error":{"code":-23402,
  "message":"Component `bevy_transform::components::transform::Transform`
             isn't registered or used in the world"}}
```

Same for `bevy_ecs::name::Name`, `bevy_camera::visibility::Visibility`,
`bevy_math::bounding::bounded3d::Aabb3d`.

`registry.schema` on the running app returns **112 types, all of them assets or
asset handles**. Consequences:

* `world.query` with component data returns **0 rows** unless the application
  registers the type itself. This is upstream Bevy 0.19, not an OpenSpore
  defect.
* `world.find_entities_by_name` cannot work, because `Name` is not registered.
  It returned `0 named entities` against 241 live entities. **OpenSpore also
  does not set `Name` on anything it spawns**, so even with reflection enabled
  there would be nothing to find.
* Component-level mutation and component watching are currently unreachable.

`BrpProbe` exists precisely to close this for the dev bridge and to prove the
read/write path works. It is a demonstration, not a fix.

**The real fix is follow-up work**: register OpenSpore's actual runtime state
(`LoadedScene`, `WorldResources`, `NormalMapPolicy`, `CameraFraming`) and give
spawned entities `Name`s. That means deriving `Reflect` on domain types, which
is a design decision about the Bevy-free domain layer — deliberately out of
scope here, per the brief's "do not redesign OpenSpore".

---

## Reproducing

```bash
 ./tools/brp/smoke.sh             # launch → interrogate → screenshot → shutdown → verify
```

It is a self-contained script rather than a series of ad-hoc commands because
the app must be started, interrogated and stopped *while it is definitely
alive*. During development the first screenshot attempt failed with
`Connection failed: … 127.0.0.1:15702` because the window had closed
24 seconds earlier (Bevy logged `No windows are open, exiting`) — a timing
artefact, not a bridge fault, and exactly the failure a hand-run sequence
invites. The script also carries an `EXIT` trap that kills only the PID it
started, so a failure between launch and shutdown cannot leave a process behind.