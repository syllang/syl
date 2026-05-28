use crate::tir::{BindingRef, TirDesign, TirType};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId, HirDesign, HirEnumVariantKey, HirResolution};

use super::{
    design::{
        ElabDef, ElabDefKind, ElabEnumVariantKey, ElabLocalKind, ElabProgram, ElabResolution,
    },
    item::{ElabBundleItem, ElabCallable, ElabConstItem, ElabEnumItem, ElabInterfaceItem},
};

pub(crate) trait ProgramLoweringInput {
    fn hir(&self) -> &HirDesign;

    fn visible_def_ids(&self, owner: DefId) -> Vec<DefId>;

    fn expr_resolution(&self, expr: ExprId) -> Option<HirResolution>;

    fn binding_type(&self, binding: BindingRef) -> Option<TirType>;

    fn expr_type(&self, expr: ExprId) -> Option<TirType>;

    fn enum_variant_value(&self, key: &HirEnumVariantKey) -> Option<u64>;
}

impl ProgramLoweringInput for TirDesign {
    fn hir(&self) -> &HirDesign {
        TirDesign::hir(self)
    }

    fn visible_def_ids(&self, owner: DefId) -> Vec<DefId> {
        self.hir().visible_def_ids(owner)
    }

    fn expr_resolution(&self, expr: ExprId) -> Option<HirResolution> {
        self.hir().expr_resolutions.get(&expr).copied()
    }

    fn binding_type(&self, binding: BindingRef) -> Option<TirType> {
        self.binding_types()
            .get(&binding)
            .and_then(|ty| self.type_table().get(*ty))
            .cloned()
    }

    fn expr_type(&self, expr: ExprId) -> Option<TirType> {
        self.expr_types()
            .get(&expr)
            .and_then(|ty| self.type_table().get(*ty))
            .cloned()
    }

    fn enum_variant_value(&self, key: &HirEnumVariantKey) -> Option<u64> {
        self.enum_variant_values().get(key).copied()
    }
}

#[non_exhaustive]
struct ElabProgramBuilder<'a, I>
where
    I: ProgramLoweringInput + ?Sized,
{
    input: &'a I,
}

impl<'a, I> ElabProgramBuilder<'a, I>
where
    I: ProgramLoweringInput + ?Sized,
{
    fn new(input: &'a I) -> Self {
        Self { input }
    }

    fn build(&self) -> ElabProgram {
        let hir = self.input.hir();
        let enum_max_values =
            hir.enum_variants
                .keys()
                .fold(BTreeMap::new(), |mut max_values, key| {
                    let Some(value) = self.input.enum_variant_value(key) else {
                        return max_values;
                    };
                    max_values
                        .entry(key.enum_def)
                        .and_modify(|current| {
                            if *current < value {
                                *current = value;
                            }
                        })
                        .or_insert(value);
                    max_values
                });
        let mut visible_defs = BTreeMap::new();
        for owner in &hir.defs {
            for def in self.input.visible_def_ids(owner.id) {
                if let Some(name) = hir.def_name(def) {
                    visible_defs.insert((owner.id, name.to_string()), def);
                }
            }
        }
        let expr_resolutions_by_id = hir
            .exprs
            .iter()
            .filter_map(|expr| {
                self.input
                    .expr_resolution(expr.id)
                    .map(|resolution| ((expr.owner, expr.id), ElabResolution::from(resolution)))
            })
            .collect();
        let local_types = hir
            .locals
            .iter()
            .filter_map(|local| {
                self.input
                    .binding_type(BindingRef::Local(local.id))
                    .map(|ty| (local.id, ty))
            })
            .collect();
        let expr_types = hir
            .exprs
            .iter()
            .filter_map(|expr| {
                self.input
                    .expr_type(expr.id)
                    .map(|ty| ((expr.owner, expr.id), ty))
            })
            .collect();
        ElabProgram {
            defs: hir
                .defs
                .iter()
                .map(|def| ElabDef {
                    name: def.name.clone(),
                    kind: ElabDefKind::from(def.kind),
                })
                .collect(),
            canonical_paths: hir
                .defs
                .iter()
                .map(|def| (def.id, def.canonical_path.clone()))
                .collect(),
            visible_defs,
            canonical_defs: hir.canonical_def_names.clone(),
            expr_resolutions_by_id,
            extension_methods: hir.extension_methods.clone(),
            expr_types,
            local_types,
            local_kinds: hir
                .locals
                .iter()
                .map(|local| (local.id, ElabLocalKind::from(local.kind)))
                .collect(),
            consts: hir
                .consts
                .iter()
                .map(|(def, item)| (*def, ElabConstItem::from(item)))
                .collect(),
            enums: hir
                .enums
                .iter()
                .map(|(def, item)| {
                    (
                        *def,
                        ElabEnumItem::new(item, enum_max_values.get(def).copied().unwrap_or(0)),
                    )
                })
                .collect(),
            enum_variants: hir
                .enum_variants
                .keys()
                .filter_map(|key| {
                    self.input
                        .enum_variant_value(key)
                        .map(|value| (ElabEnumVariantKey::from(key), value))
                })
                .collect(),
            bundles: hir
                .bundles
                .iter()
                .map(|(def, item)| (*def, ElabBundleItem::from(item)))
                .collect(),
            interfaces: hir
                .interfaces
                .iter()
                .map(|(def, item)| (*def, ElabInterfaceItem::from(item)))
                .collect(),
            callables: hir
                .callables
                .iter()
                .map(|(def, item)| (*def, ElabCallable::from(item)))
                .collect(),
        }
    }
}

impl ElabProgram {
    pub(crate) fn from_input<I>(input: &I) -> Self
    where
        I: ProgramLoweringInput + ?Sized,
    {
        ElabProgramBuilder::new(input).build()
    }

    pub(crate) fn from_tir(tir: &TirDesign) -> Self {
        Self::from_input(tir)
    }
}

#[cfg(test)]
mod tests {
    use super::{ElabProgram, ProgramLoweringInput};
    use crate::{
        program::design::{ElabDefKind, ElabResolution},
        tir::{BindingRef, TirType},
    };
    use std::collections::BTreeMap;
    use syl_hir::{
        DefId, ExprId, HirDef, HirDefKind, HirDesign, HirEnumVariant, HirEnumVariantKey, HirExpr,
        HirLocal, HirLocalKind, HirPath, HirResolution, LocalId,
    };
    use syl_span::Span;

    struct TestProgramInput {
        hir: HirDesign,
        visible_defs: BTreeMap<DefId, Vec<DefId>>,
        expr_resolutions: BTreeMap<ExprId, HirResolution>,
        binding_types: BTreeMap<BindingRef, TirType>,
        expr_types: BTreeMap<ExprId, TirType>,
        enum_variant_values: BTreeMap<HirEnumVariantKey, u64>,
    }

    impl ProgramLoweringInput for TestProgramInput {
        fn hir(&self) -> &HirDesign {
            &self.hir
        }

        fn visible_def_ids(&self, owner: DefId) -> Vec<DefId> {
            self.visible_defs.get(&owner).cloned().unwrap_or_default()
        }

        fn expr_resolution(&self, expr: ExprId) -> Option<HirResolution> {
            self.expr_resolutions.get(&expr).copied()
        }

        fn binding_type(&self, binding: BindingRef) -> Option<TirType> {
            self.binding_types.get(&binding).cloned()
        }

        fn expr_type(&self, expr: ExprId) -> Option<TirType> {
            self.expr_types.get(&expr).cloned()
        }

        fn enum_variant_value(&self, key: &HirEnumVariantKey) -> Option<u64> {
            self.enum_variant_values.get(key).copied()
        }
    }

    #[test]
    fn lowers_program_from_small_table_backed_fixture() {
        let span = Span::default();
        let owner = DefId::new(0);
        let enum_def = DefId::new(1);
        let method_def = DefId::new(2);
        let local = LocalId::new(0);
        let expr = ExprId::new(0);
        let red = HirEnumVariantKey::new(enum_def, "Red");
        let blue = HirEnumVariantKey::new(enum_def, "Blue");

        let mut hir = HirDesign::empty();
        hir.defs = vec![
            HirDef::new(
                owner,
                "Owner".to_string(),
                HirPath::new(vec!["pkg".to_string(), "Owner".to_string()]),
                HirDefKind::Const,
                span,
            ),
            HirDef::new(
                enum_def,
                "Color".to_string(),
                HirPath::new(vec!["pkg".to_string(), "Color".to_string()]),
                HirDefKind::Enum,
                span,
            ),
            HirDef::new(
                method_def,
                "decode".to_string(),
                HirPath::new(vec!["pkg".to_string(), "decode".to_string()]),
                HirDefKind::Fn,
                span,
            ),
        ];
        hir.locals = vec![HirLocal::new(
            local,
            owner,
            "flag".to_string(),
            HirLocalKind::Let,
            span,
        )];
        hir.exprs = vec![HirExpr::new(expr, owner, span)];
        hir.register_extension_method(enum_def, "decode".to_string(), method_def);
        hir.enum_variants
            .insert(red.clone(), HirEnumVariant::new(enum_def, "Red", 1, span));
        hir.enum_variants
            .insert(blue.clone(), HirEnumVariant::new(enum_def, "Blue", 4, span));

        let input = TestProgramInput {
            hir,
            visible_defs: BTreeMap::from([(owner, vec![enum_def])]),
            expr_resolutions: BTreeMap::from([(expr, HirResolution::Def(enum_def))]),
            binding_types: BTreeMap::from([(BindingRef::Local(local), TirType::Bool)]),
            expr_types: BTreeMap::from([(expr, TirType::Nat)]),
            enum_variant_values: BTreeMap::from([(red, 1), (blue, 4)]),
        };

        let program = ElabProgram::from_input(&input);

        assert_eq!(program.def_kind(enum_def), Some(ElabDefKind::Enum));
        assert_eq!(program.resolve_def_id(owner, "Color"), Some(enum_def));
        assert_eq!(
            program.canonical_path(enum_def).map(|path| path.display()),
            Some("pkg.Color".to_string())
        );
        assert_eq!(
            program.expr_resolutions_by_id.get(&(owner, expr)),
            Some(&ElabResolution::Def(enum_def))
        );
        assert_eq!(program.expr_types.get(&(owner, expr)), Some(&TirType::Nat));
        assert_eq!(program.local_types.get(&local), Some(&TirType::Bool));
        assert_eq!(
            program.enum_variant_value_for_def(enum_def, "Blue"),
            Some(4)
        );
        assert_eq!(
            program.extension_methods_for(enum_def, "decode"),
            &[method_def]
        );
    }
}
