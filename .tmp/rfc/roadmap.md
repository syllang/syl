  当前 syl 的 crate 大方向是对的：syntax -> hir -> sema -> elab -> hw -> emit，外侧再由 session/query/lsp/cli 组织工具链。主要问题不是“分层错了”，而是一些层的内部职责还太宽，尤其是 syl_elab 和 syl_sema。如果要对标 Chisel/FIRRTL/CIRCT，syl 应该学
  的是稳定 IR、pass pipeline、metadata/annotation、opaque boundary、测试门槛，而不是复制宿主语言式生成器。

  ## 与 Chisel 的关键对照

  Chisel 的成熟点：

  - Scala 负责生成器表达力，Chisel AST/FIRRTL/CIRCT 负责后端流水线。
  - FIRRTL/CIRCT 提供明确的中间表示和 transform pipeline。
  - annotation/metadata 机制支撑 transform、blackbox、backend、verification。
  - 工具链和生态成熟，能处理大项目和外部 IP。

  Chisel 的弱点正是 syl 想解决的点：

  - 生成器语义藏在 Scala 运行时里，静态分析边界弱。
  - elaboration 之后很多 source intent 已丢失。
  - 错误诊断、driver ownership、protocol/capability 很难成为语言级事实。
  - IDE/LSP 很难理解硬件语义，只能理解 Scala 外壳。

  syl 的长期方向应该是：保留语言原生静态分析优势，但补齐 Chisel/FIRRTL/CIRCT 在工程流水线上的成熟度。

  ———

```
  ## [ ] Phase N：表示未完成。 
  ## [x] Phase N：表示完成。 
```

执行时，必须遵照 AGENTS.md。S1 主 Agent 先派出任务给 Work SubAgent（使用 gpt-4-xhigh），然后 S2 Review SubAgent （ gpt-4-medium）检查并反馈问题，S3 主 Agent 审核和仔细分析后传达给 Work SubAgent 整改。S4 整改完之后 Review SubAgent 重复检查(S2~S4)……这是循环过程，直到本 Phase 问题收敛。每个 S 步骤，在对应 .tmp/rfc/roadmap.md 的对应 Phase下标记一个 Log 行。整个Phase 问题收敛完成后只有 Review Agent 有权独立地标记为完成，然后才能进入下一个 Phase.

  ## [x] Phase 0：架构契约冻结

  目标：先把每个 crate 的职责和阶段输入输出钉死，防止后续继续在错误边界上堆逻辑。

  MUST FIX：

  - syl_syntax 只负责 lexer/parser/CST/AST/error recovery，不允许出现 resolved/type/elab 语义。
  - syl_hir 只放纯 HIR 数据和 stable IDs，不放 checker 逻辑。
  - syl_sema 产出 semantic side tables：类型、名字解析、const facts、capability facts。
  - syl_elab 只消费 typed HIR + semantic facts，产出 elaboration graph / HW graph。
  - syl_emit 只消费 HW IR，不负责修复前端语义错误。
  - syl_session 是 orchestration，不拥有语义模型。
  - syl_query 是查询 API，不变成共享 DTO 垃圾桶。
  - syl_lsp 只做协议适配、UTF-16 坐标、诊断发布、取消、debounce。

  退出标准：

  - 每个 crate README 写清楚输入、输出、禁止依赖。
  - crate dependency graph 单向，不能反向依赖。
  - public API 能解释“为什么必须 public”。

  Log
  - 时间 - 完成的任务
  - 2026-05-23 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 0 架构契约冻结：补齐 crate README 职责边界、建立依赖方向验证、避免进入后续 Phase 逻辑。
  - 2026-05-23 S1 - Work SubAgent 完成 Phase 0 初版交付：更新全部 crate README，新增 architecture_contracts 测试；主 Agent 验证 `cargo test -p sylc architecture_ -- --nocapture`、`cargo check --workspace --all-targets`、`git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出 Review SubAgent，按 Phase 0 MUST FIX 和退出标准独立审查 README 契约、依赖方向测试和 public API policy。
  - 2026-05-23 S2 - Review SubAgent 判定 Phase 0 未收敛：`syl_elab` 仍公开 HIR/TIR/query-like API，`syl_session` 持有并公开 semantic stage，`syl_query` 依赖 elab stage internals，architecture_contracts 只验证文档和 Cargo DAG 且固化了错误边界。
  - 2026-05-23 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：收敛 `syl_elab/session/query` 真实边界，移除或封闭 HIR/TIR/query-like elab API 泄漏，并修正 architecture_contracts 避免固化错误架构。
  - 2026-05-23 S4 - Work SubAgent 完成边界整改：将前端语义分析与 query-like API 迁入 `syl_sema`，`syl_elab` public API 收敛为硬件编译入口，`syl_session/query` 改用 sema analysis accessor，architecture_contracts 增加 public surface/source guard；主 Agent 验证 `cargo test -p sylc architecture_ -- --nocapture`、`cargo check --workspace --all-targets`、`git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 边界整改是否满足 Phase 0，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-23 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 0 完成：前一轮 Must Fix 已实质收敛，architecture_contracts 覆盖 README、依赖方向、elab public surface、session/query source guard；主 Agent 将 Phase 0 标记为完成。

  ———

  ## [x] Phase 1：IR 所有权收敛

  目标：解决“多个阶段各自偷偷定义类似 IR”的问题。长期工业级编译器最怕 IR 边界模糊，因为后续 pass、LSP、增量编译都会被拖垮。

  建议 IR 层级：

  - AST：纯语法树，保留源码结构，服务 parser/recovery/format/LSP。
  - HIR：脱糖后的语言结构，稳定 ID，仍不带类型。
  - TIR：不是一棵新树，而是 HIR + side tables。
  - Const MIR：只服务 fn/编译期计算，禁止混入硬件 graph。
  - Map IR：只表示纯组合 map 的表达式语义。
  - EIR：elaboration graph，表示 cell/module/interface/view/driver/capability。
  - HW IR：后端无关的硬件结构图，接近 RTL/netlist，但仍保留源映射。
  - SV AST：SystemVerilog emission 专用，不应反向污染 HW IR。

  MUST FIX：

  - Const MIR 和 Map IR 的 owner 必须唯一。
  - EIR 数据结构和 builder/checker 分离。
  - HW IR 不应该携带 sema/elab 临时状态。
  - TIR 必须基于 side tables，而不是 mutable typed nodes。

  退出标准：

  - 任意一个 pass 的输入输出可以用一句话说明。
  - 删除一个 pass 不应该破坏前后 IR 的数据定义。
  - 每层 IR 都有 golden/debug dump，便于诊断和测试。

  Log
  - 2026-05-23 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 1 IR 所有权收敛：审查并整改 AST/HIR/TIR/Const MIR/Map IR/EIR/HW IR/SV AST owner，优先收敛 Const MIR/Map IR/EIR/TIR 边界并补充可执行架构证据。
  - 2026-05-23 S1 - Work SubAgent 完成 Phase 1 初版交付：Const MIR/Map IR owner 收敛到 `syl_sema`，`syl_elab` 删除重复 Map IR 文件并复用 sema-owned IR，EIR 拆分数据与 assembler/validate/facts，补充 TIR/Const MIR/Map IR/EIR/HW IR/SV AST debug dump 与 architecture_phase1_ir 测试；主 Agent 验证 `cargo fmt --all`、`cargo test -p sylc architecture_ -- --nocapture`、`cargo check --workspace --all-targets`、`git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出 Review SubAgent，按 Phase 1 MUST FIX 和退出标准独立审查 IR owner 唯一性、EIR 数据/构建分离、TIR side-table 形态、HW IR 临时状态隔离和 debug dump 覆盖。
  - 2026-05-23 S2 - Review SubAgent 判定 Phase 1 未收敛：HW IR 仍把 driver/read/create/cell summary facts 作为 IR 字段，AST/HIR 缺少 debug dump/golden 证据，architecture_phase1_ir 过度依赖 substring 且漏扫 `syl_hw::design`，EIR 数据文件仍混入 `Elaborator` orchestration。
  - 2026-05-23 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：将 driver/read/create/cell summary facts 移出 HW IR 核心模型，补齐 AST/HIR dump 证据，增强 architecture_phase1_ir 扫描范围，并把 EIR `Elaborator` 从数据文件迁出。
  - 2026-05-23 S4 - Work SubAgent 完成 Phase 1 整改：删除 HW IR 中 driver/read/create/cell summary 临时字段并迁入 elab-owned `HardwareMetadata` sidecar，新增 AST/HIR debug dump，增强 architecture_phase1_ir 全目录 guard，将 EIR `Elaborator` 迁出数据文件；主 Agent 验证 architecture tests、workspace check、相关 driver/cell tests、targeted grep 和 `git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 整改是否满足 Phase 1，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-23 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 1 完成：上一轮 Must Fix 已收敛，HW IR 不再携带 driver metadata，AST/HIR/TIR/Const MIR/Map IR/EIR/HW IR/SV AST dump 经真实 pipeline 覆盖，EIR 数据文件与 orchestration 分离；主 Agent 将 Phase 1 标记为完成。

  ———

  ## [x] Phase 2：前端工业化

  目标：让 parser 和 syntax 层成为 LSP、formatter、diagnostic 的可靠地基。

  MUST FIX：

  - AST 定义、token、parser mechanics、error recovery 分离。
  - syl_syntax 的入口文件不要继续膨胀。
  - 所有语法错误必须可恢复，不能一错全崩。
  - trivia/comment/Span 保留策略要明确。
  - AST node ID 和 source range 要稳定，支持 LSP 增量诊断。

  需要补的测试：

  - grammar golden tests。
  - invalid syntax recovery tests。
  - source span precision tests。
  - examples 全量 parse tests。

  退出标准：

  - LSP 可以在半成品代码上给诊断。
  - parser 不依赖后续阶段才能报出基本错误。
  - AST dump 稳定，可用于回归测试。

  Log
  - 2026-05-23 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 2 前端工业化：拆分 `syl_syntax` AST/token/parser/recovery/dump 边界，控制入口文件膨胀，补充 examples parse、invalid recovery、span precision 和 AST dump 回归测试。
  - 2026-05-23 S1 - Work SubAgent 完成 Phase 2 初版交付：`syl_syntax` 入口降为模块 wiring，AST/token/node-index 拆出独立模块，parser recovery 和 span 精度补强，`ParseOutput` 与 session snapshot 携带 `AstNodeIndex`，新增 syntax lib tests 与 architecture_phase2_frontend 覆盖 examples parse、invalid recovery、span precision、node id/range 和 AST dump；主 Agent 验证 `cargo fmt --all`、`cargo test -p syl_syntax --lib -- --nocapture`、`cargo test -p sylc architecture_ -- --nocapture`、`cargo check --workspace --all-targets`、`git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出 Review SubAgent，按 Phase 2 MUST FIX 和退出标准独立审查 syntax 分层、error recovery、trivia/span 策略、AstNodeIndex 稳定性、测试覆盖和文件规模。
  - 2026-05-23 S2 - Review SubAgent 判定 Phase 2 未收敛：`AstNodeId` 基于 `kind + covered text + occurrence`，在前方插入同 kind/同文本 sibling 时未改动节点 ID 漂移；README 对 LSP bookkeeping 过度承诺；node-id 稳定性测试仅覆盖 leading comment，grammar golden 证据偏弱。
  - 2026-05-23 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：改进 `AstNodeId` 稳定锚定策略，补充前置同 kind/同文本 sibling 插入测试，增强 grammar golden 证据，并校准 README 对 node index 能力边界的表述。
  - 2026-05-23 S4 - Work SubAgent 完成 Phase 2 整改：`AstNodeId` 改为结构路径锚定并移除全局 occurrence，新增前置同文本 sibling 插入稳定性测试，扩展 grammar golden 覆盖 const/fn/bundle/interface/map/module，README 明确连续不可区分 sibling run 的 ordinal 限制；主 Agent 验证 syntax lib tests、architecture tests、workspace check、syntax dependency grep 和 `git diff --check` 均通过。
  - 2026-05-23 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 node-id 稳定性整改和 Phase 2 其他退出标准是否满足，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-23 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 2 完成：node-id 稳定性前一轮 Must Fix 已修复，README 能力边界准确，grammar golden 覆盖扩展，syntax 分层、recovery、span precision、examples parse、文件规模和 `#[non_exhaustive]` 要求均未回退；主 Agent 将 Phase 2 标记为完成。

  ———

  ## [x] Phase 3：语义层硬化

  目标：把类型、名字、capability、layout、const eval 都变成可查询、可缓存、可诊断的事实。

  MUST FIX：

  - Name resolution 产出明确 package/module/import graph。
  - Type identity 必须 canonicalized，不能靠字符串或临时结构比较。
  - Domain、Clock、Reset、view capability 必须是一等语义事实。
  - Layout/encoding facts 必须进入类型或附属 side table。
  - Const eval 必须 deterministic、sandboxed、可缓存。
  - 错误必须结构化，不能靠字符串拼接。

  重点设计：

  - ResolutionTable<HirId, Res>
  - TypeTable<HirId, TyId>
  - CapabilityTable<HirId, CapabilityFacts>
  - ConstFacts<HirId, ConstValue>
  - LayoutFacts<TyId, Layout>
  - ProtocolFacts<InterfaceId, ProtocolSummary>

  退出标准：

  - sema 不生成硬件。
  - sema 的结果可以被 CLI、LSP、elab 共同消费。
  - LSP hover/go-to-definition/type info 不需要触发 elaboration。

  Log
  - 2026-05-24 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 3 语义层硬化：审查并整改 `syl_sema` 的 name/type/capability/layout/const/error facts，建立可查询 facts facade，补充 architecture_phase3_sema 证据，避免进入 elaboration pipeline 拆分。
  - 2026-05-24 S1 - Work SubAgent 完成 Phase 3 初版交付：新增 sema-owned `SemanticFacts` facade 和 Resolution/Type/Capability/Const/Layout/Protocol tables，移除 `TirType::Named` 字符串身份回退，补充 const eval step-limit/cache、结构化错误 kind、semantic_facts 与 architecture_phase3_sema 测试；主 Agent 验证 `cargo fmt --all`、`cargo test -p sylc architecture_ -- --nocapture`、`cargo test -p syl_sema -- --nocapture`、`cargo check --workspace --all-targets`、`git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出 Review SubAgent，按 Phase 3 MUST FIX 和退出标准独立审查 facts 真实性、canonical type identity、capability/layout/const/error 结构化、sema 非硬件生成边界和 architecture_phase3_sema 证据强度。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 3 未收敛：capability facts 仍用 `TirType` 结构相等 fallback 恢复 `TypeId`，`ConstFacts` 绕过带 step-limit/cache 的 `ConstEvaluator` 且只折叠顶层简单表达式，`ResolutionGraph` 缺少一等 module/import 节点与关系 API；另指出 layout opaque fallback、Debug 字符串 cache 断言和 sema 硬件集成测试边界问题。
  - 2026-05-24 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：删除 capability TypeId 结构 fallback，改造 `ConstFacts` 复用真实 `ConstEvaluator` 与 step-limit/cache 语义，增强 `ResolutionGraph` 一等 package/module/import 节点和关系 API，并修正 layout facts、Debug cache 断言和 sema 硬件集成测试边界证据。
  - 2026-05-24 S4 - Work SubAgent 完成 Phase 3 整改：删除 capability 结构相等 TypeId fallback，`ConstFacts` 改为消费 `ConstMirBuilder + ConstEvaluator` 并记录 evaluator-produced expr facts，`ResolutionGraph` 增加 `PackageNodeId`/`ImportId` 与 package/module/import 关系 API，layout opaque fallback 收敛为显式 variants，snapshot cache 状态改为显式 API，architecture_phase3_sema 增加 canonical/facts/production-boundary guards；主 Agent 验证 architecture tests、sema tests、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 sema facts 整改是否满足 Phase 3，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 3 仍未收敛：上一轮 ConstFacts、ResolutionGraph、layout、cache API 和 production-boundary 问题已实质修复，但 Clock/Reset domain facts 仍只通过 `TirType::Named { generic: Some(local) }` 找回 canonical `TypeId`，缺少直接 `Clock<Domain>`/`Reset<Domain>` 或未来非 local domain carrier 的 canonical domain fact 覆盖。
  - 2026-05-24 S3 - 主 Agent 审核第二轮 Review 结论后派 Work SubAgent 做窄整改：为 Clock/Reset 引入可表达 generic 与 builtin/direct domain carrier 的 canonical domain fact，保持禁止结构相等 fallback，并补充对应 semantic facts 与 architecture tests。
  - 2026-05-24 S4 - Work SubAgent 完成窄整改：`CapabilityKind::Clock/Reset` 改用 `DomainFact::{Named(TypeId), BuiltinDomain, Unknown}`，generic `D: Domain` 和 direct `Clock<Domain>`/`Reset<Domain>` 均有测试覆盖，architecture_phase3_sema 增加 `DomainFact` 形态与结构 fallback 禁令；主 Agent 验证 architecture tests、sema tests、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出第三轮 Review SubAgent，复查 Clock/Reset canonical domain fact 窄整改是否满足 Phase 3，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-24 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 3 完成：Clock/Reset 使用 `DomainFact` 覆盖 generic 与 builtin/direct domain carrier，结构相等 fallback 禁止守卫仍有效，ConstFacts、ResolutionGraph、layout facts、cache API 和 production-boundary 均未回退；主 Agent 将 Phase 3 标记为完成。

  ———

  ## [x] Phase 4：Elaboration 拆成严格 pipeline

  目标：syl_elab 可以继续是一个 crate，但内部必须从“巨型阶段”变成多个明确 pass。

  建议内部 pass：

  - expansion：展开 cell、实例、泛型参数。
  - binding：绑定 signal/reg/interface/view。
  - eir_build：构造 EIR。
  - map_lowering：把 map lowering 到组合表达式。
  - driver_facts：收集 driver ownership。
  - drc：multi-driver、undriven、domain、capability 检查。
  - hw_lowering：EIR 到 HW IR。
  - trace：保留 source/elaboration stack。

  MUST FIX：

  - driver analysis 不能混在 builder 副作用里。
  - EIR builder 不要同时做 lowering、validation、diagnostic。
  - multi-driver conflict 必须基于 effect/capability summary，不只靠展开后碰运气。
  - cell 和 module 边界语义必须固定：inline generator vs hierarchy boundary。

  退出标准：

  - 每个 pass 可以单独测试。
  - EIR dump 能解释“谁创建了什么、谁驱动了什么”。
  - driver conflict 能定位到调用栈和源代码位置。

  Log
  - 2026-05-24 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 4 Elaboration 严格 pipeline：审查并整改 `syl_elab` pass 边界，拆清 EIR build、driver facts/DRC、metadata、HW lowering，固定 inline cell vs module hierarchy 语义，并补充 architecture_phase4_elab 证据。
  - 2026-05-24 S1 - Work SubAgent 完成 Phase 4 初版交付：`syl_elab` pipeline 拆为 ConstMir/MapIr/EirBuild/DriverFacts/DRC/HardwareMetadata/HwLowering passes，driver facts 收集与 DRC 检查拆分，EIR/driver dump 增强，multi-driver 诊断补充 expansion call stack，architecture_phase4_elab 覆盖 stage access、dump、driver conflict、cell/module boundary；主 Agent 验证 architecture tests、syl_elab tests、driver_overlap、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出 Review SubAgent，按 Phase 4 MUST FIX 和退出标准独立审查 pass 边界真实性、DriverFacts/DRC 分离、EIR builder 边界、multi-driver facts basis、EIR dump、call stack diagnostics 和 cell/module boundary。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 4 未收敛：DriverFacts/DRC、call stack diagnostics、dump 与 cell/module boundary 已实质改善，但 `EirBuildPass` 仍经 `EirDesignAssembler::assemble` 内联执行 `EirValidator` 与 `EirFactCollector`，build/validation/diagnostic 责任未真实拆开；architecture_phase4_elab 未能拦住该边界泄漏。
  - 2026-05-24 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 做窄整改：拆开 raw EIR build、EIR validation、EIR facts collection 三个真实 pass，禁止 `EirBuildPass` 经由隐式 assembler 聚合验证和 facts，并补强 architecture_phase4_elab 的结构化边界守卫。
  - 2026-05-24 S4 - Work SubAgent 完成窄整改：`EirBuildPass` 改为只产出 raw EIR，新增独立 `EirValidationPass`、`EirFactsPass` 与 `EirComposePass`，`EirDesignAssembler` 收敛为不运行验证/事实收集的 composer，architecture_phase4_elab 增加 raw/facts 结构化断言与 pass 边界 guard；主 Agent 验证 architecture tests、syl_elab tests、driver_overlap、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 EIR build/validation/facts pass 边界整改是否满足 Phase 4，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-24 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 4 完成：EIR build/validation/facts 已真实拆成独立 pass，`EirBuildPass` 只构造 raw EIR，DriverFacts/DRC、multi-driver facts basis、EIR dump、call stack diagnostics 和 cell/module boundary 均满足 Phase 4 退出标准；主 Agent 将 Phase 4 标记为完成。

  ———

  ## [x] Phase 5：Opaque Boundary 与 Metadata

  目标：解决工业库、外部 IP、预编译包的问题。这是从玩具语言走向工业语言的分水岭。

  Chisel/FIRRTL/CIRCT 的经验是：不能只靠源码可见性，必须有 machine-readable metadata。

  MUST FIX：

  - extern module 摘要必须机器可读。
  - precompiled cell 必须带 semantic summary。
  - driver/capability/domain/layout/latency/protocol facts 必须能进入编译产物。
  - 用户不应手写全量 effect，但编译产物必须保存推导结果。
  - blackbox/vendor IP 必须有明确 trust boundary。

  建议 summary 内容：

  - ports/views/capabilities。
  - driven fields。
  - consumed fields。
  - domain behavior。
  - latency class。
  - layout/encoding。
  - protocol preservation。
  - unsafe/backend constraints。

  退出标准：

  - 一个没有源码的库仍然能参与 multi-driver 检查。
  - extern IP 不需要重复写 drives y 这种能从签名推导的信息。
  - public API 摘要能被 LSP、CLI、elab 共同读取。

  Log
  - 2026-05-24 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 5 Opaque Boundary 与 Metadata：建立 machine-readable extern/precompiled summary，确保 driver/capability/domain/layout/latency/protocol facts 可进入编译产物，定义 blackbox/vendor IP trust boundary，并补充无源码库参与 multi-driver 检查的可执行证据。
  - 2026-05-24 S1 - Work SubAgent 完成 Phase 5 初版交付：在 `syl_sema` 建立 machine-readable `OpaqueSummaryTable`/`OpaqueItemSummary`，extern 签名自动推导 source-derived summary，`syl_elab` 可注入 trusted/precompiled summary 并把 summary facts 保存到 hardware metadata，session/query 暴露只读 summary API，architecture_phase5_opaque 覆盖 extern out 自动 drive、无源码 summary 参与 DRC、trust boundary 和 query/session 边界；主 Agent 验证 architecture tests、syl_elab tests、driver_overlap、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出 Review SubAgent，按 Phase 5 MUST FIX 和退出标准独立审查 opaque/precompiled summary 模型、extern drive 推导、编译产物 metadata、trust boundary、query/session/elab 共享边界和无源码库 multi-driver 检查证据。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 5 未收敛：extern drive 自动推导、trusted summary 进入 DRC 与结构化 trust boundary 已成立，但 injected trusted/precompiled summaries 只在 `syl_elab` 合并，`syl_session`/`syl_query`/LSP 仍只读取 source-derived sema summaries，未满足公共 summary API 被 LSP、CLI、elab 共同读取的退出标准；现有测试也未覆盖 merged overlay 经 snapshot/query 可见。
  - 2026-05-24 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：将 trusted/precompiled opaque summary overlay 上移到 session/snapshot 可见的共享输入或 registry，使 session/query/LSP 与 elab/CLI 读取同一 merged summary surface，补充 snapshot/query merged overlay 结构化测试，并避免让 query 依赖 elab 或变成 DTO 垃圾桶。
  - 2026-05-24 S4 - Work SubAgent 完成整改：`AnalysisDatabase`/`AnalysisHost` 增加 workspace-level opaque summary overlay 注册入口，`SemanticCache` 产出 source-derived 与 overlay 合并后的 summary surface，`AnalysisSnapshot`/`AnalysisQueries` 读取同一 merged summaries，session-triggered elab 使用同一 overlay 输入；architecture_phase5_opaque 增加 host-registered overlay 经 snapshot/query 可见并参与 DRC 的结构化测试；主 Agent 验证 architecture tests、syl_elab tests、driver_overlap、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 session/shared overlay 整改是否满足 Phase 5，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-24 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 5 完成：opaque summary surface 已从 elab-only 合并改为 session-owned 共享输入，snapshot/query/elab 读取同一 merged summaries，extern/precompiled summary、metadata persistence、trust boundary 和无源码 multi-driver 检查均满足 Phase 5 退出标准；主 Agent 将 Phase 5 标记为完成。

  ———

  ## [x] Phase 6：Backend 与验证层分离

  目标：不要让 SystemVerilog emitter 变成最后的语义垃圾处理器。

  MUST FIX：

  - HW IR normalization 和 SV emission 分离。
  - backend-independent checks 放在 HW 层或 elab validation 层。
  - SV emitter 只负责合法打印和少量目标语言约束检查。
  - Verilator smoke test 进入 CI。
  - golden SV output 进入回归测试。

  长期应该支持：

  - SystemVerilog backend。
  - HW IR textual dump。
  - source map。
  - assertion/formal hook。
  - waveform/source trace metadata。
  - backend feature flags。

  退出标准：

  - 同一个 HW IR 可以输出 debug dump 和 SV。
  - emitter 不需要知道 HIR/TIR。
  - Verilator 可以覆盖 examples 和 integration cases。

  Log
  - 2026-05-24 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 6 Backend 与验证层分离：拆清 HW IR normalization/checks 与 SystemVerilog emission，确保 emitter 不承担前端语义修复，补充 HW IR debug dump、SV golden output 和 Verilator smoke test 证据。
  - 2026-05-24 S1 - Work SubAgent 完成 Phase 6 初版交付：`syl_hw` 增加 backend-independent `HwValidator`/`HwNormalizer`，`syl_emit::SystemVerilogBackend` 先消费 normalized HW IR 再执行 SV emission 与 backend-local checks，新增 HW IR dump + SV 同源测试、全文 golden SV 回归、Verilator lint smoke 覆盖 example 与 integration fixture；主 Agent 验证 architecture tests、syl_emit tests、syl_hw tests、driver_overlap、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出 Review SubAgent，按 Phase 6 MUST FIX 和退出标准独立审查 HW normalization/checks 与 SV emission 分离、backend-independent checks 所在层、emitter 依赖边界、golden SV 回归和 Verilator smoke/CI 证据。
  - 2026-05-24 S2 - Review SubAgent 判定 PASS 且授权标记 Phase 6 完成：HW validation/normalization 已下沉到 `syl_hw`，SystemVerilog backend 只消费 normalized HW IR 并执行 backend-local checks，emitter 无 HIR/TIR/sema/elab 依赖，golden SV 与 Verilator smoke 已进入回归/CI 路径；主 Agent 将 Phase 6 标记为完成。

  ———

  ## [ ] Phase 7：Query / Session / LSP 增量化

  目标：让 IDE 不是“每次全量编译”，而是基于稳定 query key 的编译服务。

  MUST FIX：

  - session 管 workspace、VFS、package graph、cache invalidation。
  - query 只暴露 compiler facts，不拥有 workspace DTO。
  - lsp 只做协议转换，不塞 compiler state。
  - diagnostics 要能按 file/package/stage 分组。
  - 所有 long-running query 支持取消。

  建议模型：

  - WorkspaceSnapshot
  - PackageGraph
  - SourceDatabase
  - SyntaxQuery
  - SemaQuery
  - ElabQuery
  - DiagnosticQuery
  - LspAdapter

  退出标准：

  - 改一个文件只 invalidate 相关 package。
  - hover/completion 不触发完整 emit。
  - LSP 能在 parse/sema/elab 不同失败阶段给部分结果。

  Log
  - 2026-05-24 S1 - 主 Agent 派出 Work SubAgent，执行 Phase 7 Query / Session / LSP 增量化：强化 session workspace/VFS/package graph/cache invalidation，保持 query 只暴露 compiler facts、LSP 只做协议适配，补充分阶段 diagnostics、取消支持和 hover/completion 不触发完整 emit 的可执行证据。
  - 2026-05-24 S1 - Work SubAgent 完成 Phase 7 初版交付：`syl_session` 增加 cancellation、workspace/source/package snapshot 与 package-aware invalidation 基础，`syl_query` 增加 cancellable query entrypoints 与 grouped diagnostics，`syl_lsp` 增加 protocol-only adapter 并从 grouped diagnostics 发布 LSP diagnostics，architecture_phase7_query_lsp 覆盖 package cache reuse、query boundary、stage-grouped diagnostics、hover/completion no emit、cancellation 和 LSP adapter；主 Agent 验证 architecture tests、syl_session/query/lsp tests、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出 Review SubAgent，按 Phase 7 MUST FIX 和退出标准独立审查 session workspace/cache ownership、query compiler-facts-only 边界、LSP protocol adapter 边界、stage/file/package diagnostics、cancellation、partial result 和 hover/completion no-emit 证据。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 7 未收敛：session/query/LSP 边界、结构化 grouped diagnostics 和 hover/completion no-emit 基本成立，但 semantic cache 仍按 whole-workspace snapshot 建一个 `SemanticCache`，`A+B -> A+B'` 会重算 A，未满足改一个文件只 invalidate 相关 package；取消也只在阶段边界检查，LSP request/diagnostics 没有接入可取消 token，in-flight long-running query 不能停止。
  - 2026-05-24 S3 - 主 Agent 审核 Review 结论后派 Work SubAgent 整改：实现 package-granular semantic cache 或等价分片，使 `A+B -> A+B'` 复用 A 的 package semantic cache；将 cancellation token 接入 LSP request/diagnostics 与 query/session 长查询路径，补充 in-flight 或主动取消不会继续触发后续昂贵阶段的结构化测试。
  - 2026-05-24 S4 - Work SubAgent 完成整改：`syl_session` 增加 package semantic shard/index 与 package cache probe，`AnalysisSnapshot` 内部同时持有 workspace facade 与 package shards，grouped diagnostics 改为消费 package shard，`A+B -> A+B'` 测试证明 A shard 复用而 B shard 重建；`syl_lsp` 增加 cancellation registry，diagnostics generation 与 hover/definition/completion 使用 registry token，publish 路径调用 token-aware grouped diagnostics 并测试取消后不启动后续 package；主 Agent 验证 architecture tests、syl_session/query/lsp tests、workspace check、文件规模和 `git diff --check` 均通过。
  - 2026-05-24 S2 - 主 Agent 派出第二轮 Review SubAgent，复查 S4 package semantic shard 与 end-to-end cancellation 整改是否满足 Phase 7，只有 Review Agent 判 PASS 才允许标记完成。
  - 2026-05-24 S2 - Review SubAgent 判定 Phase 7 仍未收敛：package shards 已真实服务 grouped diagnostics，cancellation 已接入 diagnostics/request 并能停止后续 package；但 hover/definition/completion 等 navigation queries 仍经 `workspace_semantic` 的 whole-workspace HIR/TIR 路径，`A+B -> A+B'` 后查询 A 仍会重算 whole workspace，未满足 package-local reuse 的端到端退出标准。
  - 2026-05-24 S3 - 主 Agent 审核第二轮 Review 结论后派 Work SubAgent 做窄整改：将 hover/definition/completion/navigation queries 改为优先使用目标文件所属 package semantic shard，而不是 whole-workspace semantic cache，并新增 `A+B -> A+B'` 后对 A 执行 hover/completion 不启动 workspace semantic 且复用 A shard 的结构化测试。

  ———

  ## [ ] Phase 8：标准库与组合 API

  目标：补齐语言生态的“好用层”，但不污染核心编译器。

  MUST FIX：

  - std 应该作为普通 Syl 库存在，而不是 compiler magic。
  - Stream/Stage/Link API 的语义必须能被 compiler summary 表达。
  - 官方组合库不能绕过 driver/capability 检查。
  - 用户自定义 cell 必须和官方库享有同等组合能力。

  建议标准库分层：

  - std.logic：基础 bit/word/map。
  - std.bundle：layout/encoding helpers。
  - std.stream：ready-valid。
  - std.stage：pipeline/skid/register slice。
  - std.cdc：跨域原语。
  - std.vendor：extern wrapper pattern。
  - std.assert：property/formal helpers。

  退出标准：

  - examples 主要通过 std 组合，而不是手写底层 wire。
  - std 本身也通过同一套 checker。
  - std 的 public summaries 可用于 opaque library 测试。

  ———

  ## [ ] Phase 9：工业质量门槛

  目标：让项目可以承载长期外部贡献和真实硬件项目。

  MUST FIX：

  - conformance suite。
  - error code 稳定化。
  - public API review。
  - crate-level MSRV / feature policy。
  - fuzz parser。
  - differential tests。
  - snapshot tests。
  - release metadata。
  - compatibility tests for examples/std。

  需要建立的质量线：

  - cargo fmt
  - cargo clippy --workspace --all-targets
  - cargo test --workspace
  - parser fuzz。
  - examples parse/sema/elab/emit。
  - Verilator smoke。
  - documentation syntax check。
  - public API surface check。

  退出标准：

  - 新增语法必须带 parser/sema/elab/backend 测试。
  - 新增 IR 字段必须解释 owner 和 lifecycle。
  - 新增 public API 必须说明消费者。

  ———
