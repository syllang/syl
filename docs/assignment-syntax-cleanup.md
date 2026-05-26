# Assignment and Register Update Cleanup

本文记录 Syl 在发布前对 `=`、`:=`、`next` 的收敛决策。目标不是做兼容修补，而是一次性清掉会持续误导用户的语义重叠。

## 背景

当前实现里，`=` 和 `:=` 在少数位置被当作兼容写法处理。这会带来三个直接问题：

- 用户看到的“赋值”表面一致，但实际语义分属 binding、continuous drive、register next-state 三类。
- 解析和语义层把赋值长期压成同一个通道，导致文档、诊断、LSP 和 lowering 都需要猜上下文。
- `reg` 如果还能被普通 `:=` 直接驱动，就会和 `next` 形成两套“写寄存器”表面语法，语言模型会变脏。

Syl 还未发布，因此这里采用破坏性清理，而不是兼容过渡。

## 语言级决策

### 1. 运算符职责固定

- `=` 只用于 binding、声明初始化、参数默认值，以及 `fn`/software-elaboration 子集里的赋值。
- `:=` 只用于 hardware continuous drive。
- `next x := expr` 是唯一合法的寄存器写法。

这里的关键不是“看起来像赋值”，而是语义事实必须一眼可见：

- `=` 表示绑定或软件更新，不表示硬件驱动。
- `:=` 表示当前拍的连续驱动。
- `next` 表示下一拍更新。

### 2. `reg` 只能通过 `next` 更新

- `reg` 可读。
- `reg` 不能被 `:=` 直接驱动。
- `reg` 的 field、slice、index 也不能被 `:=` 直接驱动。
- `reg` 的更新只能写成 `next reg_name := expr`。

这条规则是硬约束，不是 lint 建议。

### 3. 不保留兼容写法

以下写法全部视为非法：

```syl
signal x: Bit = 0
next r = x
let x := y
var x := y
y = x        // 出现在 hardware block 中
x := y       // 出现在 fn/software block 中
r := 0       // r 是 reg
r.field := 0 // r 是 reg
r[i] := 1    // r 是 reg
```

以下写法是 canonical form：

```syl
const N: Nat = 4
let x = place Foo(...)
var i: Nat = 0

signal ready: Bit := down.ready
out.valid := full
next state := next_state
```

## 语义边界

### 1. `=` 不等于“纯值”

`=` 自身不是 drive operator，但 RHS 仍然可以带有编译器可见 effect。例如：

```syl
let buffered = place skid_buffer(...)
```

这里 `let buffered = ...` 仍然是 binding 语法；真正的硬件结构 effect 来自 `place` 调用，而不是来自 `=` 本身。整改后仍保留这一点，不能把 `=` 粗暴解释成“纯软件语句”。

### 2. `:=` 只表达 continuous drive

`:=` 不再兼任 binding 初始化，也不再作为通用 assignment expression 入口。它只表示：

- `signal x: T := expr`
- `target := expr`

### 3. `next` 只表达 register next-state

`next` 保持独立语义位点，不和 `=` 共用解释路径。后续若要支持 `next` 的 field 或 slice 更新，必须单独设计，不在本轮整改中顺手引入。

当前决策是：

- 只支持 `next reg_name := expr`
- 不支持 `next reg.field := expr`
- 不支持 `next reg[i] := expr`

## 编译器整改方向

### 1. AST 和语法树分层

不能继续把 assignment 表达成 `Expr::Binary(Assign)`。这会把软件赋值和硬件驱动再次压扁。

应改为显式语句节点分层：

- `Stmt::Assign { target, value, span }`
- `Stmt::Drive { target, value, span }`
- `Stmt::Next { name, value, span }`

同时移除通用 `BinaryOp::Assign` 路径。

结果是：

- `fn`/software-elaboration 里的赋值是 `Stmt::Assign`
- hardware block 里的 `y := x` 是 `Stmt::Drive`
- `next r := x` 是 `Stmt::Next`

### 2. Parser 必须带 block context

`parse_block` 不能再无差别地把 assignment 吞进普通表达式解析。需要区分：

- `fn` block
- hardware block（`cell` / `module`）

收紧规则如下：

- `parse_let_stmt` 只接受 `=`
- `parse_var_stmt` 只接受 `=`
- `parse_signal_stmt` 只接受 `:=`
- `parse_next_stmt` 只接受 `:=`
- hardware block 中的 `target := expr` 解析成 `Stmt::Drive`
- `fn` block 中的 `target = expr` 解析成 `Stmt::Assign`
- hardware block 中出现裸 `=` assignment 直接报错
- `fn` block 中出现 `:=` 直接报错

### 3. Semantic/TIR 规则

- `Stmt::Assign` 只允许出现在 `fn`/software-elaboration 子集
- `Stmt::Drive` 只允许出现在 hardware block
- `Stmt::Next` 只允许出现在 hardware block，且目标必须解析到 `reg`
- `Stmt::Drive` 的 target 如果根对象解析到 storage/reg，直接报错

这层应该给用户主诊断，不要把此类错误留到更晚的 lowering 或 DRC 才发现。

### 4. Elaboration / Driver 规则

- `Stmt::Drive` lowering 为 continuous drive
- `Stmt::Assign` 只进入 const MIR / software lowering 路径
- `Stmt::Next` lowering 为 clocked storage next-state

Driver/Drc 仍需保留防御性不变量检查：

- continuous drive 命中 storage root 时必须拒绝
- next target 仍必须是 storage

前者面向用户诊断，后者面向内部不变量兜底。

### 5. LSP / Completion / Query 同步收口

编辑器能力不能继续鼓励旧写法。

需要同步收紧：

- `:=` 后进入 hardware expression completion
- `let/var/const ... =` 后进入 binding/software expression completion
- 不再因为“行尾是 `=`”就笼统视为 hardware expression context
- `next x =` 不应继续被视为合法上下文

## 文档与示例要求

所有面向用户的文档和示例都必须统一口径，不再出现“实现上也接受另一种写法”的表述。

统一口径如下：

- `=` 是 binding / declaration / software assignment
- `:=` 是 continuous drive
- `next x := expr` 是唯一 register write form
- `reg` 可以读，但不能被直接 drive

应同步更新：

- 语法草图和设计说明
- RFC 中对 `=` / `:=` / `next` 的描述
- examples 中相关注释
- 诊断文档中的示例代码

## 测试门禁

本整改必须附带负例测试，确保歧义不会回流。

至少覆盖以下场景：

- `signal x: Bit = 0` 报错
- `next r = x` 报错
- `let x := y` 报错
- `var x := y` 报错
- hardware block 中 `y = x` 报错
- `fn` 中 `x := y` 报错
- `reg r ...; r := 0` 报错
- `reg r ...; r.field := 0` 报错
- `reg r ...; r[i] := 1` 报错
- `next r := expr` 继续通过
- 普通 `signal`、`out`、result binding 的 `:=` 继续通过
- `fn` 中 `var x = ...; x = ...` 继续通过

Parser recovery、query、LSP snapshot 也要一起更新，因为它们不应继续依赖旧的 `BinaryOp::Assign` 表示。

## 实施顺序

建议按以下顺序落地：

1. 先改 AST，拆开 `Assign` / `Drive` / `Next`
2. 再改 parser，引入 block context 并移除 assignment binary-op 路径
3. 再改 sema/TIR，按语义层硬约束区分软件赋值和硬件驱动
4. 再改 elaboration / const MIR / driver lowering
5. 再补 direct-reg-drive 禁止规则和诊断
6. 最后统一修 LSP、examples、docs、tests

## 非目标

本轮整改不解决以下问题：

- `next reg.field := expr` 的细粒度次态更新语法
- `next reg[i] := expr` 的次态局部更新语法
- 各类 effect system 或用户可见 contract 语法

这些都应在未来单独设计，不应搭车混入这次 operator cleanup。
