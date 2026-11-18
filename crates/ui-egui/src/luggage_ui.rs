//! Dedicated luggage and mold tools. Geometry checks run off the desktop UI thread.
use crate::{SolveApp, ui_kit};
use serde_json::{Value, json};
use solidcraft_ui_text::{tr, tr_label};

pub(crate) struct Workbench {
    open: bool,
    tooling: bool,
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
        self.open && self.tooling == tooling
    }
}
pub(crate) fn open(app: &mut SolveApp, tooling: bool) {
    app.luggage.open = true;
    if app.luggage.tooling != tooling {
        app.luggage.result = None;
        app.luggage.error = None;
        if let Some(p) = app.luggage.progress.take() {
            p.cancel();
        }
        app.luggage.job = None;
        app.luggage.revision = None;
    }
    app.luggage.tooling = tooling;
    app.ui.tab = if tooling { "MOLD" } else { "LUGGAGE" }.into();
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) -> bool {
    let mut changed = false;
    ui.columns(2, |columns| {
        let mut label_id = None;
        if let Some(ui) = columns.first_mut() {
            label_id = Some(ui.add(egui::Label::new(tr(label)).wrap()).id);
        }
        if let Some(ui) = columns.get_mut(1) {
            let response = ui_kit::number_field_preserving(ui, value, f64::NEG_INFINITY..=f64::INFINITY, 0.1, "");
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
    let title = tr(if state.tooling { "cmd.luggage.estimate_tooling" } else { "cmd.luggage.workspace" });
    let (closed, action) = ui_kit::Window::new(egui::Id::unique("luggage_workbench"), "inspect", title)
        .fields()
        .actions("", tr("ui.close"), None)
        .show(ctx, true, |ui| {
            ui.set_min_width(420.0);
            if state.tooling {
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
                if state.tooling {
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
            if !state.tooling {
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
            let params = json!({"limits_mm":state.limits,"max_sum_mm":state.sum});
            state.revision = revision;
            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut copy = app.session.scratch();
                let progress = std::sync::Arc::new(solidcraft_engine::doc::progress::Progress::default());
                let worker_progress = progress.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let result = std::thread::Builder::new().name("luggage-envelope".into()).spawn(move || {
                    let result = solidcraft_engine::doc::progress::watch(worker_progress, || copy.execute("luggage.check_envelope", &params));
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
                match app.session.execute("luggage.check_envelope", &params) {
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
