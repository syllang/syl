mod expr;
mod items;

use crate::{
    constant::{Bindings, Result, dimension, invalid, number},
    design::*,
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hw::{
    HwDirection, HwInstance, HwOrigin, HwPortDomain, ParametricHwDesign, ParametricHwModule,
};

pub(crate) fn concretize(
    hw: &ParametricHwDesign,
    top: &str,
    params: &Bindings,
) -> Result<SimDesign> {
    let module = hw
        .modules()
        .iter()
        .find(|m| m.name() == top || m.canonical_path().join(".") == top)
        .ok_or_else(|| SimError::Invalid(format!("unknown top module {top}")))?;
    let origin = module
        .origin()
        .ok_or_else(|| SimError::Invalid(format!("missing origin for {top}")))?;
    let mut lower = Lower {
        hw,
        design: SimDesign::default(),
        active: BTreeSet::new(),
        clocks: BTreeMap::new(),
        resets: BTreeMap::new(),
        paths: BTreeSet::new(),
    };
    lower.instance(
        module,
        Placement {
            parent: None,
            path: top.into(),
            name: top.into(),
            origin,
            params: params.clone(),
            connections: BTreeMap::new(),
        },
    )?;
    lower.design.comb_eval_order = crate::schedule::order(&mut lower.design)?;
    Ok(lower.design)
}

struct Lower<'a> {
    hw: &'a ParametricHwDesign,
    design: SimDesign,
    active: BTreeSet<(String, Bindings)>,
    paths: BTreeSet<String>,
    clocks: BTreeMap<SignalId, DomainId>,
    resets: BTreeMap<SignalId, ResetId>,
}

struct Placement<'a> {
    parent: Option<InstanceId>,
    path: String,
    name: String,
    origin: &'a HwOrigin,
    params: Bindings,
    connections: BTreeMap<String, SimExpr>,
}

#[derive(Clone)]
struct Scope {
    instance: InstanceId,
    path: String,
    params: Bindings,
    generates: Vec<Generate>,
    signals: BTreeMap<String, SimTarget>,
    memories: BTreeMap<String, MemId>,
}

#[derive(Clone)]
struct Generate {
    label: String,
    iteration: Option<(String, u64)>,
}

impl Scope {
    fn source(&self, physical: &str, origin: &HwOrigin) -> SourceLocation {
        let local = origin
            .expansion_stack()
            .last()
            .and_then(|frame| physical.strip_prefix(frame.instance()))
            .and_then(|suffix| suffix.strip_prefix('_'))
            .unwrap_or(physical);
        let field = origin.source_field().unwrap_or(local);
        // A pre-expanded generate can add a unique prefix outside the in-place
        // stack. Keep that prefix in the path while exposing the Syl field.
        let leaf = if local.ends_with(field) && local != field {
            local
        } else {
            field
        };
        self.location(leaf, field, origin)
    }

    fn location(&self, leaf: &str, field: &str, origin: &HwOrigin) -> SourceLocation {
        let mut relative = String::new();
        let mut prefix = String::new();
        for frame in origin.expansion_stack() {
            let instance = frame.instance();
            relative.push('.');
            relative.push_str(
                frame
                    .source_name()
                    .unwrap_or_else(|| instance.strip_prefix(&prefix).unwrap_or(instance)),
            );
            prefix = format!("{instance}_");
        }
        relative.push('.');
        relative.push_str(leaf);
        let mut field = field.to_owned();
        let mut path = self.path.clone();
        for generate in &self.generates {
            if let Some((index, value)) = &generate.iteration {
                let symbolic = format!("[{index}]");
                if relative.contains(&symbolic) {
                    // A bound array element already names this iteration. Keep
                    // any enclosing anonymous generate scopes for uniqueness.
                    let concrete = format!("[{value}]");
                    relative = relative.replace(&symbolic, &concrete);
                    field = field.replace(&symbolic, &concrete);
                    continue;
                }
            }
            path.push('.');
            path.push_str(&generate.label);
        }
        path.push_str(&relative);
        SourceLocation {
            instance: self.instance,
            path,
            name: field,
            origin: origin.clone(),
        }
    }
    fn memory(&self, name: &str) -> Result<MemId> {
        self.memories
            .get(name)
            .copied()
            .ok_or_else(|| SimError::Invalid(format!("unknown memory {}.{name}", self.path)))
    }
}

impl SimTarget {
    pub(crate) fn whole(signal: SignalId) -> Self {
        Self {
            signal,
            selectors: Vec::new(),
        }
    }
}

impl SimExpr {
    fn as_target(&self) -> Result<&SimTarget> {
        if let Self::Read(target) = self {
            Ok(target)
        } else {
            invalid(format!("expected assignable signal, found {self:?}"))
        }
    }
    fn signal(&self) -> Result<SignalId> {
        let target = self.as_target()?;
        if target.selectors.is_empty() {
            Ok(target.signal)
        } else {
            invalid("expected whole signal")
        }
    }
}

fn bindings(module: &ParametricHwModule, overrides: &Bindings) -> Result<Bindings> {
    let mut out = Bindings::new();
    for p in module.params() {
        let value = if let Some(value) = overrides.get(p.name()) {
            if matches!(value, ParamValue::Bool(_)) != p.is_bool() {
                return invalid(format!(
                    "wrong parameter type for {}.{}",
                    module.name(),
                    p.name()
                ));
            }
            *value
        } else {
            let default = p.default_expr().ok_or_else(|| {
                SimError::Invalid(format!("missing parameter {}.{}", module.name(), p.name()))
            })?;
            typed(crate::constant::eval(default, &out)?, p.is_bool())?
        };
        out.insert(p.name().into(), value);
    }
    if let Some(key) = overrides.keys().find(|k| !out.contains_key(*k)) {
        return invalid(format!("unknown parameter {}.{key}", module.name()));
    }
    Ok(out)
}

fn typed(value: u64, boolean: bool) -> Result<ParamValue> {
    if boolean {
        if value > 1 {
            return invalid(format!("invalid boolean value {value}"));
        }
        Ok(ParamValue::Bool(value != 0))
    } else {
        Ok(ParamValue::Nat(value))
    }
}

impl<'a> Lower<'a> {
    fn instance(&mut self, module: &'a ParametricHwModule, place: Placement<'_>) -> Result<()> {
        let params = bindings(module, &place.params)?;
        let key = (module.name().to_owned(), params.clone());
        if !self.active.insert(key.clone()) {
            return invalid(format!("recursive instance {}", place.path));
        }
        if !self.paths.insert(place.path.clone()) {
            return invalid(format!("duplicate instance {}", place.path));
        }
        let id = InstanceId(self.design.instances.len());
        let family = self
            .design
            .families
            .iter()
            .position(|f| f.module == module.canonical_path() && f.parameters == params)
            .unwrap_or_else(|| {
                self.design.families.push(InstanceFamily {
                    module: module.canonical_path().to_vec(),
                    parameters: params.clone(),
                    members: Vec::new(),
                });
                self.design.families.len() - 1
            });
        self.design.families[family].members.push(id);
        let source = SourceLocation {
            instance: id,
            path: place.path.clone(),
            name: place.name,
            origin: place.origin.clone(),
        };
        self.design.instances.push(SimInstance {
            source,
            parent: place.parent,
            template: TemplateId(family),
            ports: Vec::new(),
        });
        let mut scope = Scope {
            instance: id,
            path: place.path,
            params,
            generates: Vec::new(),
            signals: BTreeMap::new(),
            memories: BTreeMap::new(),
        };
        let mut connections = place.connections;
        let mut extern_ports = Vec::new();
        for p in module.ports() {
            let actual = connections.remove(p.name());
            let origin = p
                .origin()
                .ok_or_else(|| SimError::Invalid(format!("missing port origin {}", p.name())))?;
            let width = dimension(p.width(), &scope.params)?;
            let source = scope.source(p.source_name(), origin);
            let signal = self.declare(&mut scope, p.name(), width, source)?;
            self.design.instances[id.0].ports.push(SimPort {
                signal,
                direction: p.direction(),
                name: p.name().into(),
            });
            self.domain(signal, p.domain(), actual.as_ref())?;
            if let Some(actual) = actual {
                let local = SimTarget::whole(signal);
                match p.direction() {
                    HwDirection::In => self.design.comb.push(CombAssign {
                        span: self.design.signal(signal).source.span(),
                        target: local,
                        value: actual.clone(),
                    }),
                    HwDirection::Out => self.design.comb.push(CombAssign {
                        span: self.design.signal(signal).source.span(),
                        target: actual.as_target()?.clone(),
                        value: SimExpr::Read(local),
                    }),
                    HwDirection::InOut => {
                        let target = actual.as_target()?.clone();
                        self.design.signals[signal.0].alias = Some(target);
                    }
                    _ => return invalid("unsupported port direction"),
                }
                extern_ports.push((signal, actual));
            }
        }
        if let Some(name) = connections.keys().next() {
            return invalid(format!("unknown formal {}.{name}", scope.path));
        }
        if module.is_extern() {
            self.design.externs.push(SimExternInstance {
                clocks: self.design.instances[id.0]
                    .ports
                    .iter()
                    .filter(|p| self.clocks.contains_key(&p.signal))
                    .map(|p| p.signal)
                    .collect(),
                instance: id,
                connections: extern_ports,
            });
        } else {
            self.block(module.items(), &mut scope)?;
        }
        self.active.remove(&key);
        Ok(())
    }

    fn declare(
        &mut self,
        scope: &mut Scope,
        physical: &str,
        width: u32,
        source: SourceLocation,
    ) -> Result<SignalId> {
        let id = SignalId(self.design.signals.len());
        if scope
            .signals
            .insert(physical.into(), SimTarget::whole(id))
            .is_some()
        {
            return invalid(format!("duplicate signal {}.{physical}", scope.path));
        }
        self.design.signals.push(SimSignal {
            source,
            width,
            alias: None,
        });
        Ok(id)
    }

    fn domain(
        &mut self,
        signal: SignalId,
        domain: &HwPortDomain,
        actual: Option<&SimExpr>,
    ) -> Result<()> {
        let source = actual.map(SimExpr::signal);
        match domain {
            HwPortDomain::Data => {}
            HwPortDomain::Clock { name } => {
                let source = source.transpose()?.unwrap_or(signal);
                let id = *self.clocks.entry(source).or_insert_with(|| {
                    let id = DomainId(self.design.clocks.len());
                    self.design.clocks.push(ClockDomain {
                        signal: source,
                        name: name.clone(),
                    });
                    id
                });
                self.clocks.insert(signal, id);
            }
            HwPortDomain::Reset {
                name,
                mode,
                polarity,
            } => {
                let source = source.transpose()?.unwrap_or(signal);
                let id = *self.resets.entry(source).or_insert_with(|| {
                    let id = ResetId(self.design.resets.len());
                    self.design.resets.push(ResetDomain {
                        signal: source,
                        name: name.clone(),
                        mode: *mode,
                        polarity: *polarity,
                    });
                    id
                });
                let parent = &self.design.resets[id.0];
                if parent.mode != *mode || parent.polarity != *polarity {
                    return invalid("incompatible reset port domains");
                }
                self.resets.insert(signal, id);
            }
            _ => return invalid("unsupported port domain"),
        }
        Ok(())
    }

    fn child(&mut self, instance: &HwInstance, origin: &HwOrigin, scope: &Scope) -> Result<()> {
        let module = self
            .hw
            .modules()
            .iter()
            .find(|m| m.canonical_path() == instance.target_canonical_path())
            .ok_or_else(|| {
                SimError::Invalid(format!(
                    "unknown instance target {:?}",
                    instance.target_canonical_path()
                ))
            })?;
        let source = match origin.source_field() {
            Some(binding) => scope.location(binding, binding, origin),
            None => scope.source(instance.source_name(), origin),
        };
        let path = source.path;
        let name = source.name;
        let mut params = Bindings::new();
        for bind in instance.params() {
            let formal = module
                .params()
                .iter()
                .find(|p| p.name() == bind.name())
                .ok_or_else(|| {
                    SimError::Invalid(format!("unknown parameter {path}.{}", bind.name()))
                })?;
            params.insert(
                bind.name().into(),
                typed(number(bind.value(), &scope.params)?, formal.is_bool())?,
            );
        }
        let mut connections = BTreeMap::new();
        for c in instance.connections() {
            connections.insert(c.formal().into(), self.expression(c.actual(), scope)?);
        }
        self.instance(
            module,
            Placement {
                parent: Some(scope.instance),
                path,
                name,
                origin,
                params,
                connections,
            },
        )
    }
}
