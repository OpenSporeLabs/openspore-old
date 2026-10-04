//! Read-only checks for `texture`, `graph`, `stats`, `diff` and
//! `verify --explain` against the installed game data.
//!
//! Skipped by default and returning early when the package is absent, exactly as
//! `real_package.rs` does. Nothing here writes to the package or to `SPORE/`;
//! `texture` writes only to a temporary directory outside the repository.
//!
//! ```text
//! cargo test -p spore-tools --test real_corpus -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

use spore_tools::{json, EXIT_OK};

/// How many failing records a bulk check explains before it stops.
///
/// The point is to prove the explanation agrees with the tally on a sample, not
/// to re-walk the corpus one record at a time.
const SAMPLED: usize = 40;

fn content_package() -> Option<PathBuf> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SPORE/Data/Spore_Content.package");
    if path.exists() {
        Some(path)
    } else {
        println!("skipped: {} is not installed", path.display());
        None
    }
}

fn run(args: Vec<String>) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = spore_tools::main_with(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf-8"),
        String::from_utf8(err).expect("stderr is utf-8"),
    )
}

/// A temporary directory outside this repository, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let mut path = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!(
            "osptool-corpus-{}-{unique}-{tag}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every identity of `type_id`, in index order.
///
/// `limit` `0` means every row. A prefix would be a bad sample: the first 40
/// raster records of `Spore_Content.package` are all DXT5, and the formats are not
/// interleaved in index order, so a prefix search for a DXT1 record can never
/// find one however far it is extended.
fn identities(package: &Path, type_id: u32, wanted: usize) -> Vec<String> {
    let (code, out, err) = run(vec![
        "list".to_owned(),
        package.display().to_string(),
        "--type".to_owned(),
        format!("{type_id:#010x}"),
        "--limit".to_owned(),
        wanted.to_string(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    identity_rows(&out)
}

/// The identities in a `list` table, which is
/// `idx type_id group_id instance stored memory comp`.
fn identity_rows(out: &str) -> Vec<String> {
    out.lines()
        .skip(2)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return None;
            }
            Some(format!("{}:{}:{}", parts[1], parts[2], parts[3]))
        })
        .collect()
}

/// A bounded, spread-out sample of `type_id`'s identities.
///
/// `[`first_record_of_fourcc`] probes at most `SAMPLED` records, so the sample has
/// to cover the whole type rather than its first `SAMPLED` rows: the raster
/// records are grouped by format in index order, so a prefix finds DXT5 and
/// nothing else.
fn sampled_identities(package: &Path, type_id: u32) -> Vec<String> {
    let all = identities(package, type_id, 0);
    if all.len() <= SAMPLED {
        return all;
    }
    let stride = all.len().div_ceil(SAMPLED);
    println!(
        "probing {} of {type_id:#010x}'s {} record(s), one every {stride}",
        all.len().div_ceil(stride),
        all.len()
    );
    all.into_iter().step_by(stride).collect()
}

/// The first record of `type_id` in the package.
fn first_record_of_type(package: &Path, type_id: u32) -> Option<String> {
    identities(package, type_id, 1).into_iter().next()
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn the_015_family_writes_a_png_whose_channels_are_the_swizzled_ones() {
    let Some(package) = content_package() else {
        return;
    };
    // The decisive corpus checks are about the `0x15` family, so this test needs
    // a record *of that family* rather than whichever raster comes first in the
    // index (the first is DXT5). `verify --explain` is how a record's fourcc is
    // read, which is the same thing the tool is being tested on.
    let Some(key) = first_record_of_fourcc(&package, spore_texture::LUMINANCE_FOURCC) else {
        panic!("the package must hold 0x15 raster records");
    };
    let scratch = Scratch::new("texture");
    let out = scratch.file("normal.png");
    let (code, stdout, stderr) = run(vec![
        "texture".to_owned(),
        package.display().to_string(),
        key.clone(),
        "--out".to_owned(),
        out.display().to_string(),
        "--json".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    let document = json::parse(&stdout).expect("valid JSON");
    let json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("no key {name}"))
    };
    println!(
        "{key}: fourcc {}",
        match get("fourcc") {
            json::Json::Str(s) => s,
            _ => "?".into(),
        }
    );
    assert_eq!(
        get("fourcc"),
        json::Json::from(format!("{:#010x}", spore_texture::LUMINANCE_FOURCC))
    );
    assert_eq!(
        get("channels"),
        json::Json::from("swizzled_from_zxy"),
        "the 0x15 family must be swizzled by default"
    );

    // The file must open, and its pixel count must match the reported geometry.
    let bytes = std::fs::read(&out).expect("the png must exist");
    let decoded = spore_tools::png::verify(&bytes).expect("the file must be a valid PNG");
    assert_eq!(get("width"), json::Json::from(decoded.width));
    assert_eq!(get("height"), json::Json::from(decoded.height));
    assert_eq!(
        decoded.pixels.len(),
        (decoded.width * decoded.height * 4) as usize
    );

    // The decisive check: lane 0 is Z and it is high in a normal map, so the
    // written image's BLUE must carry that high value while RED stays near the
    // middle. A missing swizzle puts Z in red, and this fails on most texels.
    let mut blue_high = 0usize;
    for texel in decoded.pixels.chunks_exact(4) {
        if texel[2] > texel[0] {
            blue_high += 1;
        }
    }
    let total = (decoded.width * decoded.height) as usize;
    assert!(
        blue_high * 2 > total,
        "a tangent-space normal map has Z high in most texels, so BLUE must dominate RED after \
         the swizzle: {blue_high} of {total} texels"
    );
    println!("  {blue_high}/{total} texels have B > R after the swizzle");

    // And the unestablished question must still be carried as unavailable.
    let json::Json::Obj(fields) = get("swizzle_should_be_applied") else {
        panic!("expected an object")
    };
    assert!(fields
        .iter()
        .any(|(k, v)| k == "state" && *v == json::Json::from("unavailable")));
    assert!(!fields.iter().any(|(k, _)| k == "value"));

    // `--channels stored` writes the file order unchanged: the same record, the
    // same dimensions, the high lane in RED.
    let raw = scratch.file("stored.png");
    let (code, _stdout, stderr) = run(vec![
        "texture".to_owned(),
        package.display().to_string(),
        key.clone(),
        "--out".to_owned(),
        raw.display().to_string(),
        "--channels".to_owned(),
        "stored".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    let raw_decoded = spore_tools::png::verify(&std::fs::read(&raw).unwrap()).expect("valid PNG");
    let mut red_high = 0usize;
    for texel in raw_decoded.pixels.chunks_exact(4) {
        if texel[0] > texel[2] {
            red_high += 1;
        }
    }
    assert!(
        red_high * 2 > total,
        "the stored order puts the high lane in RED: {red_high} of {total} texels"
    );
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn every_raster_record_in_the_corpus_decodes_and_the_census_says_so() {
    // The finding that refuted the two-layer reading, re-checked through the
    // tool rather than through a probe: over the 2954 raster records of
    // `Spore_Content.package` the framing identity closes on every one, `field_18`
    // is the base level's byte count on every one, and every record decodes.
    let Some(package) = content_package() else {
        return;
    };
    let (code, out, err) = run(vec![
        "stats".to_owned(),
        package.display().to_string(),
        "--type".to_owned(),
        format!("{:#010x}", spore_texture::RASTER_TYPE),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");

    let walked: i64 = out
        .lines()
        .find_map(|line| line.strip_prefix("records walked "))
        .and_then(|value| value.trim().parse().ok())
        .expect("a walked count");

    let section_rows = |title: &str, next: &str| -> Vec<(u64, String)> {
        let body = out
            .split_once(title)
            .unwrap_or_else(|| panic!("no `{title}` section in:\n{out}"))
            .1;
        let body = body.split_once(next).map_or(body, |(head, _)| head);
        body.lines()
            .filter_map(|line| {
                let rest = line.trim_start();
                let count: u64 = rest.split_whitespace().next()?.parse().ok()?;
                Some((count, rest[count.to_string().len()..].trim().to_owned()))
            })
            .collect()
    };

    let fourcc = section_rows("raster fourcc", "raster geometry");
    let decoded: u64 = fourcc
        .iter()
        .filter(|(_, label)| label.ends_with("(decoded)"))
        .map(|(count, _)| *count)
        .sum();
    assert_eq!(
        decoded, walked as u64,
        "every format in the corpus decodes, so the fourcc census must not hide a refusal: \
         {fourcc:?}"
    );
    for (_, label) in &fourcc {
        assert!(
            !label.contains("refus"),
            "a decodable format labelled refused is a confident falsehood: {label}"
        );
    }
    // The block sizes, which are the numbers the refuted reading got wrong.
    assert!(
        fourcc.iter().any(
            |(_, label)| label.contains("DXT5/BC3") && label.contains("16 bytes per 4x4 block")
        ),
        "{fourcc:?}"
    );
    assert!(
        fourcc
            .iter()
            .any(|(_, label)| label.contains("DXT1/BC1") && label.contains("8 bytes per 4x4 block")),
        "{fourcc:?}"
    );

    // The framing cross-check: two axes, both closed on every record.
    let framing = section_rows("raster framing", "decodes  (one full decode");
    assert_eq!(
        framing
            .iter()
            .filter(|(_, label)| label == "record length: exact")
            .map(|(count, _)| *count)
            .sum::<u64>(),
        walked as u64,
        "the framing identity must close on every raster record: {framing:?}"
    );
    assert_eq!(
        framing
            .iter()
            .filter(|(_, label)| label == "field_18: the base level's byte count")
            .map(|(count, _)| *count)
            .sum::<u64>(),
        walked as u64,
        "field_18 must be the base level's own byte count in all three formats: {framing:?}"
    );

    // No layer count anywhere: the refuted premise.
    let geometry = section_rows("raster geometry", "raster framing");
    assert!(
        geometry.iter().all(|(_, label)| !label.contains("layer")),
        "{geometry:?}"
    );
    for (count, label) in &fourcc {
        println!("  {count:>5}  {label}");
    }
    println!("{walked} raster record(s): framing and field_18 both close on all of them");
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn a_dxt1_record_in_the_corpus_decodes_as_bc1_and_is_written_verbatim() {
    // The corpus holds 306 DXT1 records and this build used to name the format
    // "present in the corpus, refused by name". This drives the whole command line
    // on a real one: a 256x256 BC1 record is a colour image, it is written exactly
    // as decoded, and its pixels are not the DXT5 alpha-mask signature.
    let Some(package) = content_package() else {
        return;
    };
    let Some(key) = first_record_of_fourcc(&package, spore_texture::DXT1_FOURCC) else {
        panic!("the package must hold DXT1 raster records");
    };
    println!("{key} is a DXT1 record");
    let scratch = Scratch::new("dxt1");
    let out = scratch.file("dxt1.png");
    let (code, stdout, stderr) = run(vec![
        "texture".to_owned(),
        package.display().to_string(),
        key.clone(),
        "--out".to_owned(),
        out.display().to_string(),
        "--json".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    let document = json::parse(&stdout).expect("valid JSON");
    let json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("no key {name}"))
    };
    assert_eq!(get("fourcc"), json::Json::from("0x31545844"));
    assert_eq!(
        get("channels"),
        json::Json::from("stored"),
        "a block format is never swizzled"
    );

    let bytes = std::fs::read(&out).expect("the png must exist");
    let decoded = spore_tools::png::verify(&bytes).expect("the file must be a valid PNG");
    assert_eq!(get("width"), json::Json::from(decoded.width));
    assert_eq!(get("height"), json::Json::from(decoded.height));
    assert_eq!(
        decoded.pixels.len(),
        (decoded.width * decoded.height * 4) as usize
    );

    // BC1 has no alpha channel, so every texel is opaque. That alone separates
    // this decode from the DXT5 alpha-mask signature, and it is checkable without
    // deciding what the picture depicts.
    let opaque = decoded
        .pixels
        .chunks_exact(4)
        .filter(|texel| texel[3] == 255)
        .count();
    let total = (decoded.width * decoded.height) as usize;
    assert_eq!(
        opaque, total,
        "BC1 carries no alpha, so every texel must be written A=255"
    );
    // And it is a real colour image, not a two-value mask: a synthetic ramp would
    // be one texel repeated, and this decode is thousands of distinct values.
    let mut distinct = std::collections::HashSet::new();
    for texel in decoded.pixels.chunks_exact(4) {
        distinct.insert([texel[0], texel[1], texel[2], texel[3]]);
    }
    println!(
        "{key}: {total} texels, {} distinct RGBA values",
        distinct.len()
    );
    assert!(
        distinct.len() > 64,
        "a BC1 colour image is not a two-value mask: {} distinct values",
        distinct.len()
    );

    // `--channels stored` must be byte-identical for a format that is never
    // swizzled.
    let raw = scratch.file("dxt1-stored.png");
    let (code, _stdout, stderr) = run(vec![
        "texture".to_owned(),
        package.display().to_string(),
        key.clone(),
        "--out".to_owned(),
        raw.display().to_string(),
        "--channels".to_owned(),
        "stored".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    assert_eq!(
        std::fs::read(&raw).unwrap(),
        bytes,
        "`auto` and `stored` are the same bytes for a BC1 record"
    );
}

/// A record of `type_id` whose envelope declares `fourcc`, found by asking
/// `verify --explain` over a bounded, spread-out sample — so a format this package
/// holds is always findable, and a package without such a record fails with a clear
/// panic instead of walking every raster record.
fn first_record_of_fourcc(package: &Path, fourcc: u32) -> Option<String> {
    for key in sampled_identities(package, spore_texture::RASTER_TYPE) {
        let (code, out, err) = run(vec![
            "verify".to_owned(),
            package.display().to_string(),
            "--explain".to_owned(),
            key.clone(),
        ]);
        assert_eq!(code, EXIT_OK, "{key}: {err}");
        if out.contains(&format!("fourcc={fourcc:#010x}")) {
            return Some(key);
        }
    }
    None
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn stats_measures_the_fourcc_census_and_the_gmdl_walk_outcomes() {
    let Some(package) = content_package() else {
        return;
    };
    let (code, out, err) = run(vec![
        "stats".to_owned(),
        package.display().to_string(),
        "--json".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    let document = json::parse(&out).expect("valid JSON on stdout");
    let json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let array = |name: &str| -> Vec<json::Json> {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, value)| match value {
                json::Json::Arr(rows) => Some(rows.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no array {name}"))
    };

    // The fourcc census must cover every raster record in the package: the sum of
    // its rows is the raster count in the type histogram.
    let raster_type_count: i64 = array("types")
        .iter()
        .filter_map(|row| {
            let json::Json::Obj(fields) = row else {
                return None;
            };
            let is_raster = fields.iter().any(|(k, v)| {
                k == "type_id"
                    && *v == json::Json::from(format!("{:#010x}", spore_texture::RASTER_TYPE))
            });
            let count = fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    json::Json::Int(n) => Some(*n),
                    _ => None,
                })?;
            is_raster.then_some(count)
        })
        .sum();
    let fourcc_total: i64 = array("raster_fourcc")
        .iter()
        .filter_map(|row| {
            let json::Json::Obj(fields) = row else {
                return None;
            };
            fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    json::Json::Int(n) => Some(*n),
                    _ => None,
                })
        })
        .sum();
    assert_eq!(
        fourcc_total, raster_type_count,
        "the fourcc census must cover exactly the raster records"
    );

    // The gmdl walk section must account for every gmdl record too.
    let gmdl_type_count: i64 = array("types")
        .iter()
        .filter_map(|row| {
            let json::Json::Obj(fields) = row else {
                return None;
            };
            let is_gmdl = fields.iter().any(|(k, v)| {
                k == "type_id" && *v == json::Json::from(format!("{:#010x}", spore_gmdl::GMDL_TYPE))
            });
            let count = fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    json::Json::Int(n) => Some(*n),
                    _ => None,
                })?;
            is_gmdl.then_some(count)
        })
        .sum();
    let walk_total: i64 = array("gmdl_walk")
        .iter()
        .filter_map(|row| {
            let json::Json::Obj(fields) = row else {
                return None;
            };
            fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    json::Json::Int(n) => Some(*n),
                    _ => None,
                })
        })
        .sum();
    assert_eq!(
        walk_total, gmdl_type_count,
        "every gmdl record must land in exactly one walk-outcome row"
    );
    println!("raster records: {raster_type_count}, gmdl records: {gmdl_type_count}");
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn graph_is_deterministic_over_the_whole_content_package() {
    let Some(package) = content_package() else {
        return;
    };
    let Some(key) = first_record_of_type(&package, spore_gmdl::GMDL_TYPE) else {
        panic!("the package must hold gmdl records");
    };
    let args = vec![
        "graph".to_owned(),
        key.clone(),
        "--package".to_owned(),
        package.display().to_string(),
        "--kinds".to_owned(),
        "gmdl->reference".to_owned(),
    ];
    let (first_code, first, first_err) = run(args.clone());
    let (second_code, second, second_err) = run(args);
    assert_eq!(first_code, EXIT_OK, "{first_err}");
    assert_eq!(second_code, EXIT_OK, "{second_err}");
    assert_eq!(first, second, "two runs must print the same bytes");
    assert!(first.contains("records       "), "{first}");
    assert!(first.contains("dangling references"), "{first}");
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package"]
fn explain_agrees_with_the_bulk_tally_for_every_failing_raster_and_gmdl_record() {
    // The whole point of `--explain`: bulk counts are cheap, so the check is that
    // every failing record can be named, and that naming it produces the same
    // verdict the tally counted.
    let Some(package) = content_package() else {
        return;
    };
    for type_id in [spore_texture::RASTER_TYPE, spore_gmdl::GMDL_TYPE] {
        let (code, tally, err) = run(vec![
            "verify".to_owned(),
            package.display().to_string(),
            "--type".to_owned(),
            format!("{type_id:#010x}"),
        ]);
        assert_eq!(code, EXIT_OK, "{err}");
        let failures: i64 = tally
            .lines()
            .find_map(|line| line.strip_prefix("failed         "))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or_else(|| panic!("no failure count for {type_id:#010x}: {tally}"));
        println!("{:#010x}: {failures} failing record(s)", type_id);
        if failures == 0 {
            continue;
        }
        // Name one failing record and explain it. The identity comes from the
        // graph's dangling list, which is already grouped by kind, so this does
        // not need a second corpus walk.
        for key in identities(&package, type_id, failures.min(SAMPLED as i64) as usize) {
            let (code, out, err) = run(vec![
                "verify".to_owned(),
                package.display().to_string(),
                "--explain".to_owned(),
                key.clone(),
            ]);
            assert_eq!(code, EXIT_OK, "{key}: {err}");
            assert!(
                out.contains("verdict      failed") || out.contains("verdict      decoded"),
                "{key}: every record has a verdict"
            );
        }
    }
}

#[test]
#[ignore = "needs two packages under SPORE/Data"]
fn diff_finds_the_two_revisions_of_the_globals_record_in_one_install() {
    // The stock install holds two revisions of one identity (264 bytes base,
    // 276 patched). `diff` between the same package and itself must report the
    // pair, which a first-row-wins comparison cannot see.
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SPORE/Data");
    if !data.exists() {
        println!("skipped: {} is not installed", data.display());
        return;
    }
    let mut packages: Vec<PathBuf> = std::fs::read_dir(&data)
        .expect("SPORE/Data must be readable")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "package"))
        .collect();
    packages.sort();
    assert!(
        packages.len() >= 2,
        "the stock install ships several packages; found {:?}",
        packages
    );
    let (code, out, err) = run(vec![
        "diff".to_owned(),
        packages[0].display().to_string(),
        packages[packages.len() - 1].display().to_string(),
        "--limit".to_owned(),
        "10".to_owned(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    println!("{}", out.lines().take(12).collect::<Vec<_>>().join("\n"));
    assert!(out.contains("comparison     index rows only"), "{out}");
}
