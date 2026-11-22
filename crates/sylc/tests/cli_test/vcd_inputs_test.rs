use super::*;

#[test]
fn sim_vcd_cannot_overwrite_loaded_sources_or_stimulus_through_aliases() {
    let p = TestProject::new();
    p.write("syl.toml", "[paths]\nworkspace_roots=['src']\n");
    let top = p.write(
        "src/top.syl",
        "use helper.Child\ncell Top(i:in Bit)->y:Bit {let child=place Child(i:i) y:=child}",
    );
    let dep = p.write("src/helper.syl", "cell Child(i:in Bit)->y:Bit {y:=i}");
    let stimulus = p.write("s.json", r#"[{"drive":{"i":0},"expect":{"y":0}}]"#);
    fs::hard_link(&dep, p.0.join("hard.vcd")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("s.json", p.0.join("sym.vcd")).unwrap();
    let files = [&top, &dep, &stimulus];
    let originals = files.map(|p| fs::read(p).unwrap());
    for name in [
        "src/./top.syl",
        "src/helper.syl",
        "s.json",
        "hard.vcd",
        #[cfg(unix)]
        "sym.vcd",
    ] {
        let out = p.run(
            "",
            &[
                "sim",
                "src/top.syl",
                "--top",
                "Top",
                "--stimulus",
                "s.json",
                "--vcd",
                name,
            ],
        );
        failure(&out);
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(err.contains("refers to input"), "{err}");
        assert_eq!(files.map(|p| fs::read(p).unwrap()), originals);
    }
}

#[test]
fn sim_protects_manifest_opaque_summaries_and_aliases_from_vcd() {
    let p = TestProject::new();
    p.write(
        "syl.toml",
        "[paths]\nworkspace_roots=['src']\nopaque_summaries=['summary.json']\n",
    );
    p.write("src/top.syl", "cell Top(i:in Bit)->y:Bit {y:=i}");
    p.write("s.json", r#"[{"drive":{"i":1},"expect":{"y":1}}]"#);
    let contents = r#"{"schema_version":1,"summaries":[]}"#;
    let summary = p.write("summary.json", contents);
    fs::hard_link(&summary, p.0.join("hard.vcd")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("summary.json", p.0.join("sym.vcd")).unwrap();
    let simulate = |output| {
        p.run(
            "",
            &[
                "sim",
                "src/top.syl",
                "--top",
                "Top",
                "--stimulus",
                "s.json",
                "--vcd",
                output,
            ],
        )
    };
    for alias in [
        "summary.json",
        "hard.vcd",
        #[cfg(unix)]
        "sym.vcd",
    ] {
        let out = simulate(alias);
        failure(&out);
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(
            err.contains("refers to input") && err.contains("summary.json"),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&summary).unwrap(), contents);
    }
    success(&simulate("output.vcd"));
}
