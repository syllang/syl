use crate::{
    CapabilityError, CompileError,
    capability_model::{EndpointSide, FieldCaps},
    hir::{HirDesign, HirViewDirection},
    mir::{MirTypeRef, MirTypeRefExt},
    mir_type_resolve::MirTypeDefinitionResolver,
};
use syl_hir::DefId;

#[non_exhaustive]
pub(super) struct ViewCapabilityResolver<'a> {
    hir: &'a HirDesign,
}

impl<'a> ViewCapabilityResolver<'a> {
    pub(super) fn new(hir: &'a HirDesign) -> Self {
        Self { hir }
    }

    pub(super) fn caps(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        side: EndpointSide,
    ) -> Result<Option<FieldCaps>, CompileError> {
        let Some((base, view, _)) = ty.view_shape() else {
            return Ok(None);
        };
        let resolver = MirTypeDefinitionResolver::new(self.hir);
        let interface = resolver.interface(Some(owner), base).ok_or_else(|| {
            CompileError::lowering_at(
                CapabilityError::UnknownInterface {
                    name: resolver.type_name_or_unknown(base),
                },
                base.span(),
            )
        })?;
        let view_decl = interface
            .views
            .iter()
            .find(|decl| decl.name == view)
            .ok_or_else(|| {
                CompileError::lowering_at(
                    CapabilityError::UnknownView {
                        name: view.to_string(),
                    },
                    ty.span(),
                )
            })?;
        let mut caps = FieldCaps::empty();
        for field in &view_decl.fields {
            match (side, field.direction) {
                (EndpointSide::Local, HirViewDirection::In)
                | (EndpointSide::LocalSignal, HirViewDirection::In)
                | (EndpointSide::Returned, HirViewDirection::Out) => {
                    caps.readable.insert(field.name.clone());
                }
                (EndpointSide::Local, HirViewDirection::Out)
                | (EndpointSide::LocalSignal, HirViewDirection::Out)
                | (EndpointSide::Returned, HirViewDirection::In) => {
                    caps.drivable.insert(field.name.clone());
                }
                _ => {}
            }
        }
        if side == EndpointSide::LocalSignal {
            Ok(Some(caps.with_local_drive_readback()))
        } else {
            Ok(Some(caps))
        }
    }
}
