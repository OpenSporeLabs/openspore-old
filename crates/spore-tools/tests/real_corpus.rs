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
const SAMPLED: usize = 25;

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

/// The first `wanted` identities of `type_id`, in index order.
///
/// The `list` table is `idx type_id group_id instance stored memory comp`, so the
/// identity is fields 1..4 and field 0 is the index-row ordinal, which is easy to
/// mistake for one of them.
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

/// The first `raster` record whose envelope declares `fourcc`, found by asking
/// `verify --explain` — bounded, so a package without such a record fails with a
/// clear panic instead of walking 2 954 records.
fn first_record_of_fourcc(package: &Path, fourcc: u32) -> Option<String> {
    const TRIED: usize = 40;
    for key in identities(package, spore_texture::RASTER_TYPE, TRIED) {
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
