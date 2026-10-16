# Behaviour evidence — from a binary observation to an OpenSpore specification

How a fact learned from the original game becomes something OpenSpore is allowed
to build on. Read this before wiring any original behaviour into the engine.

The short version: **a binary trace is evidence, not an abstraction.** Evidence
tells you *what happened*; a specification decides *what that means for a design*,
and the two are separate acts with separate artefacts.

---

## 1. The boundary this workflow protects

`spore-recomp` executes the original binary and emits observations. OpenSpore is
independently authored. The two are siblings, not parent and child:

```
spore-recomp  ──executes──▶  runtime observations (its own reports)
                                    │
                                    │  read-only, by hand, with provenance
                                    ▼
                       an OpenSpore behaviour specification
                                    │
                                    │  an independent domain design
                                    ▼
                crates/spore-*  (no recomp code, no generated identities)
```

Rules, all of them load-bearing:

* **Never modify `spore-recomp`**, never build against it, never import its
  generated C/C++, and never mechanically translate it into Rust.
* **Never let recomp-specific machinery into public OpenSpore types.** VAs,
  generated function identities, guest register structures, stack deltas and
  probe IDs are *research vocabulary*. They may appear in a specification's
  provenance, never in `spore-core` or any domain type.
* A **binary trace is evidence, not automatically a good abstraction.** Design
  around coherent domain concepts with explicit contracts, then use the observed
  behaviour to *validate* those choices — not the reverse.
* The CRT-startup findings (`__iob_func`, the current `fprintf` frontier) are
  evidence about the C runtime, not about creature simulation, editor rules or
  rendering. Do not let a startup-recovery result leak into a gameplay claim.

The machine-enforced half of this boundary lives in `docs/SEMANTIC-CONTRACTS.md`
and `docs/tooling/semantic-exchange.md`: the passport and runtime overlay are
*projections* that copy facts with provenance and derive nothing. That is the
model to imitate by hand.

---

## 2. The four grades, distinguished

Every behaviour statement must be exactly one of these. They are the four useful
collapses of the repository's canonical 7-rung `spore_core::EvidenceLevel`
(UNKNOWN · APPROXIMATION · INFERRED · SUPPORTED · OBSERVED · CONFIRMED ·
VERIFIED); use the fine-grained level in a machine-readable record and the plain
word in prose.

| word | means | backed by | may it drive a design? |
|---|---|---|---|
| **Observed** | directly supported by a reproducible observation or test | a committed trace, a reproducible run, a byte count over real data | yes — it is the anchor |
| **Derived** | a justified interpretation of observed behaviour | named observations + the reasoning that connects them | yes, *as a design choice*, never as "what the original does" |
| **Hypothesis** | plausible, not yet sufficiently verified | partial or single-sample evidence | as a designed experiment only; never silently |
| **Unknown** | evidence is missing or ambiguous | — | no |

The failure this table prevents is a Hypothesis quietly becoming an Observed over
a few edits. **Unknown is never a zero, an empty collection or a `false`** — it is
"we did not look", and `spore_core::Fact::unavailable(reason)` is how that is
recorded in code.

---

## 3. What every specification records

A behaviour specification is a short, versioned note. At minimum:

* **Identity** — a stable topic name (not an address) and a one-line statement.
* **Grade** — one of Observed / Derived / Hypothesis / Unknown.
* **Preconditions** — the state required to trigger the behaviour.
* **Inputs → outputs** — what goes in, what comes out, typed.
* **State changes** — what the world looks like afterwards.
* **Provenance** — where the evidence came from, in the vocabulary below.
* **Uncertainty** — the explicit list of what is still unknown.

### Provenance vocabulary

Use the existing types; do not invent a parallel one:

* `spore_core::SourceClass` — `Runtime` (an external producer such as
  `spore-recomp`), `Ghidra`, `CommittedArtifact`, `GeneratedIndex`, `Derived`.
* `spore_core::ProvenanceMode` and `EvidenceState`
  (`LIVE` / `DERIVED` / `PERSISTED` / `MISSING`) — *how* a fact was obtained,
  orthogonal to how strong it is.
* A runtime observation may claim exactly one source class, `Runtime`. A runtime
  artefact claiming `ghidra` provenance is a static artefact in disguise and must
  be refused.

---

## 4. Worked example — the raster envelope

The one boundary that has completed the whole loop, stated at the right grade:

* **Observed** — a `raster` record (`0x2F4E681C`) begins with a 32-byte envelope
  followed by DXT5 blocks, confirmed on three real records.
* **Derived** — the decode rule `decodeRasterMips` / `Dxt5`, chosen as OpenSpore's
  interpretation and checked byte-for-byte against an independent Python oracle.
* **Observed** — the real record `0x40662900` decodes to a near-black alpha mask.
* **Unknown** — what the lit pipeline's light values are; no light data has been
  observed, so the values stay `approximated` and the crate carries a
  `claims` entry saying so.

Notice the shape: observations anchor the design, a derivation is *labelled as
one*, and the unknown is written down rather than filled in. `docs/BOUNDARIES.md`
§"The replacement pattern" is the same five steps stated per subsystem.

---

## 5. Where the grades live

| artefact | holds |
|---|---|
| `crates/*/src/claims.rs` | what a crate explicitly does **not** know |
| `spore_core::evidence` | the grade/state/source vocabulary, in code |
| `docs/BOUNDARIES.md` + `docs/replacement-status.json` | per-subsystem replacement status and evidence |
| `docs/SEMANTIC-CONTRACTS.md` | engine-level rules distilled from joined evidence |
| this file | the workflow that turns one into the other |

When you learn something, extend a claim; do not delete one because it is
inconvenient. A stale "unknown" costs a reader five minutes; a missing one costs
a wrong design.
