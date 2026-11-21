use super::{ProjectError, ProjectManifest, tests::TestVfs};
use serde_json::{Value, json};
use std::path::Path;
use syl_sema::{
    BackendConstraint, OpaqueItemKind, SummaryCapability, SummaryDirection, SummaryDomain,
    SummaryDomainBehavior, SummaryFieldDirection, SummaryLatencyClass, SummaryLayout,
    SummaryLayoutConst, SummaryProtocolPreservation, SummaryWordEncoding, TrustBoundary,
};

fn load_document(document: Value) -> syl_sema::OpaqueSummaryTable {
    let mut vfs = TestVfs::default();
    vfs.insert(
        "/project/syl.toml",
        "[paths]\nopaque_summaries = ['s.json']",
    );
    vfs.insert("/project/s.json", &document.to_string());
    ProjectManifest::load_with_vfs(Path::new("/project/syl.toml"), &vfs)
        .unwrap()
        .load_opaque_summaries_with_vfs(&vfs)
        .unwrap()
}

#[test]
fn complete_json_summary_maps_layouts_capabilities_and_metadata() {
    let table = load_document(json!({"schema_version":1,"summaries":[{
        "kind":"precompiled_cell", "callable":"Vendor",
        "endpoints":[
            {"name":"data","direction":"in","layout":{"kind":"word","encoding":"uint","width":8},"capability":"value"},
            {"name":"array","direction":"out","layout":{"kind":"array","len":"N","elem":{"kind":"word","encoding":"sint","width":null}},"capability":"unknown"},
            {"name":"clk","direction":"in","layout":"clock","capability":{"kind":"clock","domain":{"named":"CoreClk"}}},
            {"name":"rst","direction":"in","layout":"reset","capability":{"kind":"reset","domain":"builtin"}},
            {"name":"bus","direction":"inout","layout":{"kind":"view","protocol":"Bus","view":"Master","fields":["valid"]},
             "capability":{"kind":"view","view":"Master","readable_fields":["ready"],"writable_fields":["valid"]},
             "protocol":{"name":"Bus","fields":["ready","valid"],"views":[{"name":"Master","fields":[{"name":"ready","direction":"in"},{"name":"valid","direction":"out"},{"name":"data","direction":"inout"}]}]}}
        ],
        "driven_fields":["bus.valid"], "consumed_fields":["data"],
        "domain_behavior":{"kind":"explicit","clock_inputs":["clk"],"reset_inputs":["rst"]},
        "latency_class":"sequential", "protocol_preservation":"preserved",
        "trust_boundary":{"kind":"vendor_black_box","vendor":"acme"},
        "backend_constraints":[{"kind":"requires_backend","backend":"systemverilog"},{"kind":"requires_black_box_artifact","artifact":"Vendor.sv"},{"kind":"forbids_retiming"},"forbids_retiming"]
    }]}));
    let summary = table.get("Vendor").unwrap();
    assert_eq!(summary.kind(), OpaqueItemKind::PrecompiledCell);
    let endpoints = summary.endpoints();
    assert_eq!(endpoints[0].name(), "data");
    assert_eq!(endpoints[0].direction(), SummaryDirection::In);
    assert_eq!(
        endpoints[0].layout(),
        &SummaryLayout::Word {
            encoding: SummaryWordEncoding::UInt,
            width: SummaryLayoutConst::Known(8)
        }
    );
    assert_eq!(endpoints[0].capability(), &SummaryCapability::Value);
    assert_eq!(endpoints[1].direction(), SummaryDirection::Out);
    assert_eq!(
        endpoints[1].layout(),
        &SummaryLayout::Array {
            len: SummaryLayoutConst::Symbol("N".into()),
            elem: Box::new(SummaryLayout::Word {
                encoding: SummaryWordEncoding::SInt,
                width: SummaryLayoutConst::Unknown
            })
        }
    );
    assert_eq!(
        endpoints[2].capability(),
        &SummaryCapability::Clock {
            domain: SummaryDomain::Named("CoreClk".into())
        }
    );
    assert_eq!(
        endpoints[3].capability(),
        &SummaryCapability::Reset {
            domain: SummaryDomain::Builtin
        }
    );
    assert_eq!(endpoints[4].direction(), SummaryDirection::InOut);
    assert_eq!(
        endpoints[4].layout(),
        &SummaryLayout::View {
            protocol: "Bus".into(),
            view: "Master".into(),
            fields: vec!["valid".into()]
        }
    );
    assert_eq!(
        endpoints[4].capability(),
        &SummaryCapability::View {
            view: "Master".into(),
            readable_fields: vec!["ready".into()],
            writable_fields: vec!["valid".into()]
        }
    );
    let protocol = endpoints[4].protocol().unwrap();
    assert_eq!(protocol.name(), "Bus");
    assert_eq!(protocol.fields(), ["ready", "valid"]);
    assert_eq!(protocol.views()[0].name(), "Master");
    assert_eq!(protocol.views()[0].fields()[0].name(), "ready");
    assert_eq!(
        protocol.views()[0]
            .fields()
            .iter()
            .map(|f| f.direction())
            .collect::<Vec<_>>(),
        [
            SummaryFieldDirection::In,
            SummaryFieldDirection::Out,
            SummaryFieldDirection::InOut
        ]
    );
    assert_eq!(summary.driven_fields()[0].display(), "bus.valid");
    assert_eq!(summary.consumed_fields()[0].display(), "data");
    assert_eq!(
        summary.domain_behavior(),
        &SummaryDomainBehavior::Explicit {
            clock_inputs: vec!["clk".into()],
            reset_inputs: vec!["rst".into()]
        }
    );
    assert_eq!(summary.latency_class(), SummaryLatencyClass::Sequential);
    assert_eq!(
        summary.protocol_preservation(),
        SummaryProtocolPreservation::Preserved
    );
    assert_eq!(
        summary.trust_boundary(),
        &TrustBoundary::VendorBlackBox {
            vendor: "acme".into()
        }
    );
    assert_eq!(
        summary.backend_constraints(),
        [
            BackendConstraint::RequiresBackend {
                backend: "systemverilog".into()
            },
            BackendConstraint::RequiresBlackBoxArtifact {
                artifact: "Vendor.sv".into()
            },
            BackendConstraint::ForbidsRetiming,
            BackendConstraint::ForbidsRetiming
        ]
    );
}

#[test]
fn all_layout_shorthands_and_remaining_variants_are_representable() {
    let layouts = [
        (json!("unknown"), SummaryLayout::Unknown),
        (json!("nat"), SummaryLayout::Nat),
        (json!("bit"), SummaryLayout::Bit),
        (json!("bool"), SummaryLayout::Bool),
        (json!("domain"), SummaryLayout::Domain),
        (json!("clock"), SummaryLayout::Clock),
        (json!("reset"), SummaryLayout::Reset),
        (json!("str"), SummaryLayout::Str),
        (
            json!({"kind":"word","encoding":"bits","width":"unknown"}),
            SummaryLayout::Word {
                encoding: SummaryWordEncoding::Bits,
                width: SummaryLayoutConst::Unknown,
            },
        ),
        (
            json!({"kind":"aggregate","name":"Pair","fields":["a"]}),
            SummaryLayout::Aggregate {
                name: "Pair".into(),
                fields: vec!["a".into()],
            },
        ),
        (
            json!({"kind":"enum","name":"Mode","variants":["Idle"]}),
            SummaryLayout::Enum {
                name: "Mode".into(),
                variants: vec!["Idle".into()],
            },
        ),
        (
            json!({"kind":"opaque","label":"T"}),
            SummaryLayout::Opaque { label: "T".into() },
        ),
    ];
    for (raw, expected) in layouts {
        let table = load_document(json!({"schema_version":1,"summaries":[{
            "kind":"source_cell","callable":"Source","endpoints":[{"name":"x","direction":"in","layout":raw,"capability":"domain"}]
        }]}));
        let summary = table.get("Source").unwrap();
        assert_eq!(summary.kind(), OpaqueItemKind::SourceCell);
        assert_eq!(summary.endpoints()[0].layout(), &expected);
        assert_eq!(
            summary.endpoints()[0].capability(),
            &SummaryCapability::Domain
        );
        assert_eq!(summary.domain_behavior(), &SummaryDomainBehavior::Unknown);
        assert_eq!(summary.latency_class(), SummaryLatencyClass::Unknown);
        assert_eq!(
            summary.protocol_preservation(),
            SummaryProtocolPreservation::Unknown
        );
        assert_eq!(summary.trust_boundary(), &TrustBoundary::SourceDerived);
    }
    let table = load_document(json!({"schema_version":1,"summaries":[{
        "kind":"extern_cell","callable":"Extern","domain_behavior":"clockless","latency_class":"transparent","protocol_preservation":"opaque",
        "trust_boundary":{"kind":"unsafe_black_box","rationale":"reviewed"},
        "endpoints":[{"name":"clk","direction":"in","layout":"clock","capability":{"kind":"clock","domain":"unknown"}}]
    }]}));
    let summary = table.get("Extern").unwrap();
    assert_eq!(summary.kind(), OpaqueItemKind::ExternCell);
    assert_eq!(summary.domain_behavior(), &SummaryDomainBehavior::Clockless);
    assert_eq!(summary.latency_class(), SummaryLatencyClass::Transparent);
    assert_eq!(
        summary.protocol_preservation(),
        SummaryProtocolPreservation::Opaque
    );
    assert_eq!(
        summary.trust_boundary(),
        &TrustBoundary::UnsafeBlackBox {
            rationale: "reviewed".into()
        }
    );
    assert_eq!(
        summary.endpoints()[0].capability(),
        &SummaryCapability::Clock {
            domain: SummaryDomain::Unknown
        }
    );
}

#[test]
fn json_and_toml_merge_in_declaration_order() {
    let mut vfs = TestVfs::default();
    vfs.insert(
        "/project/syl.toml",
        "[paths]\nopaque_summaries = ['a.json', 'b.toml', './a.json']",
    );
    vfs.insert("/project/a.json", r#"{"schema_version":1,"summaries":[{"kind":"source_cell","callable":"Vendor","driven_fields":["original"],"consumed_fields":["input"]}]}"#);
    vfs.insert(
        "/project/b.toml",
        r#"
schema_version = 1
[[summaries]]
kind = "precompiled_cell"
callable = "Vendor"
driven_fields = ["out.valid"]
trust_boundary = "trusted_precompiled"
[[summaries.endpoints]]
name = "out"
direction = "out"
layout = { kind = "array", len = 2, elem = "bit" }
capability = "value"
"#,
    );
    let table = ProjectManifest::load_with_vfs(Path::new("/project/syl.toml"), &vfs)
        .unwrap()
        .load_opaque_summaries_with_vfs(&vfs)
        .unwrap();
    assert_eq!(table.len(), 1);
    let summary = table.get("Vendor").unwrap();
    assert_eq!(summary.kind(), OpaqueItemKind::PrecompiledCell);
    assert_eq!(summary.driven_fields()[0].display(), "out.valid");
    assert_eq!(summary.consumed_fields()[0].display(), "input");
    assert_eq!(
        summary.endpoints()[0].layout(),
        &SummaryLayout::Array {
            len: SummaryLayoutConst::Known(2),
            elem: Box::new(SummaryLayout::Bit)
        }
    );
    assert_eq!(summary.trust_boundary(), &TrustBoundary::TrustedPrecompiled);
}

#[test]
fn summary_parse_validation_and_read_errors_are_distinct() {
    let cases = [
        ("bad.json", "{", "parse"),
        ("bad.toml", "[", "parse"),
        ("s.json", r#"{"schema_version":1,"unknown":0}"#, "parse"),
        ("s.json", r#"{"schema_version":2}"#, "invalid"),
        ("s.yaml", "", "invalid"),
        ("summary", "", "invalid"),
        ("missing.json", "", "read"),
    ];
    for (name, text, expected) in cases {
        let mut vfs = TestVfs::default();
        vfs.insert(
            "/p/syl.toml",
            &format!("[paths]\nopaque_summaries = ['{name}']"),
        );
        if expected != "read" {
            vfs.insert(&format!("/p/{name}"), text);
        }
        let error = ProjectManifest::load_with_vfs(Path::new("/p/syl.toml"), &vfs)
            .unwrap()
            .load_opaque_summaries_with_vfs(&vfs)
            .unwrap_err();
        assert!(matches!(
            (expected, error),
            ("parse", ProjectError::OpaqueSummaryParse { .. })
                | ("invalid", ProjectError::OpaqueSummaryInvalid { .. })
                | ("read", ProjectError::Read { .. })
        ));
    }
    for field in ["driven_fields", "consumed_fields"] {
        for value in ["", ".a", "a.", "a..b", "a. .b"] {
            let mut summary = json!({"kind":"extern_cell","callable":"X"});
            summary[field] = json!([value]);
            assert_invalid(summary);
        }
    }
    assert_invalid(json!({"kind":"extern_cell","callable":" "}));
    assert_invalid(
        json!({"kind":"extern_cell","callable":"X","endpoints":[{"name":"","direction":"in","layout":"bit","capability":"value"}]}),
    );
}

fn assert_invalid(summary: Value) {
    let mut vfs = TestVfs::default();
    vfs.insert("/p/syl.toml", "[paths]\nopaque_summaries = ['s.json']");
    vfs.insert(
        "/p/s.json",
        &json!({"schema_version":1,"summaries":[summary]}).to_string(),
    );
    assert!(matches!(
        ProjectManifest::load_with_vfs(Path::new("/p/syl.toml"), &vfs)
            .unwrap()
            .load_opaque_summaries_with_vfs(&vfs),
        Err(ProjectError::OpaqueSummaryInvalid { .. })
    ));
}
