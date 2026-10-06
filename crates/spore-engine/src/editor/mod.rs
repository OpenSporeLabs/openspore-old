//! # The editor: selection, a real catalogue, and a UI over the document.
//!
//! Nothing here owns authoring semantics. Every button in the editor ends in a
//! [`CreatorSession`](session::CreatorSession) method, which ends in a
//! `spore-creator-core` mutator, which ends in a [`CreatorDocument`]. If a UI
//! path could mutate the ECS directly, the same edit would have two
//! implementations and the tests would only be exercising one of them.
//!
//! | module | what it owns | why it is separate |
//! |---|---|---|
//! | [`session`] | the mutable document, the history, the re-projection | it is the only thing that holds a live package mapping |
//! | [`picking`] | cursor ray -> `BlockIndex` | pure maths, testable with no camera, so the interesting cases can be written down |
//! | [`ui`] | the screen's layout, proportions and colours | a screenshot cannot be asserted on, so the proportions have to be numbers first |

pub mod picking;
pub mod plugin;
pub mod session;
pub mod skin;
pub mod thumbnails;
pub mod ui;
pub mod viewport;

pub use session::{CreatorSession, SessionError};
