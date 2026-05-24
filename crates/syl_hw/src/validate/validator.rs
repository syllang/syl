use super::{HwBindingKind, HwValidationDiagnostic, HwValidationReport};
use crate::{HwExpr, HwItem, ParametricHwDesign, ParametricHwItem, ParametricHwModule};
use std::collections::{BTreeMap, BTreeSet};

struct ModuleInterface<'a> {
    params: BTreeSet<&'a str>,
    ports: BTreeSet<&'a str>,
}

pub(super) struct Validator<'a> {
    design: &'a ParametricHwDesign,
    diagnostics: Vec<HwValidationDiagnostic>,
    module_interfaces: BTreeMap<&'a str, ModuleInterface<'a>>,
}

impl<'a> Validator<'a> {
    pub(super) fn new(design: &'a ParametricHwDesign) -> Self {
        Self {
            design,
            diagnostics: Vec::new(),
            module_interfaces: BTreeMap::new(),
        }
    }

    pub(super) fn validate(&mut self) {
        self.collect_module_interfaces();
        for module in self.design.modules() {
            self.validate_module(module);
        }
    }

    pub(super) fn finish(self) -> Result<(), HwValidationReport> {
        if self.diagnostics.is_empty() {
            return Ok(());
        }
        Err(HwValidationReport::new(self.diagnostics))
    }

    fn collect_module_interfaces(&mut self) {
        for module in self.design.modules() {
            self.check_identifier(None, HwBindingKind::Module, module.name());
            if self
                .module_interfaces
                .insert(
                    module.name(),
                    ModuleInterface {
                        params: module.params().iter().map(|param| param.name()).collect(),
                        ports: module.ports().iter().map(|port| port.name()).collect(),
                    },
                )
                .is_some()
            {
                self.push(HwValidationDiagnostic::DuplicateModule {
                    name: module.name().to_string(),
                });
            }
        }
    }

    fn validate_module(&mut self, module: &'a ParametricHwModule) {
        let mut scope = Scope::new();
        for param in module.params() {
            self.check_identifier(Some(module.name()), HwBindingKind::Parameter, param.name());
            scope.declare_visible(module.name(), HwBindingKind::Parameter, param.name(), self);
        }
        for port in module.ports() {
            self.check_identifier(Some(module.name()), HwBindingKind::Port, port.name());
            self.check_width(
                module.name(),
                HwBindingKind::Port,
                port.name(),
                port.width(),
            );
            scope.declare_visible(module.name(), HwBindingKind::Port, port.name(), self);
        }
        self.validate_items(module, module.items(), &mut scope);
    }

    fn validate_items(
        &mut self,
        module: &'a ParametricHwModule,
        items: &'a [ParametricHwItem],
        scope: &mut Scope<'a>,
    ) {
        self.declare_items(module, items, scope);
        for item in items {
            self.validate_item(module, item, scope);
        }
    }

    fn declare_items(
        &mut self,
        module: &'a ParametricHwModule,
        items: &'a [ParametricHwItem],
        scope: &mut Scope<'a>,
    ) {
        for item in items {
            match item {
                ParametricHwItem::Core { item, .. } => {
                    self.declare_core_item(module, item, scope);
                }
                ParametricHwItem::StaticIf { label, .. }
                | ParametricHwItem::StaticFor { label, .. } => {
                    self.check_identifier(Some(module.name()), HwBindingKind::GenerateLabel, label);
                    scope.declare_hidden(module.name(), HwBindingKind::GenerateLabel, label, self);
                }
            }
        }
    }

    fn declare_core_item(
        &mut self,
        module: &'a ParametricHwModule,
        item: &'a HwItem,
        scope: &mut Scope<'a>,
    ) {
        match item {
            HwItem::StaticParam { name, .. } => {
                self.check_identifier(Some(module.name()), HwBindingKind::LocalParam, name);
                scope.declare_visible(module.name(), HwBindingKind::LocalParam, name, self);
            }
            HwItem::SignalDecl { width, name } => {
                self.check_identifier(Some(module.name()), HwBindingKind::Signal, name);
                self.check_width(module.name(), HwBindingKind::Signal, name, width);
                scope.declare_visible(module.name(), HwBindingKind::Signal, name, self);
            }
            HwItem::StorageDecl { width, name } => {
                self.check_identifier(Some(module.name()), HwBindingKind::Storage, name);
                self.check_width(module.name(), HwBindingKind::Storage, name, width);
                scope.declare_visible(module.name(), HwBindingKind::Storage, name, self);
            }
            HwItem::Instance(instance) => {
                self.check_identifier(
                    Some(module.name()),
                    HwBindingKind::Instance,
                    instance.name(),
                );
                scope.declare_hidden(
                    module.name(),
                    HwBindingKind::Instance,
                    instance.name(),
                    self,
                );
            }
            HwItem::StaticIf { label, .. } | HwItem::StaticFor { label, .. } => {
                self.check_identifier(Some(module.name()), HwBindingKind::GenerateLabel, label);
                scope.declare_hidden(module.name(), HwBindingKind::GenerateLabel, label, self);
            }
            HwItem::ContinuousDrive { .. }
            | HwItem::ClockedStorage { .. }
            | HwItem::InitialError { .. } => {}
        }
    }

    fn validate_item(
        &mut self,
        module: &'a ParametricHwModule,
        item: &'a ParametricHwItem,
        scope: &mut Scope<'a>,
    ) {
        match item {
            ParametricHwItem::Core { item, .. } => self.validate_core_item(module, item, scope),
            ParametricHwItem::StaticIf {
                cond,
                then_items,
                else_items,
                ..
            } => {
                self.validate_expr(module, cond, scope);
                let mut then_scope = scope.child();
                self.validate_items(module, then_items, &mut then_scope);
                let mut else_scope = scope.child();
                self.validate_items(module, else_items, &mut else_scope);
            }
            ParametricHwItem::StaticFor {
                index,
                start,
                end,
                items,
                ..
            } => {
                self.check_identifier(Some(module.name()), HwBindingKind::GenerateIndex, index);
                self.validate_expr(module, start, scope);
                self.validate_expr(module, end, scope);
                let mut loop_scope = scope.child();
                loop_scope.declare_visible(
                    module.name(),
                    HwBindingKind::GenerateIndex,
                    index,
                    self,
                );
                self.validate_items(module, items, &mut loop_scope);
            }
        }
    }

    fn validate_core_item(
        &mut self,
        module: &'a ParametricHwModule,
        item: &'a HwItem,
        scope: &Scope<'a>,
    ) {
        match item {
            HwItem::StaticParam { value, .. } => self.validate_expr(module, value, scope),
            HwItem::SignalDecl { .. } | HwItem::StorageDecl { .. } => {}
            HwItem::ContinuousDrive { lhs, rhs } => {
                self.validate_expr(module, lhs, scope);
                self.validate_expr(module, rhs, scope);
            }
            HwItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
            } => {
                self.validate_expr(module, clock, scope);
                self.validate_expr(module, target, scope);
                if let Some(reset) = reset {
                    self.validate_expr(module, reset.condition(), scope);
                    self.validate_expr(module, reset.value(), scope);
                }
                self.validate_expr(module, next, scope);
            }
            HwItem::Instance(instance) => self.validate_instance(module, instance, scope),
            HwItem::StaticIf {
                cond,
                then_items,
                else_items,
                ..
            } => {
                self.validate_expr(module, cond, scope);
                let mut then_scope = scope.child();
                self.declare_hw_items(module, then_items, &mut then_scope);
                self.validate_hw_items(module, then_items, &mut then_scope);
                let mut else_scope = scope.child();
                self.declare_hw_items(module, else_items, &mut else_scope);
                self.validate_hw_items(module, else_items, &mut else_scope);
            }
            HwItem::StaticFor {
                index,
                start,
                end,
                items,
                ..
            } => {
                self.check_identifier(Some(module.name()), HwBindingKind::GenerateIndex, index);
                self.validate_expr(module, start, scope);
                self.validate_expr(module, end, scope);
                let mut loop_scope = scope.child();
                loop_scope.declare_visible(
                    module.name(),
                    HwBindingKind::GenerateIndex,
                    index,
                    self,
                );
                self.declare_hw_items(module, items, &mut loop_scope);
                self.validate_hw_items(module, items, &mut loop_scope);
            }
            HwItem::InitialError { message } => self.validate_expr(module, message, scope),
        }
    }

    fn declare_hw_items(
        &mut self,
        module: &'a ParametricHwModule,
        items: &'a [HwItem],
        scope: &mut Scope<'a>,
    ) {
        for item in items {
            self.declare_core_item(module, item, scope);
        }
    }

    fn validate_hw_items(
        &mut self,
        module: &'a ParametricHwModule,
        items: &'a [HwItem],
        scope: &mut Scope<'a>,
    ) {
        for item in items {
            self.validate_core_item(module, item, scope);
        }
    }

    fn validate_instance(
        &mut self,
        module: &'a ParametricHwModule,
        instance: &'a crate::HwInstance,
        scope: &Scope<'a>,
    ) {
        let Some(interface) = self.module_interfaces.get(instance.module()) else {
            self.push(HwValidationDiagnostic::UnknownInstanceTarget {
                module: module.name().to_string(),
                instance: instance.name().to_string(),
                target: instance.module().to_string(),
            });
            for connection in instance.connections() {
                self.validate_expr(module, connection.actual(), scope);
            }
            return;
        };
        let target_params = interface.params.clone();
        let target_ports = interface.ports.clone();

        let mut seen_params = BTreeSet::new();
        for param in instance.params() {
            if !seen_params.insert(param.name()) {
                self.push(HwValidationDiagnostic::DuplicateInstanceBinding {
                    module: module.name().to_string(),
                    instance: instance.name().to_string(),
                    kind: HwBindingKind::Parameter,
                    name: param.name().to_string(),
                });
                continue;
            }
            if !target_params.contains(param.name()) {
                self.push(HwValidationDiagnostic::UnknownInstanceParam {
                    module: module.name().to_string(),
                    instance: instance.name().to_string(),
                    target: instance.module().to_string(),
                    name: param.name().to_string(),
                });
            }
        }

        let mut seen_ports = BTreeSet::new();
        for connection in instance.connections() {
            if !seen_ports.insert(connection.formal()) {
                self.push(HwValidationDiagnostic::DuplicateInstanceBinding {
                    module: module.name().to_string(),
                    instance: instance.name().to_string(),
                    kind: HwBindingKind::Port,
                    name: connection.formal().to_string(),
                });
            } else if !target_ports.contains(connection.formal()) {
                self.push(HwValidationDiagnostic::UnknownInstancePort {
                    module: module.name().to_string(),
                    instance: instance.name().to_string(),
                    target: instance.module().to_string(),
                    name: connection.formal().to_string(),
                });
            }
            self.validate_expr(module, connection.actual(), scope);
        }
    }

    fn validate_expr(
        &mut self,
        module: &'a ParametricHwModule,
        expr: &'a HwExpr,
        scope: &Scope<'a>,
    ) {
        match expr {
            HwExpr::Ident(name) => {
                if !scope.is_visible(name) {
                    self.push(HwValidationDiagnostic::UnknownReference {
                        module: module.name().to_string(),
                        name: name.clone(),
                    });
                }
            }
            HwExpr::Int(_) | HwExpr::Bool(_) | HwExpr::Str(_) | HwExpr::Zero => {}
            HwExpr::Unary { expr, .. } => self.validate_expr(module, expr, scope),
            HwExpr::Binary { left, right, .. } => {
                self.validate_expr(module, left, scope);
                self.validate_expr(module, right, scope);
            }
            HwExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.validate_expr(module, cond, scope);
                self.validate_expr(module, then_value, scope);
                self.validate_expr(module, else_value, scope);
            }
            HwExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.validate_expr(module, arm.guard(), scope);
                    self.validate_expr(module, arm.value(), scope);
                }
                self.validate_expr(module, default, scope);
            }
            HwExpr::Concat(parts) => {
                for part in parts {
                    self.validate_expr(module, part, scope);
                }
            }
            HwExpr::Slice { value, .. } => self.validate_expr(module, value, scope),
            HwExpr::IndexedPartSelect { value, index, .. } | HwExpr::Index { value, index } => {
                self.validate_expr(module, value, scope);
                self.validate_expr(module, index, scope);
            }
            HwExpr::Call { args, .. } => {
                for arg in args {
                    self.validate_expr(module, arg, scope);
                }
            }
        }
    }

    fn check_identifier(&mut self, module: Option<&str>, kind: HwBindingKind, name: &str) {
        if is_valid_identifier(name) {
            return;
        }
        self.push(HwValidationDiagnostic::InvalidIdentifier {
            module: module.map(ToOwned::to_owned),
            kind,
            name: name.to_string(),
        });
    }

    fn check_width(&mut self, module: &str, kind: HwBindingKind, name: &str, width: &str) {
        if !width.trim().is_empty() {
            return;
        }
        self.push(HwValidationDiagnostic::InvalidWidth {
            module: module.to_string(),
            kind,
            name: name.to_string(),
            width: width.to_string(),
        });
    }

    fn push(&mut self, diagnostic: HwValidationDiagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

struct Scope<'a> {
    visible: BTreeSet<&'a str>,
    local: BTreeSet<&'a str>,
}

impl<'a> Scope<'a> {
    fn new() -> Self {
        Self {
            visible: BTreeSet::new(),
            local: BTreeSet::new(),
        }
    }

    fn child(&self) -> Self {
        Self {
            visible: self.visible.clone(),
            local: BTreeSet::new(),
        }
    }

    fn declare_visible(
        &mut self,
        module: &str,
        kind: HwBindingKind,
        name: &'a str,
        validator: &mut Validator<'a>,
    ) {
        if !self.local.insert(name) {
            validator.push(HwValidationDiagnostic::DuplicateBinding {
                module: module.to_string(),
                kind,
                name: name.to_string(),
            });
            return;
        }
        self.visible.insert(name);
    }

    fn declare_hidden(
        &mut self,
        module: &str,
        kind: HwBindingKind,
        name: &'a str,
        validator: &mut Validator<'a>,
    ) {
        if self.local.insert(name) {
            return;
        }
        validator.push(HwValidationDiagnostic::DuplicateBinding {
            module: module.to_string(),
            kind,
            name: name.to_string(),
        });
    }

    fn is_visible(&self, name: &str) -> bool {
        self.visible.contains(name)
    }
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}
