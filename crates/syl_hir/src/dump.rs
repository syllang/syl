use crate::HirDesign;

impl HirDesign {
    pub fn debug_dump(&self) -> String {
        let defs = self
            .defs
            .iter()
            .map(|def| format!("{} {}", <&'static str>::from(def.kind), def.name))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "hir defs={} locals={} exprs={} [{}]",
            self.defs.len(),
            self.locals.len(),
            self.exprs.len(),
            defs,
        )
    }
}
