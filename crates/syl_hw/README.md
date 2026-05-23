# syl_hw

`syl_hw` defines the elaborated hardware graph data model for Syl.

It owns hardware graph structs, hardware object IDs, places, guards, expansion
origins, modules, ports, instances, and parameterized hardware designs as data.

Validation algorithms such as driver conflict checking, place overlap analysis,
undriven checks, and bounds proofs belong to `syl_elab`, not this crate.
