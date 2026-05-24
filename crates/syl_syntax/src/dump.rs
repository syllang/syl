use crate::{AstFile, Item};

impl AstFile {
    pub fn debug_dump(&self) -> String {
        let items = self
            .items
            .iter()
            .map(debug_item)
            .collect::<Vec<_>>()
            .join(", ");
        format!("ast items={} [{}]", self.items.len(), items)
    }
}

fn debug_item(item: &Item) -> String {
    match item {
        Item::Error(item) => format!("error@{}", debug_span(item.span)),
        Item::Package(item) => {
            format!("package {}@{}", item.path.join("::"), debug_span(item.span))
        }
        Item::Use(item) => format!("use {}@{}", item.path.join("::"), debug_span(item.span)),
        Item::Const(item) => format!("const {}@{}", item.name, debug_span(item.span)),
        Item::Fn(item) => format!("fn {}@{}", item.name, debug_span(item.span)),
        Item::Enum(item) => format!("enum {}@{}", item.name, debug_span(item.span)),
        Item::Bundle(item) => format!("bundle {}@{}", item.name, debug_span(item.span)),
        Item::Interface(item) => format!("interface {}@{}", item.name, debug_span(item.span)),
        Item::Map(item) => format!("map {}@{}", item.name, debug_span(item.span)),
        Item::Cell(item) => format!("cell {}@{}", item.name, debug_span(item.span)),
        Item::Module(item) => format!("module {}@{}", item.name, debug_span(item.span)),
        Item::ExternModule(item) => {
            format!("extern module {}@{}", item.name, debug_span(item.span))
        }
    }
}

fn debug_span(span: syl_span::Span) -> String {
    format!("{}..{}", span.start, span.end)
}
