use crate::{CompileError, EirError, hir::HirSignatureParam, ir::mir::MirTypeRef};
use std::collections::BTreeMap;
use syl_span::Span;

pub trait FormalBinding {
    fn binding_name(&self) -> &str;
}

impl FormalBinding for HirSignatureParam {
    fn binding_name(&self) -> &str {
        &self.name
    }
}

impl FormalBinding for (String, MirTypeRef) {
    fn binding_name(&self) -> &str {
        &self.0
    }
}

pub struct ActualFormalBinder<'a, F: FormalBinding> {
    formals: &'a [F],
    formals_by_name: BTreeMap<&'a str, usize>,
    used: Vec<bool>,
    next_positional: usize,
}

impl<'a, F: FormalBinding> ActualFormalBinder<'a, F> {
    pub fn new(formals: &'a [F]) -> Self {
        let mut formals_by_name = BTreeMap::new();
        for (idx, formal) in formals.iter().enumerate() {
            formals_by_name.entry(formal.binding_name()).or_insert(idx);
        }
        Self {
            formals,
            formals_by_name,
            used: vec![false; formals.len()],
            next_positional: 0,
        }
    }

    pub fn resolve(
        &mut self,
        callable_name: &str,
        arg_name: Option<&str>,
        arg_span: Span,
    ) -> Result<&'a F, CompileError> {
        let index = if let Some(name) = arg_name {
            self.formals_by_name.get(name).copied().ok_or_else(|| {
                CompileError::lowering_at(
                    EirError::UnknownParameter {
                        name: name.to_string(),
                        callable: callable_name.to_string(),
                    },
                    arg_span,
                )
            })?
        } else {
            while self.next_positional < self.formals.len() && self.used[self.next_positional] {
                self.next_positional += 1;
            }
            if self.next_positional == self.formals.len() {
                return Err(CompileError::lowering_at(
                    EirError::TooManyPositionalArguments {
                        callable: callable_name.to_string(),
                    },
                    arg_span,
                ));
            }
            let index = self.next_positional;
            self.next_positional += 1;
            index
        };

        if self.used[index] {
            let name = self.formals[index].binding_name().to_string();
            return Err(CompileError::lowering_at(
                EirError::DuplicateArgument { name },
                arg_span,
            ));
        }

        self.used[index] = true;
        Ok(&self.formals[index])
    }

    pub fn is_used(&self, name: &str) -> bool {
        self.formals_by_name
            .get(name)
            .copied()
            .is_some_and(|idx| self.used[idx])
    }
}

#[cfg(test)]
mod tests;
