use super::{TirType, TypePhaseChecker};
use crate::{CompileError, TirError};
use syl_hir::{DefId, MirTypeRef};

/// Reset sampling mode in the canonical semantic type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirResetMode {
    #[default]
    Sync,
    Async,
}

/// Active logic level of a semantic reset type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirResetPolarity {
    #[default]
    ActiveHigh,
    ActiveLow,
}

pub(super) fn label(
    domain: Option<&TirType>,
    mode: TirResetMode,
    polarity: TirResetPolarity,
) -> String {
    let mut args = domain.map(|d| vec![d.label()]).unwrap_or_default();
    if mode != TirResetMode::Sync || polarity != TirResetPolarity::ActiveHigh {
        args.push(format!("{mode:?}"));
        args.push(format!("{polarity:?}"));
    }
    if args.is_empty() {
        "Reset".into()
    } else {
        format!("Reset<{}>", args.join(", "))
    }
}

impl TypePhaseChecker {
    pub(in crate::tir) fn parse_reset_type_ref(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
    ) -> Result<TirType, CompileError> {
        Self::parse_reset_args(ty.args().unwrap_or_default(), ty.span(), |arg| {
            let parsed = self.type_from_mir_type_ref(owner, arg)?;
            let is_domain = match &parsed {
                TirType::Named {
                    generic: Some(id), ..
                } => self
                    .binding_types
                    .get(&super::BindingRef::Local(*id))
                    .and_then(|id| self.type_table.get(*id))
                    .is_some_and(|t| matches!(t, TirType::Domain)),
                TirType::Domain => true,
                _ => false,
            };
            if is_domain {
                Ok(parsed)
            } else {
                Err(CompileError::lowering_at(
                    TirError::InvalidResetQualifier {
                        detail: "expected a Domain parameter".into(),
                    },
                    arg.span(),
                ))
            }
        })
    }

    pub(in crate::tir) fn parse_reset_args(
        args: &[MirTypeRef],
        span: syl_span::Span,
        mut resolve: impl FnMut(&MirTypeRef) -> Result<TirType, CompileError>,
    ) -> Result<TirType, CompileError> {
        let invalid = |detail: &str| {
            CompileError::lowering_at(
                TirError::InvalidResetQualifier {
                    detail: detail.into(),
                },
                span,
            )
        };
        if args.len() > 3 {
            return Err(invalid(
                "at most one domain, mode, and polarity are allowed",
            ));
        }
        let (mut domain, mut mode, mut polarity) = (None, None, None);
        for (index, arg) in args.iter().enumerate() {
            match arg.path_name() {
                Some("Sync" | "Async") if mode.is_none() => {
                    mode = Some(if arg.path_name() == Some("Async") {
                        TirResetMode::Async
                    } else {
                        TirResetMode::Sync
                    })
                }
                Some("ActiveHigh" | "ActiveLow") if polarity.is_none() => {
                    polarity = Some(if arg.path_name() == Some("ActiveLow") {
                        TirResetPolarity::ActiveLow
                    } else {
                        TirResetPolarity::ActiveHigh
                    })
                }
                Some("Sync" | "Async" | "ActiveHigh" | "ActiveLow") => {
                    return Err(invalid("duplicate reset qualifier"));
                }
                _ if index == 0 => {
                    domain = Some(Box::new(
                        resolve(arg).map_err(|_| invalid("invalid reset domain or qualifier"))?,
                    ))
                }
                _ => return Err(invalid("unknown reset qualifier; domain must be first")),
            }
        }
        Ok(TirType::Reset {
            domain,
            mode: mode.unwrap_or_default(),
            polarity: polarity.unwrap_or_default(),
        })
    }
}
