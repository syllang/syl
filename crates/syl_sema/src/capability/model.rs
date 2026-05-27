use super::place::{Place, PlaceKey};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::LocalId;

#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum EndpointSide {
    Local,
    LocalSignal,
    Returned,
}

#[derive(Clone)]
#[non_exhaustive]
pub(super) struct CapabilityScope {
    bindings: BTreeMap<LocalId, FieldCaps>,
    local_drives: BTreeSet<PlaceKey>,
}

impl CapabilityScope {
    pub(super) fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
            local_drives: BTreeSet::new(),
        }
    }

    pub(super) fn insert(&mut self, id: LocalId, caps: FieldCaps) {
        self.bindings.insert(id, caps);
    }

    pub(super) fn can_read(&self, place: &Place) -> bool {
        if self.local_drives.contains(&place.key())
            && self
                .bindings
                .get(&place.root_id())
                .is_some_and(FieldCaps::can_read_local_drive)
        {
            return true;
        }
        self.bindings
            .get(&place.root_id())
            .map(|caps| caps.can_read(place.field()))
            .unwrap_or(false)
    }

    pub(super) fn contains(&self, place: &Place) -> bool {
        self.bindings.contains_key(&place.root_id())
    }

    pub(super) fn can_write(&self, place: &Place) -> bool {
        self.bindings
            .get(&place.root_id())
            .map(|caps| caps.can_write(place.field()))
            .unwrap_or(false)
    }

    pub(super) fn mark_local_drive(&mut self, place: &Place) {
        self.local_drives.insert(place.key());
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(super) struct FieldCaps {
    whole_read: bool,
    whole_write: bool,
    read_local_drives: bool,
    pub(super) readable: BTreeSet<String>,
    pub(super) drivable: BTreeSet<String>,
}

impl FieldCaps {
    pub(super) fn empty() -> Self {
        Self {
            whole_read: false,
            whole_write: false,
            read_local_drives: false,
            readable: BTreeSet::new(),
            drivable: BTreeSet::new(),
        }
    }

    pub(super) fn whole() -> Self {
        Self {
            whole_read: true,
            whole_write: true,
            read_local_drives: false,
            readable: BTreeSet::new(),
            drivable: BTreeSet::new(),
        }
    }

    pub(super) fn read_only() -> Self {
        Self {
            whole_read: true,
            whole_write: false,
            read_local_drives: false,
            readable: BTreeSet::new(),
            drivable: BTreeSet::new(),
        }
    }

    pub(super) fn write_only() -> Self {
        Self {
            whole_read: false,
            whole_write: true,
            read_local_drives: false,
            readable: BTreeSet::new(),
            drivable: BTreeSet::new(),
        }
    }

    pub(super) fn read_write() -> Self {
        Self {
            whole_read: true,
            whole_write: true,
            read_local_drives: false,
            readable: BTreeSet::new(),
            drivable: BTreeSet::new(),
        }
    }

    pub(super) fn with_local_drive_readback(mut self) -> Self {
        self.read_local_drives = true;
        self
    }

    fn can_read(&self, field: Option<&str>) -> bool {
        self.whole_read || field.is_some_and(|field| self.readable.contains(field))
    }

    fn can_write(&self, field: Option<&str>) -> bool {
        self.whole_write || field.is_some_and(|field| self.drivable.contains(field))
    }

    fn can_read_local_drive(&self) -> bool {
        self.read_local_drives
    }

    pub(super) fn readable_fields(&self) -> impl Iterator<Item = &str> {
        self.readable.iter().map(String::as_str)
    }

    pub(super) fn drivable_fields(&self) -> impl Iterator<Item = &str> {
        self.drivable.iter().map(String::as_str)
    }
}
