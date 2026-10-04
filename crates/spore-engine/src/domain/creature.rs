//! A creature: one or more model instances that belong together.
//!
//! # What makes this a distinct concept
//!
//! A `gmdl` record with two meshes is a **model with two parts**, and this
//! crate already handles that: the two meshes come from one record and each
//! gets its own material. A creature built from *several records* is a different
//! thing — it needs its own frame, its own extent, and a lifetime that spans
//! more than one load.
//!
//! So `Creature` here is deliberately thin: a named group of
//! [`ModelInstance`]s. It carries no skeleton, no animation and no
//! "creature-ness", because none of those has been established from the corpus,
//! and a container that looks like a character controller but has none of the
//! data would be the most expensive kind of placeholder.
//!
//! # Resource lifetime
//!
//! A creature owns **references**, not records. The records live in the
//! [`crate::domain::model::ModelDefinition`]s the loader resolved, and those are
//! shared behind an `Arc` by every instance of that record. So a creature with
//! two instances of one record holds one decoded copy, and dropping the
//! creature drops two references and not the record. That is the whole
//! lifetime story, and it is deliberate.

use crate::domain::model::ModelInstance;

/// A group of model instances presented as one thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Creature {
    /// A label, only when the caller gave one. Nothing derives one: DBPF has no
    /// string table.
    pub label: Option<String>,
    /// The instances, in declaration order. The first is the primary body.
    pub instances: Vec<ModelInstance>,
}

impl Creature {
    /// A creature with one instance and no label.
    pub fn single(instance: ModelInstance) -> Self {
        Self {
            label: None,
            instances: vec![instance],
        }
    }

    /// Adds an instance, returning the creature.
    pub fn with(mut self, instance: ModelInstance) -> Self {
        self.instances.push(instance);
        self
    }

    /// How many model records the creature references.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Whether the creature has no instances at all.
    ///
    /// A creature with no instances is an empty group, not a creature: nothing
    /// would be drawn, and a caller that built one has a bug worth naming.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// How many **distinct** records it references.
    ///
    /// The number that decides how many decodes are needed, which is not the
    /// number of instances.
    pub fn distinct_records(&self) -> usize {
        let mut keys: Vec<_> = self.instances.iter().map(|i| i.key).collect();
        keys.sort();
        keys.dedup();
        keys.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_core::ResourceKey;

    fn instance(group: u32) -> ModelInstance {
        ModelInstance::new(
            ResourceKey::new(0x00E6_BCE5, 0x4062_7100, group),
            Default::default(),
        )
    }

    #[test]
    fn one_instance_is_a_creature_of_one_model() {
        let creature = Creature::single(instance(1));
        assert_eq!(creature.len(), 1);
        assert!(!creature.is_empty());
        assert_eq!(creature.distinct_records(), 1);
    }

    #[test]
    fn a_creature_counts_instances_and_distinct_records_separately() {
        let creature = Creature::single(instance(1))
            .with(instance(2))
            .with(instance(1));
        assert_eq!(creature.len(), 3, "three instances are placed");
        assert_eq!(
            creature.distinct_records(),
            2,
            "but only two records must be decoded"
        );
    }

    #[test]
    fn an_empty_creature_says_so_instead_of_pretending_to_hold_a_body() {
        let creature = Creature {
            label: None,
            instances: Vec::new(),
        };
        assert!(creature.is_empty());
        assert_eq!(creature.distinct_records(), 0);
    }

    #[test]
    fn a_label_is_only_ever_the_one_that_was_given() {
        let mut creature = Creature::single(instance(1));
        assert_eq!(creature.label, None);
        creature.label = Some("from the command line".into());
        assert_eq!(creature.label.as_deref(), Some("from the command line"));
    }
}
