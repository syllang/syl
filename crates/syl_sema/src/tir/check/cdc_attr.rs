use super::TypePhaseChecker;
use crate::{CompileError, TirError};
use std::collections::BTreeMap;
use syl_hir::{
    DefId, HirAttribute, HirExprNode, HirPortDecl, HirPortDirection, HirSignatureResultBinding,
    HirViewDirection,
};

impl TypePhaseChecker {
    pub(super) fn check_cdc_attrs(
        &self,
        owner: DefId,
        attrs: &[HirAttribute],
        signature: (&[HirPortDecl], Option<&HirSignatureResultBinding>),
        errors: &mut Vec<CompileError>,
    ) {
        let (ports, result) = signature;
        let mut directions = BTreeMap::new();
        for port in ports {
            let clock = port.ty.type_name() == Some("Clock");
            if let Some((base, view)) = port.ty.view_select() {
                if let Some(interface) = self
                    .hir
                    .type_def_for_mir_type(owner, base)
                    .and_then(|d| self.hir.interfaces().get(&d))
                    && let Some(view) = interface.views.iter().find(|v| v.name == view)
                {
                    for field in &view.fields {
                        directions.insert(
                            format!("{}_{}", port.name, field.name),
                            (
                                matches!(field.direction, HirViewDirection::In),
                                matches!(field.direction, HirViewDirection::Out),
                                matches!(field.direction, HirViewDirection::In)
                                    && interface.fields.iter().any(|f| {
                                        f.name == field.name && f.ty.type_name() == Some("Clock")
                                    }),
                            ),
                        );
                    }
                }
            } else {
                directions.insert(
                    port.name.clone(),
                    (
                        port.direction == HirPortDirection::In,
                        port.direction == HirPortDirection::Out,
                        clock && port.direction == HirPortDirection::In,
                    ),
                );
            }
        }
        if let Some(result) = result {
            directions.insert(result.name.clone(), (false, true, false));
        }
        for attr in attrs {
            let names = attr
                .args
                .iter()
                .map(|arg| {
                    if let HirExprNode::Ident(name) = &arg.node {
                        Some(name.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>();
            let valid = attr.name == "cdc_sync"
                && names.as_deref().is_some_and(|names| match names {
                    [input, output, clock] => {
                        directions.get(*input).is_some_and(|d| d.0)
                            && directions.get(*output).is_some_and(|d| d.1)
                            && directions.get(*clock).is_some_and(|d| d.2)
                    }
                    _ => false,
                });
            if !valid {
                errors.push(CompileError::lowering_at(
                    TirError::InvalidCdcSyncAttribute {
                        reason: format!(
                            "@{} requires (input_port, output_port, input_clock)",
                            attr.name
                        ),
                    },
                    attr.span,
                ));
            }
        }
    }
}
