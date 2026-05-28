use super::{TirType, TypeId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TirTypeTable {
    types: Vec<TirType>,
}

impl TirTypeTable {
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    pub fn intern(&mut self, ty: TirType) -> TypeId {
        if let Some(index) = self.types.iter().position(|known| known == &ty) {
            return TypeId::new(index);
        }
        let id = TypeId::new(self.types.len());
        self.types.push(ty);
        id
    }

    pub fn get(&self, id: TypeId) -> Option<&TirType> {
        self.types.get(id.get())
    }

    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &TirType)> {
        self.types
            .iter()
            .enumerate()
            .map(|(index, ty)| (TypeId::new(index), ty))
    }
}
