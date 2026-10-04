//! # spore-material — what a gmdl material id means, and a texture you can sample
//!
//! The vertical slice decodes a `gmdl` record and renders its mesh, but a gmdl
//! record's material section is a list of opaque ids and a list of
//! `{instance, group}` pairs. Nothing turns those into something a renderer can
//! sample. This crate is that step, and it is deliberately the narrowest crate
//! in the workspace: no Bevy, no GPU handle, no image crate, no I/O of its own,
//! and no invented names.
//!
//! ```text
//!   spore-dbpf ┐
//!   spore-gmdl ┼─> spore-material ─> RGBA8 + graded claims ─> spore-engine
//!   spore-texture┤        ▲
//!   spore-assets ┘        └── the (type, group, instance) the lookup used
//! ```
//!
//! # Three questions, three different answers
//!
//! A gmdl record's material info gives two lists and they are both opaque:
//!
//! 1. **`material_ids`** — one 32-bit id per mesh (`0x407DFDDB` on the
//!    documented asset), written as an array immediately after the mesh table.
//!    **What a material id names is not known.** The C++ reference,
//!    `src/assets/MaterialRegistry.cpp`, keeps a registry the engine populates
//!    from outside: the reference implementation never learned the mapping
//!    either, which is the strongest evidence available that it is not in the
//!    record.
//! 2. **`texture_refs`** — `{instance_id, group_id}` pairs. **No type word.** A
//!    reference does not even say it is a raster, so the type must be assumed
//!    and the assumption has to be visible.
//!
//! Those are the two *gaps*. The *questions* a renderer actually asks are three,
//! and this crate now answers two of them:
//!
//! | question | answer | where |
//! |---|---|---|
//! | What are a record's **texels**? | **measured, per record** | [`TextureRole`], [`crate::TextureRole`] |
//! | Which **mesh** owns which texture? | **an inference, labelled — and refused when the record contradicts it** | [`BindingScope`], [`partition_texture_refs`] |
//! | Which **sampler stage** does the game bind it to? | **no** | [`SamplerRole`], one variant |
//!
//! # The per-mesh cut is checked against the record, not just the arithmetic
//!
//! A gmdl material-info walk that stops part-way leaves a texture reference list
//! that is **shorter than the record's whole set**. Dividing such a list by the
//! mesh count usually still comes out even, because what is missing is whole
//! blocks — so the arithmetic succeeds and returns a confident, structurally
//! wrong binding. Measured over `Spore_Content.package`: the walk stops on the
//! undocumented shader-data id `0x218` in 619 of 4 209 gmdl records, in 100 of
//! them before the last declared block, and **619 of 619** divided evenly, so
//! every one of them reported [`BindingScope::PerMeshBlock`] before this was
//! fixed.
//!
//! [`partition_texture_refs`] therefore takes the cut only when the record's own
//! second statement of each entry's position agrees with it — the group id's low
//! byte, which equals the entry's ordinal in 6 684 of 6 684 references measured.
//! When the record contradicts the division the scope is
//! [`BindingScope::Undetermined`], which carries the collected and contradicted
//! counts, hands every mesh the whole short list, and records the contradiction
//! as [`claims::CUT_CONTRADICTED_BY_RECORD`]. The documented creature
//! `0x00e6bce5:0x40627100:0x067c79d2` is one of those 100: block 0 yields both
//! its references, so `refs_per_mesh = 2/2 = 1` used to hand mesh 1 a texture
//! out of mesh 0's block.
//!
//! `Undetermined` is deliberately **not** folded into [`BindingScope::WholeModel`].
//! The two mean different things — an uneven list, and a list the record
//! contradicts — and collapsing them is how a wrong binding hides inside an
//! honest-sounding fallback. Its known cost is stated on [`BindingScope`]: a
//! reference whose group id is `0` states ordinal `0` whatever its position, so a
//! record naming one is refused the cut even when its list is complete.
//!
//! # Roles come from content, never from position
//!
//! [`role::classify`] takes the record's `fourcc` **and** an exact per-lane
//! statistic over its own base mip, and a record whose content does not match
//! its format word lands in [`TextureRole::Unidentified`] rather than keeping a
//! role it did not earn. Measured over `Spore_Content.package` (2026-10-04):
//!
//! * a `0x15` record is a **tangent-space normal map stored Z first** — 901 of
//!   901 records satisfy the check, and the reordering failure it exists to
//!   catch cannot pass it;
//! * a `DXT5` record is **not a colour map**: over 1 374 decodable records and
//!   720 371 712 base-mip texels the decoded green and blue channels have a
//!   span of **zero** and red takes **two** values, and its two layers differ
//!   byte-for-byte in **1 374 / 1 374** records.
//!
//! **There is no `Diffuse` variant**, and adding one would be a claim about a
//! record that does not exist: every raster record in the corpus is one of the
//! two above or a `DXT1` this build refuses by name. `tests/honesty.rs` fails if
//! a role spelling ever names an unestablished purpose.
//!
//! What a mask plane is **for** remains unknown, and
//! [`claims::MASK_LAYER_PURPOSE`] is a non-finding. Content is not purpose.
//!
//! The full derivation, with every corpus table and the engine specification,
//! is in `docs/MATERIAL-ROLES.md`.
//!
//! # The API, in one place
//!
//! ```text
//! resolve_texture(store, reference, assumed_type) -> Result<ResolvedTexture, MaterialError>
//! resolve_model_textures(store, &model, assumed_type) -> Vec<Result<ResolvedTexture, ..>>
//! resolve_model_record(store, &key, assumed_type)   -> Result<ModelTextures, MaterialError>
//! inspect_envelope(store, reference, assumed_type)   -> Result<ResolvedTexture, ..>
//! model_materials(&model)                            -> Vec<MeshMaterial>
//! partition_texture_refs(&model)                     -> Option<Vec<&[GmdlTextureRef]>>
//! ```
//!
//! `assumed_type` is a parameter of **every** entry point, never a default and
//! never a constant buried in the body: it is the thing the gmdl reference does
//! not carry, and a caller who guesses wrong must be able to see that they did.
//!
//! `inspect_envelope` is the cheap question — "how big is this record?" — asked
//! of an atlas that may hold thousands of them. It reads a 32-byte header and
//! stops, which is why its [`ResolvedTexture::pixels_decoded`] is `false` and
//! its [`ResolvedTexture::role`] is [`TextureRole::Unidentified`]: a header read
//! is not a content check, and "we did not look" is not "we looked and found
//! nothing". A refusal never returns an image at all.
//!
//! # The six things this crate will not do
//!
//! Each of these is enforced by a type or a test, not by a comment:
//!
//! | It will not | Because | Enforced by |
//! | --- | --- | --- |
//! | invent a material name from its id | the registry is external to the data | [`MeshMaterial::name`] is `None`; `material_name` returns `None` for every id |
//! | name a texture's purpose | no shader has been read, and no mask plane's job is established | [`SamplerRole`] has one variant; [`ChannelRole::purpose_established`] is `false` for three of six |
//! | call a texture "diffuse" | no raster record in the corpus decodes as a colour map | no such [`TextureRole`] variant; `tests/honesty.rs` sweeps for the word |
//! | assign a role from a texture's position in the list | position correlates with content in this corpus but does not determine it | [`role::classify`] takes content statistics; [`TextureSlot::ordinal_agrees`] is a cross-check |
//! | cut a texture list per mesh because the division worked | a short list divides just as neatly as a complete one | [`partition_texture_refs`] refuses a cut the record contradicts; [`BindingScope::Undetermined`] is the answer then |
//! | bury the type assumption | the reference carries no type word | `assumed_type` is a parameter of every entry point, and [`claims::ASSUMED_RECORD_TYPE`] records it at `INFERRED` |
//! | return an empty image instead of an error | an absent texel and a black texel must not look alike | [`decode_raster`](spore_texture::decode_raster) refusals arrive as typed [`MaterialError`] variants |
//!
//! # `png` and `rw4` are not rasters, and this is measured
//!
//! Verified on `SPORE/Data/Spore_Content.package` with `osptool` (2026-10-04):
//!
//! * a `png`-typed record (`0x2F7D0004`) begins `89 50 4E 47 0D 0A 1A 0A` and
//!   continues straight into an `IHDR` chunk — **raw PNG**;
//! * an `rw4`-typed record (`0x2F4E681B`) begins `52 57 34 77 33 32 00` —
//!   `"RW4w32"`, a **RenderWare** container;
//! * a `raster`-typed record (`0x2F4E681C`) begins the 32-byte envelope, with
//!   `fourcc` `0x35545844` (`DXT5`) or `0x00000015` for the luminance family.
//!
//! Two comments elsewhere in this workspace state that a `png`-named record is
//! "an RW4 blob, **not** raw PNG" (`spore-core`'s `type_id::RW4` doc comment,
//! and `spore-assets`' manifest `DECODABLE` list). The bytes above contradict
//! both. This crate follows the bytes and treats the type word as identity, which
//! is why a `png` assumption is a typed refusal rather than a decode attempt.
//!
//! # The group-name gap
//!
//! The documented asset `0x00e6bce5:0x40637e03:0x067a07f0` references three
//! textures, all with `instance_id = 0x067a07f0` and groups `0x40632900`,
//! `0x40632901`, `0x40632902`. None of those three group ids is in
//! [`spore_core::record::GROUP_NAMES`], so [`claims::RESOLVED_GROUP_NAME`] is a
//! **non-finding** for all three. This crate does not add them: the canonical
//! table is transcribed from the SDK plus a per-package prop directory, so an
//! entry added without a source would be a fabrication presented as a name.
//!
//! What those ids *do* encode is now measured and recorded without naming
//! them: the low byte states the entry's slot, in 6 684 of 6 684 references
//! ([`claims::GROUP_LOW_BYTE_IS_SLOT`]), and the corpus's references span six
//! `0x40XX2900` families. A cross-check is not a name.
//!
//! # What the documented asset actually does
//!
//! Measured over `SPORE/Data/Spore_Content.package` on 2026-10-04 and asserted
//! by `tests/real_corpus_roles.rs::the_documented_assets_roles`, which prints
//! every field:
//!
//! | Reference | Result | role |
//! | --- | --- | --- |
//! | `0x2f4e681c:0x40632900:0x067a07f0` | 512x512, 10 mips, **2 layers**, `DXT5` | [`TextureRole::LayeredAlphaMask`] |
//! | `0x2f4e681c:0x40632901:0x067a07f0` | 512x512, 10 mips, **2 layers**, `DXT5` | [`TextureRole::LayeredAlphaMask`] |
//! | `0x2f4e681c:0x40632902:0x067a07f0` | **512x512**, 10 mips, 1 layer, `fourcc 0x00000015` | [`TextureRole::TangentSpaceNormal`] |
//!
//! **Two corrections to what this module used to say.** The third record was
//! described here as *1024x1024* and *refused*; it is **512x512** and it
//! **decodes**. `Spore_Content.package` contains no 1024² raster of either
//! format — the `0x15` family occurs at 32², 256² and 512² only — and
//! `spore-texture` established the family's framing over 901 of 901 records.
//! The bytes win; see `docs/MATERIAL-ROLES.md` §8.
//!
//! So [`resolve_model_textures`] on the documented asset now yields **three**
//! images and no error, and [`resolve_model_record`] reports
//! [`ModelTextures::observed_shape`] = `(mask, mask, normal)` with 3 of 3
//! ordinals agreeing. A resolver that aborted on the first failure would still
//! return nothing for an asset with an undecodable texture, which is why the
//! batch returns a `Vec<Result<..>>`.
//!
//! # Testing
//!
//! **No committed raster fixture exists** (the real records live in the
//! git-ignored `SPORE/` tree), so every byte is synthesized: `tests/support/`
//! builds a DBPF v3 image, raster records with a 32-byte envelope and DXT5
//! payloads, `0x15` records from that family's framing identity, and gmdl
//! records whose material section mirrors the documented asset's. Nothing here
//! needs a game installation, a package or a network.
//!
//! Six **corpus censuses** live in `tests/real_corpus_roles.rs`, every one of
//! them `#[ignore]`d:
//!
//! ```text
//! cargo test -p spore-material --release -- --ignored --nocapture --test-threads=1
//! ```
//!
//! They re-measure this crate's tables with the **Rust** readers, so the Python
//! oracle that produced them and the Rust port that ships them have to agree
//! before a number is trusted. `--nocapture` matters: the measurements are the
//! deliverable.
//!
//! # Clean-room
//!
//! Nothing in this crate is derived from EA or Maxis code. The layouts were read
//! out of record bytes, out of the community SDK's type information and out of
//! this repository's own references (`src/assets/MaterialRegistry.cpp`,
//! `docs/MATERIALS-DESIGN.md`, `tools/spore/`).

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

pub mod claims;

mod error;
mod material;
mod resolve;
mod role;

pub use claims::{grade_of, spec_of, ClaimKind, ClaimSpec, ALL_CLAIMS};
pub use error::MaterialError;
pub use material::{
    material_name, model_materials, partition_texture_refs, BindingScope, MaterialClaim,
    MeshMaterial, SamplerRole, TextureSlot,
};
pub use resolve::{
    inspect_envelope, resolve_model_record, resolve_model_textures, resolve_texture, ModelTextures,
    OrdinalAgreement, ResolvedTexture, TexSetShape, TextureClaim, TextureKind, REQUIRED_FOURCC,
};
pub use role::{
    classify, layered_mask_signature, normal_map_signature, role_evidence, ChannelBinding,
    ChannelRole, Lane, MipStatistics, TextureRole, NORMAL_LANE_CENTRE, NORMAL_LANE_TOLERANCE,
    NORMAL_Z_LEAD, NORMAL_Z_MEAN_FLOOR,
};
