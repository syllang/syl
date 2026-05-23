use syl_span::Diagnostic;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StageOutput<T> {
    stage: Option<T>,
    diagnostics: Vec<Diagnostic>,
}

impl<T> StageOutput<T> {
    pub fn new(stage: Option<T>, diagnostics: Vec<Diagnostic>) -> Self {
        Self { stage, diagnostics }
    }

    pub fn stage(&self) -> Option<&T> {
        self.stage.as_ref()
    }

    pub fn partial_stage(&self) -> Option<&T> {
        self.stage()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_stage(self) -> Option<T> {
        self.stage
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn into_parts(self) -> (Option<T>, Vec<Diagnostic>) {
        (self.stage, self.diagnostics)
    }

    pub fn map_stage<U>(self, map: impl FnOnce(T) -> U) -> StageOutput<U> {
        StageOutput::new(self.stage.map(map), self.diagnostics)
    }
}
