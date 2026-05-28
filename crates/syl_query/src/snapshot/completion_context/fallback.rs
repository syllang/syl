use crate::snapshot::completion_context::{CompletionContext, SourceItemContext};

/// Fallback heuristic when the AST inspector cannot determine a completion
/// context — typically when the cursor sits in "gaps" between AST nodes such
/// as empty lines, whitespace between declarations, or positions that don't
/// correspond to any specific syntax node span.
///
/// Rather than returning `None` and offering no completions, this pass
/// uses source-level pattern matching (current line, surrounding tokens)
/// to infer what kind of completion makes sense at that position.
#[non_exhaustive]
pub(super) struct CompletionSourceAnalyzer<'a> {
    source: &'a str,
    offset: usize,
    item_context: SourceItemContext,
}

impl<'a> CompletionSourceAnalyzer<'a> {
    pub(super) fn new(source: &'a str, offset: usize, item_context: SourceItemContext) -> Self {
        Self {
            source,
            offset,
            item_context,
        }
    }

    pub(super) fn analyze(&self) -> Option<CompletionContext> {
        if self.rejected_assignment_context() {
            return Some(CompletionContext::Invalid);
        }
        if self.import_path_context() {
            return Some(CompletionContext::ImportPath);
        }
        if self.type_context() {
            return Some(CompletionContext::Type);
        }
        if self.field_access_context() {
            return Some(CompletionContext::FieldAccess);
        }
        if self.expression_context() {
            return Some(CompletionContext::Expression);
        }
        None
    }

    pub(super) fn rejected_assignment_context(&self) -> bool {
        let Some(line) = self.current_line() else {
            return false;
        };
        let trimmed = line.trim_start();
        let Some(operator) = self.assignment_operator(trimmed) else {
            return false;
        };
        match operator {
            AssignmentOperator::Eq => {
                trimmed.starts_with("signal ")
                    || trimmed.starts_with("next ")
                    || (matches!(self.item_context, SourceItemContext::Callable)
                        && !self.starts_binding_declaration(trimmed))
            }
            AssignmentOperator::ColonEq => {
                trimmed.starts_with("const ")
                    || trimmed.starts_with("let ")
                    || trimmed.starts_with("var ")
                    || matches!(self.item_context, SourceItemContext::Function)
            }
        }
    }

    fn import_path_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        let Some(after_use) = trimmed.strip_prefix("use") else {
            return false;
        };
        (after_use.is_empty() || after_use.starts_with(char::is_whitespace))
            && !after_use.contains(';')
    }

    fn type_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        self.after_return_arrow(trimmed)
            || self.after_type_decl_colon(trimmed)
            || self.after_port_direction(trimmed)
    }

    fn field_access_context(&self) -> bool {
        self.current_line_before_cursor()
            .is_some_and(|line| line.trim_end().ends_with('.'))
    }

    fn expression_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        let tail = line.trim_end();
        trimmed.starts_with("return ")
            || self.valid_colon_eq_expression_context(trimmed, tail)
            || self.valid_eq_expression_context(trimmed, tail)
            || tail.ends_with("return")
    }

    fn after_return_arrow(&self, line: &str) -> bool {
        let Some((_, after_arrow)) = line.rsplit_once("->") else {
            return false;
        };
        !after_arrow.contains('=')
    }

    fn after_type_decl_colon(&self, line: &str) -> bool {
        let Some(colon) = self.last_type_colon(line) else {
            return false;
        };
        let Some(before_colon) = line.get(..colon) else {
            return false;
        };
        let Some(after_colon) = line.get(colon + ':'.len_utf8()..) else {
            return false;
        };
        self.starts_type_declaration(before_colon.trim_start())
            && !after_colon.contains('=')
            && !after_colon.contains('{')
    }

    fn after_port_direction(&self, line: &str) -> bool {
        let tail = line.trim_end();
        (tail.ends_with(" in") || tail.ends_with(" out")) && line.contains(':')
    }

    fn starts_type_declaration(&self, line: &str) -> bool {
        [
            "const ", "let ", "var ", "signal ", "reg ", "module ", "cell ", "extern ", "fn ",
            "map ",
        ]
        .iter()
        .any(|keyword| line.starts_with(keyword))
    }

    fn starts_binding_declaration(&self, line: &str) -> bool {
        ["const ", "let ", "var "]
            .iter()
            .any(|keyword| line.starts_with(keyword))
    }

    fn valid_eq_expression_context(&self, trimmed: &str, tail: &str) -> bool {
        if !tail.ends_with('=') || self.assignment_operator(trimmed) != Some(AssignmentOperator::Eq)
        {
            return false;
        }
        if self.starts_binding_declaration(trimmed) {
            return true;
        }
        matches!(self.item_context, SourceItemContext::Function)
    }

    fn valid_colon_eq_expression_context(&self, trimmed: &str, tail: &str) -> bool {
        if !tail.ends_with(":=")
            || self.assignment_operator(trimmed) != Some(AssignmentOperator::ColonEq)
        {
            return false;
        }
        if matches!(self.item_context, SourceItemContext::Function) {
            return false;
        }
        (trimmed.starts_with("signal ")
            || trimmed.starts_with("next ")
            || matches!(self.item_context, SourceItemContext::Callable))
            && !self.starts_binding_declaration(trimmed)
            && !trimmed.starts_with("const ")
    }

    fn assignment_operator(&self, line: &str) -> Option<AssignmentOperator> {
        let bytes = line.as_bytes();
        let mut offset = 0;
        let mut operator = None;
        while offset < bytes.len() {
            match bytes[offset] {
                b':' if bytes.get(offset + 1) == Some(&b'=') => {
                    operator = Some(AssignmentOperator::ColonEq);
                    offset += 2;
                }
                b'=' => {
                    let prev = offset.checked_sub(1).and_then(|index| bytes.get(index));
                    let next = bytes.get(offset + 1);
                    if !matches!(prev, Some(b':' | b'=' | b'!' | b'<' | b'>'))
                        && !matches!(next, Some(b'=' | b'>'))
                    {
                        operator = Some(AssignmentOperator::Eq);
                    }
                    offset += 1;
                }
                _ => {
                    offset += 1;
                }
            }
        }
        operator
    }

    fn last_type_colon(&self, line: &str) -> Option<usize> {
        for (index, ch) in line.char_indices().rev() {
            if ch != ':' {
                continue;
            }
            let after_colon = line.get(index + ch.len_utf8()..)?;
            if after_colon.starts_with('=') {
                continue;
            }
            return Some(index);
        }
        None
    }

    fn current_line_before_cursor(&self) -> Option<&'a str> {
        let before_cursor = self.source.get(..self.offset)?;
        Some(
            before_cursor
                .rsplit_once('\n')
                .map(|(_, line)| line)
                .unwrap_or(before_cursor),
        )
    }

    fn current_line(&self) -> Option<&'a str> {
        let source = self.source;
        let line_start = source
            .get(..self.offset)?
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let line_end = source
            .get(self.offset..)?
            .find('\n')
            .map(|index| self.offset + index)
            .unwrap_or(source.len());
        source.get(line_start..line_end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssignmentOperator {
    Eq,
    ColonEq,
}

#[cfg(test)]
mod tests {
    use super::super::{CompletionContext, SourceItemContext};
    use super::CompletionSourceAnalyzer;

    #[test]
    fn binding_equals_stays_expression_context() {
        assert_eq!(
            analyze(
                "module Top() {\n    let value = \n}\n",
                "let value = ",
                SourceItemContext::Callable
            ),
            Some(CompletionContext::Expression)
        );
    }

    #[test]
    fn next_equals_is_rejected() {
        assert_eq!(
            analyze(
                "module Top() {\n    next state = value\n}\n",
                "state = value",
                SourceItemContext::Callable
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn next_equals_is_rejected_from_statement_start() {
        assert_eq!(
            analyze_from_cursor(
                "module Top() {\n    next state = value\n}\n",
                "state = value",
                SourceItemContext::Callable,
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn signal_equals_is_rejected() {
        assert_eq!(
            analyze(
                "module Top() {\n    signal ready: Bit = value\n}\n",
                "Bit = value",
                SourceItemContext::Callable
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn signal_equals_is_rejected_from_statement_start() {
        assert_eq!(
            analyze_from_cursor(
                "module Top() {\n    signal ready: Bit = value\n}\n",
                "ready: Bit = value",
                SourceItemContext::Callable,
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn let_colon_eq_is_rejected() {
        assert_eq!(
            analyze(
                "module Top() {\n    let value := input\n}\n",
                "value := input",
                SourceItemContext::Callable
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn let_colon_eq_is_rejected_from_statement_start() {
        assert_eq!(
            analyze_from_cursor(
                "module Top() {\n    let value := input\n}\n",
                "value := input",
                SourceItemContext::Callable,
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn drive_context_stays_enabled_in_callable_items() {
        assert_eq!(
            analyze(
                "module Top() {\n    out := \n}\n",
                "out := ",
                SourceItemContext::Callable
            ),
            Some(CompletionContext::Expression)
        );
    }

    #[test]
    fn drive_context_is_rejected_in_functions() {
        assert_eq!(
            analyze(
                "fn update() {\n    value := next_value\n}\n",
                "value := next_value",
                SourceItemContext::Function
            ),
            Some(CompletionContext::Invalid)
        );
    }

    #[test]
    fn software_assignment_stays_enabled_in_functions() {
        assert_eq!(
            analyze(
                "fn update() {\n    value = \n}\n",
                "value = ",
                SourceItemContext::Function
            ),
            Some(CompletionContext::Expression)
        );
    }

    fn analyze(
        source: &str,
        needle: &str,
        item_context: SourceItemContext,
    ) -> Option<CompletionContext> {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("fixture must contain marker {needle:?}"))
            + needle.len();
        CompletionSourceAnalyzer::new(source, offset, item_context).analyze()
    }

    fn analyze_from_cursor(
        source: &str,
        needle: &str,
        item_context: SourceItemContext,
    ) -> Option<CompletionContext> {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("fixture must contain marker {needle:?}"));
        CompletionSourceAnalyzer::new(source, offset, item_context).analyze()
    }
}
