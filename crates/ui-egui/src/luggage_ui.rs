//! Dedicated luggage and mold tools. Geometry checks run off the desktop UI thread.
use crate::{SolveApp, ui_kit};
use serde_json::{Value, json};
use solidcraft_ui_text::{tr, tr_label};

pub(crate) struct Workbench {
    open: bool,
    tooling: bool,
    inspection: bool,
    shrinkage: Option<ShrinkageForm>,
    limits: [f64; 3],
    sum: f64,
    values: [f64; 9],
    result: Option<Value>,
    error: Option<String>,
    job: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
    revision: Option<Value>,
    progress: Option<std::sync::Arc<solidcraft_engine::doc::progress::Progress>>,
}
impl Default for Workbench {
    fn default() -> Self {
        Self {
            open: false,
            tooling: false,
            inspection: false,
            shrinkage: None,
            limits: [400.0, 550.0, 230.0],
            sum: 1180.0,
            values: [4676.0, 0.0, 2.0, 40.0, 1.2, 0.6, 4.0, 204794.0, 369478.0],
            result: None,
            error: None,
            job: None,
            revision: None,
            progress: None,
        }
    }
}
impl Drop for Workbench {
    fn drop(&mut self) {
        if let Some(p) = &self.progress {
            p.cancel();
        }
    }
}
impl Workbench {
    pub(crate) fn is_open(&self, tooling: bool) -> bool {
        self.open && !self.inspection && self.tooling == tooling
    }
}
pub(crate) fn open(app: &mut SolveApp, tooling: bool) {
    app.luggage.shrinkage = None;
    app.luggage.open = true;
    if app.luggage.tooling != tooling || app.luggage.inspection {
        app.luggage.result = None;
        app.luggage.error = None;
        if let Some(p) = app.luggage.progress.take() {
            p.cancel();
        }
        app.luggage.job = None;
        app.luggage.revision = None;
    }
    app.luggage.inspection = false;
    app.luggage.tooling = tooling;
    app.ui.tab = if tooling { "MOLD" } else { "LUGGAGE" }.into();
}

pub(crate) fn open_inspection(app: &mut SolveApp) {
    let from_mold = app.ui.tab == "MOLD";
    // Inspection and envelope share the read-only worker, but never its result.
    app.luggage.inspection = true;
    open(app, false);
    app.luggage.inspection = true;
    if from_mold {
        app.ui.tab = "MOLD".into();
    }
}

pub(crate) fn inspection_is_open(app: &SolveApp) -> bool {
    app.luggage.open && app.luggage.inspection
}

struct ShrinkageForm {
    body: String,
    percent: [f64; 3],
    origin: [f64; 3],
    error: Option<String>,
}

pub(crate) fn shrinkage_is_open(app: &SolveApp) -> bool {
    app.luggage.shrinkage.is_some()
}

pub(crate) fn open_shrinkage(app: &mut SolveApp) {
    open(app, true);
    app.luggage.open = false;
    let bodies = mold_bodies(app);
    let selected: std::collections::BTreeSet<_> = app
        .session
        .selection
        .iter()
        .filter_map(|pick| {
            use solidcraft_engine::Sel;
            let name = match pick {
                Sel::Body { name } => name,
                Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => body,
                _ => return None,
            };
            Some(name.clone())
        })
        .collect();
    let body = match selected.len() {
        0 => bodies.into_iter().next().unwrap_or_default(),
        1 => selected.into_iter().next().filter(|name| bodies.contains(name)).unwrap_or_default(),
        _ => String::new(),
    };
    app.luggage.shrinkage = Some(ShrinkageForm { body, percent: [0.0; 3], origin: [0.0; 3], error: None });
}

// Use the viewport's current world placement cache, never definition-local body names.
// If placement is still computing, leave the chooser empty until the next frame.
fn mold_bodies(app: &SolveApp) -> Vec<String> {
    app.session
        .cached_world_state()
        .map(|state| state.bodies.iter().filter(|b| !b.body.is_mesh()).map(|b| b.name.clone()).collect())
        .unwrap_or_default()
}

fn shrinkage_ready(app: &SolveApp, form: &ShrinkageForm) -> bool {
    app.session.active_component == 0
        && mold_bodies(app).contains(&form.body)
        && form.percent.iter().all(|v| v.is_finite() && (0.0..20.0).contains(v))
        && form.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9)
}

fn submit_shrinkage(app: &mut SolveApp, form: &ShrinkageForm) -> Result<Value, String> {
    if !shrinkage_ready(app, form) {
        return Err(tr("luggage.mold_input_hint").into());
    }
    app.run_heavy("luggage.mold_shrinkage", json!({"body":form.body,"shrinkage_percent":form.percent,"origin_mm":form.origin}))
}

fn show_shrinkage(app: &mut SolveApp, ctx: &egui::Context) {
    let Some(mut form) = app.luggage.shrinkage.take() else { return };
    let bodies = mold_bodies(app);
    let mut submit = false;
    let (closed, action) = ui_kit::Window::new(egui::Id::unique("luggage_mold_shrinkage"), "scale", tr("cmd.luggage.mold_shrinkage"))
        .fields()
        .actions("", tr("ui.cancel"), None)
        .show(ctx, true, |ui| {
            ui.set_min_width(420.0);
            ui.add(egui::Label::new(tr("luggage.mold_copy_hint")).wrap());
            // Keep a missing selection explicit if its body was deleted while the form was open.
            let mut options = vec![tr("luggage.mold_select_body")];
            options.extend(bodies.iter().map(String::as_str));
            let mut selected = bodies.iter().position(|b| *b == form.body).map_or(0, |i| i + 1);
            ui_kit::choice_row(ui, tr("luggage.mold_body"), "mold_source_body", &mut selected, &options);
            form.body = selected.checked_sub(1).and_then(|i| bodies.get(i)).cloned().unwrap_or_default();
            for (label, value) in ["luggage.mold_shrink_x", "luggage.mold_shrink_y", "luggage.mold_shrink_z"].into_iter().zip(form.percent.iter_mut())
            {
                number(ui, label, value);
            }
            for (label, value) in ["luggage.mold_origin_x", "luggage.mold_origin_y", "luggage.mold_origin_z"].into_iter().zip(form.origin.iter_mut())
            {
                number(ui, label, value);
            }
            ui.add(egui::Label::new(tr("luggage.mold_input_hint")).wrap());
            let response = ui_kit::button(ui, tr("luggage.mold_create"), ui_kit::Button::Enabled(shrinkage_ready(app, &form)));
            crate::scenario::publish_target("luggage:mold-create", response.rect);
            submit = response.clicked();
            if let Some(error) = &form.error {
                ui.add(egui::Label::new(error).wrap());
            }
        });
    if closed || action == ui_kit::Action::Cancel {
        return;
    }
    if submit {
        match submit_shrinkage(app, &form) {
            Ok(_) => return, // Deferred completion and errors use the standard operations UI.
            Err(error) => form.error = Some(error),
        }
    }
    app.luggage.shrinkage = Some(form);
}
fn inspection_status(status: &str) -> &str {
    tr(match status {
        "passed" => "luggage.inspection_passed",
        "failed" => "luggage.inspection_failed",
        "not_supported" => "luggage.inspection_unsupported",
        _ => "luggage.inspection_unchecked",
    })
}

fn show_inspection_result(ui: &mut egui::Ui, result: &Value) {
    ui.label(inspection_status(if result["passed"].as_bool() == Some(true) { "passed" } else { "failed" }));
    ui.label(format!("{}: {} · {}: {}", tr("Bodies"), result["body_count"], tr("luggage.inspection_open_count"), result["open_body_count"]));
    if let Some(bodies) = result["bodies"].as_array() {
        for (index, body) in bodies.iter().enumerate() {
            let name = body["name"].as_str().unwrap_or_default();
            ui_kit::section(ui, format!("luggage_body_inspection_{index}"), name, false, |ui| {
                ui.label(tr(if body["mesh"].as_bool() == Some(true) { "Mesh" } else { "luggage.inspection_brep" }));
                ui.label(tr(if body["closed"].as_bool() == Some(true) { "luggage.inspection_closed" } else { "luggage.inspection_open" }));
                for (key, label) in [("faces", "Faces"), ("edges", "Edges"), ("vertices", "Vertices")] {
                    if let Some(count) = body["kernel"][key].as_u64() {
                        ui.label(format!("{}: {count}", tr(label)));
                    }
                }
                for (key, label) in [
                    ("record_check", "luggage.inspection_record"),
                    ("native_check", "luggage.inspection_native"),
                    ("measurement", "luggage.inspection_measurement"),
                ] {
                    ui.label(format!("{}: {}", tr(label), inspection_status(body[key]["status"].as_str().unwrap_or_default())));
                    if let Some(messages) = body[key]["messages"].as_array() {
                        for message in messages.iter().filter_map(Value::as_str) {
                            ui.add(egui::Label::new(message).wrap());
                        }
                    }
                }
                let volume =
                    body["volume_mm3"].as_f64().map(|v| format!("{v:.3} mm³")).unwrap_or_else(|| tr("luggage.inspection_unavailable").into());
                ui.label(format!("{}: {volume}", tr("luggage.inspection_volume")));
                if let (Some(min), Some(max)) = (body["bounds_mm"]["min"].as_array(), body["bounds_mm"]["max"].as_array()) {
                    let point = |v: &[Value]| v.iter().filter_map(Value::as_f64).map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(", ");
                    ui.add(egui::Label::new(format!("{}: ({}) → ({}) mm", tr("luggage.inspection_bounds"), point(min), point(max))).wrap());
                }
                for key in ["notes", "errors"] {
                    if let Some(messages) = body[key].as_array() {
                        for message in messages.iter().filter_map(Value::as_str) {
                            ui.add(egui::Label::new(message).wrap());
                        }
                    }
                }
            });
        }
    }
    ui_kit::section(ui, "luggage_inspection_diagnostics", tr("luggage.inspection_diagnostics"), false, |ui| {
        if let Some(features) = result["feature_diagnostics"].as_array() {
            for feature in features {
                ui.separator();
                ui.label(format!("{} (#{} )", feature["name"].as_str().unwrap_or_default(), feature["feature"]));
                for (key, label) in [("error", "luggage.inspection_error"), ("warning", "luggage.inspection_warning")] {
                    if let Some(message) = feature[key].as_str() {
                        ui.add(egui::Label::new(format!("{}: {message}", tr(label))).wrap());
                    }
                }
                if feature["stale"].as_bool() == Some(true) {
                    ui.label(tr("luggage.inspection_stale"));
                }
                let report = &feature["import_report"];
                if report.is_object() {
                    ui.label(format!(
                        "{} · {}: {} · {}: {}",
                        tr("luggage.inspection_import"),
                        tr("Parts"),
                        report["parts"],
                        tr("luggage.inspection_solids"),
                        report["solids"]
                    ));
                    if let Some(issues) = report["issues"].as_array() {
                        for issue in issues {
                            let part = issue["part"].as_str().unwrap_or_default();
                            let detail = issue["detail"].as_str().unwrap_or_default();
                            ui.add(egui::Label::new(format!("{part}: {detail}")).wrap());
                        }
                    }
                }
            }
        }
        if let Some(parameters) = result["parameter_errors"].as_object() {
            for (name, error) in parameters {
                if let Some(message) = error.as_str() {
                    ui.add(egui::Label::new(format!("{name}: {message}")).wrap());
                }
            }
        }
        ui.separator();
        ui.label(tr("luggage.inspection_unchecked"));
        if let Some(items) = result["unchecked"].as_array() {
            for message in items.iter().filter_map(Value::as_str) {
                ui.add(egui::Label::new(format!("• {message}")).wrap());
            }
        }
    });
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) -> bool {
    let mut changed = false;
    ui.columns(2, |columns| {
        let mut label_id = None;
        if let Some(ui) = columns.first_mut() {
            label_id = Some(ui.add(egui::Label::new(tr(label)).wrap()).id);
        }
        if let Some(ui) = columns.get_mut(1) {
            let response =
                ui_kit::named_control(ui, tr(label), |ui| ui_kit::number_field_preserving(ui, value, f64::NEG_INFINITY..=f64::INFINITY, 0.1, ""));
            let response = if let Some(id) = label_id { response.labelled_by(id) } else { response };
            changed = response.changed();
        }
    });
    changed
}
fn format_support(ui: &mut egui::Ui, format: &str, action: &str, mut supported: bool) {
    let label = format!("{}: {}", tr_label(format), tr(action));
    ui.add_enabled_ui(false, |ui| {
        let size = solidcraft_theme::Tokens::get().space_14;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        ui_kit::checkbox_at(ui, rect, &label, &mut supported);
    });
}
pub(crate) fn show(app: &mut SolveApp, ctx: &egui::Context) {
    show_shrinkage(app, ctx);
    if !app.luggage.open {
        return;
    }
    let mut state = std::mem::take(&mut app.luggage);
    if let Some(rx) = &state.job {
        match rx.try_recv() {
            Ok(result) => {
                state.job = None;
                state.progress = None;
                match result {
                    Ok(r) => state.result = Some(r),
                    Err(e) => state.error = Some(e),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.job = None;
                state.progress = None;
                state.error = Some(tr("luggage.worker_failed").into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
    let revision = app.session.execute("document.revision", &json!({})).ok();
    if !state.tooling && state.revision.is_some() && state.revision != revision {
        if let Some(p) = state.progress.take() {
            p.cancel();
        }
        state.result = None;
        state.job = None;
        state.revision = None;
        state.error = Some(tr("luggage.changed").into());
    }
    let mut calculate = false;
    let mut changed = false;
    let title = tr(if state.inspection {
        "cmd.luggage.inspect_bodies"
    } else if state.tooling {
        "cmd.luggage.estimate_tooling"
    } else {
        "cmd.luggage.workspace"
    });
    let (closed, action) = ui_kit::Window::new(egui::Id::unique("luggage_workbench"), "inspect", title)
        .fields()
        .actions("", tr("ui.close"), None)
        .show(ctx, true, |ui| {
            ui.set_min_width(420.0);
            if state.inspection {
                ui.add(egui::Label::new(tr("luggage.inspection_hint")).wrap());
            } else if state.tooling {
                ui.add(egui::Label::new(tr("luggage.tooling_hint")).wrap());
                for (label, value) in [
                    "luggage.projected_area",
                    "luggage.runner_area",
                    "luggage.cavities",
                    "luggage.pressure",
                    "luggage.safety",
                    "luggage.shrinkage",
                    "luggage.sheet",
                    "luggage.opening",
                    "luggage.formed",
                ]
                .into_iter()
                .zip(state.values.iter_mut())
                {
                    changed |= number(ui, label, value);
                }
            } else {
                ui.add(egui::Label::new(tr("luggage.envelope_hint")).wrap());
                for (label, value) in ["luggage.limit_x", "luggage.limit_y", "luggage.limit_z"].into_iter().zip(state.limits.iter_mut()) {
                    changed |= number(ui, label, value);
                }
                changed |= number(ui, "luggage.limit_sum", &mut state.sum);
                ui.add(egui::Label::new(tr("luggage.envelope_note")).wrap());
            }
            let button = ui_kit::button(ui, tr("luggage.calculate"), ui_kit::Button::Enabled(state.job.is_none()));
            crate::scenario::publish_target("luggage:calculate", button.rect);
            calculate = button.clicked();
            if state.job.is_some() {
                ui.spinner();
            }
            if let Some(error) = &state.error {
                ui.add(egui::Label::new(error).wrap());
            }
            if !changed && let Some(result) = &state.result {
                ui.separator();
                if state.inspection {
                    show_inspection_result(ui, result);
                } else if state.tooling {
                    for (label, key, unit) in [
                        ("luggage.clamp", "clamp_kn", "kN"),
                        ("luggage.clamp", "clamp_tf", "tf"),
                        ("luggage.scale", "mold_scale", ""),
                        ("luggage.average", "estimated_average_thickness_mm", "mm"),
                    ] {
                        if let Some(v) = result.get(key).and_then(Value::as_f64) {
                            ui.label(format!("{}: {v:.4} {unit}", tr(label)));
                        }
                    }
                    ui.add(egui::Label::new(tr("luggage.tooling_note")).wrap());
                } else {
                    ui.label(tr(if result.get("passed").and_then(Value::as_bool) == Some(true) { "luggage.pass" } else { "luggage.fail" }));
                    for (label, key) in [("luggage.dimensions", "size_mm"), ("luggage.excess", "excess_mm")] {
                        if let Some(values) = result.get(key).and_then(Value::as_array) {
                            let text = values.iter().filter_map(Value::as_f64).map(|v| format!("{v:.2}")).collect::<Vec<_>>().join(" × ");
                            ui.label(format!("{}: {text} mm", tr(label)));
                        }
                    }
                    ui.label(format!("{}: {} / {}", tr("luggage.bodies"), result["body_count"], result["mesh_body_count"]));
                    if let (Some(sum), Some(limit)) = (result["sum_mm"].as_f64(), result["max_sum_mm"].as_f64()) {
                        ui.label(format!("{}: {sum:.2} / {limit:.2} mm", tr("luggage.sum")));
                    }
                }
                if ui_kit::button(ui, tr("luggage.copy"), ui_kit::Button::Normal).clicked()
                    && let Ok(text) = serde_json::to_string_pretty(result)
                {
                    ctx.copy_text(text);
                }
            }
            if !state.tooling && !state.inspection {
                ui.separator();
                ui_kit::section(ui, "luggage_formats_section", tr("luggage.formats"), false, |ui| {
                    ui.add(egui::Label::new(tr("luggage.format_note")).wrap());
                    let wrap_width = ui.available_width() * 0.5;
                    egui::Grid::new("luggage_formats").striped(true).max_col_width(wrap_width).show(ui, |ui| {
                        ui.add(egui::Label::new(tr("luggage.format")).wrap());
                        ui.add(egui::Label::new(tr("luggage.read")).wrap());
                        ui.add(egui::Label::new(tr("luggage.write")).wrap());
                        ui.end_row();
                        for f in solidcraft_engine::io::formats::FORMATS {
                            ui.add(egui::Label::new(tr_label(f.label)).wrap());
                            format_support(ui, f.label, "luggage.read", f.readable(true));
                            format_support(ui, f.label, "luggage.write", f.export.is_some() || f.id == "design");
                            ui.end_row();
                        }
                    });
                });
            }
        });
    if changed {
        if let Some(p) = state.progress.take() {
            p.cancel();
        }
        state.job = None;
        state.revision = None;
        state.result = None;
        state.error = None;
    }
    if calculate {
        state.result = None;
        state.error = None;
        if state.tooling {
            let mut params = serde_json::Map::new();
            for (key, value) in [
                "projected_area_mm2",
                "runner_area_mm2",
                "cavities",
                "cavity_pressure_mpa",
                "safety_factor",
                "shrinkage_percent",
                "sheet_thickness_mm",
                "opening_area_mm2",
                "formed_area_mm2",
            ]
            .into_iter()
            .zip(state.values)
            {
                params.insert(
                    key.into(),
                    if key == "cavities" && value.is_finite() && value.fract() == 0.0 && (1.0..=1024.0).contains(&value) {
                        json!(value as u32)
                    } else {
                        json!(value)
                    },
                );
            }
            match app.session.execute("luggage.estimate_tooling", &Value::Object(params)) {
                Ok(r) => state.result = Some(r),
                Err(e) => state.error = Some(e.to_string()),
            }
        } else {
            let command = if state.inspection { "luggage.inspect_bodies" } else { "luggage.check_envelope" };
            let params = if state.inspection { json!({}) } else { json!({"limits_mm":state.limits,"max_sum_mm":state.sum}) };
            state.revision = revision;
            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut copy = app.session.scratch();
                let progress = std::sync::Arc::new(solidcraft_engine::doc::progress::Progress::default());
                let worker_progress = progress.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let result = solidcraft_engine::kernel::kernel_thread("luggage-envelope").spawn(move || {
                    let result = solidcraft_engine::doc::progress::watch(worker_progress, || copy.execute(command, &params));
                    let _ = tx.send(result.map_err(|e| e.to_string()));
                });
                match result {
                    Ok(_) => {
                        state.job = Some(rx);
                        state.progress = Some(progress);
                        ctx.request_repaint();
                    }
                    Err(e) => state.error = Some(e.to_string()),
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                match app.session.execute(command, &params) {
                    Ok(r) => state.result = Some(r),
                    Err(e) => state.error = Some(e.to_string()),
                }
            }
        }
    }
    if closed || action == ui_kit::Action::Cancel {
        state.open = false;
        if let Some(p) = state.progress.take() {
            p.cancel();
        }
        state.job = None;
    }
    app.luggage = state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mold_form_uses_the_selected_product_and_does_not_guess_between_multiple_bodies() {
        let mut h = crate::scenario::Harness::new();
        for name in ["First product", "Selected product"] {
            h.app.run("solid.box", json!({"length":10,"width":20,"height":30,"body_name":name,"operation":"new"})).unwrap();
        }
        h.app.session.selection = vec![solidcraft_engine::Sel::Body { name: "Selected product".into() }];
        open_shrinkage(&mut h.app);
        assert_eq!(h.app.luggage.shrinkage.as_ref().unwrap().body, "Selected product");
        h.app.session.selection.push(solidcraft_engine::Sel::Body { name: "First product".into() });
        open_shrinkage(&mut h.app);
        assert!(h.app.luggage.shrinkage.as_ref().unwrap().body.is_empty());
        h.app.session.selection = vec![solidcraft_engine::Sel::Body { name: "Unavailable product".into() }];
        open_shrinkage(&mut h.app);
        assert!(h.app.luggage.shrinkage.as_ref().unwrap().body.is_empty());
    }

    #[test]
    fn inspection_tool_switch_and_revision_change_discard_stale_worker_results() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.start("luggage.workspace");
        let (old, receiver) = std::sync::mpsc::channel();
        h.app.luggage.job = Some(receiver);
        h.app.start("luggage.inspect_bodies");
        assert!(old.send(Ok(json!({"passed":true}))).is_err());
        assert!(inspection_is_open(&h.app));
        assert!(!h.app.luggage.is_open(false));
        let revision = h.app.session.revision;
        h.app.luggage.revision = h.app.session.execute("document.revision", &json!({})).ok();
        let (reply, receiver) = std::sync::mpsc::channel();
        h.app.luggage.job = Some(receiver);
        reply.send(Ok(json!({"passed":true,"bodies":[]}))).unwrap();
        h.frames(1);
        assert_eq!(h.app.session.revision, revision, "reading inspection results cannot edit the design");
        assert!(h.app.luggage.result.is_some());
        h.app.run("solid.box", json!({"length":10,"width":20,"height":30})).unwrap();
        h.frames(1);
        assert!(h.app.luggage.result.is_none());
        assert!(h.app.luggage.error.is_some());
        h.app.start("luggage.estimate_tooling");
        assert!(!inspection_is_open(&h.app));
        assert!(h.app.luggage.is_open(true));
    }

    #[test]
    fn mold_form_queues_exact_world_inputs_without_changing_the_product() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length":40,"width":30,"height":10})).unwrap();
        let revision = h.app.session.revision;
        h.app.start("luggage.mold_shrinkage");
        assert_eq!(h.app.ui.tab, "MOLD");
        assert!(!h.app.luggage.is_open(true));
        let mut form = h.app.luggage.shrinkage.take().unwrap();
        form.percent = [0.5, 1.0, 1.5];
        form.origin = [-12.0, 3.0, 4.0];
        let (held, _release) = crate::deferred::Deferred::never_done("solid.scale", json!({}), revision);
        h.app.deferred = Some(held);
        assert_eq!(submit_shrinkage(&mut h.app, &form).unwrap(), Value::Null);
        let (id, params, _) = h.app.deferred_queue.last().unwrap();
        assert_eq!(id, "luggage.mold_shrinkage");
        assert_eq!(*params, json!({"body":form.body,"shrinkage_percent":[0.5,1.0,1.5],"origin_mm":[-12.0,3.0,4.0]}));
        assert_eq!(h.app.session.revision, revision);
        assert_eq!(h.app.session.model.state().bodies.len(), 1);
    }

    #[test]
    fn mold_form_refuses_missing_body_non_root_and_invalid_values_before_queuing() {
        let mut h = crate::scenario::Harness::new();
        h.app.start("luggage.mold_shrinkage");
        let mut form = h.app.luggage.shrinkage.take().unwrap();
        assert!(!shrinkage_ready(&h.app, &form));
        h.app.run("solid.box", json!({"length":40,"width":30,"height":10})).unwrap();
        form.body = h.app.session.model.state().bodies[0].name.clone();
        assert!(shrinkage_ready(&h.app, &form));
        for invalid in [-1.0, 20.0, f64::NAN, f64::INFINITY] {
            form.percent[1] = invalid;
            assert!(submit_shrinkage(&mut h.app, &form).is_err());
        }
        form.percent = [0.6; 3];
        form.origin[0] = 1e9;
        assert!(submit_shrinkage(&mut h.app, &form).is_err());
        form.origin = [0.0; 3];
        h.app.session.active_component = 123;
        assert!(submit_shrinkage(&mut h.app, &form).is_err());
        h.app.session.active_component = 0;
        form.body = "Deleted product".into();
        assert!(submit_shrinkage(&mut h.app, &form).is_err());
        assert!(h.app.deferred.is_none());
        assert!(h.app.deferred_queue.is_empty());
        assert_eq!(h.app.session.model.state().bodies.len(), 1);
    }

    #[test]
    fn deferred_history_keeps_committed_envelope_until_commit_then_discards_a_late_reply() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length":60,"width":20,"height":10})).unwrap();
        h.app.run("solid.box", json!({"corner":[5,5,10],"length":10,"width":10,"height":5,"operation":"join"})).unwrap();
        h.app.start("luggage.workspace");
        let revision = h.app.session.execute("document.revision", &json!({})).unwrap();
        let params = json!({"limits_mm":[100,100,100],"max_sum_mm":300});
        let (history, release_history) = crate::deferred::Deferred::never_done("timeline.roll_to", json!({"position":1}), h.app.session.revision);
        h.app.deferred = Some(history);

        // The history worker has not committed: a scratch inspection sees the complete
        // block and boss, under the same revision as the live design.
        let committed = h.app.session.scratch().execute("luggage.check_envelope", &params).unwrap();
        assert_eq!(committed["size_mm"], json!([60.0, 20.0, 15.0]));
        let (reply, receiver) = std::sync::mpsc::channel();
        h.app.luggage.revision = Some(revision.clone());
        h.app.luggage.job = Some(receiver);
        reply.send(Ok(committed.clone())).unwrap();
        h.frames(1);
        assert!(h.app.deferred.is_some(), "the held history worker still waits");
        assert_eq!(h.app.session.doc.marker, None);
        assert_eq!(h.app.session.execute("document.revision", &json!({})).unwrap(), revision);
        assert_eq!(h.app.luggage.result.as_ref(), Some(&committed));
        assert!(h.app.luggage.error.is_none());

        // Both completions are ready before the next frame. The frame first commits the
        // history command, then receives the old inspection; revision checking must
        // reject that reply even though it arrived successfully in this same frame.
        let (late_reply, receiver) = std::sync::mpsc::channel();
        h.app.luggage.result = None;
        h.app.luggage.job = Some(receiver);
        late_reply.send(Ok(committed)).unwrap();
        release_history.send(None).unwrap();
        h.frames(1);
        assert!(h.app.deferred.is_none());
        assert_eq!(h.app.session.doc.marker, Some(1));
        assert_ne!(h.app.session.execute("document.revision", &json!({})).unwrap(), revision);
        assert!(h.app.luggage.result.is_none(), "the old boss envelope cannot describe the committed rollback");
        assert!(h.app.luggage.job.is_none());
        assert!(h.app.luggage.revision.is_none());
        assert!(h.app.luggage.error.is_some());
        let rolled = h.app.session.execute("luggage.check_envelope", &params).unwrap();
        assert_eq!(rolled["size_mm"], json!([60.0, 20.0, 10.0]));
    }

    #[test]
    fn changing_design_or_tool_discards_pending_envelope_results() {
        let mut h = crate::scenario::Harness::new();
        h.app.start("luggage.workspace");
        h.app.luggage.revision = h.app.session.execute("document.revision", &json!({})).ok();
        let (tx, rx) = std::sync::mpsc::channel();
        h.app.luggage.job = Some(rx);
        let progress = std::sync::Arc::new(solidcraft_engine::doc::progress::Progress::default());
        h.app.luggage.progress = Some(progress);
        h.app.run("solid.box", json!({"length":80,"width":50,"height":6})).unwrap();
        // A completed result can arrive in the same frame as a model revision change.
        tx.send(Ok(json!({"passed":true,"body_count":999}))).unwrap();
        h.frames(2);
        assert!(h.app.luggage.result.is_none());
        assert!(h.app.luggage.job.is_none());
        assert!(h.app.luggage.error.is_some());

        let (tx, rx) = std::sync::mpsc::channel();
        h.app.luggage.job = Some(rx);
        h.app.start("luggage.estimate_tooling");
        assert!(tx.send(Ok(json!({"passed":true}))).is_err(), "the old tool's result has no receiver");
        assert!(h.app.luggage.result.is_none());
        assert!(h.app.luggage.error.is_none());
        assert!(h.app.luggage.is_open(true));
    }
}
