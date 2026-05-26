use crate::{
    CompileError, HirError, SemanticSourceFile,
    hir::{
        HirBlock, HirBodyExpr, HirBundleItem, HirCallable, HirCallableItem, HirConstItem, HirDef,
        HirDefKind, HirDesign, HirEnumItem, HirEnumVariant, HirEnumVariantKey, HirExprNode,
        HirFieldDecl, HirFnItem, HirImport, HirInterfaceItem, HirLocal, HirLocalKind, HirMapItem,
        HirMemberDecl, HirMemberKind, HirPackage, HirSignatureGenericParam, HirSignatureParam,
        HirStmt, HirViewDecl, HirViewField,
    },
    hir_resolve::HirNameResolver,
};
use std::collections::BTreeSet;
use syl_hir::{DefId, LocalId, PackageId, name::HirPath};
use syl_span::Span;
use syl_syntax::{
    BundleItem, ConstItem, EnumItem, ExternModuleItem, FnItem, InterfaceItem, Item, MapItem,
};

mod index;

#[non_exhaustive]
pub struct HirResolver<'files> {
    sources: Vec<SemanticSourceFile<'files>>,
    design: HirDesign,
}

impl<'files> HirResolver<'files> {
    pub fn new(files: &'files [syl_syntax::AstFile]) -> Self {
        let sources = files
            .iter()
            .enumerate()
            .map(|(index, ast)| SemanticSourceFile::new(vec![format!("file{index}")], ast))
            .collect();
        Self::new_sources(sources)
    }

    pub fn new_sources(sources: Vec<SemanticSourceFile<'files>>) -> Self {
        Self {
            sources,
            design: HirDesign::empty(),
        }
    }

    pub fn resolve(mut self) -> Result<HirDesign, CompileError> {
        self.build_index()?;
        HirNameResolver::new(&mut self.design).resolve()?;
        Ok(self.design)
    }

    pub fn resolve_collect(mut self) -> Result<HirDesign, Vec<CompileError>> {
        let mut errors = self.build_index_collect();
        if let Err(mut resolve_errors) =
            HirNameResolver::new_collect(&mut self.design).resolve_collect()
        {
            errors.append(&mut resolve_errors);
        }
        if errors.is_empty() {
            Ok(self.design)
        } else {
            Err(errors)
        }
    }

    pub fn resolve_partial(mut self) -> (HirDesign, Vec<CompileError>) {
        let mut errors = self.build_index_collect();
        if let Err(mut resolve_errors) =
            HirNameResolver::new_collect(&mut self.design).resolve_collect()
        {
            errors.append(&mut resolve_errors);
        }
        (self.design, errors)
    }

    fn build_index(&mut self) -> Result<(), CompileError> {
        let sources = std::mem::take(&mut self.sources);
        for source in sources {
            let package = PackageScope::new(source.module_path());
            self.insert_package(&source);
            self.insert_imports(&source, &package);
            for item in &source.ast().items {
                self.insert_item(item, &package)?;
            }
        }
        self.validate_imports()?;
        self.register_extension_methods();
        Ok(())
    }

    fn build_index_collect(&mut self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        let sources = std::mem::take(&mut self.sources);
        for source in sources {
            let package = PackageScope::new(source.module_path());
            self.insert_package(&source);
            self.insert_imports(&source, &package);
            for item in &source.ast().items {
                if let Err(error) = self.insert_item(item, &package) {
                    errors.push(error);
                }
            }
        }
        errors.extend(self.validate_imports_collect());
        self.register_extension_methods();
        errors
    }

    fn insert_package(&mut self, source: &SemanticSourceFile<'_>) {
        let id = PackageId::new(self.design.packages.len());
        self.design.packages.push(HirPackage::new(
            id,
            source.module_path().to_vec(),
            source
                .ast()
                .items
                .first()
                .map(Item::span)
                .unwrap_or_default(),
        ));
    }

    fn insert_imports(&mut self, source: &SemanticSourceFile<'_>, package: &PackageScope) {
        for item in &source.ast().items {
            let Item::Use(import) = item else {
                continue;
            };
            self.design.imports.push(HirImport::new(
                import.path.clone(),
                package.path.clone(),
                import.span,
            ));
        }
    }

    fn validate_imports(&self) -> Result<(), CompileError> {
        match self.validate_imports_collect().into_iter().next() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn validate_imports_collect(&self) -> Vec<CompileError> {
        self.design
            .imports
            .iter()
            .filter(|import| {
                !self
                    .design
                    .canonical_def_names
                    .contains_key(&HirPath::new(import.path.clone()))
            })
            .map(|import| {
                CompileError::lowering_at(
                    HirError::UnknownImport {
                        path: import.path.join("."),
                        start: import.span.start,
                        end: import.span.end,
                    },
                    import.span,
                )
            })
            .collect()
    }

    fn insert_item(&mut self, item: &Item, package: &PackageScope) -> Result<(), CompileError> {
        match item {
            Item::Const(item) => self.insert_const(item, package),
            Item::Fn(item) => self.insert_fn(item, package),
            Item::Enum(item) => self.insert_enum(item, package),
            Item::Bundle(item) => self.insert_bundle(item, package),
            Item::Interface(item) => self.insert_interface(item, package),
            Item::Map(item) => self.insert_map(item, package),
            Item::Cell(item) => self.insert_callable(
                &item.name,
                HirCallable::Cell(HirCallableItem::from(item)),
                package,
            ),
            Item::Module(item) => self.insert_callable(
                &item.name,
                HirCallable::Module(HirCallableItem::from(item)),
                package,
            ),
            Item::ExternModule(item) => self.insert_extern_module(item, package),
            Item::Error(_) | Item::Use(_) => Ok(()),
            _ => Ok(()),
        }
    }

    fn insert_const(
        &mut self,
        item: &ConstItem,
        package: &PackageScope,
    ) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateConst { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Const, item.span);
        let mut item = HirConstItem::from(item);
        self.index_const(owner, &mut item);
        self.design.consts.insert(owner, item);
        Ok(())
    }

    fn insert_fn(&mut self, item: &FnItem, package: &PackageScope) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateFn { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Fn, item.span);
        let mut item = HirFnItem::from(item);
        self.register_params(owner, &mut item.params);
        self.register_block_locals(owner, &mut item.body);
        self.index_fn(owner, &mut item);
        self.design.fns.insert(owner, item);
        Ok(())
    }

    fn insert_enum(&mut self, item: &EnumItem, package: &PackageScope) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateEnum { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Enum, item.span);
        let mut seen = BTreeSet::new();
        for (idx, variant) in item.variants.iter().enumerate() {
            if !seen.insert(variant.name.clone()) {
                return Err(CompileError::lowering_at(
                    HirError::DuplicateEnumVariant {
                        name: variant.name.clone(),
                    },
                    variant.span,
                ));
            }
            if let Ok(value) = u64::try_from(idx) {
                self.design.enum_variants.insert(
                    HirEnumVariantKey::new(owner, variant.name.clone()),
                    HirEnumVariant::new(owner, variant.name.clone(), value, variant.span),
                );
            }
        }
        self.design.enums.insert(owner, HirEnumItem::from(item));
        Ok(())
    }

    fn insert_bundle(
        &mut self,
        item: &BundleItem,
        package: &PackageScope,
    ) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateBundle { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Bundle, item.span);
        let mut item = HirBundleItem::from(item);
        self.register_generics(owner, &mut item.generics);
        self.register_bundle_members(owner, &item.fields);
        self.index_bundle(owner, &mut item);
        self.design.bundles.insert(owner, item);
        Ok(())
    }

    fn insert_interface(
        &mut self,
        item: &InterfaceItem,
        package: &PackageScope,
    ) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateInterface { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Interface, item.span);
        let mut item = HirInterfaceItem::from(item);
        self.register_generics(owner, &mut item.generics);
        self.register_interface_members(owner, &item.fields, &item.views);
        self.index_interface(owner, &mut item);
        self.design.interfaces.insert(owner, item);
        Ok(())
    }

    fn insert_map(&mut self, item: &MapItem, package: &PackageScope) -> Result<(), CompileError> {
        self.reject_duplicate(package, &item.name, item.span, |name| {
            HirError::DuplicateMap { name }
        })?;
        let owner = self.register_def(package, &item.name, HirDefKind::Map, item.span);
        let mut item = HirMapItem::from(item);
        self.register_generics(owner, &mut item.generics);
        self.register_params(owner, &mut item.params);
        self.index_map(owner, &mut item);
        self.design.maps.insert(owner, item);
        Ok(())
    }

    fn insert_extern_module(
        &mut self,
        item: &ExternModuleItem,
        package: &PackageScope,
    ) -> Result<(), CompileError> {
        self.insert_callable(
            &item.name,
            HirCallable::Extern(crate::hir::HirExternModuleItem::from(item)),
            package,
        )
    }

    fn insert_callable(
        &mut self,
        name: &str,
        mut callable: HirCallable,
        package: &PackageScope,
    ) -> Result<(), CompileError> {
        let span = match &callable {
            HirCallable::Cell(item) | HirCallable::Module(item) => item.span,
            HirCallable::Extern(item) => item.span,
            _ => unreachable!("HirResolver only constructs current callable variants"),
        };
        self.reject_duplicate(package, name, span, |name| HirError::DuplicateCallable {
            name,
        })?;
        let kind = match &callable {
            HirCallable::Cell(_) => HirDefKind::Cell,
            HirCallable::Module(_) => HirDefKind::Module,
            HirCallable::Extern(_) => HirDefKind::ExternModule,
            _ => unreachable!("HirResolver only constructs current callable variants"),
        };
        let owner = self.register_def(package, name, kind, span);
        match &mut callable {
            HirCallable::Cell(item) | HirCallable::Module(item) => {
                self.register_callable_locals(owner, item);
                self.index_callable(owner, item);
            }
            HirCallable::Extern(item) => {
                self.register_generics(owner, &mut item.generics);
                self.register_params(owner, &mut item.params);
                if let Some(result) = &mut item.result {
                    result.id = Some(self.register_local(
                        owner,
                        &result.name,
                        HirLocalKind::Result,
                        result.span,
                    ));
                }
                self.index_extern_module(owner, item);
            }
            _ => unreachable!("HirResolver only constructs current callable variants"),
        }
        self.design.callables.insert(owner, callable);
        Ok(())
    }

    fn reject_duplicate(
        &self,
        package: &PackageScope,
        name: &str,
        span: Span,
        error: impl FnOnce(String) -> HirError,
    ) -> Result<(), CompileError> {
        if self
            .design
            .canonical_def_names
            .contains_key(&package.canonical_def_path(name))
        {
            return Err(CompileError::lowering_at(error(name.to_string()), span));
        }
        Ok(())
    }

    fn register_def(
        &mut self,
        package: &PackageScope,
        name: &str,
        kind: HirDefKind,
        span: Span,
    ) -> DefId {
        let id = DefId::new(self.design.defs.len());
        let canonical_path = package.canonical_def_path(name);
        self.design
            .def_names
            .entry(name.to_string())
            .or_default()
            .push(id);
        self.design
            .canonical_def_names
            .insert(canonical_path.clone(), id);
        self.design.defs.push(HirDef::new(
            id,
            name.to_string(),
            canonical_path,
            kind,
            span,
        ));
        id
    }

    fn register_callable_locals(&mut self, owner: DefId, item: &mut HirCallableItem) {
        self.register_generics(owner, &mut item.generics);
        self.register_params(owner, &mut item.params);
        if let Some(result) = &mut item.result {
            result.id =
                Some(self.register_local(owner, &result.name, HirLocalKind::Result, result.span));
        }
        self.register_block_locals(owner, &mut item.body);
    }

    fn register_generics(&mut self, owner: DefId, generics: &mut [HirSignatureGenericParam]) {
        for generic in generics {
            generic.id = Some(self.register_local(
                owner,
                &generic.name,
                HirLocalKind::Generic,
                generic.span,
            ));
        }
    }

    fn register_params(&mut self, owner: DefId, params: &mut [HirSignatureParam]) {
        for param in params {
            param.id =
                Some(self.register_local(owner, &param.name, HirLocalKind::Param, param.span));
        }
    }

    fn register_extension_methods(&mut self) {
        let maps = self
            .design
            .maps
            .iter()
            .filter_map(|(owner, item)| {
                item.params
                    .first()
                    .filter(|param| param.is_receiver())
                    .map(|param| (*owner, item.name.clone(), param.ty.clone()))
            })
            .collect::<Vec<_>>();
        let fns = self
            .design
            .fns
            .iter()
            .filter_map(|(owner, item)| {
                item.params
                    .first()
                    .filter(|param| param.is_receiver())
                    .map(|param| (*owner, item.name.clone(), param.ty.clone()))
            })
            .collect::<Vec<_>>();
        for (owner, name, ty) in maps.into_iter().chain(fns) {
            if let Some(receiver) = self.design.type_def_for_mir_type(owner, &ty) {
                self.design.register_extension_method(receiver, name, owner);
            }
        }
    }

    fn register_block_locals(&mut self, owner: DefId, body: &mut HirBlock) {
        for stmt in &mut body.stmts {
            match stmt {
                HirStmt::Const { id, name, span, .. } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Const, *span));
                }
                HirStmt::Let {
                    id,
                    name,
                    value,
                    span,
                    ..
                } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Let, *span));
                    if let Some(value) = value {
                        self.register_expr_locals(owner, value);
                    }
                }
                HirStmt::Var { id, name, span, .. } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Var, *span));
                }
                HirStmt::Signal { id, name, span, .. } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Signal, *span));
                }
                HirStmt::Reg { id, name, span, .. } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Reg, *span));
                }
                HirStmt::Assign { target, value, .. } | HirStmt::Drive { target, value, .. } => {
                    self.register_expr_locals(owner, target);
                    self.register_expr_locals(owner, value);
                }
                HirStmt::ElabIf {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.register_block_locals(owner, then_block);
                    if let Some(block) = else_block {
                        self.register_block_locals(owner, block);
                    }
                }
                HirStmt::ElabFor {
                    id,
                    name,
                    body,
                    span,
                    ..
                } => {
                    *id = Some(self.register_local(owner, name, HirLocalKind::Loop, *span));
                    self.register_block_locals(owner, body);
                }
                HirStmt::While { body, .. } => self.register_block_locals(owner, body),
                HirStmt::Expr(expr) => self.register_expr_locals(owner, expr),
                HirStmt::Next { value, .. } => {
                    self.register_expr_locals(owner, value);
                }
                HirStmt::Return(Some(expr), _) => {
                    self.register_expr_locals(owner, expr);
                }
                HirStmt::Return(None, _) | HirStmt::Error { .. } => {}
                _ => {}
            }
        }
        if let Some(tail) = body.tail.as_deref_mut() {
            self.register_expr_locals(owner, tail);
        }
    }

    fn register_expr_locals(&mut self, owner: DefId, expr: &mut HirBodyExpr) {
        let expr_span = expr.span();
        match &mut expr.node {
            HirExprNode::Unary { expr, .. } | HirExprNode::Group(expr) => {
                self.register_expr_locals(owner, expr);
            }
            HirExprNode::Binary { left, right, .. } => {
                self.register_expr_locals(owner, left);
                self.register_expr_locals(owner, right);
            }
            HirExprNode::Call { callee, args } | HirExprNode::Place { callee, args } => {
                self.register_expr_locals(owner, callee);
                for arg in args {
                    self.register_expr_locals(owner, &mut arg.value);
                }
            }
            HirExprNode::GenericApp { callee, .. } => self.register_expr_locals(owner, callee),
            HirExprNode::Aggregate { fields, .. } => {
                for field in fields {
                    self.register_expr_locals(owner, &mut field.value);
                }
            }
            HirExprNode::Field { base, .. } => self.register_expr_locals(owner, base),
            HirExprNode::Index { base, index } => {
                self.register_expr_locals(owner, base);
                self.register_expr_locals(owner, index);
            }
            HirExprNode::Block(block) => self.register_block_locals(owner, block),
            HirExprNode::Match { expr, arms } => {
                self.register_expr_locals(owner, expr);
                for arm in arms {
                    self.register_expr_locals(owner, &mut arm.value);
                }
            }
            HirExprNode::Select { arms, .. } => {
                for arm in arms {
                    self.register_expr_locals(owner, &mut arm.pattern);
                    self.register_expr_locals(owner, &mut arm.value);
                }
            }
            HirExprNode::CompileError { message } => self.register_expr_locals(owner, message),
            HirExprNode::Range { start, end } => {
                self.register_expr_locals(owner, start);
                self.register_expr_locals(owner, end);
            }
            HirExprNode::For {
                id,
                name,
                range,
                body,
            } => {
                self.register_expr_locals(owner, range);
                *id = Some(self.register_local(owner, name, HirLocalKind::Loop, expr_span));
                self.register_block_locals(owner, body);
            }
            HirExprNode::Ident(_)
            | HirExprNode::Int(_)
            | HirExprNode::Str(_)
            | HirExprNode::Bool(_)
            | HirExprNode::Unsupported => {}
            _ => {}
        }
    }

    fn register_bundle_members(&mut self, owner: DefId, fields: &[HirFieldDecl]) {
        for field in fields {
            self.design.member_decls.push(HirMemberDecl::new(
                owner,
                field.name.clone(),
                HirMemberKind::Field {
                    ty: field.ty.clone(),
                },
                field.span,
            ));
        }
    }

    fn register_interface_members(
        &mut self,
        owner: DefId,
        fields: &[HirFieldDecl],
        views: &[HirViewDecl],
    ) {
        self.register_bundle_members(owner, fields);
        for view in views {
            self.design.member_decls.push(HirMemberDecl::new(
                owner,
                view.name.clone(),
                HirMemberKind::View,
                view.span,
            ));
            self.register_view_fields(owner, &view.name, &view.fields);
        }
    }

    fn register_view_fields(&mut self, owner: DefId, view: &str, fields: &[HirViewField]) {
        for field in fields {
            self.design.member_decls.push(HirMemberDecl::new(
                owner,
                field.name.clone(),
                HirMemberKind::ViewField {
                    view: view.to_string(),
                },
                field.span,
            ));
        }
    }

    fn register_local(
        &mut self,
        owner: DefId,
        name: &str,
        kind: HirLocalKind,
        span: Span,
    ) -> LocalId {
        let id = LocalId::new(self.design.locals.len());
        self.design
            .locals
            .push(HirLocal::new(id, owner, name.to_string(), kind, span));
        id
    }
}

#[non_exhaustive]
struct PackageScope {
    path: HirPath,
}

impl PackageScope {
    fn new(module_path: &[String]) -> Self {
        let path = HirPath::new(module_path.to_vec());
        Self { path }
    }

    fn canonical_def_path(&self, name: &str) -> HirPath {
        self.path.with_leaf(name)
    }
}
