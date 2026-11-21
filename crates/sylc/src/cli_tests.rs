use super::*;

fn parse(args: &[&str]) -> Result<CliInvocation, String> {
    SylcApp::try_parse_from(args)
        .map_err(|e| e.to_string())?
        .into_invocation()
}

#[test]
fn legacy_and_subcommand_invocations_share_global_options() {
    for args in [
        vec!["sylc", "a.syl"],
        vec!["sylc", "--out", "o.sv", "--std-root", "s", "a.syl"],
        vec!["sylc", "check", "a.syl"],
        vec!["sylc", "check", "--manifest", "m.toml"],
        vec!["sylc", "--manifest", "m.toml", "check"],
        vec![
            "sylc",
            "emit",
            "--out",
            "o.sv",
            "--package-root",
            "p",
            "a.syl",
        ],
        vec!["sylc", "--diagnostics-format", "json", "check", "a.syl"],
        vec!["sylc", "--out", "o.sv", "emit", "a.syl"],
    ] {
        assert!(parse(&args).is_ok(), "{args:?}: {:?}", parse(&args));
    }
    let legacy = parse(&["sylc", "--out", "o.sv", "--std-root", "s", "a.syl"]).unwrap();
    assert_eq!(
        legacy.mode,
        RunMode::Emit {
            out_path: Some("o.sv".into())
        }
    );
    assert_eq!(legacy.inputs, [PathBuf::from("a.syl")]);
    assert_eq!(legacy.common.std_roots, [PathBuf::from("s")]);
    let check = parse(&[
        "sylc",
        "--std-root",
        "first",
        "check",
        "--std-root",
        "second",
        "--package-root",
        "p",
        "--manifest",
        "m.toml",
        "--diagnostics-format",
        "json",
    ])
    .unwrap();
    assert_eq!(check.mode, RunMode::Check);
    assert_eq!(
        check.common.std_roots,
        [PathBuf::from("first"), PathBuf::from("second")]
    );
    assert_eq!(check.common.package_roots, [PathBuf::from("p")]);
    assert_eq!(check.common.manifest, Some("m.toml".into()));
    assert_eq!(check.common.diagnostics_format, DiagnosticsFormat::Json);
}

#[test]
fn invalid_output_and_positional_combinations_are_rejected() {
    for args in [
        vec!["sylc", "check", "--out", "o.sv"],
        vec!["sylc", "--out", "o.sv", "check"],
        vec!["sylc", "--out", "a.sv", "emit", "--out", "b.sv"],
        vec!["sylc", "a.syl", "check"],
        vec!["sylc", "a.syl", "emit"],
        vec!["sylc", "check", "--diagnostics-format", "yaml"],
    ] {
        assert!(parse(&args).is_err(), "accepted {args:?}");
    }
}

#[test]
fn shared_options_appear_in_every_help_screen() {
    for args in [
        vec!["sylc", "--help"],
        vec!["sylc", "check", "--help"],
        vec!["sylc", "emit", "--help"],
    ] {
        let help = SylcApp::try_parse_from(args).unwrap_err();
        assert_eq!(help.kind(), clap::error::ErrorKind::DisplayHelp);
        let help = help.to_string();
        for flag in [
            "--manifest",
            "--std-root",
            "--package-root",
            "--diagnostics-format",
        ] {
            assert!(help.contains(flag));
        }
    }
}

#[test]
fn repeated_root_recovery_respects_equals_values_and_end_of_options() {
    let named_roots = parse(&[
        "sylc",
        "--std-root",
        "check",
        "check",
        "--package-root",
        "emit",
    ])
    .unwrap();
    assert_eq!(named_roots.common.std_roots, [PathBuf::from("check")]);
    assert_eq!(named_roots.common.package_roots, [PathBuf::from("emit")]);
    let invocation = parse(&[
        "sylc",
        "--std-root=first",
        "--package-root=p",
        "check",
        "--std-root=second",
        "--package-root",
        "q",
        "--",
        "--std-root=filename",
    ])
    .unwrap();
    assert_eq!(
        invocation.common.std_roots,
        [PathBuf::from("first"), PathBuf::from("second")]
    );
    assert_eq!(
        invocation.common.package_roots,
        [PathBuf::from("p"), PathBuf::from("q")]
    );
    assert_eq!(invocation.inputs, [PathBuf::from("--std-root=filename")]);
    let invocation = parse(&[
        "sylc",
        "--manifest",
        "check",
        "--out",
        "emit",
        "--std-root",
        "root",
        "a.syl",
    ])
    .unwrap();
    assert_eq!(invocation.common.manifest, Some("check".into()));
    assert_eq!(
        invocation.mode,
        RunMode::Emit {
            out_path: Some("emit".into())
        }
    );
}

#[cfg(unix)]
#[test]
fn repeated_roots_preserve_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(b"--std-root=\xff".to_vec());
    let app = SylcApp::try_parse_from([
        OsString::from("sylc"),
        path,
        OsString::from("check"),
        OsString::from("--std-root=second"),
    ])
    .unwrap();
    assert_eq!(
        app.common.std_roots,
        [
            PathBuf::from(OsString::from_vec(vec![0xff])),
            PathBuf::from("second")
        ]
    );
}
