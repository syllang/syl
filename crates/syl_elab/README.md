# syl_elab

`syl_elab` owns the Syl hardware elaboration pipeline.

It consumes semantic IR and facts, expands hardware-generating constructs,
tracks origins, analyzes drivers, validates hardware structure, and lowers into
the `syl_hw` hardware graph data model.

The crate is intentionally organized as pipeline stages rather than one global
elaborator object so each stage can keep a narrow input and output boundary.
