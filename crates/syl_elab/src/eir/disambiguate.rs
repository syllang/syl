use super::{EirItem, EirModule};
use std::collections::{BTreeMap, BTreeSet};

impl EirModule {
    pub(crate) fn disambiguate(mut modules: Vec<Self>) -> Vec<Self> {
        let mut counts = BTreeMap::new();
        for module in &modules {
            *counts.entry(module.name()).or_insert(0) += 1;
        }
        let mut names = BTreeMap::new();
        let mut occupied = BTreeSet::new();
        let mut candidates = BTreeMap::<String, Vec<Vec<String>>>::new();
        for module in &modules {
            let leaf = module.name();
            let unique = counts[leaf] == 1;
            let candidate = identifier(if unique {
                leaf.to_string()
            } else {
                module.canonical_path.join("__")
            });
            if module.is_extern() || (unique && candidate == leaf) {
                occupied.insert(leaf.to_owned());
                names.insert(module.canonical_path.clone(), leaf.to_owned());
            } else {
                candidates
                    .entry(candidate)
                    .or_default()
                    .push(module.canonical_path.clone());
            }
        }
        let mut collisions = Vec::new();
        for (candidate, mut paths) in candidates {
            if paths.len() == 1 && occupied.insert(candidate.clone()) {
                names.insert(paths.pop().unwrap(), candidate);
            } else {
                paths.sort();
                collisions.push((candidate, paths));
            }
        }
        for (candidate, paths) in collisions {
            for path in paths {
                let hash = path.join("\0").bytes().fold(0xcbf29ce484222325u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
                }) as u32;
                let base = format!("{candidate}__h{hash:08x}");
                let mut name = base.clone();
                let mut index = 0;
                while !occupied.insert(name.clone()) {
                    index += 1;
                    name = format!("{base}_{index}");
                }
                names.insert(path, name);
            }
        }
        for module in &mut modules {
            module.name.clone_from(&names[&module.canonical_path]);
            rewrite(&mut module.items, &names);
        }
        modules
    }
}

fn identifier(name: String) -> String {
    let clean = name.replace(|c: char| !(c.is_ascii_alphanumeric() || c == '_'), "_");
    if clean.starts_with(|c: char| c.is_ascii_digit())
        || KEYWORDS.split_ascii_whitespace().any(|word| word == clean)
    {
        format!("syl_{clean}")
    } else {
        clean
    }
}

const KEYWORDS: &str = "
accept_on alias always always_comb always_ff always_latch and assert assign assume automatic
before begin bind bins binsof bit break buf bufif0 bufif1 byte case casex casez cell chandle
checker class clocking cmos config const constraint context continue cover covergroup
coverpoint cross deassign default defparam design disable dist do edge else end endcase
endchecker endclass endclocking endconfig endfunction endgenerate endgroup endinterface
endmodule endpackage endprimitive endprogram endproperty endsequence endspecify endtable
endtask enum event eventually expect export extends extern final first_match for force foreach
forever fork forkjoin function generate genvar global highz0 highz1 if iff ifnone ignore_bins
illegal_bins implements implies import incdir include initial inout input inside instance int
integer interconnect interface intersect join join_any join_none large let liblist library
local localparam logic longint macromodule matches medium modport module nand negedge nettype
new nexttime nmos nor noshowcancelled not notif0 notif1 null or output package packed parameter
pmos posedge primitive priority program property protected pull0 pull1 pulldown pullup
pulsestyle_ondetect pulsestyle_onevent pure rand randc randcase randsequence rcmos real
realtime ref reg reject_on release repeat restrict return rnmos rpmos rtran rtranif0 rtranif1
s_always s_eventually s_nexttime s_until s_until_with scalared sequence shortint shortreal
showcancelled signed small soft solve specify specparam static string strong strong0 strong1
struct super supply0 supply1 sync_accept_on sync_reject_on table tagged task this throughout
time timeprecision timeunit tran tranif0 tranif1 tri tri0 tri1 triand trior trireg type typedef
union unique unique0 unsigned until until_with untyped use uwire var vectored virtual void wait
wait_order wand weak weak0 weak1 while wildcard wire with within wor xnor xor
";

fn rewrite(items: &mut [EirItem], names: &BTreeMap<Vec<String>, String>) {
    for item in items {
        match item {
            EirItem::Instance(instance) => {
                if let Some(name) = names.get(instance.target_canonical_path()) {
                    instance.module.clone_from(name);
                }
            }
            EirItem::CellExpansion(expansion) => rewrite(&mut expansion.items, names),
            EirItem::SymbolicStaticFor { items, .. } => rewrite(items, names),
            EirItem::SymbolicStaticIf {
                then_items,
                else_items,
                ..
            } => {
                rewrite(then_items, names);
                rewrite(else_items, names);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "disambiguate_tests.rs"]
mod tests;
