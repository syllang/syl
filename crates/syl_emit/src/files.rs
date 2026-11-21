use super::{
    CompileError, SystemVerilogBackend, check,
    sv_ir::{self, SvItem, SvModule},
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hw::ParametricHwDesign;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmitOptions {
    pub source_comments: bool,
    pub header_comment: bool,
}
impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            source_comments: false,
            header_comment: true,
        }
    }
}
impl EmitOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_source_comments(mut self, enabled: bool) -> Self {
        self.source_comments = enabled;
        self
    }
    pub fn with_header_comment(mut self, enabled: bool) -> Self {
        self.header_comment = enabled;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceLocation {
    file: String,
    line: usize,
}
impl SourceLocation {
    pub fn new(file: impl Into<String>, line: usize) -> Self {
        Self {
            file: file.into(),
            line,
        }
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub fn line(&self) -> usize {
        self.line
    }
}
pub trait SourceLocationLookup {
    fn lookup(&self, source_index: usize, byte_offset: usize) -> Option<SourceLocation>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmittedSvFile {
    file_name: String,
    module_name: String,
    canonical_path: Vec<String>,
    content: String,
}
impl EmittedSvFile {
    pub fn file_name(&self) -> &str {
        &self.file_name
    }
    pub fn module_name(&self) -> &str {
        &self.module_name
    }
    pub fn canonical_path(&self) -> &[String] {
        &self.canonical_path
    }
    pub fn content(&self) -> &str {
        &self.content
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmittedSvBundle {
    files: Vec<EmittedSvFile>,
    filelist: String,
}
impl EmittedSvBundle {
    pub fn files(&self) -> &[EmittedSvFile] {
        &self.files
    }
    pub fn filelist(&self) -> &str {
        &self.filelist
    }
    pub fn filelist_lines(&self) -> Vec<&str> {
        self.filelist.lines().collect()
    }
}

impl SystemVerilogBackend {
    pub fn emit_files(&self, hwir: &ParametricHwDesign) -> Result<EmittedSvBundle, CompileError> {
        self.emit_files_with_options(hwir, &EmitOptions::default(), None)
    }
    pub fn emit_files_with_options(
        &self,
        hwir: &ParametricHwDesign,
        options: &EmitOptions,
        lookup: Option<&dyn SourceLocationLookup>,
    ) -> Result<EmittedSvBundle, CompileError> {
        let design = self.compile_sv_design(hwir, options.source_comments, lookup)?;
        check::SvBackendValidator::new(&design).validate()?;
        let mut files = Vec::new();
        let mut filelist = String::new();
        for index in dependency_order(&design.modules) {
            let module = &design.modules[index];
            let content =
                sv_ir::printer::text(std::slice::from_ref(module), options.header_comment);
            check::SvSourceValidator::new().validate(&content)?;
            let file_name = format!("{}.sv", module.name);
            filelist.push_str(&file_name);
            filelist.push('\n');
            files.push(EmittedSvFile {
                file_name,
                module_name: module.name.clone(),
                canonical_path: hwir.modules()[index].canonical_path().to_vec(),
                content,
            });
        }
        Ok(EmittedSvBundle { files, filelist })
    }
}

fn dependency_order(modules: &[SvModule]) -> Vec<usize> {
    let indices = modules
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let mut parents = vec![Vec::new(); modules.len()];
    let mut counts = Vec::new();
    for (parent, module) in modules.iter().enumerate() {
        let mut children = BTreeSet::new();
        dependencies(&module.items, &indices, &mut children);
        counts.push(children.len());
        for child in children {
            parents[child].push(parent);
        }
    }
    let mut ready = (0..modules.len())
        .filter(|i| counts[*i] == 0)
        .collect::<BTreeSet<_>>();
    let mut pending = (0..modules.len()).collect::<BTreeSet<_>>();
    let mut order = Vec::new();
    while let Some(&first) = pending.first() {
        let index = ready.pop_first().unwrap_or(first);
        pending.remove(&index);
        order.push(index);
        for &parent in &parents[index] {
            counts[parent] -= 1;
            if counts[parent] == 0 && pending.contains(&parent) {
                ready.insert(parent);
            }
        }
    }
    order
}
fn dependencies(items: &[SvItem], indices: &BTreeMap<&str, usize>, out: &mut BTreeSet<usize>) {
    for item in items {
        match item {
            SvItem::Instance(instance) => {
                if let Some(&child) = indices.get(instance.module.as_str()) {
                    out.insert(child);
                }
            }
            SvItem::GenerateIf {
                then_items,
                else_items,
                ..
            } => {
                dependencies(then_items, indices, out);
                dependencies(else_items, indices, out);
            }
            SvItem::GenerateFor { items, .. } => dependencies(items, indices, out),
            _ => {}
        }
    }
}
