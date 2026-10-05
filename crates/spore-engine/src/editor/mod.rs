//! # The editor: selection, a real catalogue, and a UI over the document.
//!
//! Nothing here owns authoring semantics. Every button in the editor ends in a
//! [`CreatorSession`](session::CreatorSession) method, which ends in a
//! `spore-creator-core` mutator, which ends in a [`CreatorDocument`]. If a UI
//! path could mutate the ECS directly, the same edit would have two
//! implementations and the tests would only be exercising one of them.

pub mod session;

pub use session::{CreatorSession, SessionError};
