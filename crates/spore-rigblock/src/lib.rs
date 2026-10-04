//! The **EditorRigblock part layer**: what a creature part actually *is*.
//!
//! # Why this crate exists
//!
//! A `<sporemodel>` document says where a part goes. It does not say what the
//! part is. That question is answered three records away, in a chain that is now
//! fully walked except for one hop:
//!
//! ```text
//! <blockref><blockid>   the authoring reference        spore-creator-core
//!     -> rigblock       0x5CAA5E28, a baked bounds grid   <- this crate
//!     -> prop           0x00B1B104, a pointer record
//!     -> 0x40606000     the part itself, a PAIR:
//!                          0x2F4E681B (rw4) container  <- this crate: the skeleton
//!                          0x01C135DA (gmsh) geometry
//! ```
//!
//! # The one structural fact that organises the crate
//!
//! **A part is a pair, not a file.** The geometry record (`gmsh`) cannot name a
//! bone — its bones are an anonymous local ordinal list. The container record
//! (`rw4`) holds a `Skeleton` section that is a rooted, parented, *named* body
//! graph. Neither record contains the other's type id; the join is by instance id
//! alone, and it is the load that binds geometry to names.
//!
//! So [`skeleton::Skeleton`] is the interesting type here, and decoding a mesh
//! without its sibling container yields half a part.
//!
//! # Evidence discipline
//!
//! Every structural claim in this crate was measured over the real corpus and is
//! re-checked at decode time rather than trusted — a decoder that assumes the
//! layout it was written against will happily accept a record that violates it.
//! Claims that are *not* verified live in [`claims`] and stay there.
//!
//! Nothing in this crate is Bevy-free *by accident*: it has no Bevy dependency and
//! must not acquire one. It sits below the renderer with `spore-core` as its only
//! runtime dependency.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod claims;
pub mod error;
pub mod gmsh;
pub mod rigblock;
pub mod skeleton;
pub mod vocabulary;

pub use vocabulary::entries;

pub use error::{Result, RigblockError};
pub use gmsh::{Block as GmshBlock, BoneRange, GmshMesh};
pub use rigblock::{Cell, Rigblock, TYPE_RIGBLOCK};
pub use skeleton::{Bone, Skeleton, SECTION_SKELETON};
pub use vocabulary::{identity_of, BoneName, Confidence, Entry};
