use super::{CompileError, VerilogError, sv_ir::*};

#[derive(Debug)]
#[non_exhaustive]
pub(super) struct SvBackendValidator<'a> {
    design: &'a SvDesign,
}

impl<'a> SvBackendValidator<'a> {
    pub(super) fn new(design: &'a SvDesign) -> Self {
        Self { design }
    }

    pub(super) fn validate(&self) -> Result<(), CompileError> {
        for module in &self.design.modules {
            self.check_module(module)?;
        }
        Ok(())
    }

    fn check_module(&self, module: &SvModule) -> Result<(), CompileError> {
        for item in &module.items {
            self.check_item(module, item)?;
        }
        Ok(())
    }

    fn check_item(&self, module: &SvModule, item: &SvItem) -> Result<(), CompileError> {
        match item {
            SvItem::LocalParam { value, .. } => Self::check_expr(&module.name, value),
            SvItem::Wire { .. } | SvItem::Reg { .. } => Ok(()),
            SvItem::Assign { lhs, rhs } => {
                Self::check_expr(&module.name, lhs)?;
                Self::check_expr(&module.name, rhs)
            }
            SvItem::AlwaysReg {
                clock,
                target,
                reset,
                next,
            } => {
                Self::check_expr(&module.name, clock)?;
                Self::check_expr(&module.name, target)?;
                if let Some(reset) = reset {
                    Self::check_expr(&module.name, &reset.condition)?;
                    Self::check_expr(&module.name, &reset.value)?;
                }
                Self::check_expr(&module.name, next)
            }
            SvItem::Instance(instance) => {
                for connection in &instance.connections {
                    Self::check_expr(&module.name, &connection.actual)?;
                }
                Ok(())
            }
            SvItem::GenerateIf {
                cond,
                then_items,
                else_items,
                ..
            } => {
                Self::check_expr(&module.name, cond)?;
                self.check_items(module, then_items)?;
                self.check_items(module, else_items)
            }
            SvItem::GenerateFor {
                start, end, items, ..
            } => {
                Self::check_expr(&module.name, start)?;
                Self::check_expr(&module.name, end)?;
                self.check_items(module, items)
            }
            SvItem::InitialError { message } => Self::check_expr(&module.name, message),
        }
    }

    fn check_items(&self, module: &SvModule, items: &[SvItem]) -> Result<(), CompileError> {
        for item in items {
            self.check_item(module, item)?;
        }
        Ok(())
    }

    fn check_expr(module_name: &str, expr: &SvExpr) -> Result<(), CompileError> {
        match expr {
            SvExpr::Ident(_) | SvExpr::Int(_) | SvExpr::Bool(_) | SvExpr::Str(_) | SvExpr::Zero => {
                Ok(())
            }
            SvExpr::Unary { expr, .. } => Self::check_expr(module_name, expr),
            SvExpr::Binary { left, right, .. } => {
                Self::check_expr(module_name, left)?;
                Self::check_expr(module_name, right)
            }
            SvExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                Self::check_expr(module_name, cond)?;
                Self::check_expr(module_name, then_value)?;
                Self::check_expr(module_name, else_value)
            }
            SvExpr::Select { arms, default, .. } => {
                for arm in arms {
                    Self::check_expr(module_name, &arm.guard)?;
                    Self::check_expr(module_name, &arm.value)?;
                }
                Self::check_expr(module_name, default)
            }
            SvExpr::Concat(parts) => {
                for part in parts {
                    Self::check_expr(module_name, part)?;
                }
                Ok(())
            }
            SvExpr::Slice { value, .. } => Self::check_expr(module_name, value),
            SvExpr::IndexedPartSelect { value, index, .. } | SvExpr::Index { value, index } => {
                Self::check_expr(module_name, value)?;
                Self::check_expr(module_name, index)
            }
            SvExpr::Call { name, args } => {
                if !name.starts_with('$') {
                    return Err(CompileError::verilog(
                        VerilogError::UnsupportedFunctionCall {
                            module: module_name.to_string(),
                            name: name.clone(),
                        },
                    ));
                }
                for arg in args {
                    Self::check_expr(module_name, arg)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub(super) struct SvSourceValidator {
    delimiters: Vec<(char, usize)>,
    module_depth: usize,
    generate_depth: usize,
    begin_depth: usize,
}

impl SvSourceValidator {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn validate(mut self, source: &str) -> Result<(), CompileError> {
        for (line_idx, raw_line) in source.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = self.code_only(raw_line);
            self.check_delimiters(&line, line_no)?;
            self.check_words(&line, line_no)?;
        }
        self.check_final_state()
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
        if !matches!((open, close), ('(', ')') | ('[', ']') | ('{', '}')) {
            return Err(CompileError::verilog(VerilogError::MismatchedDelimiter {
                open,
                open_line,
                close,
                line: line_no,
            }));
        }
        Ok(())
    }

    fn check_words(&mut self, line: &str, line_no: usize) -> Result<(), CompileError> {
        let words = self.words(line);
        let mut idx = 0usize;
        while let Some(word) = words.get(idx) {
            match word.as_str() {
                "module" => {
                    self.module_depth += 1;
                    if words.get(idx + 1).is_none() {
                        return Err(CompileError::verilog(VerilogError::ModuleWithoutName {
                            line: line_no,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_module(name: &str) -> SvModule {
        SvModule::new(name, Vec::new(), Vec::new(), Vec::new())
    }

    #[test]
    fn rejects_non_system_function_calls() {
        let err = SvBackendValidator::new(&SvDesign::new(vec![SvModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![SvItem::Assign {
                lhs: SvExpr::Ident("y".to_string()),
                rhs: SvExpr::Call {
                    name: "helper".to_string(),
                    args: vec![SvExpr::Int(1)],
                },
            }],
        )]))
        .validate()
        .expect_err("generated SV must not reference functions that the backend did not emit");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UnsupportedFunctionCall { module, name }
            } if module == "Top" && name == "helper"
        ));
    }

    #[test]
    fn source_validator_rejects_unmatched_endmodule() {
        let err = SvSourceValidator::new()
            .validate("endmodule\n")
            .expect_err("source validation must reject closing an unopened module");

        assert!(matches!(
            err,
            CompileError::Verilog {
                kind: VerilogError::UnmatchedEndModule { line }
            } if line == 1
        ));
    }

    #[test]
    fn source_validator_accepts_emitted_empty_module() {
        let source = SvDesign::new(vec![empty_module("Top")]).emit_text();

        SvSourceValidator::new()
            .validate(&source)
            .expect("well-formed emitted module text should satisfy backend-local source checks");
    }
}
