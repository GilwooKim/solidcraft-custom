//! Read-only world-body checks. Source-file completeness and assembly inference are separate work.
use super::CommandSpec;
use crate::params::bad;
use crate::schema::luggage_inspection::*;
use crate::schema::{TypedContract, output_schema_for};
use crate::{Result, Session};
use serde_json::Value;

const ID: &str = "luggage.inspect_bodies";
const CONTRACT: TypedContract<InspectBodiesParams> = TypedContract::new();
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new(ID, "Inspect Luggage Bodies", crate::checked!("luggage.inspect_bodies", inspect))
        .icon("inspect")
        .noundo()
        .read_only()
        .contract(&CONTRACT)
        .output(output_schema_for::<InspectBodiesResult>)
        .params("bodies?: [world body names], empty means all including hidden bodies. Read-only current-document checks: exact world bounds, B-rep counts, closedness, record/native validity, nullable volume and existing feature/import diagnostics. Does not prove original-file completeness, original units, assembly hierarchy or manufacturing readiness."),
];

fn cancellation() -> Result<()> {
    if solidcraft_doc::progress::cancelled() { Err(bad(ID, "body inspection cancelled")) } else { Ok(()) }
}

fn check(status: CheckStatus, messages: Vec<String>) -> Check {
    Check { status, messages }
}

fn inspect_body(b: &solidcraft_doc::ModelBody, definitions: &solidcraft_doc::ModelState) -> Result<BodyInspection> {
    cancellation()?;
    let mesh = b.body.is_mesh();
    let closed = super::inspect::encloses_volume(&b.body);
    let mut errors = Vec::new();
    let bounds_mm = match b.body.exact_bounds() {
        Ok(bounds) if !bounds.is_empty() && bounds.min.is_finite() && bounds.max.is_finite() => {
            Some(Bounds { min: bounds.min.into(), max: bounds.max.into() })
        }
        Ok(_) => {
            errors.push("the body has empty or non-finite exact bounds".into());
            None
        }
        Err(e) => {
            errors.push(format!("exact bounds: {e}"));
            None
        }
    };
    cancellation()?;
    let (record_check, native_check) = if mesh {
        let message = "B-rep validity is not supported for a mesh body; closedness alone is not full mesh validity";
        (check(CheckStatus::NotSupported, vec![message.into()]), check(CheckStatus::NotSupported, vec![message.into()]))
    } else {
        let result = solidcraft_kernel::validity(&b.body);
        let record = match result.record {
            Ok(()) => check(CheckStatus::Passed, Vec::new()),
            Err(e) => check(CheckStatus::Failed, vec![e]),
        };
        let native = match result.smlib {
            Some(solidcraft_kernel::SmlibCheck::Valid) => check(CheckStatus::Passed, Vec::new()),
            Some(solidcraft_kernel::SmlibCheck::Invalid(e)) => check(CheckStatus::Failed, vec![e]),
            Some(solidcraft_kernel::SmlibCheck::NotSupported(e)) => check(CheckStatus::NotSupported, vec![e]),
            None => check(CheckStatus::NotChecked, vec!["native validity did not run".into()]),
        };
        (record, native)
    };
    cancellation()?;
    let mut notes = Vec::new();
    let (volume_mm3, measurement) = match super::inspect::measure_body(b, Some(definitions)) {
        Ok(m) => {
            notes.extend(m.notes);
            notes.push(format!("measurement method: {}", m.measured_by));
            if !closed {
                notes.push("open body: volume omitted; this is not by itself proof of an import defect".into());
                (None, check(CheckStatus::Passed, Vec::new()))
            } else if m.volume.is_finite() && m.volume > 0.0 {
                (Some(m.volume), check(CheckStatus::Passed, Vec::new()))
            } else {
                (None, check(CheckStatus::Failed, vec!["closed body measurement has no finite positive volume".into()]))
            }
        }
        Err(e) => (None, check(CheckStatus::Failed, vec![e.to_string()])),
    };
    cancellation()?;
    Ok(BodyInspection {
        name: b.name.clone(),
        definition: b.definition_name().to_owned(),
        component: b.component,
        feature: b.feature,
        mesh,
        kernel: (!mesh).then(|| KernelCounts {
            faces: b.body.face_count(),
            edges: b.body.edge_count(),
            vertices: b.body.vertex_count(),
            shells: b.body.shell_count(),
        }),
        closed,
        bounds_mm,
        record_check,
        native_check,
        volume_mm3,
        measurement,
        notes,
        errors,
    })
}

fn inspect(s: &mut Session, params: &Value) -> Result<Value> {
    let input: InspectBodiesParams = serde_json::from_value(params.clone()).map_err(|e| bad(ID, e.to_string()))?;
    if input.bodies.len() > 20_000 || input.bodies.iter().any(|n| n.trim().is_empty() || n.len() > 4096) {
        return Err(bad(ID, "at most 20000 nonempty body names of at most 4096 bytes are accepted"));
    }
    let names: std::collections::BTreeSet<_> = input.bodies.iter().collect();
    if names.len() != input.bodies.len() {
        return Err(bad(ID, "body names must not be repeated"));
    }
    cancellation()?;
    let revision = s.design_revision();
    let state = s.world_state()?;
    let definitions = s.model.state();
    if state.bodies.len() > 20_000 {
        return Err(bad(ID, "at most 20000 placed bodies can be checked"));
    }
    for name in &input.bodies {
        if !state.bodies.iter().any(|b| &b.name == name) {
            return Err(bad(ID, format!("unknown body: {name}")));
        }
    }
    let mut bodies = Vec::new();
    for b in &state.bodies {
        if names.is_empty() || names.contains(&b.name) {
            bodies.push(inspect_body(b, &definitions)?);
        }
    }
    let mut feature_diagnostics = Vec::new();
    let active_features: std::collections::BTreeSet<_> =
        s.doc.features.iter().take(s.doc.marker.unwrap_or(s.doc.features.len())).filter(|f| !f.suppressed).map(|f| f.id).collect();
    for f in s.model.results.iter().filter(|f| active_features.contains(&f.id)) {
        cancellation()?;
        if !f.skipped && (f.error.is_some() || f.warning.is_some() || f.stale || f.import_report.is_some()) {
            feature_diagnostics.push(FeatureDiagnostic {
                feature: f.id,
                name: f.name.clone(),
                error: f.error.clone(),
                warning: f.warning.clone(),
                stale: f.stale,
                import_report: f.import_report.as_ref().map(serde_json::to_value).transpose().map_err(|e| bad(ID, e.to_string()))?,
            });
        }
    }
    let passed = !bodies.is_empty()
        && s.model.param_errors.is_empty()
        && s.model
            .results
            .iter()
            .filter(|f| !f.skipped && active_features.contains(&f.id))
            .all(|f| f.error.is_none() && f.warning.is_none() && !f.stale && f.import_report.as_ref().is_none_or(|r| r.issues.is_empty()))
        && bodies.iter().all(|b| {
            b.closed
                && b.errors.is_empty()
                && b.bounds_mm.is_some()
                && b.record_check.status == CheckStatus::Passed
                && b.native_check.status == CheckStatus::Passed
                && b.measurement.status == CheckStatus::Passed
        });
    let result = InspectBodiesResult {
        document_id: s.document_id.clone(),
        revision,
        document_units: s.doc.units.clone(),
        measurement_units: "mm; mm^3".into(),
        body_count: bodies.len(),
        open_body_count: bodies.iter().filter(|b| !b.closed).count(),
        passed,
        bodies,
        feature_diagnostics,
        parameter_errors: s.model.param_errors.clone(),
        unchecked: vec![
            "Original-file entity completeness and original unit interpretation are not checked.".into(),
            "Missing parts, assembly hierarchy, mates, contacts and source correctness are not checked.".into(),
            "No export/reimport comparison or manufacturing certification is performed.".into(),
            "Document feature diagnostics include all active features, even when only some bodies are selected.".into(),
        ],
    };
    cancellation()?;
    serde_json::to_value(result).map_err(|e| bad(ID, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_body_inspection_measures_world_bounds_without_changing_document() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"name":"Product","length":10,"width":20,"height":30,"corner":[100,-20,5]})).unwrap();
        let before = s.doc.to_json();
        let revision = s.revision;
        let dirty = s.is_dirty();
        let result = s.execute(ID, &json!({})).unwrap();
        assert_eq!(result["body_count"], 1);
        assert_eq!(result["passed"], true);
        let body = &result["bodies"][0];
        assert_eq!(body["closed"], true);
        assert_eq!(body["kernel"]["faces"], 6);
        assert_eq!(body["bounds_mm"]["min"], json!([100.0, -20.0, 5.0]));
        assert_eq!(body["bounds_mm"]["max"], json!([110.0, 0.0, 35.0]));
        assert!((body["volume_mm3"].as_f64().unwrap() - 6000.0).abs() < 1e-7);
        assert_eq!(s.doc.to_json(), before);
        assert_eq!(s.revision, revision);
        assert_eq!(s.is_dirty(), dirty);
    }

    #[test]
    fn current_body_inspection_rejects_unknown_selection_and_preserves_feature_failures() {
        let mut s = Session::default();
        assert_eq!(s.execute(ID, &json!({})).unwrap()["passed"], false);
        s.execute("solid.box", &json!({"length":1,"width":2,"height":3})).unwrap();
        assert!(s.execute(ID, &json!({"bodies":["absent"]})).is_err());
        assert!(s.execute(ID, &json!({"bodies":["Body1","Body1"]})).is_err());
        assert!(s.execute(ID, &json!({"unexpected":true})).is_err());
        s.model.results.last_mut().unwrap().error = Some("recorded feature failure".into());
        let r = inspect(&mut s, &json!({})).unwrap();
        assert_eq!(r["passed"], false);
        assert_eq!(r["feature_diagnostics"][0]["error"], "recorded feature failure");
    }

    #[test]
    fn current_body_inspection_honors_cancellation_without_editing() {
        let mut s = Session::default();
        let before = s.doc.to_json();
        let progress = std::sync::Arc::new(solidcraft_doc::progress::Progress::default());
        progress.cancel();
        assert!(solidcraft_doc::progress::watch(progress, || inspect(&mut s, &json!({}))).is_err());
        assert_eq!(s.doc.to_json(), before);
    }

    #[test]
    fn current_body_inspection_does_not_call_an_open_mesh_a_valid_solid() {
        use solidcore_record::Vec3;
        let body = solidcraft_kernel::mesh_body(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 20.0, 0.0)], &[[0, 1, 2]]).unwrap();
        let b = solidcraft_doc::ModelBody::new("Open mesh".into(), body, 1);
        let s = Session::default();
        let row = inspect_body(&b, &s.model.state()).unwrap();
        assert!(!row.closed);
        assert!(row.volume_mm3.is_none());
        assert!(row.kernel.is_none());
        assert_eq!(row.record_check.status, CheckStatus::NotSupported);
        assert_eq!(row.native_check.status, CheckStatus::NotSupported);
        assert_eq!(row.bounds_mm.unwrap().max, [10.0, 20.0, 0.0]);
    }

    #[test]
    fn current_body_inspection_excludes_suppressed_and_rolled_back_feature_diagnostics() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length":1,"width":2,"height":3})).unwrap();
        s.model.results.last_mut().unwrap().warning = Some("cached warning".into());
        assert_eq!(inspect(&mut s, &json!({})).unwrap()["feature_diagnostics"].as_array().unwrap().len(), 1);
        s.doc_mut().features.last_mut().unwrap().suppressed = true;
        assert!(inspect(&mut s, &json!({})).unwrap()["feature_diagnostics"].as_array().unwrap().is_empty());
        s.doc_mut().features.last_mut().unwrap().suppressed = false;
        s.doc_mut().marker = Some(0);
        assert!(inspect(&mut s, &json!({})).unwrap()["feature_diagnostics"].as_array().unwrap().is_empty());
        s.doc_mut().marker = None;
        assert_eq!(inspect(&mut s, &json!({})).unwrap()["feature_diagnostics"][0]["warning"], "cached warning");
    }

    #[test]
    fn current_body_inspection_retains_clean_import_report_without_inventing_a_failure() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length":1,"width":2,"height":3})).unwrap();
        s.model.results.last_mut().unwrap().import_report =
            Some(std::sync::Arc::new(solidcraft_kernel::ImportReport { parts: 1, solids: 1, issues: Vec::new() }));
        let r = inspect(&mut s, &json!({})).unwrap();
        assert_eq!(r["passed"], true);
        assert_eq!(r["feature_diagnostics"][0]["import_report"]["parts"], 1);
        s.model.results.last_mut().unwrap().warning = Some("original import warning".into());
        let r = inspect(&mut s, &json!({})).unwrap();
        assert_eq!(r["passed"], false);
        assert_eq!(r["feature_diagnostics"][0]["warning"], "original import warning");
    }
}
