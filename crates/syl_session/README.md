# syl_session

`syl_session` owns workspace and analysis-session state for Syl.

It provides document URIs, document versions, virtual file system access,
workspace roots, import resolution, source overlays, resolved snapshots,
incremental cache state, and build orchestration.

The crate may call compiler stages, but stage-local semantic logic remains in
the stage crates. Protocol-neutral editor queries live in `syl_query`.
