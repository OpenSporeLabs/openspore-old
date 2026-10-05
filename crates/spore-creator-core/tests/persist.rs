//! `spore-creator-doc/1` as a **public contract**: the round trip, the
//! absent-vs-empty distinction, the `-0.0` case, and determinism.
//!
//! Everything here goes through the crate's public API rather than its internals,
//! because the claim being tested is a claim about the format, not about a
//! private function. The exhaustive 2^32-bit float proof and the header's
//! non-self-referential digest live in the unit tests, where the private float
//! codec is reachable.
//!
//! No game install and no game bytes: every document here is either built in
//! Rust or decoded from a synthetic `<sporemodel>` string written by hand.

use spore_creator_core::persist::{load, load_bytes, save, save_bytes};
use spore_creator_core::{
    parse_sporemodel, validate, Block, BlockId, BlockTransform, ChannelWeight, CreatorDocument,
    DocumentProperties, Family, Limb, Matrix3, PaintRegion, PersistError, Severity, UnitRgb, Vec3,
    FORMAT,
};

/// A document with nested children, a reciprocal symmetry pair, paint regions,
/// handles, a limb, every transform field written on at least one block, and one
/// block with fields deliberately absent.
fn kitchen_sink() -> CreatorDocument {
    let skin = UnitRgb::new(0.341176, 0.360784, 0.576470).unwrap();
    CreatorDocument {
        format_version: Some(16),
        properties: DocumentProperties {
            model_type: Some(0x9ea3_031a),
            name: Some("Round Trip".to_owned()),
            description: Some(String::new()),
            skin_effects: Some([Some(0x1234_5678), None, Some(0x9ea3_031a)]),
            skin_effect_seeds: Some([Some(1), Some(2), Some(3)]),
            skin_colors: Some([Some(skin), None, Some(UnitRgb::new(1.0, 1.0, 1.0).unwrap())]),
            required_pack: Some(0x0241_0000),
            zcorp_score: Some(0),
            has_thumbnail: Some(true),
            cache_id: Some(0xffff_ffff),
        },
        blocks: vec![
            // Block 0: the root. Every optional field present, and every
            // transform field written.
            Block {
                block_id: Some(BlockId::new(0x4062_6000, 0xd05c_53a3)),
                transform: BlockTransform {
                    position: Vec3 {
                        x: 0.0,
                        y: 0.450979,
                        z: 7.26504e-9,
                    },
                    scale: 1.0,
                    orientation: Matrix3::IDENTITY,
                    triangle_direction: Some(Vec3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                    triangle_pick_origin: Some(Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }),
                },
                snapped: Some(true),
                // Nested children: 0 owns 1, 1 owns 2.
                children: Some(vec![1]),
                symmetric: Some(3),
                paint: Some(vec![PaintRegion {
                    region: 0x1a,
                    paint_id: 0xf596_7f30,
                    color1: skin,
                    color2: UnitRgb::new(0.0, 0.0, 0.0).unwrap(),
                }]),
                handles: Some(vec![
                    ChannelWeight {
                        channel: 3,
                        weight: 0.5,
                    },
                    ChannelWeight {
                        channel: 11,
                        weight: -0.0,
                    },
                ]),
                limb: Some(Limb {
                    limb_type: Some(2),
                    muscle_scale: Some(1.5),
                    base_muscle_scale: None,
                }),
                user_orientation: Some(Matrix3 {
                    row0: [1.0, 0.0, 0.0],
                    row1: [0.0, 1.0, 0.0],
                    row2: [0.0, 0.0, 1.0],
                }),
                bbox: Some((
                    Vec3 {
                        x: -1.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    Vec3 {
                        x: 1.0,
                        y: 0.0,
                        z: 0.0,
                    },
                )),
            },
            // Block 1: a middle block — a child of 0 and the parent of 2. This
            // is the shape a compaction has to survive, because its own index is
            // both above and below the ones that reference it.
            Block {
                block_id: Some(BlockId::new(0x4062_6000, 0x1111_1111)),
                transform: BlockTransform {
                    position: Vec3 {
                        x: 1.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    scale: 0.28,
                    orientation: Matrix3::default(),
                    triangle_direction: Some(Vec3::default()),
                    // The field deliberately absent here: 10 of the 680 corpus
                    // blocks, all of them in the one `formatversion 11`
                    // document, write no `<trianglepickorigin>`.
                    triangle_pick_origin: None,
                },
                snapped: Some(false),
                children: Some(vec![2]),
                symmetric: None,
                paint: None,
                handles: None,
                limb: None,
                user_orientation: None,
                bbox: None,
            },
            // Block 2: a leaf, which is how the corpus writes 160 of 680.
            Block {
                block_id: Some(BlockId::new(0x4062_6000, 0x2222_2222)),
                transform: BlockTransform {
                    position: Vec3 {
                        x: 2.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    scale: 2.254181,
                    orientation: Matrix3 {
                        row0: [0.0, -1.0, 0.0],
                        row1: [1.0, 0.0, 0.0],
                        row2: [0.0, 0.0, 1.0],
                    },
                    ..BlockTransform::default()
                },
                snapped: Some(true),
                children: None,
                symmetric: None,
                paint: None,
                handles: None,
                limb: None,
                user_orientation: None,
                bbox: None,
            },
            // Block 3: the mirror partner of 0. Reciprocal, so the round trip has
            // an index to preserve on both sides.
            Block {
                block_id: Some(BlockId::new(0x4062_6000, 0x3333_3333)),
                transform: BlockTransform {
                    position: Vec3 {
                        x: -1.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    scale: 1.0,
                    orientation: Matrix3::IDENTITY,
                    triangle_direction: Some(Vec3::default()),
                    triangle_pick_origin: Some(Vec3::default()),
                },
                snapped: Some(true),
                children: None,
                symmetric: Some(0),
                paint: None,
                handles: None,
                limb: None,
                user_orientation: None,
                bbox: None,
            },
        ],
    }
}

/// The synthetic `<sporemodel>` a hermetic test can decode. Shaped like the
/// corpus: both spellings of the same word, an exponent-form float, a leaf with
/// no `<childlist>`, a `<childlist>` that was written **empty**, and one
/// reciprocal `<symmetric>` pair. It validates with no `Error`, so a save-format
/// failure cannot be confused with a document that was already wrong.
const SYNTHETIC: &str = r#"<sporemodel>
  <formatversion>16</formatversion>
  <properties>
    <modeltype>0x9ea3031a</modeltype>
    <name>Synthetic</name>
    <skincolor1>0.341176,0.360784,0.576470</skincolor1>
    <requiredpack>37814272</requiredpack>
  </properties>
  <blocks count="4">
    <blockref>
      <blockid>0x40626000, 0xd05c53a3</blockid>
      <transform>
        <position>0,0,0</position>
        <scale>1</scale>
        <orientation>
          <row0>1,0,0</row0>
          <row1>0,1,0</row1>
          <row2>0,0,1</row2>
        </orientation>
        <triangledirection>0,1,0</triangledirection>
        <trianglepickorigin>0,0,0</trianglepickorigin>
      </transform>
      <snapped>true</snapped>
      <childlist count="1"><childid>1</childid></childlist>
      <symmetric>3</symmetric>
    </blockref>
    <blockref>
      <blockid>0x40626000, 0x11111111</blockid>
      <transform>
        <position>1,0,0</position>
        <scale>0.28</scale>
        <orientation>
          <row0>1,0,0</row0>
          <row1>0,1,0</row1>
          <row2>0,0,1</row2>
        </orientation>
        <triangledirection>0,1,0</triangledirection>
        <musclescale>1.5</musclescale>
      </transform>
      <snapped>false</snapped>
    </blockref>
    <blockref>
      <blockid>0x40626000, 0x22222222</blockid>
      <transform>
        <position>2,0,0</position>
        <scale>1.0</scale>
        <orientation>
          <row0>1,0,0</row0>
          <row1>0,1,0</row1>
          <row2>0,0,1</row2>
        </orientation>
        <triangledirection>7.26504e-9,0,0</triangledirection>
      </transform>
      <snapped>true</snapped>
      <childlist count="0"/>
    </blockref>
    <blockref>
      <blockid>0x40626000, 0x33333333</blockid>
      <transform>
        <position>0,0,0</position>
        <scale>1</scale>
        <orientation>
          <row0>1,0,0</row0>
          <row1>0,1,0</row1>
          <row2>0,0,1</row2>
        </orientation>
        <triangledirection>0,1,0</triangledirection>
        <trianglepickorigin>0,0,0</trianglepickorigin>
      </transform>
      <snapped>true</snapped>
      <symmetric>0</symmetric>
      <handles count="1"><weight channel="0x3">0.5</weight></handles>
    </blockref>
  </blocks>
</sporemodel>"#;

#[test]
fn a_synthetic_document_round_trips_bit_exactly() {
    let document = kitchen_sink();
    let text = save(&document).unwrap();

    // One header line plus one line per block, in document order.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), document.len() + 1, "{text}");
    assert!(lines[0].contains(&format!("\"schema\":\"{FORMAT}\"")));
    assert!(lines[0].contains(&format!("\"block_count\":{}", document.len())));
    assert!(lines[0].contains(&format!("\"root_count\":{}", document.roots().len())));
    for (position, line) in lines.iter().enumerate().skip(1) {
        assert!(
            line.starts_with(&format!("{{\"index\":{}", position - 1)),
            "content line {position} must declare its own index: {line}"
        );
    }

    let reloaded = load(&text).unwrap();
    assert_eq!(
        reloaded, document,
        "a save that reads back as a different document is not a save"
    );
    // The graph, not just the values: nested children and the mirror pair.
    assert_eq!(reloaded.roots(), vec![0, 3]);
    assert_eq!(reloaded.blocks[0].children.as_deref(), Some(&[1][..]));
    assert_eq!(reloaded.blocks[1].children.as_deref(), Some(&[2][..]));
    assert_eq!(reloaded.blocks[3].symmetric, Some(0));
    assert_eq!(reloaded.blocks[0].symmetric, Some(3));
    // And the byte-level API is the same contract.
    assert_eq!(
        load_bytes(&save_bytes(&document).unwrap()).unwrap(),
        document
    );
}

#[test]
fn a_document_decoded_from_the_grammar_round_trips() {
    // The point of a save format is that what the decoder produced can be
    // written out and read back, so this goes through `parse_sporemodel` rather
    // than building the document by hand.
    let document = parse_sporemodel(Family::Creature, SYNTHETIC).unwrap();
    assert!(
        validate(&document, Some(Family::Creature))
            .iter()
            .all(|issue| issue.severity == Severity::Warning),
        "{:?}",
        validate(&document, Some(Family::Creature))
    );

    let reloaded = load(&save(&document).unwrap()).unwrap();
    assert_eq!(reloaded, document);
    assert_eq!(
        reloaded.format_version,
        Some(16),
        "the document-level word the corpus writes as a bare decimal survives"
    );
    assert_eq!(
        reloaded.properties.model_type,
        Some(0x9ea3_031a),
        "both spellings of modeltype are the same field"
    );
    assert_eq!(
        reloaded.properties.required_pack,
        Some(0x0241_0000),
        "a bare decimal word and its `0x` spelling are the same field"
    );
    assert_eq!(
        reloaded.blocks[2].transform.triangle_direction,
        Some(Vec3 {
            x: 7.26504e-9,
            y: 0.0,
            z: 0.0
        }),
        "an exponent-form float survives"
    );
    assert_eq!(
        reloaded.blocks[1].limb,
        Some(Limb {
            limb_type: None,
            muscle_scale: Some(1.5),
            base_muscle_scale: None
        })
    );
    assert_eq!(
        reloaded.blocks[3].handles.as_deref().map(<[_]>::len),
        Some(1)
    );
}

#[test]
fn an_absent_field_and_an_empty_one_are_different_in_both_directions() {
    let document = parse_sporemodel(Family::Creature, SYNTHETIC).unwrap();

    // Block 1 writes no `<childlist>`; block 2 writes an empty one. `validate`
    // reports the second as `empty_child_list` precisely because the two are not
    // the same, which is the whole argument for keeping them apart.
    assert!(!document.blocks[1].writes_children());
    assert!(document.blocks[2].writes_children());
    let codes: Vec<&str> = validate(&document, None)
        .iter()
        .map(|issue| issue.code)
        .collect();
    assert!(codes.contains(&"empty_child_list"), "{codes:?}");

    let text = save(&document).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        !lines[2].contains("\"children\""),
        "block 1 wrote no list, so its save line carries no key at all: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("\"children\":[]"),
        "block 2 wrote an empty list, which is `[]` and not nothing: {}",
        lines[3]
    );
    assert!(
        !text.contains("\"children\":null"),
        "absence is an absent key, never a null and never []"
    );

    // Forward: the reloaded document keeps the distinction.
    let reloaded = load(&text).unwrap();
    assert_eq!(reloaded, document);
    assert!(!reloaded.blocks[1].writes_children());
    assert!(reloaded.blocks[2].writes_children());
    assert_eq!(
        validate(&reloaded, None).len(),
        validate(&document, None).len()
    );

    // Backward: promoting an absence to an empty list is one field, one
    // different file, and one different document — in both directions.
    for (from, to) in [(1usize, 2usize), (2, 1)] {
        let mut promoted = reloaded.clone();
        if promoted.blocks[from].children.is_none() {
            promoted.blocks[from].children = Some(Vec::new());
        } else {
            promoted.blocks[from].children = None;
        }
        let other = save(&promoted).unwrap();
        assert_ne!(other, text, "block {from} -> {to} must change the bytes");
        assert_ne!(load(&other).unwrap(), reloaded);
        assert_eq!(
            load(&other).unwrap(),
            promoted,
            "and it must still round trip"
        );
    }

    // The same distinction for every other optional collection: a written-empty
    // `<paintlist>` / `<handles>` is `Some(vec![])`, not `None`.
    let mut written_empty = kitchen_sink();
    written_empty.blocks[1].paint = Some(Vec::new());
    written_empty.blocks[1].handles = Some(Vec::new());
    let reloaded = load(&save(&written_empty).unwrap()).unwrap();
    assert_eq!(reloaded, written_empty);
    assert_eq!(reloaded.blocks[1].paint.as_deref(), Some(&[][..]));
    assert_eq!(reloaded.blocks[1].handles.as_deref(), Some(&[][..]));
    assert_eq!(reloaded.blocks[0].paint.as_ref().map(Vec::len), Some(1));
}

#[test]
fn a_negative_zero_survives_as_a_negative_zero() {
    // The single most likely thing to be silently wrong: `0.0 == -0.0` under
    // IEEE, this crate's `PartialEq` is bitwise for exactly that reason, and a
    // format that normalises `-0.0` to `0.0` would pass every numeric assertion
    // and ship a different document.
    let mut document = kitchen_sink();
    document.blocks[0].transform.position = Vec3 {
        x: -0.0,
        y: 0.0,
        z: -0.0,
    };
    document.blocks[0].transform.scale = -0.0;
    document.blocks[0].transform.orientation.row2 = [-0.0, 0.0, -0.0];
    document.blocks[0].handles.as_mut().expect("handles")[1].weight = -0.0;
    document.blocks[0].limb.as_mut().expect("limb").muscle_scale = Some(-0.0);

    let text = save(&document).unwrap();
    // Visible in the bytes, not only in the value. Seven places carry a negative
    // zero: position x and z, scale, orientation row2 x and z, one handle weight
    // and one muscle scale.
    assert_eq!(text.matches("\"-0.0\"").count(), 7, "{text}");
    assert!(text.contains("\"position\":{\"x\":\"-0.0\",\"y\":\"0.0\",\"z\":\"-0.0\"}"));
    assert!(text.contains("\"scale\":\"-0.0\""));
    assert!(text.contains("\"row2\":[\"-0.0\",\"0.0\",\"-0.0\"]"));

    let reloaded = load(&text).unwrap();
    assert_eq!(reloaded, document);
    assert_eq!(
        reloaded.blocks[0].transform.position.x.to_bits(),
        0x8000_0000
    );
    assert_eq!(
        reloaded.blocks[0].transform.position.y.to_bits(),
        0x0000_0000
    );
    // Compared as a `Vec3`, because this crate's `PartialEq` is the bitwise one:
    // `f32`'s own `!=` would call `-0.0` and `0.0` equal and pass here wrongly.
    assert_ne!(
        reloaded.blocks[0].transform.position,
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0
        },
        "IEEE says these compare equal; this crate does not"
    );

    // And flipping one sign bit changes the file.
    let mut flipped = reloaded.clone();
    flipped.blocks[0].transform.position.x = 0.0;
    let other = save(&flipped).unwrap();
    assert_ne!(other, text);
    assert_ne!(load(&other).unwrap(), reloaded);
}

#[test]
fn saving_the_same_document_twice_gives_identical_bytes() {
    let document = kitchen_sink();
    assert_eq!(save(&document).unwrap(), save(&document).unwrap());
    assert_eq!(
        save_bytes(&document).unwrap(),
        save_bytes(&document).unwrap()
    );

    // Determinism also has to survive a *clone*, or it would be reading the
    // caller's allocation rather than the document.
    let clone = document.clone();
    assert_eq!(save(&clone).unwrap(), save(&document).unwrap());
    assert_eq!(
        load(&save(&clone).unwrap()).unwrap(),
        load(&save(&document).unwrap()).unwrap()
    );
}

#[test]
fn an_empty_document_is_a_document_with_nothing_in_it_and_not_a_creature() {
    let document = CreatorDocument::default();
    let text = save(&document).unwrap();
    assert_eq!(text.lines().count(), 1, "a header and no blocks: {text}");
    assert!(text.contains("\"block_count\":0"));
    assert!(text.contains("\"root_count\":0"));
    let reloaded = load(&text).unwrap();
    assert_eq!(reloaded, document);
    assert!(reloaded.is_empty());
}

#[test]
fn a_malformed_file_is_refused_with_a_typed_error_naming_the_line() {
    // These are the refusals the **public** API can reach without re-signing the
    // header, which is deliberate: re-signing is not a capability this crate
    // offers, because a caller holding it could write a file that passes the
    // digest and says something false. The line-numbered *body* refusals are
    // covered in the unit tests, where the hasher is reachable.
    let document = kitchen_sink();
    let text = save(&document).unwrap();

    // A file that is a different format is refused as one, before anything is
    // interpreted.
    assert!(matches!(
        load(&text.replace(FORMAT, "spore-creator-doc/2")),
        Err(PersistError::Schema {
            site: spore_creator_core::PersistSite {
                line: 1,
                block: None
            },
            ..
        })
    ));
    // So is one that declares a float encoding this build does not read, and one
    // that claims an evidence grade the corpus does not support.
    assert!(matches!(
        load(&text.replace("shortest-round-trip-decimal", "some-other-rule")),
        Err(PersistError::UnsupportedFloatEncoding {
            site: spore_creator_core::PersistSite {
                line: 1,
                block: None
            },
            ..
        })
    ));
    assert!(matches!(
        load(&text.replace(
            "\"evidence_level\":\"OBSERVED\"",
            "\"evidence_level\":\"VERIFIED\""
        )),
        Err(PersistError::Schema {
            site: spore_creator_core::PersistSite {
                line: 1,
                block: None
            },
            ..
        })
    ));
    // A corrupted body is reported as corruption, before any field is read.
    assert!(matches!(
        load(&text.replace("\"scale\":\"1.0\"", "\"scale\":\"1.5\"")),
        Err(PersistError::Checksum {
            site: spore_creator_core::PersistSite {
                line: 1,
                block: None
            },
            ..
        })
    ));
    // A header that disagrees with its own body is a mismatch, not a document.
    assert!(matches!(
        load(&text.replacen("\"block_count\":4", "\"block_count\":5", 1)),
        Err(PersistError::HeaderMismatch {
            site: spore_creator_core::PersistSite { line: 1, block: None },
            field,
            declared: 5,
            found: 4,
            ..
        }) if field == "block_count"
    ));
    // Bytes that are not UTF-8 are refused, not lossily decoded.
    let mut bytes = save_bytes(&document).unwrap();
    bytes[3] = 0xff;
    assert!(matches!(
        load_bytes(&bytes),
        Err(PersistError::Syntax {
            site: spore_creator_core::PersistSite {
                line: 1,
                block: None
            },
            ..
        })
    ));

    // Every error names its line in the text a caller would print.
    let error = load(&text.replace(FORMAT, "spore-creator-doc/2")).unwrap_err();
    assert!(error.to_string().starts_with("line 1: "), "{error}");
}

#[test]
fn validation_still_has_the_last_word_about_a_document_a_save_will_carry() {
    // `load` checks the *file*, not the *document*. A document with a dangling
    // edge is a real finding, `validate` reports it, and a save format that
    // refused to carry it would be a second opinion about the same record.
    let mut document = kitchen_sink();
    document.blocks[1].children = Some(vec![99]);
    document.blocks[3].symmetric = Some(42);

    let reloaded = load(&save(&document).unwrap()).unwrap();
    assert_eq!(reloaded, document, "an invalid document still round trips");

    let codes: Vec<&str> = validate(&reloaded, None)
        .iter()
        .map(|issue| issue.code)
        .collect();
    assert!(codes.contains(&"child_index_out_of_range"), "{codes:?}");
    assert!(codes.contains(&"symmetric_out_of_range"), "{codes:?}");
    assert!(validate(&reloaded, None)
        .iter()
        .any(|issue| issue.severity == Severity::Error));
}
