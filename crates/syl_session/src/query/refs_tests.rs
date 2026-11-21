use std::collections::BTreeSet;

use syl_span::SourceId;
use syl_syntax::Item;
use syl_syntax::parser::SourceParser;

use super::inplace_callees;

fn parse_first_item(source: &str) -> Item {
    let parsed = SourceParser::new_in(source, SourceId::new(0)).parse_file_partial();
    assert!(
        parsed.diagnostics.is_empty(),
        "test fixture must parse cleanly: {:?}",
        parsed.diagnostics
    );
    parsed.file.items.into_iter().next().expect("parsed item")
}

#[test]
fn inplace_callees_traverses_inplace_args_and_nested_expressions() {
    let source = "\
cell Top(x: in Bit, y: out Bit) {
    let a = inplace Outer(v: inplace InnerArg(x: x))
    let b = place Separate(v: inplace InsidePlaceArg(x: x))
    let c = match x {
        1 => inplace InMatchArm(x: x),
        _ => (inplace InField(x: x)).val,
    }
    let d = select priority {
        x => (inplace InSelectArm(x: x))[0],
        default => Pair { field: inplace InAggregate(x: x) },
    }
    let e = inplace GenericCell<8>(v: (inplace InIndexBase(x: x))[inplace InIndexExpr(x: x)])
    reg r: Bit reset(inplace InRegDomain(x: x), inplace InRegValue(x: x))
    (inplace InDriveTarget(x: x)) := x
    for i in inplace InRangeStart(x: x)..inplace InRangeEnd(x: x) {
        compile_error(inplace InCompileError(x: x))
    }
    y := x
}
";
    let item = parse_first_item(source);
    let callees = inplace_callees(&item);
    let expected: BTreeSet<String> = [
        "Outer",
        "InnerArg",
        "InsidePlaceArg",
        "InMatchArm",
        "InField",
        "InSelectArm",
        "InAggregate",
        "GenericCell",
        "InIndexBase",
        "InIndexExpr",
        "InRegDomain",
        "InRegValue",
        "InDriveTarget",
        "InRangeStart",
        "InRangeEnd",
        "InCompileError",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(callees, expected);
    assert!(
        !callees.contains("Separate"),
        "non-inplace place callee must not be collected"
    );
}
