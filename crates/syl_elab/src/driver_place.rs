use crate::{
    eir_expr::{EirBinaryOp, EirBound, EirExpr, EirUnaryOp},
    eir_place::EirPlace,
};
use std::collections::BTreeMap;
use syl_hw::ObjectId;

mod bounds;
mod overlap;

pub(crate) use bounds::{DriverBitRange, DriverBound, DriverStaticRange};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum DriverPlace {
    #[allow(
        dead_code,
        reason = "Driver overlap unit tests keep an unresolved root variant; production resolver rejects unknown roots."
    )]
    Ident(String),
    Object(DriverObject),
    Slice {
        base: Box<DriverPlace>,
        range: DriverBitRange,
    },
    IndexedPartSelect {
        base: Box<DriverPlace>,
        index: DriverExpr,
        width: DriverBound,
    },
    Index {
        base: Box<DriverPlace>,
        index: DriverExpr,
    },
    Expr(DriverExpr),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) struct DriverObject {
    id: ObjectId,
    name: String,
}

impl DriverObject {
    fn new(id: ObjectId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    pub(crate) fn id(&self) -> ObjectId {
        self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

#[non_exhaustive]
pub(crate) struct DriverObjectTable {
    objects: BTreeMap<(String, String), ObjectId>,
    widths: BTreeMap<ObjectId, DriverBound>,
}

impl DriverObjectTable {
    pub(crate) fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            widths: BTreeMap::new(),
        }
    }

    pub(crate) fn intern(&mut self, module: &str, name: &str) -> ObjectId {
        let key = (module.to_string(), name.to_string());
        if let Some(id) = self.objects.get(&key) {
            return *id;
        }
        let id = ObjectId::new(self.objects.len());
        self.objects.insert(key, id);
        id
    }

    pub(crate) fn intern_with_bound(
        &mut self,
        module: &str,
        name: &str,
        width: &EirBound,
    ) -> ObjectId {
        let id = self.intern(module, name);
        self.widths.insert(id, DriverBound::from_eir_bound(width));
        id
    }

    pub(crate) fn width(&self, id: ObjectId) -> Option<&DriverBound> {
        self.widths.get(&id)
    }

    pub(crate) fn object_id(&self, module: &str, name: &str) -> Option<ObjectId> {
        self.objects
            .get(&(module.to_string(), name.to_string()))
            .copied()
    }

    fn object(&self, module: &str, name: &str) -> Option<DriverObject> {
        self.objects
            .get(&(module.to_string(), name.to_string()))
            .copied()
            .map(|id| DriverObject::new(id, name))
    }
}

#[non_exhaustive]
pub(crate) struct DriverPlaceResolver<'module, 'objects> {
    module: &'module str,
    objects: &'objects DriverObjectTable,
}

impl<'module, 'objects> DriverPlaceResolver<'module, 'objects> {
    pub(crate) fn new(module: &'module str, objects: &'objects DriverObjectTable) -> Self {
        Self { module, objects }
    }

    pub(crate) fn resolve_place(&self, place: &EirPlace) -> Result<DriverPlace, DriverPlaceError> {
        match place {
            EirPlace::Ident(name) => self.ident_place(name),
            EirPlace::Slice { base, high, low } => Ok(DriverPlace::Slice {
                base: Box::new(self.resolve_place(base)?),
                range: DriverBitRange::from_eir_bounds(low, high),
            }),
            EirPlace::IndexedPartSelect { base, index, width } => {
                Ok(DriverPlace::IndexedPartSelect {
                    base: Box::new(self.resolve_place(base)?),
                    index: DriverExpr::from_eir_expr(index)?,
                    width: DriverBound::from_eir_bound(width),
                })
            }
            EirPlace::Index { base, index } => Ok(DriverPlace::Index {
                base: Box::new(self.resolve_place(base)?),
                index: DriverExpr::from_eir_expr(index)?,
            }),
        }
    }

    fn ident_place(&self, name: &str) -> Result<DriverPlace, DriverPlaceError> {
        self.objects
            .object(self.module, name)
            .map(DriverPlace::Object)
            .ok_or_else(|| DriverPlaceError::UnknownObject {
                module: self.module.to_string(),
                name: name.to_string(),
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum DriverExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
    Zero,
    Unary {
        op: EirUnaryOp,
        expr: Box<DriverExpr>,
    },
    Binary {
        op: EirBinaryOp,
        left: Box<DriverExpr>,
        right: Box<DriverExpr>,
    },
    Mux {
        cond: Box<DriverExpr>,
        then_value: Box<DriverExpr>,
        else_value: Box<DriverExpr>,
    },
    Concat(Vec<DriverExpr>),
    Slice {
        value: Box<DriverExpr>,
        range: DriverBitRange,
    },
    IndexedPartSelect {
        value: Box<DriverExpr>,
        index: Box<DriverExpr>,
        width: DriverBound,
    },
    Index {
        value: Box<DriverExpr>,
        index: Box<DriverExpr>,
    },
    Call {
        name: String,
        args: Vec<DriverExpr>,
    },
}

impl DriverExpr {
    pub(crate) fn from_eir_expr(expr: &EirExpr) -> Result<Self, DriverExprError> {
        match expr {
            EirExpr::Ident(name) => Ok(Self::Ident(name.clone())),
            EirExpr::Int(value) => Ok(Self::Int(*value)),
            EirExpr::Bool(value) => Ok(Self::Bool(*value)),
            EirExpr::Str(value) => Ok(Self::Str(value.clone())),
            EirExpr::Zero => Ok(Self::Zero),
            EirExpr::Unary { op, expr } => Ok(Self::Unary {
                op: *op,
                expr: Box::new(Self::from_eir_expr(expr)?),
            }),
            EirExpr::Binary { op, left, right } => Ok(Self::Binary {
                op: *op,
                left: Box::new(Self::from_eir_expr(left)?),
                right: Box::new(Self::from_eir_expr(right)?),
            }),
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => Ok(Self::Mux {
                cond: Box::new(Self::from_eir_expr(cond)?),
                then_value: Box::new(Self::from_eir_expr(then_value)?),
                else_value: Box::new(Self::from_eir_expr(else_value)?),
            }),
            EirExpr::Select { arms, default, .. } => {
                let mut expr = Self::from_eir_expr(default)?;
                for arm in arms.iter().rev() {
                    expr = Self::Mux {
                        cond: Box::new(Self::from_eir_expr(arm.guard())?),
                        then_value: Box::new(Self::from_eir_expr(arm.value())?),
                        else_value: Box::new(expr),
                    };
                }
                Ok(expr)
            }
            EirExpr::Concat(parts) => Ok(Self::Concat(
                parts
                    .iter()
                    .map(Self::from_eir_expr)
                    .collect::<Result<Vec<_>, DriverExprError>>()?,
            )),
            EirExpr::Slice { value, high, low } => Ok(Self::Slice {
                value: Box::new(Self::from_eir_expr(value)?),
                range: DriverBitRange::from_eir_bounds(low, high),
            }),
            EirExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => Ok(Self::IndexedPartSelect {
                value: Box::new(Self::from_eir_expr(value)?),
                index: Box::new(Self::from_eir_expr(index)?),
                width: DriverBound::from_eir_bound(width),
            }),
            EirExpr::Index { value, index } => Ok(Self::Index {
                value: Box::new(Self::from_eir_expr(value)?),
                index: Box::new(Self::from_eir_expr(index)?),
            }),
            EirExpr::Call { name, args } => Ok(Self::Call {
                name: name.clone(),
                args: args
                    .iter()
                    .map(Self::from_eir_expr)
                    .collect::<Result<Vec<_>, DriverExprError>>()?,
            }),
            EirExpr::Unsupported { .. } => Err(DriverExprError),
        }
    }

    pub(crate) fn display(&self) -> String {
        match self {
            Self::Ident(name) => name.clone(),
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Str(value) => format!("str({value})"),
            Self::Zero => "zero".to_string(),
            Self::Unary { op, expr } => format!("{op:?}({})", expr.display()),
            Self::Binary { op, left, right } => {
                format!("{op:?}({},{})", left.display(), right.display())
            }
            Self::Mux {
                cond,
                then_value,
                else_value,
            } => format!(
                "mux({},{},{})",
                cond.display(),
                then_value.display(),
                else_value.display()
            ),
            Self::Concat(parts) => {
                let parts = parts
                    .iter()
                    .map(Self::display)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("concat({parts})")
            }
            Self::Slice { value, range } => {
                format!("slice({},{})", value.display(), range.display())
            }
            Self::IndexedPartSelect {
                value,
                index,
                width,
            } => format!(
                "part({},{},{})",
                value.display(),
                index.display(),
                width.source()
            ),
            Self::Index { value, index } => {
                format!("idx({},{})", value.display(), index.display())
            }
            Self::Call { name, args } => {
                let args = args.iter().map(Self::display).collect::<Vec<_>>().join(",");
                format!("call({name},{args})")
            }
        }
    }

    fn references_root(&self, root: &str) -> bool {
        match self {
            Self::Ident(name) => name == root,
            Self::Unary { expr, .. } => expr.references_root(root),
            Self::Binary { left, right, .. } => {
                left.references_root(root) || right.references_root(root)
            }
            Self::Mux {
                cond,
                then_value,
                else_value,
            } => {
                cond.references_root(root)
                    || then_value.references_root(root)
                    || else_value.references_root(root)
            }
            Self::Concat(parts) => parts.iter().any(|expr| expr.references_root(root)),
            Self::Slice { value, .. } => value.references_root(root),
            Self::IndexedPartSelect { value, index, .. } | Self::Index { value, index } => {
                value.references_root(root) || index.references_root(root)
            }
            Self::Call { args, .. } => args.iter().any(|expr| expr.references_root(root)),
            Self::Int(_) | Self::Bool(_) | Self::Str(_) | Self::Zero => false,
        }
    }

    pub(crate) fn as_int(&self) -> Option<u64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct DriverExprError;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum DriverPlaceError {
    UnsupportedExpr,
    UnknownObject { module: String, name: String },
}

impl From<DriverExprError> for DriverPlaceError {
    fn from(_: DriverExprError) -> Self {
        Self::UnsupportedExpr
    }
}

impl DriverPlace {
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Ident(name) => name.clone(),
            Self::Object(object) => object.name().to_string(),
            Self::Slice { base, range } => {
                format!("slice({},{})", base.display(), range.display())
            }
            Self::IndexedPartSelect { base, index, width } => {
                format!(
                    "part({},{},{})",
                    base.display(),
                    index.display(),
                    width.source()
                )
            }
            Self::Index { base, index } => format!("idx({},{})", base.display(), index.display()),
            Self::Expr(expr) => expr.display(),
        }
    }

    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        overlap::DriverPlaceOverlap::new(self, other).may_overlap()
    }
}

#[cfg(test)]
mod tests {
    use super::{DriverExpr, DriverObjectTable, DriverPlaceResolver};
    use crate::{eir_expr::EirExpr, eir_place::EirPlace};

    #[test]
    fn driver_expr_rejects_unsupported_eir_expr() {
        let expr = EirExpr::mux(
            EirExpr::ident("cond"),
            EirExpr::unsupported("bad branch"),
            EirExpr::Int(0),
        );

        assert!(DriverExpr::from_eir_expr(&expr).is_err());
    }

    #[test]
    fn place_resolver_rejects_unsupported_projection_index() {
        let mut objects = DriverObjectTable::new();
        objects.intern("Top", "word");
        let resolver = DriverPlaceResolver::new("Top", &objects);
        let place = EirPlace::Index {
            base: Box::new(EirPlace::Ident("word".to_string())),
            index: EirExpr::unsupported("bad index"),
        };

        assert!(resolver.resolve_place(&place).is_err());
    }

    #[test]
    fn place_resolver_rejects_unknown_ident() {
        let objects = DriverObjectTable::new();
        let resolver = DriverPlaceResolver::new("Top", &objects);
        let place = EirPlace::Ident("missing".to_string());

        let error = resolver
            .resolve_place(&place)
            .expect_err("unknown EIR place root must not become a string fallback");

        assert_eq!(
            error,
            super::DriverPlaceError::UnknownObject {
                module: "Top".to_string(),
                name: "missing".to_string()
            }
        );
    }
}
