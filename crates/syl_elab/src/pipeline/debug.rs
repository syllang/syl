use super::{
    DrcStage, DriverFactsStage, EirBuildStage, EirFactsStage, EirStage, EirValidationStage,
};
use crate::{
    driver::{CreateKind, DriveEffect},
    eir::{EirDriveKind, EirItem, EirModule, EirObjectKind, EirSignalActivity},
    eir::{EirGuard, EirGuardFrame, EirOrigin, EirPlace},
};

pub(super) fn eir_build_stage_dump(stage: &EirBuildStage) -> String {
    let mut lines = vec![format!(
        "eir_build modules={}",
        stage.design.modules().len()
    )];
    for module in stage.design.modules() {
        lines.push(format!("module {}", module.name()));
        dump_module(module, &mut lines);
    }
    lines.join("\n")
}

pub(super) fn eir_validation_stage_dump(stage: &EirValidationStage) -> String {
    format!("eir_validation modules={} status=ok", stage.module_count())
}

pub(super) fn eir_facts_stage_dump(stage: &EirFactsStage) -> String {
    let mut lines = vec![format!(
        "eir_facts objects={} drives={} reads={}",
        stage.facts.objects().len(),
        stage.facts.drives().len(),
        stage.facts.reads().len(),
    )];
    for object in stage.facts.objects() {
        lines.push(format!(
            "create {} {}.{} width={} activity={} origin={}",
            object_kind_name(object.kind()),
            object.module(),
            object.name(),
            object.width_bound().source(),
            activity_name(object.activity()),
            origin_text(object.origin()),
        ));
    }
    for drive in stage.facts.drives() {
        lines.push(format!(
            "drive {} {} kind={} guard={} origin={}",
            drive.module(),
            place_text(drive.target_place()),
            eir_drive_kind_name(drive.kind()),
            guard_text(drive.guard()),
            origin_text(drive.origin()),
        ));
    }
    for read in stage.facts.reads() {
        lines.push(format!(
            "read {} {} guard={} origin={}",
            read.module(),
            place_text(read.source_place()),
            guard_text(read.guard()),
            origin_text(read.origin()),
        ));
    }
    lines.join("\n")
}

pub(super) fn eir_stage_dump(stage: &EirStage) -> String {
    let mut lines = vec![format!(
        "eir modules={} objects={} drives={} reads={}",
        stage.design.modules().len(),
        stage.design.objects().len(),
        stage.design.drives().len(),
        stage.design.reads().len(),
    )];
    for module in stage.design.modules() {
        lines.push(format!("module {}", module.name()));
        dump_module(module, &mut lines);
    }
    for object in stage.design.objects() {
        lines.push(format!(
            "create {} {}.{} width={} activity={} origin={}",
            object_kind_name(object.kind()),
            object.module(),
            object.name(),
            object.width_bound().source(),
            activity_name(object.activity()),
            origin_text(object.origin()),
        ));
    }
    for drive in stage.design.drives() {
        lines.push(format!(
            "drive {} {} kind={} guard={} origin={}",
            drive.module(),
            place_text(drive.target_place()),
            eir_drive_kind_name(drive.kind()),
            guard_text(drive.guard()),
            origin_text(drive.origin()),
        ));
    }
    for read in stage.design.reads() {
        lines.push(format!(
            "read {} {} guard={} origin={}",
            read.module(),
            place_text(read.source_place()),
            guard_text(read.guard()),
            origin_text(read.origin()),
        ));
    }
    lines.join("\n")
}

pub(super) fn driver_facts_stage_dump(stage: &DriverFactsStage) -> String {
    let mut lines = vec![format!(
        "driver_facts drives={} reads={} creates={} cells={}",
        stage.facts.drives().len(),
        stage.facts.reads().len(),
        stage.facts.creates().len(),
        stage.facts.summary_cells().len(),
    )];
    for create in stage.facts.creates() {
        lines.push(format!(
            "create {} {}.{} origin={}",
            create_kind_name(create.kind()),
            create.module(),
            create.name(),
            origin_text(create.origin()),
        ));
    }
    for drive in stage.facts.drives() {
        lines.push(format!(
            "drive {} {} effect={} guard={} origin={}",
            drive.module(),
            drive.target_place().display(),
            drive_effect_name(drive.effect()),
            guard_text(drive.guard()),
            origin_text(drive.origin()),
        ));
    }
    for read in stage.facts.reads() {
        lines.push(format!(
            "read {} {} guard={} origin={}",
            read.module(),
            read.source_place().display(),
            guard_text(read.guard()),
            origin_text(read.origin()),
        ));
    }
    for summary in stage.facts.summary_cells() {
        lines.push(format!(
            "cell {} as {} creates={} drives={} reads={} origin={}",
            summary.callable(),
            summary.instance(),
            summary.creates().len(),
            summary.drives().len(),
            summary.reads().len(),
            origin_text(summary.origin()),
        ));
    }
    lines.join("\n")
}

pub(super) fn drc_stage_dump(stage: &DrcStage) -> String {
    format!(
        "drc modules={} drives={} reads={} creates={}",
        stage.module_count(),
        stage.drive_count(),
        stage.read_count(),
        stage.create_count(),
    )
}

fn dump_module(module: &EirModule, lines: &mut Vec<String>) {
    for item in module.items() {
        dump_item(item, 2, lines);
    }
}

fn dump_item(item: &EirItem, indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    match item {
        EirItem::Signal { name, origin, .. } => {
            lines.push(format!("{pad}signal {name} origin={}", origin_text(origin)));
        }
        EirItem::Storage { name, origin, .. } => {
            lines.push(format!(
                "{pad}storage {name} origin={}",
                origin_text(origin)
            ));
        }
        EirItem::Drive { lhs, origin, .. } => {
            lines.push(format!(
                "{pad}drive {} origin={}",
                place_text(lhs),
                origin_text(origin)
            ));
        }
        EirItem::ClockedStorage { target, origin, .. } => {
            lines.push(format!(
                "{pad}next {} origin={}",
                place_text(target),
                origin_text(origin)
            ));
        }
        EirItem::CellExpansion(expansion) => {
            lines.push(format!(
                "{pad}cell inline {} as {}",
                expansion.callable(),
                expansion.instance()
            ));
            for nested in expansion.items() {
                dump_item(nested, indent + 2, lines);
            }
        }
        EirItem::Instance(instance) => {
            lines.push(format!(
                "{pad}instance {} as {} origin={}",
                instance.module(),
                instance.name(),
                origin_text(instance.origin())
            ));
        }
        EirItem::SymbolicStaticIf {
            label,
            then_items,
            else_items,
            origin,
            ..
        } => {
            lines.push(format!(
                "{pad}static if {label} origin={}",
                origin_text(origin)
            ));
            for nested in then_items {
                dump_item(nested, indent + 2, lines);
            }
            for nested in else_items {
                dump_item(nested, indent + 2, lines);
            }
        }
        EirItem::SymbolicStaticFor {
            label,
            index,
            items,
            origin,
            ..
        } => {
            lines.push(format!(
                "{pad}static for {label} index={index} origin={}",
                origin_text(origin)
            ));
            for nested in items {
                dump_item(nested, indent + 2, lines);
            }
        }
        EirItem::StaticParam { name, origin, .. } => {
            lines.push(format!(
                "{pad}static param {name} origin={}",
                origin_text(origin)
            ));
        }
        EirItem::ClockedAssert { origin, .. } => {
            lines.push(format!("{pad}assert origin={}", origin_text(origin)));
        }
        EirItem::InitialError { origin, .. } => {
            lines.push(format!("{pad}initial error origin={}", origin_text(origin)));
        }
    }
}

fn origin_text(origin: &EirOrigin) -> String {
    let span = origin.span();
    let mut text = format!("{}:{}-{}", span.source.get(), span.start, span.end);
    if !origin.expansion_stack().is_empty() {
        let trace = origin
            .expansion_stack()
            .iter()
            .map(|expansion| {
                let span = expansion.span();
                format!(
                    "{}@{}:{}-{}",
                    expansion.instance(),
                    span.source.get(),
                    span.start,
                    span.end
                )
            })
            .collect::<Vec<_>>()
            .join(" -> ");
        text.push_str(&format!(" trace=[{trace}]"));
    }
    text
}

fn place_text(place: &EirPlace) -> String {
    place.to_expr().fact_key()
}

fn guard_text(guard: &EirGuard) -> String {
    if guard.is_root() {
        return "root".to_string();
    }
    guard
        .frames()
        .iter()
        .map(guard_frame_text)
        .collect::<Vec<_>>()
        .join("/")
}

fn guard_frame_text(frame: &EirGuardFrame) -> String {
    match frame {
        EirGuardFrame::IfThen { label } => format!("{}:then", label.display()),
        EirGuardFrame::IfElse { label } => format!("{}:else", label.display()),
        EirGuardFrame::Loop { label, .. } => label.display().to_string(),
    }
}

fn object_kind_name(kind: EirObjectKind) -> &'static str {
    match kind {
        EirObjectKind::Signal => "signal",
        EirObjectKind::Storage => "storage",
    }
}

fn activity_name(activity: EirSignalActivity) -> &'static str {
    match activity {
        EirSignalActivity::Required => "required",
        EirSignalActivity::Optional => "optional",
    }
}

fn eir_drive_kind_name(kind: EirDriveKind) -> &'static str {
    match kind {
        EirDriveKind::Continuous => "continuous",
        EirDriveKind::Next => "next",
    }
}

fn create_kind_name(kind: CreateKind) -> &'static str {
    match kind {
        CreateKind::Signal => "signal",
        CreateKind::Storage => "storage",
    }
}

fn drive_effect_name(effect: &DriveEffect) -> String {
    match effect {
        DriveEffect::Continuous => "continuous".to_string(),
        DriveEffect::Next { storage_target } => {
            format!("next({})", storage_target.display())
        }
    }
}
