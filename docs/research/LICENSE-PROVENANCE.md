# License & provenance policy

Owner: orchestrator · Status: active · Date: 2026-10-16 · Supersedes: none

This is the authoritative policy `AGENTS.md` links to. It records the reuse
stance, the provenance categories every artifact must carry, and the verified
per-repo license matrix produced by the 2026-10-16 ecosystem survey.

## 1. Distribution stance

OpenSpore is distributed **GPL-3.0-compatible and non-commercial in intent**.
That is a real choice with real consequences, so it is stated plainly:

- **GPL-3.0 / GPL-3.0-or-later community code may be ported or adapted.**
  Porting it makes OpenSpore GPL-3.0; that is accepted.
- **MIT / Apache-2.0 / BSD / Unlicense / public-domain code may be reused**
  with attribution and its notice preserved.
- **No license means no permission.** A repository with no `LICENSE`/`COPYING`
  and no permissive header is **All Rights Reserved by default**. "It is public
  on GitHub" and "we are non-profit" do **not** create a license. Reusing such
  *code* is copyright infringement; there is no policy that can waive this. Its
  **factual knowledge** (field names, ids, hashes, layouts, docs, Ghidra labels)
  remains readable and reusable — facts are not copyrightable — but the code is
  blocked pending explicit author permission.
- **Never in scope, under any license:** proprietary EA/Maxis source, and any
  committed Spore asset byte (models, textures, sounds, scripts, `.package`).
  OpenSpore never had rights to these; this is the project's premise, not a
  licensing preference.

## 2. Provenance categories (record one per reused artifact)

| Category | Meaning | Obligation |
|---|---|---|
| `independent-from-spec` | written from a published/community *format* description | none beyond attribution |
| `independent-from-observation` | written from observed real files / differential counts | record the corpus + counts |
| `informed-by-re` | design shaped by reverse-engineering research (Ghidra, `spore-recomp` reports) | record the report; keep RE detail out of the public domain model |
| `ported-community` | code ported/adapted from an identified community repo | preserve license + notice; copyleft propagates if GPL |
| `direct-dependency` | an external crate/library | license in the dependency graph |
| `blocked` | source identified but its license is absent/ambiguous | **do not reuse code**; knowledge only |

Every format crate's `claims` module and every differential test already cite a
`basis`; that citation **is** the provenance record. Keep citing it.

## 3. Verified license matrix (survey 2026-10-16)

Source: `github.com/Spore-Community` (23 repos) + linked projects. Branch
verified by raw fetch; GitHub REST API returned HTTP 403 during the survey, so
last-commit dates are **Unknown, not guessed**. Treat as a snapshot and refresh.

| Repo | License | Verified | Contents | Reuse |
|---|---|---|---|---|
| `Spore-Community/SporeModder-FX` | **GPL-3.0** | `LICENSE.md` + headers | Java FX parsers/writers (dbpf, prop, gmdl, rw4, raster, dds, anim, tlsa, pctp, spui, shaders, simulator, creaturedata, cell, cnv, locale…); `reg_type.txt`/`reg_file.txt`/`reg_property.txt`/`reg_simulator.txt`; `Documentation/*.txt` | **ported-community** (copyleft) or clean-room from its docs; registries are factual knowledge |
| `Spore-Community/SporeModder-Blender-Addons` | **GPL-3.0** | full LICENSE fetched | RW4/GMDL/ANIMATION import + export in Blender | **ported-community**; also a round-trip conformance oracle |
| `Spore-Community/Spore-ModAPI` | **none** | no LICENSE/COPYING in root | C++ SDK injecting `.dll` into `SporeApp.exe`; `SDKtoGhidra/ImportSporeSDK.java`; `SporeGhidra_*.xml` DB | **BLOCKED** (code); knowledge/DB readable |
| `Spore-Community/SporeTools` | **MIT** | `Copyright (c) 2023 Kade` | asset-id tool, FNV hash tool (browser) | reuse with notice |
| `Spore-Community/ModAPI-Launcher-Kit` | **MIT** | `Copyright 2017-2024 Splitwirez` | `.sporemod` installer/launcher (injection) | format knowledge reusable; injector out of scope |
| `Spore-Community/ModBrowser` | **MIT** | `Copyright (c) 2020 Kade` | mod index/browser | reuse with notice |
| `SporeHash-VSCode` | MIT (badge) | README | FNV name↔id extension | reuse with notice |
| `SporeGlobals-Python`, `Spore-AdventureLocalizer`, `Spore-CreationZIP-Porter` | MIT (badge) | org page | misc | reuse with notice |
| `sporeget`, `Spore_EP1Stegano-Python` | Unlicense (badge) | org page | misc | public-domain-style |
| `Server-IDs`, `SporeModder-FX-Updater`, `spore-community.github.io`, `SporeRedirectServer`, `PNG-Decoder-*`, `PNG-Analyzer-Discord-Bot`, `Adventure-Decoder` | **none shown** | org page | misc | **BLOCKED** pending clarification |
| `emd4600/SporeModder-FX` (upstream), `Rosalie241/SporeModLoader` | check upstream at use time | — | original / loader | verify before reuse |

`storytold/photocraft` is **MIT + Apache-2.0** — methodology reference only, not a
dependency (see `../architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md` §6).

## 4. Ambiguous / blocked register

Recorded, never guessed. Update only when the author grants a license or a
clarifying statement is found:

- `Spore-ModAPI` — no license. Code blocked; API surface + Ghidra DB are knowledge.
- `Server-IDs`, `SporeModder-FX-Updater`, `spore-community.github.io`,
  `SporeRedirectServer`, `PNG-Decoder-NetCore`, `PNG-Decoder-Python`,
  `PNG-Analyzer-Discord-Bot`, `Adventure-Decoder` — no license shown.
- GitHub REST API was rate-limited during the survey; per-repo last-commit dates
  and the full file trees of the badge-only repos were not raw-confirmed.

## 5. The one load-bearing factual hash (non-copyrightable)

`SporeModder-FX` `src/sporemodder/HashManager.java` (`fnvHash`) proves the name→id
hash is **FNV-1, not FNV-1a**: offset basis `0x811C9DC5`, then per lowercased
16-bit char `value *= 0x1000193; value ^= char` (multiply-then-xor). This
corroborates `AGENTS.md` and `spore_rigblock::vocabulary` independently and may be
reimplemented freely — an algorithm is a fact.
