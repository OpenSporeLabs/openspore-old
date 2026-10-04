//! Argument parsing and the exit-code contract, end to end.
//!
//! Every subcommand, every flag, every documented error path, and the process
//! exit code each one produces. The parser itself is unit-tested next to its
//! code; this file checks the same surface through the public entry point, which
//! is where an error that is typed but never *mapped* to an exit code would hide.

mod common;

use common::{osptool, run_owned, TempFile, MINI_PACKAGE};
use spore_tools::{
    main_with, parse, CliError, Request, EXIT_DECODE, EXIT_IO, EXIT_NOT_FOUND, EXIT_OK, EXIT_USAGE,
};

/// A package that exists but is not a DBPF image, for the decode-failure path.
fn not_a_package() -> TempFile {
    TempFile::with("not-a-package", b"this is definitely not a DBPF index")
}

fn parse_error(args: &[&str]) -> CliError {
    parse(args.iter().map(|a| (*a).to_owned())).expect_err("the command line must be rejected")
}

#[test]
fn a_bare_invocation_is_a_usage_error_and_prints_usage_on_stderr() {
    let (code, out, err) = osptool(&[]);
    assert_eq!(code, EXIT_USAGE);
    assert!(out.is_empty(), "stdout must stay clean on a usage error");
    assert!(err.contains("USAGE:"), "{err}");
    assert!(err.contains("osptool:"), "the message is prefixed: {err}");
}

#[test]
fn help_exits_zero_on_stdout() {
    for spelling in ["-h", "--help", "help"] {
        let (code, out, err) = osptool(&[spelling]);
        assert_eq!(code, EXIT_OK, "{spelling}");
        assert!(out.contains("USAGE:"), "{spelling}: {out}");
        assert!(err.is_empty(), "{spelling}: {err}");
    }
}

#[test]
fn usage_documents_the_exit_codes_it_actually_returns() {
    let (_code, out, _err) = osptool(&["--help"]);
    for line in [
        "0  ok",
        "2  bad command line",
        "3  record not found, or no package loaded",
        "4  a record or package was found but the container layer refused it",
        "5  an I/O failure",
    ] {
        assert!(out.contains(line), "usage must document `{line}`");
    }
}

#[test]
fn an_unknown_subcommand_exits_two() {
    let (code, _out, err) = osptool(&["frobnicate", "x"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("unknown subcommand `frobnicate`"), "{err}");
}

#[test]
fn every_subcommand_parses_its_documented_form() {
    let cases: &[(&[&str], &str)] = &[
        (&["list", MINI_PACKAGE], "list"),
        (&["list", MINI_PACKAGE, "--type", "0x58545354"], "list"),
        (&["list", MINI_PACKAGE, "--group", "0x11111111"], "list"),
        (&["list", MINI_PACKAGE, "--limit", "0"], "list"),
        (&["list", MINI_PACKAGE, "--json"], "list"),
        (&["list", MINI_PACKAGE, "--names"], "list"),
        (
            &["find", MINI_PACKAGE, "0x58545354:0x11111111:0x22222222"],
            "find",
        ),
        (
            &[
                "find",
                MINI_PACKAGE,
                "0x58545354:0x11111111:0x22222222",
                "--json",
            ],
            "find",
        ),
        (
            &["describe", MINI_PACKAGE, "0x58545354:0x11111111:0x22222222"],
            "describe",
        ),
        (&["manifest", MINI_PACKAGE], "manifest"),
        (&["manifest", MINI_PACKAGE, "--stats"], "manifest"),
        (&["manifest", MINI_PACKAGE, "--json"], "manifest"),
        (&["verify", MINI_PACKAGE], "verify"),
        (&["verify", MINI_PACKAGE, "--type", "0x58545354"], "verify"),
        (&["types"], "types"),
        (&["types", "--group"], "types"),
        (&["types", "--package", MINI_PACKAGE], "types"),
        (
            &[
                "texture",
                MINI_PACKAGE,
                "0x58545354:0x11111111:0x22222222",
                "--out",
                "/tmp/osptool-cli.png",
            ],
            "texture",
        ),
        (
            &[
                "texture",
                MINI_PACKAGE,
                "0x58545354:0x11111111:0x22222222",
                "--out",
                "/tmp/osptool-cli.png",
                "--layer",
                "1",
                "--mip",
                "2",
                "--channels",
                "stored",
                "--json",
            ],
            "texture",
        ),
        (
            &[
                "graph",
                "0x58545354:0x11111111:0x22222222",
                "--package",
                MINI_PACKAGE,
            ],
            "graph",
        ),
        (
            &[
                "graph",
                "0x58545354:0x11111111:0x22222222",
                "--package",
                MINI_PACKAGE,
                "--package",
                MINI_PACKAGE,
                "--depth",
                "3",
                "--kinds",
                "gmdl->texture,group-sibling",
                "--dependants",
                "--limit",
                "0",
                "--json",
            ],
            "graph",
        ),
        (&["stats", MINI_PACKAGE], "stats"),
        (
            &[
                "stats",
                MINI_PACKAGE,
                "--type",
                "0x58545354",
                "--limit",
                "0",
                "--json",
            ],
            "stats",
        ),
        (&["diff", MINI_PACKAGE, MINI_PACKAGE], "diff"),
        (
            &[
                "diff",
                MINI_PACKAGE,
                MINI_PACKAGE,
                "--bytes",
                "--limit",
                "0",
                "--json",
            ],
            "diff",
        ),
        (
            &[
                "verify",
                MINI_PACKAGE,
                "--explain",
                "0x58545354:0x11111111:0x22222222",
            ],
            "verify",
        ),
    ];
    for (args, expected) in cases {
        let request = parse(args.iter().map(|a| (*a).to_owned()))
            .unwrap_or_else(|error| panic!("{args:?}: {error}"));
        let actual = match request {
            Request::List(_) => "list",
            Request::Find(_) => "find",
            Request::Describe(_) => "describe",
            Request::Extract(_) => "extract",
            Request::Manifest(_) => "manifest",
            Request::Verify(_) => "verify",
            Request::Types(_) => "types",
            Request::Texture(_) => "texture",
            Request::Graph(_) => "graph",
            Request::Stats(_) => "stats",
            Request::Diff(_) => "diff",
            Request::Help => "help",
        };
        assert_eq!(actual, *expected, "{args:?}");
    }
}

#[test]
fn every_documented_error_path_is_a_typed_parse_error() {
    // A missing flag value.
    for flag in ["--type", "--group", "--limit", "--out", "--package"] {
        let args: Vec<&str> = match flag {
            "--out" => vec!["extract", "p", "1:2:3", flag],
            "--package" => vec!["types", flag],
            "--limit" => vec!["list", "p", flag],
            _ => vec!["list", "p", flag],
        };
        assert_eq!(
            parse_error(&args),
            CliError::MissingValue {
                flag: flag.to_owned()
            },
            "{args:?}"
        );
    }
    // An unknown flag, per subcommand.
    for (command, extra) in [
        ("list", vec!["p"]),
        ("find", vec!["p", "1:2:3"]),
        ("describe", vec!["p", "1:2:3"]),
        ("extract", vec!["p", "1:2:3"]),
        ("manifest", vec!["p"]),
        ("verify", vec!["p"]),
        ("types", vec![]),
        ("stats", vec!["p"]),
        ("diff", vec!["p", "q"]),
        ("graph", vec!["1:2:3"]),
        ("texture", vec!["p", "1:2:3", "--out", "/tmp/x.png"]),
    ] {
        let mut args = vec![command];
        args.extend(extra);
        args.push("--turbo");
        assert_eq!(
            parse_error(&args),
            CliError::UnknownFlag {
                command,
                flag: "--turbo".into()
            },
            "{args:?}"
        );
    }
    // A number that is not a number.
    assert!(matches!(
        parse_error(&["list", "p", "--limit", "lots"]),
        CliError::BadValue { ref flag, .. } if flag == "--limit"
    ));
    assert!(matches!(
        parse_error(&["list", "p", "--type", "0xzzzz"]),
        CliError::BadValue { ref flag, .. } if flag == "--type"
    ));
    assert!(matches!(
        parse_error(&["list", "p", "--group", ""]),
        CliError::BadValue { ref flag, .. } if flag == "--group"
    ));
    // A triple that is not a triple.
    for command in ["find", "describe", "extract", "graph", "verify"] {
        for bad in ["1:2", "1:2:3:4", "a:b:c", "", "0x1:0x2:"] {
            // `graph` takes the identity first and its packages by flag; the
            // rest take a package first and the identity second. `verify`'s
            // identity arrives on `--explain`.
            let mut args = match command {
                "graph" => vec![command],
                _ => vec![command, "p"],
            };
            if command == "verify" {
                args.push("--explain");
            }
            args.push(bad);
            if command == "extract" {
                args.extend(["--out", "/tmp/whatever"]);
            }
            if command == "graph" {
                args.extend(["--package", "p"]);
            }
            assert!(
                matches!(parse_error(&args), CliError::BadValue { .. }),
                "{command} accepted `{bad}`"
            );
        }
    }
    // Too many positionals.
    assert!(matches!(
        parse_error(&["list", "a", "b"]),
        CliError::Positional { given: 2, .. }
    ));
    assert!(matches!(
        parse_error(&["verify", "a", "b"]),
        CliError::Positional { given: 2, .. }
    ));
    // A repeated single-valued flag.
    assert_eq!(
        parse_error(&["types", "--package", "a", "--package", "b"]),
        CliError::Repeated {
            flag: "--package".into()
        }
    );
    // `manifest` insists on at least one package.
    assert!(matches!(
        parse_error(&["manifest"]),
        CliError::Positional { given: 0, .. }
    ));
}

#[test]
fn extract_refuses_an_out_dir_that_could_write_outside_it() {
    for refused in ["/", ".", "..", "out/.."] {
        let error = parse_error(&["extract", "p", "1:2:3", "--out", refused]);
        assert_eq!(error, CliError::UnsafeOutDir(refused.to_owned()));
        assert!(error.to_string().contains("refused"), "{error}");
        // And it never reaches the filesystem.
        let mut args = vec!["extract".to_owned(), "p".to_owned(), "1:2:3".to_owned()];
        args.push("--out".to_owned());
        args.push(refused.to_owned());
        let (code, _out, err) = run_owned(args);
        assert_eq!(code, EXIT_USAGE, "{err}");
    }
}

#[test]
fn a_missing_package_file_is_an_io_failure() {
    for command in [
        vec!["list", "/nonexistent/nope.package"],
        vec!["describe", "/nonexistent/nope.package", "1:2:3"],
        vec!["find", "/nonexistent/nope.package", "1:2:3"],
        vec![
            "extract",
            "/nonexistent/nope.package",
            "1:2:3",
            "--out",
            "/tmp/osptool-x",
        ],
        vec!["verify", "/nonexistent/nope.package"],
        vec!["types", "--package", "/nonexistent/nope.package"],
        vec!["manifest", "/nonexistent/nope.package"],
    ] {
        let (code, _out, err) = osptool(&command);
        assert_eq!(code, EXIT_IO, "{command:?}: {err}");
    }
}

#[test]
fn a_file_that_is_not_a_package_is_a_decode_failure() {
    let file = not_a_package();
    let path = file.path().display().to_string();
    for command in [
        vec!["list", &path],
        vec!["describe", &path, "1:2:3"],
        vec!["verify", &path],
        vec!["manifest", &path],
    ] {
        let (code, _out, err) = osptool(&command);
        assert_eq!(code, EXIT_DECODE, "{command:?}: {err}");
    }
}

#[test]
fn a_missed_record_is_exit_three_and_names_the_record() {
    let (code, out, err) = osptool(&["find", MINI_PACKAGE, "0xdeadbeef:0x11111111:0x22222222"]);
    assert_eq!(code, EXIT_NOT_FOUND, "stdout was: {out}");
    assert!(err.contains("0xdeadbeef:0x11111111:0x22222222"), "{err}");

    let (code, _out, err) =
        osptool(&["describe", MINI_PACKAGE, "0x58545354:0x11111111:0x99999999"]);
    assert_eq!(code, EXIT_NOT_FOUND, "{err}");
}

#[test]
fn a_miss_with_a_known_type_offers_the_candidates_the_store_computed() {
    // Right type, wrong instance: the store can suggest, and `find` prints the
    // suggestion rather than reimplementing the search.
    let (code, _out, err) = osptool(&["find", MINI_PACKAGE, "0x58545354:0x11111111:0xdeadbeef"]);
    assert_eq!(code, EXIT_NOT_FOUND);
    assert!(err.contains("near-miss candidates"), "{err}");
    assert!(err.contains("0x58545354:0x11111111:0x22222222"), "{err}");
}

#[test]
fn a_miss_with_an_unknown_type_says_it_has_no_candidates_rather_than_printing_none() {
    let (code, _out, err) = osptool(&["find", MINI_PACKAGE, "0xdeadbeef:1:2"]);
    assert_eq!(code, EXIT_NOT_FOUND);
    assert!(err.contains("no near-miss candidates"), "{err}");
}

#[test]
fn a_usable_command_line_and_a_present_record_always_exits_zero() {
    // `describe` on a type with no decoder is a complete answer, not a failure.
    let (code, out, _err) =
        osptool(&["describe", MINI_PACKAGE, "0x58545354:0x11111111:0x22222222"]);
    assert_eq!(code, EXIT_OK, "{out}");
}

#[test]
fn main_with_and_run_agree_on_the_exit_code() {
    let args: Vec<String> = ["describe", "/nonexistent/x.package", "1:2:3"]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = main_with(args, &mut out, &mut err);
    assert_eq!(code, EXIT_IO);
    // `run` returns the typed error whose code produced that number.
    let error = spore_tools::run(
        ["describe", "/nonexistent/x.package", "1:2:3"].map(str::to_owned),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect_err("the package does not exist");
    assert_eq!(spore_tools::ToolError::code(&error), code);
}
