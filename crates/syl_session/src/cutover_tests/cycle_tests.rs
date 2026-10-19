//! Language recursion stays inside semantic analysis.
//!
//! A Salsa query cycle is a separate failure: the probe query calls itself and
//! panics. These programs must return the same diagnostics twice. A const cycle
//! used as a width stops as an unsupported width; it is not a query panic.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig};

use super::TempTree;

#[test]
#[should_panic(expected = "dependency graph cycle")]
fn salsa_query_cycle_is_not_a_language_diagnostic() {
    let snapshot = snapshot_of(&[("top.syl", "cell Top(y: out Bit) { y := 1 }\n")]);
    snapshot.probe_query_cycle();
}

#[test]
fn language_recursion_returns_deterministic_diagnostics() {
    let mut report = String::new();
    for (name, files) in cases() {
        report.push_str(name);
        report.push('\n');
        match catch_unwind(AssertUnwindSafe(|| diagnose(files))) {
            Ok(lines) => {
                if lines.is_empty() {
                    report.push_str("(no diagnostics)\n");
                } else {
                    report.push_str(&lines.join("\n"));
                    report.push('\n');
                }
            }
            Err(payload) => {
                let message = panic_message(&payload);
                report.push_str("PANIC ");
                report.push_str(&message);
                report.push('\n');
            }
        }
        report.push_str("---\n");
    }
    assert_eq!(report, EXPECTED, "language recursion contract changed");
}

fn cases() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "import cycle",
            vec![
                ("a.syl", "use b.beta\nfn alpha() -> nat { return 1 }\n"),
                ("b.syl", "use a.alpha\nfn beta() -> nat { return 2 }\n"),
            ],
        ),
        (
            "const cycle",
            vec![(
                "top.syl",
                "const A: nat = B\nconst B: nat = A\ncell Top(y: out UInt<A>) { y := 0 }\n",
            )],
        ),
        (
            "enum discriminant cycle",
            vec![(
                "top.syl",
                "enum State {\n    Idle = Busy,\n    Busy = Idle,\n}\ncell Top(y: out Bit) { y := 1 }\n",
            )],
        ),
        (
            "struct field cycle",
            vec![(
                "top.syl",
                "struct Node {\n    child: Node,\n}\ncell Top(y: out Bit) { y := 1 }\n",
            )],
        ),
        (
            "generic struct cycle",
            vec![(
                "top.syl",
                "struct Box<T> {\n    value: T,\n}\nstruct Rec {\n    child: Box<Rec>,\n}\ncell Top(y: out Bit) { y := 1 }\n",
            )],
        ),
        (
            "bounded function recursion",
            vec![(
                "top.syl",
                "fn depth(n: nat) -> nat {\n    if n == 0 {\n        return 0\n    } else {\n        return depth(0)\n    }\n}\nconst STEPS: nat = depth(1)\ncell Top(y: out UInt<STEPS>) { y := 0 }\n",
            )],
        ),
        (
            "const fn step limit",
            vec![(
                "top.syl",
                "fn burn(limit: nat) -> nat {\n    var i: nat = 0\n    while i < limit {\n        i = i + 1\n    }\n    return i\n}\nconst WIDTH: nat = burn(20000)\ncell Top(y: out UInt<WIDTH>) { y := 0 }\n",
            )],
        ),
        (
            "map call",
            vec![(
                "top.syl",
                "map ping(x: Bit) -> Bit =\n    x\ncell Top(x: in Bit, y: out Bit) { y := ping(x) }\n",
            )],
        ),
        (
            "map recursion",
            vec![(
                "top.syl",
                "map ping(x: Bit) -> Bit =\n    pong(x)\nmap pong(x: Bit) -> Bit =\n    ping(x)\ncell Top(x: in Bit, y: out Bit) { y := ping(x) }\n",
            )],
        ),
        (
            "missing imported name",
            vec![
                (
                    "a.syl",
                    "use b.missing\ncell Top(y: out Bit) { y := missing }\n",
                ),
                ("b.syl", "use a.Top\nfn beta() -> nat { return 2 }\n"),
            ],
        ),
        (
            "cross-module const name cycle",
            vec![
                (
                    "a.syl",
                    "use b.beta\nconst alpha: nat = beta\ncell Top(y: out UInt<alpha>) { y := 0 }\n",
                ),
                ("b.syl", "use a.alpha\nconst beta: nat = alpha\n"),
            ],
        ),
    ]
}

const EXPECTED: &str = "\
import cycle
parse: none
hir: none
tir: none
semantic: none
hwir_modules=0
---
const cycle
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y unsupported(const cycle)
---
enum discriminant cycle
parse: none
hir: none
tir: E_MIDDLE_REQUIRES_NAT enum discriminant State.Idle requires nat expression @0:24..28
semantic: E_MIDDLE_REQUIRES_NAT enum discriminant State.Idle requires nat expression @0:24..28
hwir_modules=0
---
struct field cycle
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y 1
---
generic struct cycle
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y 1
---
bounded function recursion
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y call(depth,1)
---
const fn step limit
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y call(burn,20000)
---
map call
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top x 1
port Top y 1
---
map recursion
parse: none
hir: none
tir: none
semantic: E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR expression is not supported by hardware value lowering @0:110..122
hwir_modules=0
---
missing imported name
parse: none
hir: E_MIDDLE_UNKNOWN_IMPORT unknown import b.missing at 0..13 @0:0..13
hir: E_MIDDLE_UNRESOLVED_NAME unresolved name missing @0:42..49
tir: E_MIDDLE_UNRESOLVED_NAME unresolved name missing @0:42..49
semantic: E_MIDDLE_UNKNOWN_IMPORT unknown import b.missing at 0..13 @0:0..13
semantic: E_MIDDLE_UNRESOLVED_NAME unresolved name missing @0:42..49
semantic: E_MIDDLE_UNKNOWN_IMPORT unknown import b.missing at 0..13 @0:0..13
semantic: E_MIDDLE_UNRESOLVED_NAME unresolved name missing @0:42..49
hwir_modules=0
---
cross-module const name cycle
parse: none
hir: none
tir: none
semantic: none
hwir_modules=1
port Top y unsupported(const cycle)
---
";

fn diagnose(files: Vec<(&str, &str)>) -> Vec<String> {
    let snapshot = snapshot_of(&files);
    let first = report(&snapshot);
    let second = report(&snapshot);
    assert_eq!(
        first, second,
        "a language cycle must stay a memoized diagnostic result"
    );
    first
}

fn report(snapshot: &AnalysisSnapshot) -> Vec<String> {
    let token = CancellationToken::new();
    let mut lines = Vec::new();
    push_diagnostics(&mut lines, "parse", snapshot.diagnostics());
    push_diagnostics(
        &mut lines,
        "hir",
        snapshot
            .hir_diagnostics_with_token(&token)
            .expect("hir diagnostics"),
    );
    push_diagnostics(
        &mut lines,
        "tir",
        snapshot
            .tir_diagnostics_with_token(&token)
            .expect("tir diagnostics"),
    );
    push_diagnostics(&mut lines, "semantic", &snapshot.semantic_diagnostics());
    let design = snapshot.hwir();
    let modules = design.map(|design| design.modules().len()).unwrap_or(0);
    lines.push(format!("hwir_modules={modules}"));
    if let Some(design) = design {
        for module in design.modules() {
            for port in module.ports() {
                lines.push(format!(
                    "port {} {} {}",
                    module.name(),
                    port.name(),
                    port.width()
                ));
            }
        }
    }
    lines
}

fn push_diagnostics(lines: &mut Vec<String>, stage: &str, diagnostics: &[syl_span::Diagnostic]) {
    if diagnostics.is_empty() {
        lines.push(format!("{stage}: none"));
        return;
    }
    for diagnostic in diagnostics {
        lines.push(format!(
            "{stage}: {} {} @{}:{}..{}",
            diagnostic.code.as_deref().unwrap_or("-"),
            diagnostic.message,
            diagnostic.span.source.get(),
            diagnostic.span.start,
            diagnostic.span.end
        ));
    }
}

fn snapshot_of(files: &[(&str, &str)]) -> AnalysisSnapshot {
    let root = TempTree::new();
    for (name, source) in files {
        std::fs::write(root.path.join(name), source).unwrap();
    }
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![root.path.join(files[0].0)]);
    let snapshot = host.snapshot().expect("cycle snapshot");
    // Inputs are copied into the snapshot. Keep the tree alive until return
    // so Drop does not race the publish.
    let _keep = (root, host);
    snapshot
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "non-string panic".to_string()
}
