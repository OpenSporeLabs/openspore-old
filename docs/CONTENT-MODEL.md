# Content model — extensibility requirements

What OpenSpore must keep true so that community content can be added **without
recompiling the engine**, and what is deliberately *not* built yet.

This is a requirements and boundary document for the first milestone. The rules
below are the choices made today; the loader, package manifest and extension API
are explicitly future work. Do not read this as a shipped mod system.

---

## 1. What exists today

| concern | today | where |
|---|---|---|
| content identity | `(type, group, instance)` — the DBPF "TGI" | `spore_core::ResourceKey` |
| record naming | none; DBPF has no string table | `spore-assets` |
| authored documents | Spore's own `<sporemodel>` XML, **versioned** by `formatversion` (11 / 15 / 16 seen) | `spore-creator-core` |
| authoring operations | six edit ops over a creator document | `spore-creator-core` (`src/edit.rs`) |
| record graph | model→texture, model→rw4, model→raster edges, each graded | `spore-assets::graph` |
| canonical manifest | one deterministic row per record | `spore-assets::manifest` |

So the *base-game* content model already has stable IDs, versioned documents and
a deterministic manifest. What is missing is everything about **external packs**.

---

## 2. The requirements (the contract to keep)

1. **Stable IDs for content and references.** A reference must survive a rebuild
   and a reload. `ResourceKey` is the identity; nothing may key content on a
   filesystem path, an array index or a pointer.
2. **Versioned serialization with explicit migration boundaries.** Every
   persisted document carries a version, and a reader either understands it or
   refuses it *by name*. Never infer a field's presence from a version number:
   `spore-creator-core` already documents that `v15` and `v16` documents both
   write and omit `<handles>`, so a field is gated on its own presence, not on
   `formatversion`.
3. **Data-driven definitions.** Adding an ordinary asset or a supported data
   definition must not require editing Rust. Bulk content is data read at
   runtime; the engine change is only for *new behaviour*.
4. **Base-game vs external pack, distinguished.** A pack is a separately
   identified unit with its own provenance, not a pile of files merged into the
   base set. Which pack answered a lookup must be reportable.
5. **A package manifest with dependency, compatibility and conflict metadata.**
   A pack declares what it needs, which engine/content versions it accepts, and
   what it cannot coexist with.
6. **Validation tooling.** Missing dependencies and invalid content are
   *detected and named*, never silently ignored — the same rule as every other
   "unavailable is not empty" decision in this repository.
7. **A versioned extension API** for the behaviour that cannot be expressed as
   data — designed later, once the domain contracts above are stable.

---

## 3. Choices made today that keep the door open

* **Identity is a value, not a path.** `ResourceKey` is already serializable and
  comparable, so a pack's contents can be joined to the base game by identity
  with no filesystem assumption.
* **Parsers refuse what they do not understand.** gmdl versions other than 8 are
  rejected; undocumented shader-data ids are rejected; the `0x15xx` luminance
  raster family is rejected. A pack format must carry the same property: an
  unknown version is an error with a name, not a best-effort read.
* **Unavailable is never a zero.** A missing dependency is a named absence, in
  the same vocabulary `spore_core::Fact::unavailable` already provides.
* **The manifest is deterministic by construction.** Determinism is what makes a
  content change diffable and therefore reviewable; a pack loader must not throw
  that away.
* **Presentation is not content.** Colour and materials are derived by the
  renderer from domain meaning (see `domain::assembly` and `domain::materials`),
  so a pack describes *what a thing is*, not how this build paints it.

---

## 4. Non-goals for this milestone (do not build these yet)

* A full mod loader or load-order resolver.
* A binary plugin ABI.
* A scripting VM or a sandbox.
* A promise that arbitrary expansions already work.

Each of these needs the contracts in §2 to exist and be tested first. Building
them now would be a speculative framework ahead of the content that would
validate it.

---

## 5. The first concrete step, when it is taken

A pack *manifest* value type and a **pure, testable validator** — dependencies,
engine-compatibility range, and conflicts — with no loading and no filesystem
writes. It is the smallest piece that makes requirement 5 and 6 real, it can be
tested with no window and no game install, and it does not commit the project to
a loader design. It belongs in a Bevy-free crate, following the same rule as
every other domain layer (`docs/BOUNDARIES.md`, `docs/RUST-ENGINE.md` §1).
