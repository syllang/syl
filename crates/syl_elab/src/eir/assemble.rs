use super::{EirDesign, EirDesignFacts, EirRawDesign};
use std::sync::Arc;

#[non_exhaustive]
pub(crate) struct EirDesignComposer;

impl EirDesignComposer {
    pub(crate) fn compose(raw: Arc<EirRawDesign>, facts: Arc<EirDesignFacts>) -> EirDesign {
        EirDesign::from_parts(raw, facts)
    }
}
