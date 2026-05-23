use super::{CompileError, VerilogError, sv_ir::*};
use std::collections::BTreeSet;

#[non_exhaustive]
pub(super) struct SvValidator<'a> {
    design: &'a SvDesign,
    source: Option<&'a str>,
    delimiters: Vec<(char, usize)>,
    module_depth: usize,
    generate_depth: usize,
    begin_depth: usize,
    source_modules: BTreeSet<String>,
}

impl<'a> SvValidator<'a> {
    pub(super) fn new(design: &'a SvDesign) -> Self {
        Self {
            design,
            source: None,
            delimiters: Vec::new(),
            module_depth: 0,
            generate_depth: 0,
            begin_depth: 0,
            source_modules: BTreeSet::new(),
        }
    }

    pub(super) fn with_source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    pub(super) fn validate(mut self) -> Result<(), CompileError> {
        self.check_design()?;
        if let Some(source) = self.source {
            self.check_source(source)?;
        }
        Ok(())
    }

    fn check_design(&self) -> Result<(), CompileError> {
        let mut modules = BTreeSet::new();
        for module in &self.design.modules {
            if !modules.insert(module.name.clone()) {
                return Err(CompileError::verilog(VerilogError::DuplicateModule {
                    name: module.name.clone(),
                }));
            }
        }
        for module in &self.design.modules {
            self.check_module(module, &modules)?;
        }
        Ok(())
    }

    fn check_module(
        &self,
        module: &SvModule,
        known_modules: &BTreeSet<String>,
    ) -> Result<(), CompileError> {
        let mut scope = ValidatorScope::new();
        for param in &module.params {
            scope.declare(&module.name, &param.name)?;
        }
        for port in &module.ports {
            scope.declare(&module.name, &port.name)?;
        }
        self.check_items(module, &module.items, &mut scope, known_modules)
    }

    fn check_items(
        &self,
        module: &SvModule,
        items: &[SvItem],
        scope: &mut ValidatorScope,
        known_modules: &BTreeSet<String>,
    ) -> Result<(), CompileError> {
        self.declare_items(module, items, scope)?;
        for item in items {
            self.check_item(module, item, scope, known_modules)?;
        }
        Ok(())
    }

    fn declare_items(
        &self,
        module: &SvModule,
        items: &[SvItem],
        scope: &mut ValidatorScope,
    ) -> Result<(), CompileError> {
        for item in items {
            match item {
                SvItem::LocalParam { name, .. }
                | SvItem::Wire { name, .. }
                | SvItem::Reg { name, .. } => scope.declare(&module.name, name)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn check_item(
        &self,
        module: &SvModule,
        item: &SvItem,
        scope: &mut ValidatorScope,
        known_modules: &BTreeSet<String>,
    ) -> Result<(), CompileError> {
        match item {
            SvItem::LocalParam { value, .. } => {
                SvExprValidator::new(module, scope).check(value)?;
            }
            SvItem::Wire { .. } | SvItem::Reg { .. } => {}
            SvItem::Assign { lhs, rhs } => {
                let exprs = SvExprValidator::new(module, scope);
                exprs.check(lhs)?;
                exprs.check(rhs)?;
            }
            SvItem::AlwaysReg {
                clock,
                target,
                reset,
                next,
            } => {
                let exprs = SvExprValidator::new(module, scope);
                exprs.check(clock)?;
                exprs.check(target)?;
                if let Some(reset) = reset {
                    exprs.check(&reset.condition)?;
                    exprs.check(&reset.value)?;
                }
                exprs.check(next)?;
            }
            SvItem::Instance(instance) => {
                self.check_instance(module, instance, scope, known_modules)?;
            }
            SvItem::GenerateIf {
                cond,
                then_items,
                else_items,
                ..
            } => {
                SvExprValidator::new(module, scope).check(cond)?;
                let mut then_scope = scope.child();
                self.check_items(module, then_items, &mut then_scope, known_modules)?;
                let mut else_scope = scope.child();
                self.check_items(module, else_items, &mut else_scope, known_modules)?;
            }
            SvItem::GenerateFor {
                genvar,
                start,
                end,
                items,
                ..
            } => {
                let exprs = SvExprValidator::new(module, scope);
                exprs.check(start)?;
                exprs.check(end)?;
                let mut loop_scope = scope.child();
                loop_scope.declare(&module.name, genvar)?;
                self.check_items(module, items, &mut loop_scope, known_modules)?;
            }
            SvItem::InitialError { message } => {
                SvExprValidator::new(module, scope).check(message)?;
            }
        }
        Ok(())
    }

    fn check_instance(
        &self,
        module: &SvModule,
        instance: &SvInstance,
        scope: &ValidatorScope,
        known_modules: &BTreeSet<String>,
    ) -> Result<(), CompileError> {
        if !known_modules.contains(&instance.module) {
            return Err(CompileError::verilog(VerilogError::UnknownInstanceModule {
                module: module.name.clone(),
                instance: instance.name.clone(),
                target: instance.module.clone(),
            }));
        }
        for connection in &instance.connections {
            SvExprValidator::new(module, scope).check(&connection.actual)?;
        }
        Ok(())
    }

    fn check_source(&mut self, source: &str) -> Result<(), CompileError> {
        for (line_idx, raw_line) in source.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = self.code_only(raw_line);
            self.check_delimiters(&line, line_no)?;
            self.check_words(&line, line_no)?;
        }
        self.check_final_state()
    }

    fn check_words(&mut self, line: &str, line_no: usize) -> Result<(), CompileError> {
        let words = self.words(line);
        let mut idx = 0usize;
        while let Some(word) = words.get(idx) {
            match word.as_str() {
                "module" => {
                    self.module_depth += 1;
                    let Some(name) = words.get(idx + 1) else {
                        return Err(CompileError::verilog(VerilogError::ModuleWithoutName {
                            line: line_no,
                        }));
                    };
                    if !self.source_modules.insert(name.clone()) {
                        return Err(CompileError::verilog(VerilogError::DuplicateModule {
                            name: name.clone(),
                        }));
                    }
                    idx += 1;
                }
                "endmodule" => self.close_module(line_no)?,
                "generate" => self.generate_depth += 1,
                "endgenerate" => self.close_generate(line_no)?,
                "begin" => self.begin_depth += 1,
                "end" => self.close_begin(line_no)?,
                _ => {}
            }
            idx += 1;
        }
        Ok(())
    }

    fn close_module(&mut self, line_no: usize) -> Result<(), CompileError> {
        if self.module_depth == 0 {
            return Err(CompileError::verilog(VerilogError::UnmatchedEndModule {
                line: line_no,
            }));
        }
        self.module_depth -= 1;
        Ok(())
    }

    fn close_generate(&mut self, line_no: usize) -> Result<(), CompileError> {
        if self.generate_depth == 0 {
            return Err(CompileError::verilog(VerilogError::UnmatchedEndGenerate {
                line: line_no,
            }));
        }
        self.generate_depth -= 1;
        Ok(())
    }

    fn close_begin(&mut self, line_no: usize) -> Result<(), CompileError> {
        if self.begin_depth == 0 {
            return Err(CompileError::verilog(VerilogError::UnmatchedEnd {
                line: line_no,
            }));
        }
        self.begin_depth -= 1;
        Ok(())
    }

    fn check_final_state(&mut self) -> Result<(), CompileError> {
        if let Some((open, line)) = self.delimiters.pop() {
            return Err(CompileError::verilog(VerilogError::UnmatchedDelimiter {
                open,
                open_line: line,
            }));
        }
        if self.module_depth != 0 {
            return Err(CompileError::verilog(VerilogError::UnclosedModule));
        }
        if self.generate_depth != 0 {
            return Err(CompileError::verilog(VerilogError::UnclosedGenerateBlock));
        }
        if self.begin_depth != 0 {
            return Err(CompileError::verilog(VerilogError::UnclosedBeginBlock));
        }
        Ok(())
    }

    fn code_only(&self, line: &str) -> String {
        let mut out = String::new();
        let mut chars = line.chars().peekable();
        let mut in_string = false;
        while let Some(ch) = chars.next() {
            if in_string {
                if ch == '\\' {
                    let _ = chars.next();
                    out.push(' ');
                    continue;
                }
                if ch == '"' {
                    in_string = false;
                }
                out.push(' ');
                continue;
            }
            if ch == '"' {
                in_string = true;
                out.push(' ');
                continue;
            }
            if ch == '/' && chars.peek().copied() == Some('/') {
                break;
            }
            out.push(ch);
        }
        out
    }

    fn check_delimiters(&mut self, line: &str, line_no: usize) -> Result<(), CompileError> {
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => self.delimiters.push((ch, line_no)),
                ')' | ']' | '}' => self.close_delimiter(ch, line_no)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn close_delimiter(&mut self, close: char, line_no: usize) -> Result<(), CompileError> {
        let Some((open, open_line)) = self.delimiters.pop() else {
            return Err(CompileError::verilog(VerilogError::UnmatchedDelimiter {
                open: close,
                open_line: line_no,
            }));
        };
        if !self.matches_pair(open, close) {
            return Err(CompileError::verilog(VerilogError::MismatchedDelimiter {
                open,
                open_line,
                close,
                line: line_no,
            }));
        }
        Ok(())
    }

    fn matches_pair(&self, open: char, close: char) -> bool {
        matches!((open, close), ('(', ')') | ('[', ']') | ('{', '}'))
    }

    fn words(&self, line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut current = String::new();
        for ch in line.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                current.push(ch);
            } else if !current.is_empty() {
                out.push(current);
                current = String::new();
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
        out
    }
}

struct SvExprValidator<'a> {
    module_name: &'a str,
    scope: &'a ValidatorScope,
}

impl<'a> SvExprValidator<'a> {
    fn new(module: &'a SvModule, scope: &'a ValidatorScope) -> Self {
        Self {
            module_name: &module.name,
            scope,
        }
    }

    fn check(&self, expr: &SvExpr) -> Result<(), CompileError> {
        match expr {
            SvExpr::Ident(name) => self.scope.require_visible(self.module_name, name)?,
            SvExpr::Int(_) | SvExpr::Bool(_) | SvExpr::Str(_) | SvExpr::Zero => {}
            SvExpr::Unary { expr, .. } => self.check(expr)?,
            SvExpr::Binary { left, right, .. } => {
                self.check(left)?;
                self.check(right)?;
            }
            SvExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.check(cond)?;
                self.check(then_value)?;
                self.check(else_value)?;
            }
            SvExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.check(&arm.guard)?;
                    self.check(&arm.value)?;
                }
                self.check(default)?;
            }
            SvExpr::Concat(parts) => {
                for part in parts {
                    self.check(part)?;
                }
            }
            SvExpr::Slice { value, .. } => self.check(value)?,
            SvExpr::IndexedPartSelect { value, index, .. } | SvExpr::Index { value, index } => {
                self.check(value)?;
                self.check(index)?;
            }
            SvExpr::Call { name, args } => {
                self.check_call(name)?;
                for arg in args {
                    self.check(arg)?;
                }
            }
        }
        Ok(())
    }

    fn check_call(&self, name: &str) -> Result<(), CompileError> {
        if name.starts_with('$') {
            return Ok(());
        }
        Err(CompileError::verilog(
            VerilogError::UnsupportedFunctionCall {
                module: self.module_name.to_string(),
                name: name.to_string(),
            },
        ))
    }
}

struct ValidatorScope {
    visible: BTreeSet<String>,
    local: BTreeSet<String>,
}

impl ValidatorScope {
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

    fn declare(&mut self, module: &str, name: &str) -> Result<(), CompileError> {
        if !self.local.insert(name.to_string()) {
            return Err(CompileError::verilog(VerilogError::DuplicateDeclaration {
                module: module.to_string(),
                name: name.to_string(),
            }));
        }
        self.visible.insert(name.to_string());
        Ok(())
    }

    fn require_visible(&self, module: &str, name: &str) -> Result<(), CompileError> {
        if self.visible.contains(name) {
            return Ok(());
        }
        Err(CompileError::verilog(
            VerilogError::UndeclaredSignalReference {
                module: module.to_string(),
                name: name.to_string(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ValidatorHarness;

    impl ValidatorHarness {
        fn validate(design: SvDesign) -> Result<(), CompileError> {
            SvValidator::new(&design).validate()
        }

        fn empty_module(name: &str) -> SvModule {
            SvModule::new(name, Vec::new(), Vec::new(), Vec::new())
        }

        fn bit_input(name: &str) -> SvPort {
            SvPort::new(SvDirection::Input, "1", name)
        }

        fn bit_output(name: &str) -> SvPort {
            SvPort::new(SvDirection::Output, "1", name)
        }
    }

    #[test]
    fn rejects_duplicate_module_names() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![
            ValidatorHarness::empty_module("Top"),
            ValidatorHarness::empty_module("Top"),
        ]))
        .expect_err("duplicate SV modules make emitted design ambiguous");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::DuplicateModule { name }
            } if name == "Top"
        ));
    }

    #[test]
    fn rejects_duplicate_port_names() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            vec![
                ValidatorHarness::bit_input("x"),
                ValidatorHarness::bit_output("x"),
            ],
            Vec::new(),
        )]))
        .expect_err("duplicate SV ports make the module interface invalid");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::DuplicateDeclaration { module, name }
            } if module == "Top" && name == "x"
        ));
    }

    #[test]
    fn rejects_duplicate_signal_and_local_declarations() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                SvItem::Wire {
                    width: "1".to_string(),
                    name: "tmp".to_string(),
                },
                SvItem::LocalParam {
                    name: "tmp".to_string(),
                    value: SvExpr::Int(1),
                },
            ],
        )]))
        .expect_err("duplicate SV signal/local declarations must be rejected");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::DuplicateDeclaration { module, name }
            } if module == "Top" && name == "tmp"
        ));
    }

    #[test]
    fn rejects_unknown_instance_module() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            vec![ValidatorHarness::bit_input("x")],
            vec![SvItem::Instance(SvInstance::new(
                "Missing",
                Vec::new(),
                "u_missing",
                vec![SvConnection::new("x", SvExpr::Ident("x".to_string()))],
            ))],
        )]))
        .expect_err("instances must target a known emitted or extern module");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UnknownInstanceModule {
                    module,
                    instance,
                    target,
                }
            } if module == "Top" && instance == "u_missing" && target == "Missing"
        ));
    }

    #[test]
    fn allows_instance_of_empty_extern_stub_module() {
        ValidatorHarness::validate(SvDesign::new(vec![
            ValidatorHarness::empty_module("Vendor"),
            SvModule::new(
                "Top",
                Vec::new(),
                vec![ValidatorHarness::bit_input("x")],
                vec![SvItem::Instance(SvInstance::new(
                    "Vendor",
                    Vec::new(),
                    "u_vendor",
                    vec![SvConnection::new("x", SvExpr::Ident("x".to_string()))],
                ))],
            ),
        ]))
        .expect("extern/blackbox stubs present in IR must satisfy instance target lookup");
    }

    #[test]
    fn rejects_assign_reference_to_undeclared_signal() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            vec![ValidatorHarness::bit_output("y")],
            vec![SvItem::Assign {
                lhs: SvExpr::Ident("y".to_string()),
                rhs: SvExpr::Ident("missing".to_string()),
            }],
        )]))
        .expect_err("assign expressions must not reference undeclared signals");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UndeclaredSignalReference { module, name }
            } if module == "Top" && name == "missing"
        ));
    }

    #[test]
    fn rejects_process_reference_to_undeclared_signal() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            vec![ValidatorHarness::bit_input("clk")],
            vec![
                SvItem::Reg {
                    width: "1".to_string(),
                    name: "q".to_string(),
                },
                SvItem::AlwaysReg {
                    clock: SvExpr::Ident("clk".to_string()),
                    target: SvExpr::Ident("q".to_string()),
                    reset: None,
                    next: SvExpr::Ident("missing".to_string()),
                },
            ],
        )]))
        .expect_err("clocked process expressions must not reference undeclared signals");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UndeclaredSignalReference { module, name }
            } if module == "Top" && name == "missing"
        ));
    }

    #[test]
    fn rejects_non_system_function_calls() {
        let err = ValidatorHarness::validate(SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            vec![ValidatorHarness::bit_output("y")],
            vec![SvItem::Assign {
                lhs: SvExpr::Ident("y".to_string()),
                rhs: SvExpr::Call {
                    name: "helper".to_string(),
                    args: vec![SvExpr::Int(1)],
                },
            }],
        )]))
        .expect_err("generated SV must not reference functions that the backend did not emit");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UnsupportedFunctionCall { module, name }
            } if module == "Top" && name == "helper"
        ));
    }
}
