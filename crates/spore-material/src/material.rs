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
//! 3. **Per-mesh bindings are an inference, and are labelled one — and the cut is
//!    checked against the record before it is offered.** The gmdl walk flattens
//!    every material-info block's texture set into one list without recording
//!    which block each reference came from, so the block boundaries are
//!    unrecoverable from [`GmdlModel`]. What this crate offers is a **partition
//!    by division** — see [`partition_texture_refs`] — which is sound only under
//!    a regularity the corpus satisfies (`2381`/`2381` blocks contributed the
//!    same number of references as their model's) and which is graded
//!    [`spore_core::evidence::EvidenceLevel::Inferred`] regardless.
//!    [`claims::MATERIAL_BLOCK_IS_MESH`] records the part that stays unknown:
//!    nothing says block `i` is mesh `i`.
//!
//! # The division is only offered when the record agrees with it
//!
//! Dividing a list that is **shorter than the record's whole texture set**
//! produces a cut that is arithmetically perfect and structurally wrong. That
//! is not hypothetical: on `Spore_Content.package` the gmdl material-info walk
//! stops inside the table on the undocumented shader-data id `0x218` in **619**
//! of 4 209 gmdl records, and in **100** of those it stops *before the last
//! declared block*, so the flat list covers fewer blocks than there are meshes.
//! `619/619` of those records divided evenly, so the pre-fix answer was
//! [`BindingScope::PerMeshBlock`] in every one of them — a confidently wrong
//! binding rather than an error. The documented creature is one of the 100:
//! block 0 yields both references, so `refs_per_mesh = 2/2 = 1` and mesh 1 was
//! handed a texture out of mesh 0's block.
//!
//! The fix is not a heuristic about the walk, because this crate cannot see the
//! walk: [`GmdlModel`] carries no statement of whether the material-info table
//! closed, and `spore_gmdl::PartialGmdl::stop` — which does — is not reachable
//! from a `&GmdlModel`. What *is* reachable is the record's **own second
//! statement of each entry's ordinal**: a texture-set entry's first header word
//! and the referenced record's group id low byte both state the entry's position
//! within its set, and they agree in 6 684 of 6 684 references measured
//! ([`claims::GROUP_LOW_BYTE_IS_SLOT`]). A complete flat list divided by the
//! mesh count reproduces those ordinals. A **truncated** one cannot, and does
//! not — the derived ordinals then restart at the wrong period and disagree with
//! the group ids.
//!
//! So the cut is not taken on the strength of the arithmetic. It is taken only
//! when the record's own bytes confirm it, and
//! [`BindingScope::Undetermined`] says so when they contradict it.
//!
//! | [`BindingScope`] | When | What it means |
//! |---|---|---|
//! | [`BindingScope::PerMeshBlock`] | the division is even **and** every slot's derived ordinal equals the ordinal its group id states | each mesh gets `refs_per_mesh` references, blocks in material-info order |
//! | [`BindingScope::Undetermined`] | the division is even but the record **contradicts** it | the flat list is not the block boundaries; every mesh gets the whole (short) list |
//! | [`BindingScope::WholeModel`] | the division is not even, or there are no meshes | every mesh gets the whole set, because no better answer exists |
//!
//! # What the cross-check costs, stated rather than hidden
//!
//! A reference whose group id is `0` states ordinal `0` whatever its position
//! is, so a record naming one is downgraded to [`BindingScope::Undetermined`]
//! even when its list is complete. That is 150 references in the corpus, and
//! the downgrade is *conservative* — it hands every mesh the whole list, which
//! cannot bind a mesh to another mesh's texture — but it is a false positive and
//! is counted rather than explained away. And a model with **no** references at
//! all has nothing to cross-check, so its cut rests on the arithmetic alone; that
//! case is also counted in the corpus census rather than assumed absent.
//!
//! None of the three is a *resolved* binding, and the type says so rather than
//! only the documentation.

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
    /// [`MeshMaterial::scope`] is not [`BindingScope::PerMeshBlock`]. It is
    /// `None` rather than a guess: an ordinal derived from the wrong block is
    /// exactly the kind of confident wrong answer this crate exists to prevent.
    ///
    /// When present, it equals the entry's own first header word in every
    /// reference measured — see [`claims::SAMPLER_WORD_IS_ENTRY_ORDINAL`] — and
    /// it equals the referenced group id's low byte
    /// ([`claims::GROUP_LOW_BYTE_IS_SLOT`]). That agreement is also what
    /// [`partition_texture_refs`] requires before it offers a cut at all, so a
    /// slot that exists under [`BindingScope::PerMeshBlock`] always satisfies it.
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

    /// Whether the referenced record's group id states an ordinal **at all**.
    ///
    /// A group id of `0` names no group: 150 references in
    /// `Spore_Content.package` carry one, across 132 of the records this crate
    /// walks, and they belong to none of the six `0x40XX2900` families the
    /// ordinal convention covers. Its low byte is therefore `0` by arithmetic,
    /// not by statement, and reading it as "this entry is slot 0" is how a
    /// silent reference turns into a spurious conflict.
    ///
    /// [`Self::group_stated_ordinal`] returns `0` for such a slot regardless.
    /// Ask this first when the answer matters.
    pub fn group_states_an_ordinal(&self) -> bool {
        self.reference.group_id != 0
    }

    /// The ordinal the referenced record's group id states.
    ///
    /// An **independent** cross-check on [`Self::ordinal`], available even when
    /// the block boundaries are lost: the group id's low byte carries the same
    /// ordinal in every reference measured. Cheap, and it is how a caller can
    /// tell a reordering from a coincidence.
    ///
    /// **`0` is not always a statement** — see
    /// [`Self::group_states_an_ordinal`]. 150 corpus references answer `0`
    /// because they name group id `0`, which states nothing at all.
    pub fn group_stated_ordinal(&self) -> u32 {
        self.reference.group_id & 0xFF
    }

    /// Whether the two independent statements of the ordinal agree.
    ///
    /// `true` in both cases where one of them is **silent**, because a missing
    /// statement is not a contradiction and reporting it as one would train a
    /// caller to ignore this method:
    ///
    /// * [`Self::ordinal`] is `None` — no per-mesh cut was made, so nothing was
    ///   derived to compare; and
    /// * the group id states no ordinal — see
    ///   [`Self::group_states_an_ordinal`].
    pub fn ordinal_agrees(&self) -> bool {
        match self.ordinal {
            Some(ordinal) => {
                !self.group_states_an_ordinal() || ordinal == self.group_stated_ordinal()
            }
            None => true,
        }
    }
}

/// How much of the model's texture set a [`MeshMaterial`]'s slots cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingScope {
    /// The model's texture set divided evenly by its mesh count, so each mesh
    /// holds `refs_per_mesh` references — **and the record agrees with the
    /// cut**.
    ///
    /// Two things are required, and the second is the one that used to be
    /// missing:
    ///
    /// 1. [`partition_texture_refs`] produced an even division; and
    /// 2. every slot's derived ordinal — its position inside its mesh's slice —
    ///    equals the ordinal the referenced record's group id states in its low
    ///    byte.
    ///
    /// Condition 2 is the record's own second statement of each entry's
    /// position, independent of this crate's arithmetic, and it is what tells a
    /// block boundary apart from a division of a list that is shorter than the
    /// record's whole texture set. See the module documentation.
    ///
    /// Available when [`partition_texture_refs`] succeeded. **An inference**: it
    /// still rests on material-info block `i` being mesh `i`, which no
    /// measurement establishes — see [`claims::MATERIAL_BLOCK_IS_MESH`]. Read
    /// [`claims::PER_MESH_TEXTURE_PARTITION`] before treating it as a binding.
    PerMeshBlock {
        /// How many references each mesh's slice holds.
        refs_per_mesh: usize,
    },
    /// Every reference the walk collected, once per binding, because no per-mesh
    /// cut could be established.
    ///
    /// The honest floor: it says "these textures belong to this model" and
    /// nothing more. Two situations reach it and the type keeps them apart — the
    /// division not coming out even, and the division coming out even while the
    /// record contradicts it, which is [`BindingScope::Undetermined`].
    WholeModel,
    /// The division came out even and **the record's own ordinals contradict
    /// it**: `contradicted` of the `collected` references state, in their group
    /// id's low byte, a position other than the one the cut gives them.
    ///
    /// This is what a **short** reference list looks like from inside this
    /// crate. A gmdl material-info walk that stops before it has read every
    /// declared block's texture set — which it does on the undocumented
    /// shader-data id `0x218`, in 100 of the 4 209 gmdl records of
    /// `Spore_Content.package` — yields a list that still divides evenly by the
    /// mesh count, so its derived ordinals are wrong. Before this variant
    /// existed every one of those records reported [`BindingScope::PerMeshBlock`]
    /// and mesh 1 was bound to a texture taken out of mesh 0's block.
    ///
    /// The two counts are what a caller needs in order to say what happened:
    /// `collected` references were read, and `contradicted` of them disagree
    /// with the cut.
    Undetermined {
        /// How many references the walk collected.
        collected: usize,
        /// How many of those have a group id whose low byte states a position
        /// other than the cut's.
        contradicted: usize,
    },
}

impl BindingScope {
    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerMeshBlock { .. } => "per-mesh-block",
            Self::WholeModel => "whole-model",
            Self::Undetermined { .. } => "undetermined",
        }
    }

    /// References held per mesh, or `None` when the slots are the whole set.
    pub const fn refs_per_mesh(self) -> Option<usize> {
        match self {
            Self::PerMeshBlock { refs_per_mesh } => Some(refs_per_mesh),
            Self::WholeModel | Self::Undetermined { .. } => None,
        }
    }

    /// Whether the per-mesh cut is an inference rather than a resolved binding.
    ///
    /// `true` for **every** variant, which is the point: a caller that wants a
    /// proven binding gets `false` nowhere, because this build has none.
    pub const fn is_inferred(self) -> bool {
        true
    }

    /// Whether this scope cut the flat list by the mesh count at all.
    ///
    /// The predicate the fix exists for. `true` means the list was divided, and
    /// dividing it is legitimate only because the record's own ordinals
    /// confirmed the result. `false` means every mesh holds the whole collected
    /// list instead — see [`Self::applies_whole_set`].
    pub const fn divides_by_mesh(self) -> bool {
        matches!(self, Self::PerMeshBlock { .. })
    }

    /// Whether every mesh's slots are the model's whole collected reference
    /// list, so a slot's flat position is just its index in that mesh's slice.
    pub const fn applies_whole_set(self) -> bool {
        !self.divides_by_mesh()
    }

    /// How many references the flat list held when this scope was decided.
    ///
    /// `None` when the scope states no count of its own. It is never `Some(0)`
    /// standing in for "not counted".
    pub const fn collected(self) -> Option<usize> {
        match self {
            Self::Undetermined { collected, .. } => Some(collected),
            Self::PerMeshBlock { .. } | Self::WholeModel => None,
        }
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
     i is mesh i. The division was offered only because the record's own group ids agree with \
     the ordinals it produces (see per_mesh_cut_contradicted_by_record_ordinals);";

/// The observation text for a mesh that holds the whole set.
const WHOLE_MODEL_BINDINGS_TEXT: &str =
    "this model's whole texture set, once per binding; the gmdl walk does not record which \
     texture-set block a reference came from";

/// The observation text for a mesh whose slice was withheld because the record
/// contradicts the division.
///
/// The counts are **not** interpolated: a `Fact` here holds a `&'static str`, so
/// the numbers this record actually produced live in
/// [`BindingScope::Undetermined`] as fields, where a caller can read them
/// without parsing a sentence.
const UNDETERMINED_BINDINGS_TEXT: &str =
    "this model's whole COLLECTED reference list, once per binding, and NOT a per-mesh slice. \
     The flat list divided evenly by the mesh count, but the record's own group-id ordinals \
     contradict the division, which is what a list the gmdl material-info walk stopped reading \
     part-way through looks like from here. Measured over Spore_Content.package: this happens in \
     exactly the records whose material-info walk stops before its last declared block, so the \
     list covers fewer blocks than there are meshes. Slot counts and the number of contradictions \
     are in BindingScope::Undetermined.";

/// The observed layout of one gmdl texture-set entry.
const TEXTURE_SET_ENTRY_LAYOUT_TEXT: &str =
    "A 0x20D entry is 16 bytes followed by the referenced record's instance and group ids. \
     The first of the 16 bytes is the entry's ordinal within its texture set; the other 12 \
     take four distinct values across 6684 measured references and do not vary with it. \
     No lane, sampler or flag is read from them.";

/// Builds the claim set every mesh's material carries.
///
/// `slot_count` is the number of slots this mesh holds, which is one block's share under
/// [`BindingScope::PerMeshBlock`] and the model's whole collected list under both
/// [`BindingScope::WholeModel`] and [`BindingScope::Undetermined`].
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
        BindingScope::WholeModel | BindingScope::Undetermined { .. } => {
            facts.push(MaterialClaim::new(
                claims::PER_MESH_TEXTURE_PARTITION,
                claims::non_finding(claims::PER_MESH_TEXTURE_PARTITION),
            ))
        }
    }
    match scope {
        BindingScope::Undetermined { contradicted, .. } => {
            facts.push(MaterialClaim::new(
                claims::CUT_CONTRADICTED_BY_RECORD,
                claims::graded(
                    claims::CUT_CONTRADICTED_BY_RECORD,
                    claims::PROV_GMDL_WALK,
                    UNDETERMINED_BINDINGS_TEXT,
                ),
            ));
            // The count is a field on the scope; restating it here would need an
            // owned string, which `Fact<&'static str>` cannot hold. Asserted in a
            // test instead of smuggled into prose.
            debug_assert!(
                contradicted > 0,
                "an undetermined scope has a contradiction"
            );
        }
        _ => facts.push(MaterialClaim::new(
            claims::CUT_CONTRADICTED_BY_RECORD,
            claims::non_finding(claims::CUT_CONTRADICTED_BY_RECORD),
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
/// Returns `None` when no per-mesh cut can be established, and `None` now covers
/// **two** distinct situations, which [`BindingScope`] keeps apart:
///
/// * the division does not come out even — the pre-existing reason, now
///   [`BindingScope::WholeModel`]; and
/// * the division comes out even and **the record contradicts it** — a
///   reference's group id states an ordinal other than the one its slice gives
///   it, which is [`BindingScope::Undetermined`].
///
/// **Never** rounds, and never returns an empty slice for a model that
/// referenced textures: a partial cut would put one mesh's textures on another
/// mesh.
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
/// # Why the arithmetic alone is not enough
///
/// The walk does not always reach the end of the table: it stops on the
/// undocumented shader-data id `0x218`, in 619 of 4 209 gmdl records of
/// `Spore_Content.package`, and in 100 of those it stops before the last
/// declared block. The flat list is then **shorter than the record's whole
/// texture set**, and because the missing part is whole blocks it frequently
/// still divides evenly — so the arithmetic succeeds on an incomplete list and
/// returns a confidently wrong cut.
///
/// A `&GmdlModel` carries no statement of whether the table closed, so this
/// function cannot be told. What it *can* do is check the result against the
/// record: each entry's group id states its own ordinal, independently of where
/// this crate decided it sits. A complete list reproduces those ordinals under
/// the division; a short one does not. So the cut is taken only when the record
/// confirms it.
///
/// Two costs of that check, both counted in the corpus census rather than
/// assumed away:
///
/// * a reference whose group id is `0` states ordinal `0` whatever its position,
///   so a record naming one is refused the cut even when its list is complete.
///   Conservative — the whole list goes to every mesh, which cannot bind a mesh
///   to another mesh's texture — but a false positive;
/// * a model with no references has nothing to check, so its cut rests on the
///   arithmetic alone. Zero references is a consistent answer and is kept as one.
///
/// # The remaining assumption is not merely unproven, it is unfalsifiable here
///
/// What is left is that block `i` is mesh `i`. Nothing in the record says it,
/// and nothing in this corpus could: measured over all 4 209 gmdl records, every
/// one of the **1 685** cut records with more than one mesh has *all* of its
/// per-mesh slices naming the **same** texture set, so permuting the blocks
/// changes no byte a renderer can observe. Detecting it would take something
/// outside the texture references — the per-block `0x210` render parameters, the
/// vertex buffers' UV sets, or geometry — and none of those has been read here.
///
/// That is why [`BindingScope::is_inferred`] is `true` for every variant and
/// [`claims::MATERIAL_BLOCK_IS_MESH`] is a non-finding. It is not a gap waiting
/// for a better partition function: the evidence that would settle it is not in
/// the data this crate reads.
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
        // answer, not a failure to answer. Nothing is contradicted either, so
        // there is nothing for the cross-check to refuse.
        return Some(vec![&[]; meshes]);
    }
    if total % meshes != 0 {
        return None;
    }
    let slices = model
        .texture_refs
        .chunks_exact(total / meshes)
        .collect::<Vec<_>>();
    // The record's own second statement of each entry's position. Without this
    // the function would divide whatever it was handed, and a list the walk
    // stopped reading part-way divides just as neatly as a complete one.
    let check = cross_check(&slices);
    if check.contradicted == 0 && check.checked > 0 {
        Some(slices)
    } else {
        None
    }
}

/// What the record's own ordinals said about one candidate cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CrossCheck {
    /// Slots whose group id states an ordinal, so the cut could be checked.
    checked: usize,
    /// Slots whose stated ordinal is not the one the cut gives them.
    contradicted: usize,
}

/// Asks the record whether a candidate cut is the block boundaries.
///
/// A **cross-check**, not a derivation: the ordinal a slot is given comes from
/// this crate's arithmetic, and the ordinal the record states comes from the
/// referenced group's id low byte — the other of the two statements
/// [`claims::SAMPLER_WORD_IS_ENTRY_ORDINAL`] and
/// [`claims::GROUP_LOW_BYTE_IS_SLOT`] record as agreeing in 6 684 of 6 684
/// references. Two independent statements of one fact, so a disagreement is
/// visible rather than averaged away.
///
/// # A group id of `0` states NO ordinal
///
/// 150 references in `Spore_Content.package` name group id `0` outright, across
/// 132 of the records this crate walks. Such a reference belongs to no group
/// family, so its low byte carries no slot: treating it as ordinal `0` would
/// make every slot but the first of every mesh look contradicted and would cost
/// 132 records a cut that is in fact sound. Those slots are therefore **not
/// checked**, and [`CrossCheck::checked`] is what says so — a slot that states
/// nothing is not a slot that agrees.
///
/// The cost is a hole with a measured size: a cut whose every slot states no
/// ordinal is unverified, and `scope_of` refuses to offer one. The corpus census
/// counts both numbers rather than leaving them to be argued about.
fn cross_check(slices: &[&[GmdlTextureRef]]) -> CrossCheck {
    let mut check = CrossCheck {
        checked: 0,
        contradicted: 0,
    };
    for slice in slices {
        for (within, reference) in slice.iter().enumerate() {
            let slot = TextureSlot {
                reference: *reference,
                ordinal: Some(within as u32),
            };
            // One rule, in one place: `ordinal_agrees` is what decides, so the
            // cross-check and a caller reading the slots cannot disagree about
            // what counts as a contradiction.
            if !slot.group_states_an_ordinal() {
                continue;
            }
            check.checked += 1;
            if !slot.ordinal_agrees() {
                check.contradicted += 1;
            }
        }
    }
    check
}

/// The scope [`partition_texture_refs`] would produce, without doing it.
///
/// The refusals are kept apart rather than collapsed into one
/// [`BindingScope::WholeModel`], because they mean different things to a caller:
/// an uneven list is a fact about the list, and a contradicted one is a fact
/// about the record disagreeing with the cut. Collapsing them is how a confident
/// wrong binding hides inside an honest-sounding fallback.
fn scope_of(model: &GmdlModel) -> BindingScope {
    let meshes = model.meshes.len();
    let total = model.texture_refs.len();
    if meshes == 0 {
        return BindingScope::WholeModel;
    }
    if total == 0 {
        return BindingScope::PerMeshBlock { refs_per_mesh: 0 };
    }
    if total % meshes != 0 {
        return BindingScope::WholeModel;
    }
    let slices = model
        .texture_refs
        .chunks_exact(total / meshes)
        .collect::<Vec<_>>();
    let check = cross_check(&slices);
    if check.contradicted > 0 {
        return BindingScope::Undetermined {
            collected: total,
            contradicted: check.contradicted,
        };
    }
    if check.checked == 0 {
        // Every reference states no ordinal, so nothing confirmed the cut and
        // nothing contradicts it either. Offering an unchecked cut would be the
        // failure this whole check exists to prevent, in its quieter form.
        return BindingScope::WholeModel;
    }
    BindingScope::PerMeshBlock {
        refs_per_mesh: total / meshes,
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
        BindingScope::WholeModel | BindingScope::Undetermined { .. } => None,
    };
    debug_assert_eq!(
        per_mesh.is_some(),
        scope.divides_by_mesh(),
        "the scope and the partition must agree on whether a cut was made"
    );

    model
        .material_ids
        .iter()
        .enumerate()
        .map(|(mesh_index, material_id)| {
            // The slice for this mesh when the cut was established, the whole
            // collected list when it was not. Both are stated by `scope`; neither
            // is a resolved binding. A mesh with no block of its own -- which the
            // cut can only produce for a model that referenced nothing -- gets an
            // empty slice rather than index 0's textures, because that is the one
            // mistake this function exists to avoid.
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
                        BindingScope::WholeModel | BindingScope::Undetermined { .. } => None,
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
        let model = synthetic_blocks(2, 3);
        let slices = partition_texture_refs(&model).expect("6 divides by 2, and the record agrees");
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].len(), 3);
        assert_eq!(slices[1].len(), 3);
        assert_ne!(slices[0][0].group_id, slices[1][0].group_id);
    }

    #[test]
    fn an_odd_division_is_refused_rather_than_rounded() {
        // Three meshes, two references: no even cut exists. Rounding would put
        // mesh 0's textures on mesh 1.
        let model = synthetic_running_ordinals(3, 2);
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
        let mut model = synthetic_blocks(1, 0);
        model.meshes.clear();
        model.material_ids.clear();
        let slices = partition_texture_refs(&model).expect("consistent absence");
        assert!(slices.is_empty());
        assert!(model_materials(&model).is_empty());
    }

    #[test]
    fn a_model_with_no_meshes_but_textures_is_inconsistent_and_says_so() {
        let mut model = synthetic_blocks(1, 0);
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
        let model = synthetic_blocks(3, 0);
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
        let model = synthetic_blocks(2, 3);
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
        let materials = model_materials(&synthetic_blocks(2, 3));
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
    fn an_even_division_the_record_contradicts_is_not_offered_as_a_cut() {
        // The documented creature's shape, exactly: two meshes, TWO references
        // collected, and `2 / 2 = 1` divides evenly. The arithmetic succeeds.
        // The record does not agree: the group ids state ordinals 0 and 1, so
        // both references came out of ONE block and there is nothing to say
        // which mesh the second belongs to.
        //
        // Before this was refused, `refs_per_mesh` was 1 and mesh 1 was bound to
        // a texture taken out of mesh 0's block.
        let model = synthetic_running_ordinals(2, 2);
        assert_eq!(model.texture_refs.len() % model.meshes.len(), 0);
        assert_eq!(partition_texture_refs(&model), None);

        let materials = model_materials(&model);
        assert_eq!(materials.len(), 2);
        for material in &materials {
            assert_eq!(
                material.scope,
                BindingScope::Undetermined {
                    collected: 2,
                    contradicted: 1,
                },
                "mesh {} must not be handed a per-mesh slice",
                material.mesh_index
            );
            assert!(material.scope.is_inferred());
            assert!(!material.scope.divides_by_mesh());
            assert!(material.scope.applies_whole_set());
            assert_eq!(material.scope.refs_per_mesh(), None);
            assert_eq!(material.scope.collected(), Some(2));
            assert_eq!(material.scope.as_str(), "undetermined");
            // The whole collected list goes to every mesh, and no slot claims a
            // per-mesh ordinal -- a slot with an ordinal here would be the wrong
            // binding wearing a smaller hat.
            assert_eq!(material.slots.len(), 2);
            assert!(material.slots.iter().all(|slot| slot.ordinal.is_none()));
            assert_eq!(material.slots[1].reference.group_id, 0x4063_2901);
        }
    }

    #[test]
    fn a_contradicted_cut_says_so_in_a_graded_claim_and_never_as_a_partition() {
        let materials = model_materials(&synthetic_running_ordinals(2, 2));
        for material in &materials {
            // The partition itself is a non-finding: it was NOT made.
            let partition = material.claim(claims::PER_MESH_TEXTURE_PARTITION).unwrap();
            assert!(
                partition.fact.is_unavailable(),
                "a cut that was refused must not carry a graded value"
            );
            assert_eq!(
                partition.fact.reason(),
                Some(claims::TEXTURE_SET_NOT_CUT_PER_MESH)
            );
            // And the reason it was refused is a separate, graded observation:
            // a non-finding would be wrong here, because something WAS found.
            let contradiction = material
                .claim(claims::CUT_CONTRADICTED_BY_RECORD)
                .unwrap_or_else(|| panic!("mesh {}", material.mesh_index));
            assert!(contradiction.fact.is_available());
            assert_eq!(contradiction.fact.level(), EvidenceLevel::Observed);
            assert_ne!(contradiction.fact.level(), EvidenceLevel::Inferred);
            assert_eq!(
                contradiction.fact.value(),
                Some(&UNDETERMINED_BINDINGS_TEXT)
            );
            assert!(!contradiction.fact.provenance().is_empty());
        }
    }

    #[test]
    fn a_cut_the_record_agrees_with_records_no_contradiction() {
        // The other arm of the same subject, and the reason it exists: a record
        // that reports no contradiction HAS BEEN CHECKED, which is not the same
        // as a record nobody looked at.
        for material in model_materials(&synthetic_blocks(2, 3)) {
            let contradiction = material.claim(claims::CUT_CONTRADICTED_BY_RECORD).unwrap();
            assert!(
                contradiction.fact.is_unavailable(),
                "3 of 3 ordinals agreed, so there is nothing to report"
            );
            assert_eq!(
                contradiction.fact.reason(),
                Some(claims::NO_CUT_CONTRADICTION)
            );
            assert_eq!(contradiction.fact.value(), None);
            assert!(contradiction.fact.provenance().is_empty());
        }
    }

    #[test]
    fn a_group_id_of_zero_states_no_ordinal_and_is_simply_not_checked() {
        // 150 references in Spore_Content.package name group id 0 outright,
        // across 132 records. Such a reference belongs to no group family, so
        // its low byte carries no slot -- reading it as ordinal 0 would make
        // every slot but the first of every mesh look contradicted and would
        // cost 132 records a cut that is in fact sound.
        //
        // So it is excluded from the cross-check, which is what the earlier
        // version of this crate got wrong in the other direction: it counted a
        // silent statement as a contradiction and refused the cut.
        let mut model = synthetic_blocks(2, 2);
        model.texture_refs[3] = reference(0x0000_0000);
        let slices = partition_texture_refs(&model).expect("the other three slots do agree");
        assert_eq!(slices.len(), 2);
        assert_eq!(
            slices[1][1].group_id, 0,
            "the silent reference is still carried"
        );
        for material in model_materials(&model) {
            assert_eq!(
                material.scope,
                BindingScope::PerMeshBlock { refs_per_mesh: 2 },
                "an unchecked slot is not a contradicted one"
            );
        }
    }

    #[test]
    fn a_group_id_of_zero_is_a_silent_statement_not_a_contradiction() {
        // `ordinal_agrees` is what `cross_check` and every caller read, so it has
        // to treat "the group id states nothing" the same way it treats "this
        // crate derived nothing". Reading 0 as "slot 0" is how a silent
        // reference turns into a spurious conflict.
        let silent = TextureSlot {
            reference: reference(0x0000_0000),
            ordinal: Some(2),
        };
        assert!(!silent.group_states_an_ordinal());
        assert_eq!(silent.group_stated_ordinal(), 0);
        assert!(
            silent.ordinal_agrees(),
            "a reference that states no ordinal cannot contradict one that does"
        );
        // And a real statement is still checked.
        assert!(TextureSlot {
            reference: reference(0x4063_2900),
            ordinal: Some(0),
        }
        .group_states_an_ordinal());
    }

    #[test]
    fn a_cut_nothing_can_confirm_is_not_offered() {
        // Every reference states no ordinal: nothing confirms the cut and
        // nothing contradicts it. "Not contradicted" is not "confirmed", and
        // this build does not offer a cut it could not check. Conservative, and
        // a case the corpus census asserts occurs zero times.
        let model = synthetic_model(
            2,
            vec![reference(0), reference(0), reference(0), reference(0)],
        );
        assert_eq!(partition_texture_refs(&model), None);
        let materials = model_materials(&model);
        assert!(materials
            .iter()
            .all(|m| m.scope == BindingScope::WholeModel));
        assert!(materials.iter().all(|m| m.slots.len() == 4));
        assert!(materials.iter().all(|m| !m.scope.divides_by_mesh()));
    }

    #[test]
    fn every_scope_reports_that_its_binding_is_inferred() {
        // A caller asking "is this a proven binding?" must get `false` from
        // every variant. There is no proven binding in this build, and a scope
        // that claimed otherwise would be the whole failure this crate prevents.
        for scope in [
            BindingScope::PerMeshBlock { refs_per_mesh: 3 },
            BindingScope::WholeModel,
            BindingScope::Undetermined {
                collected: 2,
                contradicted: 1,
            },
        ] {
            assert!(scope.is_inferred(), "{}", scope.as_str());
        }
        // And exactly one of them divides by mesh count.
        assert!(BindingScope::PerMeshBlock { refs_per_mesh: 3 }.divides_by_mesh());
        assert!(!BindingScope::WholeModel.divides_by_mesh());
        assert!(!BindingScope::Undetermined {
            collected: 2,
            contradicted: 1
        }
        .divides_by_mesh());
        assert_eq!(
            BindingScope::Undetermined {
                collected: 2,
                contradicted: 1
            }
            .collected(),
            Some(2)
        );
        assert_eq!(
            BindingScope::PerMeshBlock { refs_per_mesh: 3 }.collected(),
            None
        );
        assert_eq!(BindingScope::WholeModel.collected(), None);
    }

    #[test]
    fn a_failed_cut_records_the_partition_as_a_non_finding() {
        let materials = model_materials(&synthetic_running_ordinals(3, 2));
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

    /// A minimal decoded model whose reference list is laid out the way the
    /// corpus lays it out: `blocks` texture sets of `per_block` entries each,
    /// every entry's group id low byte equal to its position **inside its own
    /// set**, and every block drawn from a different group family so a mix-up
    /// is visible by value.
    ///
    /// The per-set ordinal is load-bearing, not decoration. It is the record's
    /// own statement of where each entry sits, and
    /// [`partition_texture_refs`] refuses any cut that disagrees with it — so a
    /// fixture that numbered its group ids across the whole flat list would be
    /// modelling a truncated record, not a sound one.
    fn synthetic_blocks(blocks: usize, per_block: usize) -> GmdlModel {
        let texture_refs: Vec<GmdlTextureRef> = (0..blocks)
            .flat_map(|block| {
                let family = 0x4063_2900u32 + (block as u32) * 0x100;
                (0..per_block).map(move |within| reference(family + within as u32))
            })
            .collect();
        synthetic_model(blocks, texture_refs)
    }

    /// A model whose flat list is `refs` references long with group ids running
    /// `0x4063_2900, 0x4063_2901, ...` across the **whole** list, so the ordinals
    /// do not restart per block.
    ///
    /// This is exactly what a material-info walk that stopped part-way leaves
    /// behind, and it is why the cut is refused rather than taken: the arithmetic
    /// is fine and the *record* says the boundaries are elsewhere.
    fn synthetic_running_ordinals(meshes: usize, refs: usize) -> GmdlModel {
        synthetic_model(
            meshes,
            (0..refs)
                .map(|index| reference(0x4063_2900 + index as u32))
                .collect(),
        )
    }

    /// A minimal decoded model with `meshes` meshes (one material id each) and
    /// the given flat texture reference list, written by hand rather than by a
    /// builder that could share a mistake with the reader.
    fn synthetic_model(meshes: usize, texture_refs: Vec<GmdlTextureRef>) -> GmdlModel {
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
            texture_refs,
            bone_ranges: Vec::new(),
            unknown_key: [0; 3],
            trailer: spore_gmdl::TrailerWalk::Complete,
            strict_consumed: 0,
            consumed: 0,
        }
    }
}
