//! The material side: what a gmdl material id means, and the texture bindings
//! that can honestly be attached to it.
//!
//! # The three findings this module is built around
//!
//! 1. **A material id has no known meaning.** The gmdl record carries one 32-bit
//!    id per mesh (`0x407DFDDB` on the documented asset) and nothing in the
//!    record says what it names. The C++ reference, `src/assets/MaterialRegistry.cpp`,
//!    keeps a registry *the engine populates from outside* — which is the
//!    strongest available evidence that the mapping is not in the game data at
//!    all. So [`MeshMaterial::name`] is always `None` and the id's meaning is
//!    recorded as `Fact::unavailable("no_material_registry")`: a non-finding, not
//!    a default colour and not a name derived from the bits.
//!
//! 2. **A texture's *content* role is answerable; its *sampler* role is not.**
//!    Those are two different questions and this module keeps them apart. The
//!    content role is derived per record from its format word and a statistic
//!    over its own base mip — see [`crate::role`] — and it is a real answer. The
//!    sampler stage is a property of a shader nobody here has read, so
//!    [`SamplerRole`] still has exactly one variant, [`SamplerRole::Unresolved`].
//!    Note that this crate's reason for the sampler non-finding **changed**: the
//!    16 bytes of a texture-set entry have now been read, and they carry no role
//!    field. See [`claims::SAMPLER_ROLE`].
//!
//! 3. **Per-mesh bindings are an inference, and are labelled one.** The gmdl
//!    walk flattens every material-info block's texture set into one list
//!    without recording which block each reference came from, so the block
//!    boundaries are unrecoverable from [`GmdlModel`]. What this crate offers is
//!    a **partition by division** — see [`partition_texture_refs`] — which is
//!    sound only under a regularity the corpus satisfies
//!    (`2381`/`2381` blocks contributed the same number of references as their
//!    model's) and which is graded [`spore_core::evidence::EvidenceLevel::Inferred`]
//!    regardless. [`claims::MATERIAL_BLOCK_IS_MESH`] records the part that stays
//!    unknown: nothing says block `i` is mesh `i`.
//!
//! # Which textures belong to which mesh
//!
//! [`MeshMaterial::slots`] holds the mesh's own slice of the model's texture
//! set when the division works, and [`MeshMaterial::scope`] says which of the
//! two cases this is:
//!
//! | [`BindingScope`] | When | What it means |
//! |---|---|---|
//! | [`BindingScope::PerMeshBlock`] | `texture_refs.len()` divides by the mesh count | each mesh gets `refs_per_mesh` references, blocks in material-info order |
//! | [`BindingScope::WholeModel`] | it does not | every mesh gets the whole set, because no better answer exists |
//!
//! Neither case is a *resolved* binding, and the type says so rather than only
//! the documentation.

use spore_core::evidence::Fact;
use spore_core::ResourceKey;
use spore_gmdl::{GmdlModel, GmdlTextureRef};

use crate::claims;

/// One graded statement about a material or a texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialClaim {
    /// The stable subject string; see the [`claims`] module for the full list.
    pub subject: String,
    /// The statement, with its grade and provenance — or a non-finding.
    pub fact: Fact<&'static str>,
}

impl MaterialClaim {
    /// Builds a claim from one of the crate's declared subjects.
    pub fn new(subject: &'static str, fact: Fact<&'static str>) -> Self {
        Self {
            subject: subject.to_owned(),
            fact,
        }
    }

    /// Looks a claim up by subject.
    pub fn claim<'a>(claims: &'a [MaterialClaim], subject: &str) -> Option<&'a MaterialClaim> {
        claims.iter().find(|entry| entry.subject == subject)
    }
}

/// Which sampler stage a texture binding feeds.
///
/// The enum has exactly one variant, and the reason is now a **measurement**
/// rather than an omission. A texture-set entry is 16 bytes plus
/// `{instance, group}`; of those 16, the first is the entry's ordinal (equal to
/// its position in 6684 of 6684 references measured over `Spore_Content`) and
/// the other 12 take four distinct values across those same references without
/// varying with the ordinal. **There is no role field in the entry.** Which
/// shader stage the game binds a texture to is stated somewhere this workspace
/// has never read.
///
/// Adding a variant would mean claiming a mapping between a value nobody has
/// decoded and a shader nobody has read. See [`claims::SAMPLER_ROLE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplerRole {
    /// The binding's sampler stage is not known. Always, in this build.
    Unresolved,
}

/// One texture reference at a known position, before anything is decoded.
///
/// Deliberately carries **no role**: a role is a property of the resolved
/// record, and a binding exists before the record has been read. Roles live on
/// [`crate::ResolvedTexture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSlot {
    /// The reference exactly as the gmdl record carried it.
    pub reference: GmdlTextureRef,
    /// The entry's ordinal within its material-info block's texture set.
    ///
    /// `None` when the block boundaries are unrecoverable, i.e. when
    /// [`MeshMaterial::scope`] is [`BindingScope::WholeModel`]. It is `None`
    /// rather than a guess: an ordinal derived from the wrong block is exactly
    /// the kind of confident wrong answer this crate exists to prevent.
    ///
    /// When present, it equals the entry's own first header word in every
    /// reference measured — see [`claims::SAMPLER_WORD_IS_ENTRY_ORDINAL`] — and
    /// it equals the referenced group id's low byte
    /// ([`claims::GROUP_LOW_BYTE_IS_SLOT`]).
    pub ordinal: Option<u32>,
}

impl TextureSlot {
    /// The identity this slot would resolve to under `assumed_type`.
    ///
    /// The type word is a parameter and not part of the slot, because the
    /// record does not carry one; see [`claims::TEXTURE_REFERENCE_TYPE_WORD`].
    pub fn key(&self, assumed_type: u32) -> ResourceKey {
        ResourceKey::new(
            assumed_type,
            self.reference.group_id,
            self.reference.instance_id,
        )
    }

    /// The ordinal the referenced record's group id states, if it has one.
    ///
    /// An **independent** cross-check on [`Self::ordinal`], available even when
    /// the block boundaries are lost: the group id's low byte carries the same
    /// ordinal in every reference measured. Cheap, and it is how a caller can
    /// tell a reordering from a coincidence.
    pub fn group_stated_ordinal(&self) -> u32 {
        self.reference.group_id & 0xFF
    }

    /// Whether the two independent statements of the ordinal agree.
    ///
    /// `true` whenever [`Self::ordinal`] is `None` — a missing statement is not
    /// a contradiction, and reporting it as one would train a caller to ignore
    /// this method.
    pub fn ordinal_agrees(&self) -> bool {
        match self.ordinal {
            Some(ordinal) => ordinal == self.group_stated_ordinal(),
            None => true,
        }
    }
}

/// How much of the model's texture set a [`MeshMaterial`]'s slots cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingScope {
    /// The model's texture set divided evenly by its mesh count, so each mesh
    /// holds `refs_per_mesh` references.
    ///
    /// Available when [`partition_texture_refs`] succeeded. **An inference**: it
    /// rests on material-info block `i` being mesh `i`, which no measurement
    /// establishes — see [`claims::MATERIAL_BLOCK_IS_MESH`]. Read
    /// [`claims::PER_MESH_TEXTURE_PARTITION`] before treating it as a binding.
    PerMeshBlock {
        /// How many references each mesh's slice holds.
        refs_per_mesh: usize,
    },
    /// Every reference in the model, because the gmdl walk does not record
    /// which texture-set block a reference came from and the division did not
    /// come out even.
    ///
    /// The honest floor: it says "these textures belong to this model" and
    /// nothing more.
    WholeModel,
}

impl BindingScope {
    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerMeshBlock { .. } => "per-mesh-block",
            Self::WholeModel => "whole-model",
        }
    }

    /// References held per mesh, or `None` when the scope is the whole model.
    pub const fn refs_per_mesh(self) -> Option<usize> {
        match self {
            Self::PerMeshBlock { refs_per_mesh } => Some(refs_per_mesh),
            Self::WholeModel => None,
        }
    }

    /// Whether the per-mesh cut is an inference rather than a resolved binding.
    ///
    /// `true` for **both** variants, which is the point: a caller that wants a
    /// proven binding gets `false` nowhere, because this build has none.
    pub const fn is_inferred(self) -> bool {
        true
    }
}

/// One mesh's material, as far as this build can honestly describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshMaterial {
    /// The mesh's position in the record's mesh table, and the position of the
    /// material id below.
    pub mesh_index: u32,
    /// The opaque 32-bit id, carried verbatim from the gmdl record. **One per
    /// mesh, in record order** — the mesh table writes them as an array
    /// immediately after the `{index_buffer, vertex_buffer}` pairs, so the
    /// mesh↔material association is a structural observation rather than an
    /// inference. See [`claims::MATERIAL_ID_READ_VERBATIM`].
    pub material_id: u32,
    /// A name for [`Self::material_id`], when this build has one. Always `None`
    /// here: see [`claims::MATERIAL_NAME`] and [`material_name`].
    pub name: Option<&'static str>,
    /// This mesh's texture references. Read [`Self::scope`] before assuming
    /// they are the mesh's alone.
    pub slots: Vec<TextureSlot>,
    /// Every graded statement this crate makes about this material, including
    /// the non-findings.
    pub claims: Vec<MaterialClaim>,
    /// The extent of [`Self::slots`], stated in the type rather than only in a
    /// comment.
    pub scope: BindingScope,
}

impl MeshMaterial {
    /// Looks a claim up by subject.
    pub fn claim(&self, subject: &str) -> Option<&MaterialClaim> {
        MaterialClaim::claim(&self.claims, subject)
    }

    /// The claims that are non-findings, i.e. the things this build does not
    /// know.
    ///
    /// A renderer that wants a "what is missing here" summary reads this rather
    /// than scanning for absence itself.
    pub fn unknowns(&self) -> impl Iterator<Item = &MaterialClaim> {
        self.claims
            .iter()
            .filter(|claim| claim.fact.is_unavailable())
    }
}

/// The name of a material id, which this build does not have.
///
/// Returns `None` for **every** id, including `0x407DFDDB`. This function exists
/// so that the "no registry" fact is a single greppable statement instead of an
/// assumption spread through the code, and so that the field
/// [`MeshMaterial::name`] has exactly one place it is filled from.
pub fn material_name(_material_id: u32) -> Option<&'static str> {
    // A registry would be a lookup keyed on the id. The reference implementation
    // has one and the engine populates it from outside, which means the mapping
    // is not carried by the records this crate can read. Deriving a name from
    // the bits would be a fabrication presented as a fact.
    None
}

/// The observation text for a material id that was read and not interpreted.
const MATERIAL_ID_OBSERVED_TEXT: &str =
    "The id is read verbatim from the gmdl mesh table, one word per mesh and in mesh-table \
     order, and nothing in the record or in any record this build can read interprets it.";

/// The observation text for a model that referenced no textures at all.
const NO_BINDINGS_TEXT: &str = "this model referenced no textures";

/// The observation text for a mesh whose slice came out of an even division.
const PER_MESH_BINDINGS_TEXT: &str =
    "this mesh's slice of the model's texture set, obtained by dividing the flat reference \
     list by the mesh count: INFERRED, because nothing establishes that material-info block \
     i is mesh i";

/// The observation text for a mesh that holds the whole set.
const WHOLE_MODEL_BINDINGS_TEXT: &str =
    "this model's whole texture set, once per binding; the gmdl walk does not record which \
     texture-set block a reference came from";

/// The observed layout of one gmdl texture-set entry.
const TEXTURE_SET_ENTRY_LAYOUT_TEXT: &str =
    "A 0x20D entry is 16 bytes followed by the referenced record's instance and group ids. \
     The first of the 16 bytes is the entry's ordinal within its texture set; the other 12 \
     take four distinct values across 6684 measured references and do not vary with it. \
     No lane, sampler or flag is read from them.";

/// Builds the claim set every mesh's material carries.
///
/// `slot_count` is the number of slots this mesh holds, which is the model's
/// whole texture-set length under [`BindingScope::WholeModel`] and one block's
/// share under [`BindingScope::PerMeshBlock`].
fn material_claims(slot_count: usize, scope: BindingScope) -> Vec<MaterialClaim> {
    let mut facts = vec![
        MaterialClaim::new(
            claims::MATERIAL_NAME,
            claims::non_finding(claims::MATERIAL_NAME),
        ),
        MaterialClaim::new(
            claims::MATERIAL_ID_MEANING,
            claims::non_finding(claims::MATERIAL_ID_MEANING),
        ),
        MaterialClaim::new(
            claims::SAMPLER_ROLE,
            claims::non_finding(claims::SAMPLER_ROLE),
        ),
        MaterialClaim::new(
            claims::MATERIAL_TEXTURE_OWNERSHIP,
            claims::non_finding(claims::MATERIAL_TEXTURE_OWNERSHIP),
        ),
        MaterialClaim::new(
            claims::MATERIAL_BLOCK_IS_MESH,
            claims::non_finding(claims::MATERIAL_BLOCK_IS_MESH),
        ),
        MaterialClaim::new(
            claims::TEXTURE_SET_ENTRY_LAYOUT,
            claims::graded(
                claims::TEXTURE_SET_ENTRY_LAYOUT,
                claims::PROV_GMDL_WALK,
                TEXTURE_SET_ENTRY_LAYOUT_TEXT,
            ),
        ),
        MaterialClaim::new(
            claims::MATERIAL_ID_READ_VERBATIM,
            claims::graded(
                claims::MATERIAL_ID_READ_VERBATIM,
                claims::PROV_GMDL_WALK,
                MATERIAL_ID_OBSERVED_TEXT,
            ),
        ),
    ];
    match scope {
        BindingScope::PerMeshBlock { .. } => facts.push(MaterialClaim::new(
            claims::PER_MESH_TEXTURE_PARTITION,
            claims::graded(
                claims::PER_MESH_TEXTURE_PARTITION,
                claims::PROV_MATERIAL_ROLES,
                PER_MESH_BINDINGS_TEXT,
            ),
        )),
        BindingScope::WholeModel => facts.push(MaterialClaim::new(
            claims::PER_MESH_TEXTURE_PARTITION,
            claims::non_finding(claims::PER_MESH_TEXTURE_PARTITION),
        )),
    }
    facts.push(MaterialClaim::new(
        claims::MATERIAL_BINDING_COUNT,
        claims::graded(
            claims::MATERIAL_BINDING_COUNT,
            claims::PROV_RESOLVE,
            if slot_count == 0 {
                NO_BINDINGS_TEXT
            } else {
                WHOLE_MODEL_BINDINGS_TEXT
            },
        ),
    ));
    facts
}

/// Cuts a model's flattened texture list into one slice per mesh.
///
/// Returns `None` when the cut does not come out even, which is the signal to
/// fall back to [`BindingScope::WholeModel`]. **Never** rounds, and never
/// returns an empty slice for a model that referenced textures: a partial cut
/// would put one mesh's textures on another mesh.
///
/// # Why division, and what it assumes
///
/// The gmdl walk reads `material_info_count` blocks and pushes every block's
/// texture references into one list. Two measured facts make the division
/// sound on this corpus:
///
/// * every block of a model contributed the same number of references
///   (`2381`/`2381` blocks), so the total is a multiple of the block count;
/// * the block count equals the mesh count (`1819`/`1819` records the
///   repository's Python oracle walks), so dividing by the mesh count is
///   dividing by the block count.
///
/// The remaining assumption — that block `i` is mesh `i` — is **not** measured,
/// and is why [`BindingScope::is_inferred`] is `true` for both variants and
/// [`claims::MATERIAL_BLOCK_IS_MESH`] is a non-finding.
pub fn partition_texture_refs(model: &GmdlModel) -> Option<Vec<&[GmdlTextureRef]>> {
    let meshes = model.meshes.len();
    let total = model.texture_refs.len();
    if meshes == 0 {
        // A model with no meshes has no blocks. That is an absence, and it is
        // only consistent when it referenced no textures either.
        return if total == 0 { Some(Vec::new()) } else { None };
    }
    if total == 0 {
        // Every block contributed zero references, so every mesh's slice is
        // empty. Consistent, and `Some` rather than `None`: "no textures" is an
        // answer, not a failure to answer.
        return Some(vec![&[]; meshes]);
    }
    if total % meshes != 0 {
        return None;
    }
    let per_mesh = total / meshes;
    Some(model.texture_refs.chunks_exact(per_mesh).collect())
}

/// The scope [`partition_texture_refs`] would produce, without doing it.
fn scope_of(model: &GmdlModel) -> BindingScope {
    let meshes = model.meshes.len();
    let total = model.texture_refs.len();
    if meshes == 0 {
        return BindingScope::WholeModel;
    }
    if total % meshes == 0 {
        BindingScope::PerMeshBlock {
            refs_per_mesh: total / meshes,
        }
    } else {
        BindingScope::WholeModel
    }
}

/// One [`MeshMaterial`] per mesh in `model`, in record order.
///
/// A model that declared no meshes yields an empty vector. That is an absence
/// of materials in the record, not an error — it is the shape of the real
/// record captured in `tests/expected/real_gmdl_1006.json` (`meshCount` 0).
pub fn model_materials(model: &GmdlModel) -> Vec<MeshMaterial> {
    let scope = scope_of(model);
    let per_mesh: Option<Vec<&[GmdlTextureRef]>> = match scope {
        BindingScope::PerMeshBlock { .. } => partition_texture_refs(model),
        BindingScope::WholeModel => None,
    };

    model
        .material_ids
        .iter()
        .enumerate()
        .map(|(mesh_index, material_id)| {
            // The slice for this mesh when the cut worked, the whole list when
            // it did not. Both are stated by `scope`; neither is a resolved
            // binding.
            let refs: &[GmdlTextureRef] = match &per_mesh {
                Some(slices) => slices.get(mesh_index).copied().unwrap_or(&[]),
                None => &model.texture_refs,
            };
            let slots: Vec<TextureSlot> = refs
                .iter()
                .enumerate()
                .map(|(ordinal, reference)| TextureSlot {
                    reference: *reference,
                    ordinal: match scope {
                        BindingScope::PerMeshBlock { .. } => Some(ordinal as u32),
                        BindingScope::WholeModel => None,
                    },
                })
                .collect();
            MeshMaterial {
                mesh_index: mesh_index as u32,
                material_id: *material_id,
                name: material_name(*material_id),
                claims: material_claims(slots.len(), scope),
                slots,
                scope,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_core::evidence::{EvidenceLevel, EvidenceState};

    /// The documented asset's material id, measured over
    /// `SPORE/Data/Spore_Content.package` (2026-10-04).
    const REAL_MATERIAL_ID: u32 = 0x407D_FDDB;

    fn reference(group_id: u32) -> GmdlTextureRef {
        GmdlTextureRef {
            instance_id: 0x067A_07F0,
            group_id,
        }
    }

    #[test]
    fn no_material_id_ever_gets_a_name() {
        for id in [REAL_MATERIAL_ID, 0, 1, u32::MAX, 0x1234_5678] {
            assert_eq!(material_name(id), None, "id 0x{id:08x} must stay unnamed");
        }
    }

    #[test]
    fn sampler_role_has_exactly_one_variant() {
        // Not a style preference: a second variant would be a claim with no
        // evidence behind it. The reason code changed when the entry header was
        // read; the variant count must not.
        let role = SamplerRole::Unresolved;
        assert_eq!(role, SamplerRole::Unresolved);
        assert_eq!(
            claims::grade_of(claims::SAMPLER_ROLE),
            EvidenceLevel::Unknown
        );
    }

    #[test]
    fn a_slot_key_is_the_reference_plus_the_caller_assumed_type() {
        let slot = TextureSlot {
            reference: reference(0x4063_2900),
            ordinal: Some(0),
        };
        assert_eq!(
            slot.key(spore_texture::RASTER_TYPE),
            ResourceKey::new(spore_texture::RASTER_TYPE, 0x4063_2900, 0x067A_07F0)
        );
    }

    #[test]
    fn the_two_statements_of_an_ordinal_are_cross_checkable() {
        // Measured over 6684 references: the entry's own first header word and
        // the referenced group id's low byte state the same ordinal. Two
        // independent statements, so a disagreement is visible.
        for (group, ordinal) in [
            (0x4063_2900u32, 0u32),
            (0x4063_2901, 1),
            (0x4063_2902, 2),
            (0x4066_2900, 0),
            (0x4064_2902, 2),
        ] {
            let slot = TextureSlot {
                reference: reference(group),
                ordinal: Some(ordinal),
            };
            assert_eq!(slot.group_stated_ordinal(), ordinal, "0x{group:08x}");
            assert!(slot.ordinal_agrees(), "0x{group:08x}");
        }
    }

    #[test]
    fn a_disagreeing_ordinal_is_reported_rather_than_accepted() {
        let slot = TextureSlot {
            reference: reference(0x4063_2900), // states ordinal 0
            ordinal: Some(2),
        };
        assert!(!slot.ordinal_agrees());
    }

    #[test]
    fn a_missing_ordinal_is_not_a_contradiction() {
        // `WholeModel` has no ordinal to check. Reporting that as disagreement
        // would teach a caller to ignore `ordinal_agrees` entirely.
        let slot = TextureSlot {
            reference: reference(0x4063_2901),
            ordinal: None,
        };
        assert!(slot.ordinal_agrees());
    }

    #[test]
    fn an_even_division_yields_one_slice_per_mesh() {
        // Two meshes, six references: the corpus's (2 blocks, 6 refs) shape.
        let model = synthetic_model(2, 6);
        let slices = partition_texture_refs(&model).expect("6 divides by 2");
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].len(), 3);
        assert_eq!(slices[1].len(), 3);
        assert_ne!(slices[0][0].group_id, slices[1][0].group_id);
    }

    #[test]
    fn an_odd_division_is_refused_rather_than_rounded() {
        // Three meshes, two references: no even cut exists. Rounding would put
        // mesh 0's textures on mesh 1.
        let model = synthetic_model(3, 2);
        assert_eq!(partition_texture_refs(&model), None);
        let materials = model_materials(&model);
        assert_eq!(materials.len(), 3);
        assert!(materials
            .iter()
            .all(|m| m.scope == BindingScope::WholeModel));
        assert!(materials.iter().all(|m| m.slots.len() == 2));
        assert!(materials
            .iter()
            .all(|m| m.slots.iter().all(|s| s.ordinal.is_none())));
    }

    #[test]
    fn a_model_with_no_meshes_and_no_textures_partitions_to_nothing() {
        let mut model = synthetic_model(1, 0);
        model.meshes.clear();
        model.material_ids.clear();
        let slices = partition_texture_refs(&model).expect("consistent absence");
        assert!(slices.is_empty());
        assert!(model_materials(&model).is_empty());
    }

    #[test]
    fn a_model_with_no_meshes_but_textures_is_inconsistent_and_says_so() {
        let mut model = synthetic_model(1, 0);
        model.meshes.clear();
        model.material_ids.clear();
        model.texture_refs = vec![reference(0x4063_2900)];
        assert_eq!(partition_texture_refs(&model), None);
    }

    #[test]
    fn meshes_with_no_textures_each_get_an_empty_slice() {
        // "No textures" is an answer, not a failure to answer: this must not
        // fall back to `WholeModel`, which would be indistinguishable from the
        // division having failed.
        let model = synthetic_model(3, 0);
        let slices = partition_texture_refs(&model).expect("zero divides");
        assert_eq!(slices.len(), 3);
        assert!(slices.iter().all(|slice| slice.is_empty()));
        let materials = model_materials(&model);
        assert_eq!(materials.len(), 3);
        assert!(materials
            .iter()
            .all(|m| m.scope == BindingScope::PerMeshBlock { refs_per_mesh: 0 }));
    }

    #[test]
    fn every_mesh_gets_a_material_id_in_record_order() {
        let model = synthetic_model(2, 6);
        let materials = model_materials(&model);
        assert_eq!(materials.len(), 2);
        assert_eq!(materials[0].mesh_index, 0);
        assert_eq!(materials[1].mesh_index, 1);
        assert_eq!(materials[0].material_id, model.material_ids[0]);
        assert_eq!(materials[1].material_id, model.material_ids[1]);
        assert!(materials.iter().all(|m| m.name.is_none()));
    }

    #[test]
    fn an_even_cut_labels_itself_inferred_and_records_the_non_finding() {
        let materials = model_materials(&synthetic_model(2, 6));
        for material in &materials {
            assert_eq!(
                material.scope,
                BindingScope::PerMeshBlock { refs_per_mesh: 3 }
            );
            assert!(material.scope.is_inferred());
            let partition = material.claim(claims::PER_MESH_TEXTURE_PARTITION).unwrap();
            assert_eq!(partition.fact.level(), EvidenceLevel::Inferred);
            assert!(partition
                .fact
                .value()
                .unwrap()
                .contains("INFERRED, because nothing establishes"));
            // And the ordering claim stays a non-finding even on the happy path.
            let ordering = material.claim(claims::MATERIAL_BLOCK_IS_MESH).unwrap();
            assert!(ordering.fact.is_unavailable());
            assert_eq!(
                ordering.fact.reason(),
                Some(claims::BLOCK_ORDER_NOT_ESTABLISHED)
            );
        }
    }

    #[test]
    fn a_failed_cut_records_the_partition_as_a_non_finding() {
        let materials = model_materials(&synthetic_model(3, 2));
        for material in &materials {
            let partition = material.claim(claims::PER_MESH_TEXTURE_PARTITION).unwrap();
            assert!(
                partition.fact.is_unavailable(),
                "a cut that was not made must not carry a graded value"
            );
            assert!(material.scope.is_inferred());
        }
    }

    #[test]
    fn the_material_claims_are_five_non_findings_and_graded_observations() {
        let claims = material_claims(3, BindingScope::WholeModel);
        for subject in [
            claims::MATERIAL_NAME,
            claims::MATERIAL_ID_MEANING,
            claims::SAMPLER_ROLE,
            claims::MATERIAL_TEXTURE_OWNERSHIP,
            claims::MATERIAL_BLOCK_IS_MESH,
        ] {
            let claim = MaterialClaim::claim(&claims, subject).expect(subject);
            assert!(claim.fact.is_unavailable(), "{subject}");
            assert_eq!(claim.fact.level(), EvidenceLevel::Unknown, "{subject}");
            assert_eq!(
                claim.fact.evidence_state(),
                EvidenceState::Missing,
                "{subject}"
            );
            assert!(claim.fact.provenance().is_empty(), "{subject}");
            assert!(claim.fact.value().is_none(), "{subject}");
        }
        for subject in [
            claims::TEXTURE_SET_ENTRY_LAYOUT,
            claims::MATERIAL_ID_READ_VERBATIM,
            claims::MATERIAL_BINDING_COUNT,
        ] {
            let claim = MaterialClaim::claim(&claims, subject).expect(subject);
            assert!(claim.fact.is_available(), "{subject}");
            assert_ne!(claim.fact.level(), EvidenceLevel::Unknown, "{subject}");
            assert!(!claim.fact.provenance().is_empty(), "{subject}");
        }
    }

    #[test]
    fn the_entry_layout_text_no_longer_claims_the_header_was_skipped() {
        // The old text said "not decoded". The header has been read since; a
        // comment asserting otherwise would be a stale claim of the exact kind
        // this repository treats as a defect.
        assert!(TEXTURE_SET_ENTRY_LAYOUT_TEXT.contains("ordinal"));
        assert!(!TEXTURE_SET_ENTRY_LAYOUT_TEXT.contains("skipped"));
    }

    #[test]
    fn binding_count_is_stated_not_silently_zero() {
        let empty = material_claims(0, BindingScope::PerMeshBlock { refs_per_mesh: 0 });
        let claim = MaterialClaim::claim(&empty, claims::MATERIAL_BINDING_COUNT).unwrap();
        assert_eq!(
            claim.fact.value(),
            Some(&NO_BINDINGS_TEXT),
            "no bindings is a statement, not an absent one"
        );
        let some = material_claims(3, BindingScope::WholeModel);
        let claim = MaterialClaim::claim(&some, claims::MATERIAL_BINDING_COUNT).unwrap();
        assert_eq!(claim.fact.value(), Some(&WHOLE_MODEL_BINDINGS_TEXT));
    }

    #[test]
    fn binding_scope_spells_itself_and_reports_its_own_inference() {
        assert_eq!(BindingScope::WholeModel.as_str(), "whole-model");
        assert_eq!(
            BindingScope::PerMeshBlock { refs_per_mesh: 3 }.as_str(),
            "per-mesh-block"
        );
        assert_eq!(BindingScope::WholeModel.refs_per_mesh(), None);
        assert_eq!(
            BindingScope::PerMeshBlock { refs_per_mesh: 3 }.refs_per_mesh(),
            Some(3)
        );
        assert!(BindingScope::WholeModel.is_inferred());
    }

    /// A minimal decoded model with `meshes` meshes and `refs` texture
    /// references, written by hand rather than by a builder that could share a
    /// mistake with the reader.
    fn synthetic_model(meshes: usize, refs: usize) -> GmdlModel {
        GmdlModel {
            version: spore_gmdl::SUPPORTED_VERSION,
            referenced_files: Vec::new(),
            mesh_count: meshes as u32,
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
            radius: 0.0,
            index_buffers: Vec::new(),
            descriptors: Vec::new(),
            vertex_buffers: Vec::new(),
            meshes: (0..meshes)
                .map(|_| spore_gmdl::GmdlMeshRef {
                    index_buffer: 0,
                    vertex_buffer: 0,
                })
                .collect(),
            material_ids: (0..meshes).map(|_| REAL_MATERIAL_ID).collect(),
            texture_refs: (0..refs)
                .map(|index| reference(0x4063_2900 + index as u32))
                .collect(),
            bone_ranges: Vec::new(),
            unknown_key: [0; 3],
            trailer: spore_gmdl::TrailerWalk::Complete,
            strict_consumed: 0,
            consumed: 0,
        }
    }
}
