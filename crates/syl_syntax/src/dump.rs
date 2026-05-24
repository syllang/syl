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
        Item::Error(_) => "error".to_string(),
        Item::Package(item) => format!("package {}", item.path.join("::")),
        Item::Use(item) => format!("use {}", item.path.join("::")),
        Item::Const(item) => format!("const {}", item.name),
        Item::Fn(item) => format!("fn {}", item.name),
        Item::Enum(item) => format!("enum {}", item.name),
        Item::Bundle(item) => format!("bundle {}", item.name),
        Item::Interface(item) => format!("interface {}", item.name),
        Item::Map(item) => format!("map {}", item.name),
        Item::Cell(item) => format!("cell {}", item.name),
        Item::Module(item) => format!("module {}", item.name),
        Item::ExternModule(item) => format!("extern module {}", item.name),
    }
}
