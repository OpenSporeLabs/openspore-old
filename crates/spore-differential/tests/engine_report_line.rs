//! The engine's `OPENSPORE-STAGED v1` report line is a **published interface**.
//!
//! It is the only machine-readable thing the engine emits, and a script that
//! reads it parses it by field name. So the two properties worth protecting are
//! narrow and checkable:
//!
//! 1. the line is **versioned** -- a consumer can tell which shape it got, and a
//!    change to the shape is a version bump rather than a silent break;
//! 2. the **field set is fixed** -- every field present, in one order, each with
//!    a value of the right shape. A field that quietly disappears or is renamed
//!    breaks a consumer with no error anywhere.
//!
//! # What this file is, and is not
//!
//! A **consumer**. It parses the line strictly and refuses anything that is not
//! exactly the documented shape -- which is what makes it worth having: it fails
//! on a missing field, a renamed field, a reordered field, a value of the wrong
//! shape, and a version bump, all of which `starts_with("OPENSPORE-STAGED v1")`
//! happily accepts.
//!
//! It is deliberately **not** a test of the engine's internals, and it asserts
//! nothing about the numbers: the mesh count, the triangle count and the bounds
//! are the engine's business. Only the *shape* is pinned.
//!
//! # The hermetic half and the corpus half
//!
//! The hermetic half feeds the parser a canonical line and every malformation of
//! it, so the parser cannot rot. It needs no game, no window and no GPU.
//!
//! The corpus half runs the engine binary and parses its real output. It is
//! `#[ignore]`d, and it takes the binary's path from **`OPENSPORE_ENGINE_BIN`**,
//! which it requires to be set: a binary found by searching `target/` is a trap,
//! because a stale one still runs and still prints a line, and the test would
//! then report a field-set failure that is really a build-order problem.
//!
//! ```text
//! OPENSPORE_ENGINE_BIN=target/release/openspore \
//!   cargo test -p spore-differential --release --test engine_report_line -- --ignored --nocapture
//! ```
//!
//! Without the variable it prints a `SKIP:` line and returns.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The version token. A change to the field set means a change to this number,
/// and this test fails when the line carries a different one.
const VERSION: &str = "v1";

/// The banner, exactly.
const PREFIX: &str = "OPENSPORE-STAGED";

/// The field set, **in order**. Order is part of the contract: a consumer that
/// splits on whitespace and indexes positionally is exactly the consumer this
/// protects.
const FIELDS: [&str; 10] = [
    "key",
    "package",
    "format",
    "meshes",
    "triangles",
    "bounds_min",
    "bounds_max",
    "normals",
    "textures",
    "decode",
];

/// One field's value, split so a consumer cannot accidentally accept a shape it
/// should refuse.
#[derive(Debug, Clone, PartialEq)]
struct Staged {
    key: String,
    package: String,
    format: String,
    meshes: u32,
    triangles: u32,
    bounds_min: [f64; 3],
    bounds_max: [f64; 3],
    normals: String,
    /// Resolved and attempted, from `textures=n/m`.
    textures: (u32, u32),
    decode: String,
}

impl Staged {
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        (self.bounds_min, self.bounds_max)
    }
}

fn err(line: &str, why: &str) -> String {
    format!("{why}\n  line: {line}")
}

/// Parses one `OPENSPORE-STAGED` line, or says exactly why it cannot.
///
/// Strict on purpose. Every arm below is a shape a lenient parser would accept
/// and a consumer would then mis-read.
fn parse(line: &str) -> Result<Staged, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let banner = format!("{PREFIX} {VERSION}");
    if !line.starts_with(&banner) {
        // Distinguish "wrong shape entirely" from "right shape, wrong version",
        // because only the second is a version bump this test can report.
        if line.starts_with(PREFIX) {
            return Err(err(
                line,
                "the line carries the banner but not the expected version token; a field-set \
                 change is a version bump",
            ));
        }
        return Err(err(
            line,
            "the line does not start with the expected banner",
        ));
    }
    let rest = line[banner.len()..].trim_start();
    if rest.is_empty() {
        return Err(err(line, "the line carries a banner and nothing else"));
    }

    // Split into `key=value` tokens. A bare token is a shape error: every field
    // is a key=value pair, so a positional field would be ambiguous.
    let mut tokens: Vec<(&str, &str)> = Vec::new();
    for token in rest.split_whitespace() {
        let Some((name, value)) = token.split_once('=') else {
            return Err(err(
                line,
                "every field must be `name=value`; a bare token would be ambiguous",
            ));
        };
        if value.is_empty() {
            return Err(err(line, "a field must carry a value, not an empty one"));
        }
        tokens.push((name, value));
    }

    let names: Vec<&str> = tokens.iter().map(|(name, _)| *name).collect();
    if names != FIELDS {
        return Err(err(
            line,
            &format!("field set is {names:?}, expected exactly {FIELDS:?} in that order"),
        ));
    }
    let map: BTreeMap<&str, &str> = tokens.iter().copied().collect();

    let number = |name: &str| -> Result<u32, String> {
        map[name]
            .parse::<u32>()
            .map_err(|_| err(line, &format!("`{name}` must be a non-negative integer")))
    };
    let triple = |name: &str| -> Result<[f64; 3], String> {
        let parts: Vec<&str> = map[name].split(',').collect();
        if parts.len() != 3 {
            return Err(err(
                line,
                &format!(
                    "`{name}` must be three comma-separated numbers, got {:?}",
                    map[name]
                ),
            ));
        }
        let mut out = [0.0f64; 3];
        for (slot, part) in out.iter_mut().zip(parts) {
            let value: f64 = part.parse().map_err(|_| {
                err(
                    line,
                    &format!("`{name}` component {part:?} is not a number"),
                )
            })?;
            if !value.is_finite() {
                return Err(err(
                    line,
                    &format!("`{name}` component {part:?} is not finite"),
                ));
            }
            *slot = value;
        }
        Ok(out)
    };

    let textures_parts: Vec<&str> = map["textures"].split('/').collect();
    if textures_parts.len() != 2 {
        return Err(err(
            line,
            &format!(
                "`textures` must be `resolved/attempted`, got {:?}",
                map["textures"]
            ),
        ));
    }
    let resolved = textures_parts[0].parse::<u32>().map_err(|_| {
        err(
            line,
            "`textures`'s resolved count must be a non-negative integer",
        )
    })?;
    let attempted = textures_parts[1].parse::<u32>().map_err(|_| {
        err(
            line,
            "`textures`'s attempted count must be a non-negative integer",
        )
    })?;
    // `0/0` means "we looked and there were none"; anything else must have
    // attempted at least what it resolved. A consumer distinguishes those two by
    // this pair, so the pair has to mean what it says.
    if resolved > attempted {
        return Err(err(
            line,
            "`textures` resolved more references than it attempted, which the two numbers cannot \
             mean",
        ));
    }

    Ok(Staged {
        key: map["key"].to_owned(),
        package: map["package"].to_owned(),
        format: map["format"].to_owned(),
        meshes: number("meshes")?,
        triangles: number("triangles")?,
        bounds_min: triple("bounds_min")?,
        bounds_max: triple("bounds_max")?,
        normals: map["normals"].to_owned(),
        textures: (resolved, attempted),
        decode: map["decode"].to_owned(),
    })
}

/// A canonical line, written out rather than produced by the engine, so the
/// parser is tested against the *contract* and not against whatever the engine
/// currently emits.
const CANONICAL: &str = "OPENSPORE-STAGED v1 key=0x00e6bce5:0x40637e03:0x067a07f0 \
                         package=Spore_Content format=gmdl meshes=1 triangles=20 \
                         bounds_min=-2.070057,0.139144,0.081329 \
                         bounds_max=-0.371524,2.164293,5.326164 normals=derived textures=0/0 \
                         decode=ok";

#[test]
fn a_canonical_line_parses_into_every_field() {
    let staged = parse(CANONICAL).expect("the canonical line must parse");
    assert_eq!(staged.key, "0x00e6bce5:0x40637e03:0x067a07f0");
    assert_eq!(staged.package, "Spore_Content");
    assert_eq!(staged.format, "gmdl");
    assert_eq!(staged.meshes, 1);
    assert_eq!(staged.triangles, 20);
    assert_eq!(
        staged.bounds(),
        (
            [-2.070057, 0.139144, 0.081329],
            [-0.371524, 2.164293, 5.326164]
        )
    );
    assert_eq!(staged.normals, "derived");
    assert_eq!(staged.textures, (0, 0));
    assert_eq!(staged.decode, "ok");
}

#[test]
fn zero_of_zero_textures_is_looked_at_and_none_found_not_a_failure() {
    // The two absences a consumer has to tell apart, and the reason `textures`
    // is a pair rather than a number.
    let none = parse(CANONICAL).expect("parses");
    assert_eq!(none.textures, (0, 0), "no references were requested");

    let failed = parse(&CANONICAL.replace("textures=0/0", "textures=0/2")).expect("parses");
    assert_eq!(
        failed.textures,
        (0, 2),
        "two were attempted and none resolved, which is a different fact"
    );
    let resolved = parse(&CANONICAL.replace("textures=0/0", "textures=2/2")).expect("parses");
    assert_eq!(resolved.textures, (2, 2));
}

#[test]
fn every_malformation_of_the_shape_is_refused_with_a_reason() {
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "a different version token",
            CANONICAL.replace(" v1 ", " v2 "),
            "version bump",
        ),
        (
            "no version token at all",
            CANONICAL.replace("OPENSPORE-STAGED v1 ", "OPENSPORE-STAGED "),
            "version bump",
        ),
        (
            "a renamed field",
            CANONICAL.replace("triangles=", "tris="),
            "field set",
        ),
        (
            "a missing field",
            CANONICAL.replace(" normals=derived", ""),
            "field set",
        ),
        (
            "a field appended without a version bump",
            format!("{CANONICAL} extra=1"),
            "field set",
        ),
        (
            "two fields swapped",
            CANONICAL.replace("meshes=1 triangles=20", "triangles=20 meshes=1"),
            "field set",
        ),
        (
            "a duplicated field",
            CANONICAL.replace("normals=derived", "normals=derived normals=derived"),
            "field set",
        ),
        (
            "a bare token",
            format!("{CANONICAL} trailing"),
            "name=value",
        ),
        (
            "an empty value",
            CANONICAL.replace("format=gmdl", "format="),
            "empty",
        ),
        (
            "a non-numeric mesh count",
            CANONICAL.replace("meshes=1", "meshes=one"),
            "non-negative integer",
        ),
        (
            "a negative triangle count",
            CANONICAL.replace("triangles=20", "triangles=-20"),
            "non-negative integer",
        ),
        (
            "a two-component bound",
            CANONICAL.replace(
                "bounds_min=-2.070057,0.139144,0.081329",
                "bounds_min=-2.070057,0.139144",
            ),
            "three comma-separated numbers",
        ),
        (
            "a non-numeric bound",
            CANONICAL.replace("bounds_max=-0.371524,2.164293,5.326164", "bounds_max=a,b,c"),
            "not a number",
        ),
        (
            "a bound that is not finite",
            CANONICAL.replace("0.139144", "NaN"),
            "not finite",
        ),
        (
            "an infinite bound",
            CANONICAL.replace("0.139144", "inf"),
            "not finite",
        ),
        (
            "textures without the pair",
            CANONICAL.replace("textures=0/0", "textures=0"),
            "resolved/attempted",
        ),
        (
            "textures resolved above attempted",
            CANONICAL.replace("textures=0/0", "textures=3/2"),
            "cannot mean",
        ),
        (
            "a banner and nothing else",
            "OPENSPORE-STAGED v1 ".to_owned(),
            "banner",
        ),
        ("an unrelated line", "all good".to_owned(), "banner"),
    ];
    for (what, line, expected) in cases {
        let error = match parse(&line) {
            Ok(parsed) => panic!("{what} was accepted as {parsed:?}"),
            Err(reason) => reason,
        };
        assert!(
            error.contains(expected),
            "{what}: the reason should mention {expected:?}, got:\n{error}"
        );
    }
}

#[test]
fn a_trailing_newline_is_tolerated_because_a_line_arrives_from_a_pipe() {
    let with_newline = format!("{CANONICAL}\n");
    assert_eq!(
        parse(&with_newline).expect("a trailing newline is not a shape change"),
        parse(CANONICAL).expect("parses")
    );
    assert_eq!(
        parse(&format!("{CANONICAL}\r\n")).expect("CRLF is still one line"),
        parse(CANONICAL).expect("parses")
    );
}

/// Runs the engine and parses its real output. **`#[ignore]`d.**
///
/// Requires `OPENSPORE_ENGINE_BIN`; see the module documentation for why it is
/// not discovered.
#[test]
#[ignore = "needs OPENSPORE_ENGINE_BIN pointing at a freshly built binary, and SPORE/"]
fn the_engine_prints_exactly_the_published_line_and_prints_it_the_same_way_twice() {
    let Some(binary) = std::env::var_os("OPENSPORE_ENGINE_BIN").map(PathBuf::from) else {
        println!(
            "SKIP: OPENSPORE_ENGINE_BIN is not set; point it at a freshly built `openspore` so a \
             stale binary cannot be mistaken for a shape change"
        );
        return;
    };
    if !binary.is_file() {
        println!("SKIP: {} is not a file", binary.display());
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize();
    let Ok(root) = root else {
        println!("SKIP: the workspace root could not be resolved");
        return;
    };
    let package = root.join("SPORE/Data/Spore_Content.package");
    if !package.exists() {
        println!("SKIP: SPORE/Data/Spore_Content.package is not installed");
        return;
    }

    let run = || {
        let output = std::process::Command::new(&binary)
            .args([
                "--info",
                "--preset",
                "documented-asset",
                "--package",
                package.to_str().expect("a utf-8 path"),
            ])
            .output()
            .unwrap_or_else(|error| panic!("{}: {error}", binary.display()));
        assert!(
            output.status.success(),
            "{} exited with {:?}:\n{}",
            binary.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find(|line| line.starts_with(PREFIX))
            .unwrap_or_else(|| {
                panic!(
                    "the engine printed no `{PREFIX}` line:\n{}",
                    String::from_utf8_lossy(&output.stdout)
                )
            })
            .to_owned()
    };

    let first = run();
    let staged = parse(&first)
        .unwrap_or_else(|error| panic!("the engine's own line did not parse: {error}\n{first}"));
    println!("parsed: {staged:?}");
    assert_eq!(
        staged.key, "0x00e6bce5:0x40637e03:0x067a07f0",
        "the documented asset's identity"
    );
    assert_eq!(staged.format, "gmdl");
    assert!(
        staged.meshes > 0,
        "a model with no meshes decoded to nothing"
    );
    assert!(
        staged.triangles > 0,
        "a model with no triangles decoded to nothing"
    );

    // Determinism: the same binary, the same package, the same line. A field
    // that varied between runs would be unusable in a comparison.
    let second = run();
    assert_eq!(
        first, second,
        "two runs of the same load printed different lines"
    );
}
