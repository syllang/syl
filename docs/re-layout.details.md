# eir.rs 机械拆分方案

> 执行时间：~30 分钟。所有决策已锁定，不需要中间判断。
> 验证命令：`cargo check -p syl_elab && cargo test -p syl_elab`

---

## 操作概览

```
eir.rs (592行, 1文件)
  ↓ git mv + 拆出 3 文件
eir/mod.rs + design.rs + signal.rs + module.rs (4文件, 合计 ~592行)
```

**对 lib.rs 和外部消费者零影响** — 只需要 `mod eir`（已有），路径 `crate::eir::XXX` 不变。

已有子模块 `eir/assemble.rs`、`eir/facts.rs`、`eir/validate.rs` **不需要改** — 它们用 `use super::` 导入，`super` 现在指向 `eir/mod.rs`，而 `mod.rs` 重导出了所有类型。

---

## Step 1 — 创建目录并移动文件

```bash
# 把 eir.rs 变成目录的 mod.rs
mkdir -p crates/syl_elab/src/eir
mv crates/syl_elab/src/eir.rs crates/syl_elab/src/eir/mod.rs
```

---

## Step 2 — 创建 design.rs

从 `mod.rs`（原名 `eir.rs`）中摘出 **设计容器** 相关的 3 个结构体及其 impl。

### 文件内容

```rust
//! 顶层设计容器。
//!
//! 原始设计 → 事实收集 → 最终设计 的构造链。

use std::sync::Arc;

use crate::eir::module::EirModule;
use crate::eir::signal::{EirDrive, EirObject, EirRead};

// ── EirRawDesign ──

#[non_exhaustive]
pub(crate) struct EirRawDesign {
    modules: Vec<EirModule>,
}

impl EirRawDesign {
    pub(crate) fn new(modules: Vec<EirModule>) -> Self {
        Self { modules }
    }

    pub(crate) fn modules(&self) -> &[EirModule] {
        &self.modules
    }
}

// ── EirDesign ──

#[non_exhaustive]
pub(crate) struct EirDesign {
    raw: Arc<EirRawDesign>,
    facts: Arc<EirDesignFacts>,
}

impl EirDesign {
    pub(crate) fn from_parts(raw: Arc<EirRawDesign>, facts: Arc<EirDesignFacts>) -> Self {
        Self { raw, facts }
    }

    pub(crate) fn modules(&self) -> &[EirModule] {
        self.raw.modules()
    }

    pub(crate) fn objects(&self) -> &[EirObject] {
        self.facts.objects()
    }

    pub(crate) fn drives(&self) -> &[EirDrive] {
        self.facts.drives()
    }

    pub(crate) fn reads(&self) -> &[EirRead] {
        self.facts.reads()
    }
}

// ── EirDesignFacts ──

#[non_exhaustive]
pub(crate) struct EirDesignFacts {
    objects: Vec<EirObject>,
    drives: Vec<EirDrive>,
    reads: Vec<EirRead>,
}

impl EirDesignFacts {
    pub(crate) fn new(
        objects: Vec<EirObject>,
        drives: Vec<EirDrive>,
        reads: Vec<EirRead>,
    ) -> Self {
        Self {
            objects,
            drives,
            reads,
        }
    }

    pub(crate) fn objects(&self) -> &[EirObject] {
        &self.objects
    }

    pub(crate) fn drives(&self) -> &[EirDrive] {
        &self.drives
    }

    pub(crate) fn reads(&self) -> &[EirRead] {
        &self.reads
    }
}
```

### 摘取范围（对照原 `eir.rs`）

| 原行 | 内容 | 处理 |
|------|------|------|
| 1-8 | `use` 语句 | 替换为新的三行 import |
| 10-12 | `mod assemble; mod facts; mod validate;` | **不留** — 属于 `mod.rs` |
| 14-17 | `pub(crate) use ...` | **不留** — 属于 `mod.rs` |
| 19-33 | `EirRawDesign` + impl | 原样移入 |
| 35-61 | `EirDesign` + impl | 原样移入 |
| 63-89 | `EirDesignFacts` + impl | 原样移入 |

---

## Step 3 — 创建 signal.rs

从 `mod.rs` 中摘出 **信号对象 + 驱动 + 读取 + 复位**。

### 文件内容

```rust
//! 信号对象及其行为：驱动、读取、复位。

use crate::eir_expr::{EirBound, EirExpr};
use crate::eir_guard::EirGuard;
use crate::eir_origin::EirOrigin;
use crate::eir_place::EirPlace;

// ── EirObject ──

#[non_exhaustive]
pub(crate) struct EirObject {
    module: String,
    name: String,
    width: EirBound,
    kind: EirObjectKind,
    activity: EirSignalActivity,
    origin: EirOrigin,
}

#[non_exhaustive]   // ← 修复：补上缺失的 #[non_exhaustive]
pub(crate) struct EirObjectInput {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) width: EirBound,
    pub(crate) kind: EirObjectKind,
    pub(crate) activity: EirSignalActivity,
    pub(crate) origin: EirOrigin,
}

impl EirObject {
    pub(crate) fn new(input: EirObjectInput) -> Self { /* 原样 */ }

    pub(crate) fn module(&self) -> &str { /* 原样 */ }
    pub(crate) fn name(&self) -> &str { /* 原样 */ }
    pub(crate) fn width_bound(&self) -> &EirBound { /* 原样 */ }
    pub(crate) fn kind(&self) -> EirObjectKind { /* 原样 */ }
    pub(crate) fn activity(&self) -> EirSignalActivity { /* 原样 */ }
    pub(crate) fn origin(&self) -> &EirOrigin { /* 原样 */ }
}

// ── EirObjectKind ──

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum EirObjectKind { Signal, Storage }

// ── EirSignalActivity ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirSignalActivity { Required, Optional }

// ── EirDrive ──

#[non_exhaustive]
pub(crate) struct EirDrive {
    module: String,
    target: EirPlace,
    value: Option<EirExpr>,
    kind: EirDriveKind,
    guard: EirGuard,
    origin: EirOrigin,
}

impl EirDrive {
    /// 用参数对象避免 too_many_arguments（字段数 > 5）。
    pub(crate) fn new(input: EirDriveInput) -> Self {
        Self {
            module: input.module,
            target: input.target,
            kind: input.kind,
            value: input.value,
            guard: input.guard,
            origin: input.origin,
        }
    }

    pub(crate) fn module(&self) -> &str { /* 原样 */ }
    pub(crate) fn target_place(&self) -> &EirPlace { /* 原样 */ }
    pub(crate) fn value(&self) -> Option<&EirExpr> { /* 原样 */ }
    pub(crate) fn kind(&self) -> EirDriveKind { /* 原样 */ }
    pub(crate) fn guard(&self) -> &EirGuard { /* 原样 */ }
    pub(crate) fn origin(&self) -> &EirOrigin { /* 原样 */ }
}

#[non_exhaustive]
/// 参数对象 — 替换原本的 6 参数 `EirDrive::new`。
pub(crate) struct EirDriveInput {
    pub(crate) module: String,
    pub(crate) target: EirPlace,
    pub(crate) kind: EirDriveKind,
    pub(crate) value: Option<EirExpr>,
    pub(crate) guard: EirGuard,
    pub(crate) origin: EirOrigin,
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum EirDriveKind { Continuous, Next }

// ── EirRead ──

#[non_exhaustive]
pub(crate) struct EirRead {
    module: String,
    source: EirPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl EirRead {
    pub(crate) fn new(/* 原样 4 参数 — 未超限 */) -> Self { /* 原样 */ }
    pub(crate) fn module(&self) -> &str { /* 原样 */ }
    pub(crate) fn source_place(&self) -> &EirPlace { /* 原样 */ }
    pub(crate) fn guard(&self) -> &EirGuard { /* 原样 */ }
    pub(crate) fn origin(&self) -> &EirOrigin { /* 原样 */ }
}

// ── EirReset ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirReset {
    condition: EirExpr,
    value: EirExpr,
}

impl EirReset {
    pub(crate) fn new(condition: EirExpr, value: EirExpr) -> Self { /* 原样 */ }
    pub(crate) fn condition(&self) -> &EirExpr { /* 原样 */ }
    pub(crate) fn value(&self) -> &EirExpr { /* 原样 */ }
}
```

> **为什么 EirReset 放 signal.rs 不是 module.rs？**
> `EirReset` 语义上是信号行为（什么时候复位、复位值是什么），不是模块结构。
> 它只有两个字段 `condition: EirExpr`、`value: EirExpr`，和信号行为层的亲和度更高。
> `module.rs` 通过 `use crate::eir::signal::EirReset` 引入它——单向依赖，不循环。

### 摘取范围

| 原行 | 内容 | 处理 |
|------|------|------|
| 92-107 | `EirObject` + `EirObjectInput` | 移入，`EirObjectInput` 加 `#[non_exhaustive]` |
| 111-147 | `impl EirObject` | 移入 |
| 149-153 | `EirObjectKind` | 移入 |
| 156-159 | `EirSignalActivity` | 移入 |
| 162-216 | `EirDrive` + impl | 移入，`new` 改为参数对象模式 |
| 218-222 | `EirDriveKind` | 移入 |
| 224-263 | `EirRead` + impl | 移入 |
| 472-490 | `EirReset` + impl | 移入 |

---

## Step 4 — 创建 module.rs

从 `mod.rs` 中摘出 **模块结构** 相关内容。

### 文件内容

```rust
//! 模块结构：模块定义、端口、参数、实例化。

use crate::{
    CellBoundarySummary,
    eir_cell::EirCellExpansion,
    eir_expr::{EirBound, EirExpr},
    eir_origin::EirOrigin,
    eir_place::EirPlace,
};
use crate::eir::signal::{EirReset, EirSignalActivity};

// ── EirModule ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirModule {
    name: String,
    kind: EirModuleKind,
    params: Vec<EirParam>,
    ports: Vec<EirPort>,
    items: Vec<EirItem>,
}

impl EirModule { /* 原样 — 4 参数 new，不超限 */ }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EirModuleKind { Defined, Extern }

// ── EirParam ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirParam { /* 原样 */ }
impl EirParam { /* 原样 */ }

// ── EirPort ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirPort { /* 原样 */ }
impl EirPort { /* 原样 */ }

// ── EirDirection ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirDirection { In, InOut, Out }

// ── EirItem ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum EirItem { /* 10 个变体，原样 */ }

// ── EirInstance ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirInstance { /* 原样 */ }
impl EirInstance { /* 原样 */ }

// ── EirParamBind ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirParamBind { /* 原样 */ }
impl EirParamBind { /* 原样 */ }

// ── EirConnection ──

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirConnection { /* 原样 */ }
impl EirConnection { /* 原样 */ }
```

### 摘取范围

| 原行 | 内容 |
|------|------|
| 265-330 | `EirModule` + impl |
| 332-354 | `EirParam` + impl |
| 356-400 | `EirPort` + impl |
| 401-406 | `EirDirection` |
| 409-470 | `EirItem` |
| 493-546 | `EirInstance` + impl |
| 548-570 | `EirParamBind` + impl |
| 572-592 | `EirConnection` + impl |

---

## Step 5 — 重写 mod.rs

将 `mod.rs`（原 `eir.rs`）替换为模块根文件，只保留子模块声明 + 重导出。

```rust
//! EIR (Elaboration Intermediate Representation) 数据模型。
//!
//! 按职责域拆分：
//! - [`design`]  — 顶层容器：`EirRawDesign`, `EirDesign`, `EirDesignFacts`
//! - [`signal`]  — 信号对象及其行为：驱动、读取、复位
//! - [`module`]  — 模块结构：端口、参数、实例化、项目枚举
//!
//! 所有 `pub(crate)` 类型均标记 `#[non_exhaustive]`。
//! 构造器参数超过 5 个的使用参数对象或 Builder 模式。

#![deny(unsafe_code)]

mod design;
mod signal;
mod module;

mod assemble;
mod facts;
mod validate;

// ── 设计容器 ──
pub(crate) use design::{EirDesign, EirDesignFacts, EirRawDesign};

// ── 信号域 ──
pub(crate) use signal::{
    EirDrive, EirDriveInput, EirDriveKind, EirObject, EirObjectInput, EirObjectKind, EirRead,
    EirSignalActivity,
};

// ── 模块结构 ──
// EirReset 语义上属于信号行为，但被 module::EirItem::ClockedStorage 使用，
// 统一从 mod.rs 导出，调用方无需感知。
pub(crate) use module::{
    EirConnection, EirDirection, EirInstance, EirItem, EirModule, EirParam, EirParamBind, EirPort,
};

pub(crate) use signal::EirReset;

// ── 子模块公共 API ──
pub(crate) use assemble::EirDesignComposer;
pub(crate) use facts::EirFactCollector;
pub(crate) use validate::EirValidator;
```

---

## Step 6 — 更新调用方 `eir/facts.rs`

`eir/facts.rs` 里 5 处 `EirDrive::new(6个参数)` 全部改为 `EirDrive::new(EirDriveInput { ... })`。

### 改动位置

| 文件 | 行号 | 原代码 | 改为 |
|------|------|--------|------|
| `eir/facts.rs` | 112-119 | `EirDrive::new(&self.module, lhs.clone(), EirDriveKind::Continuous, Some(rhs.clone()), self.guard(), origin.clone())` | `EirDrive::new(EirDriveInput { module: self.module.clone(), target: lhs.clone(), kind: EirDriveKind::Continuous, value: Some(rhs.clone()), guard: self.guard(), origin: origin.clone() })` |
| `eir/facts.rs` | 128-134 | `EirDrive::new(&self.module, target.clone(), EirDriveKind::Next, None, self.guard(), origin.clone())` | `EirDrive::new(EirDriveInput { module: self.module.clone(), target: target.clone(), kind: EirDriveKind::Next, value: None, guard: self.guard(), origin: origin.clone() })` |
| `eir/facts.rs` | 276-282 | 同上模式（Continuous） | 同上改为 `EirDriveInput` |
| `eir/facts.rs` | 294-300 | 同上模式（Continuous） | 同上改为 `EirDriveInput` |
| `eir/facts.rs` | 323-330 | 同上模式（Continuous） | 同上改为 `EirDriveInput` |

另外，`eir/facts.rs` 需要在顶部的 `use super::{...}` 中加 `EirDriveInput`。

### 调用方清单确认

```bash
# 当前：EirDrive::new(6个参数)
# 改为：EirDrive::new(EirDriveInput { ... })
EirDrive::new 的 5 处调用全部在 eir/facts.rs 中，无其他文件。
```

---

## Step 7 — 验证

```bash
cargo check -p syl_elab 2>&1 | head -50
# 预期：编译通过，零 warning 新增

cargo test -p syl_elab 2>&1 | tail -20
# 预期：所有测试通过
```

---

## 变更总览

| 操作 | 文件 |
|------|------|
| 删除 | `crates/syl_elab/src/eir.rs` |
| 创建 | `crates/syl_elab/src/eir/mod.rs` |
| 创建 | `crates/syl_elab/src/eir/design.rs` |
| 创建 | `crates/syl_elab/src/eir/signal.rs` |
| 创建 | `crates/syl_elab/src/eir/module.rs` |
| 修改 | `crates/syl_elab/src/eir/facts.rs` — 5 处调用 + 1 行 import |

**不变的文件：** `lib.rs`、`eir/assemble.rs`、`eir/validate.rs`、`eir_cell.rs`、`eir_body.rs`、`hw_lower.rs`、`pipeline.rs` 等所有外部消费者。

---

## 回滚方案

如果编译失败且无法立即修复：

```bash
# 恢复原状
rm -rf crates/syl_elab/src/eir/
git checkout crates/syl_elab/src/eir.rs
```

`eir/facts.rs` 的改动用 `git checkout crates/syl_elab/src/eir/facts.rs` 恢复。
