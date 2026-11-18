//! Luggage-specific checks, using world-space geometry and explicit project criteria.
use super::CommandSpec;
use crate::params::bad;
use crate::schema::luggage::*;
use crate::schema::{TypedContract, output_schema_for};
use crate::{Result, Session};
use serde_json::{Value, json};
use solidcraft_geom::Aabb3;

const WORKSPACE: TypedContract<WorkspaceParams> = TypedContract::new();
const ENVELOPE: TypedContract<EnvelopeParams> = TypedContract::new();
const TOOLING: TypedContract<ToolingParams> = TypedContract::new();
const SHRINKAGE: TypedContract<ShrinkageParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("luggage.mold_shrinkage", "Mold Shrinkage Copy", crate::checked!("luggage.mold_shrinkage", shrinkage))
        .at("MOLD", "MODIFY").icon("scale")
        .contract(&SHRINKAGE).output(output_schema_for::<ShrinkageResult>)
        .params("body: product body name; shrinkage_percent: [X,Y,Z] in 0..<20; origin_mm: fixed world point. Creates a linked copy and scale timeline features in one undo step. Original product remains unchanged. World XYZ axes, not inferred material flow directions."),
    CommandSpec::new("luggage.workspace", "Luggage Workbench", crate::checked!("luggage.workspace", workspace))
        .at("LUGGAGE", "DESIGN").icon("inspect").noundo().read_only()
        .contract(&WORKSPACE).output(output_schema_for::<WorkspaceResult>)
        .params("Read the supported file formats; interactive use opens the luggage workbench."),
    CommandSpec::new("luggage.check_envelope", "Check Luggage Envelope", crate::checked!("luggage.check_envelope", envelope))
        .icon("measure").noundo().read_only()
        .contract(&ENVELOPE).output(output_schema_for::<EnvelopeResult>)
        .params("limits_mm: [X,Y,Z], max_sum_mm: positive mm. Includes every placed body, including hidden ones. Axis-aligned envelope only, not a quality or airline certification."),
    CommandSpec::new("luggage.estimate_tooling", "Tooling Estimates", crate::checked!("luggage.estimate_tooling", tooling))
        .icon("measure").noundo().read_only()
        .contract(&TOOLING).output(output_schema_for::<ToolingResult>)
        .params("projected_area_mm2, runner_area_mm2, cavities, cavity_pressure_mpa, safety_factor, shrinkage_percent, sheet_thickness_mm, opening_area_mm2, formed_area_mm2. Preliminary calculations from supplied values; no automatic machine selection or local thickness prediction."),
];

/// Nested feature commands ordinarily receive component-local inputs. This recipe's
/// inputs are world coordinates, so use the same mapper and copy placement capture
/// as a top-level command before running each existing constructor.
fn feature_in_world(s: &mut Session, id: &str, params: Value) -> Result<Value> {
    let mut mapped = crate::frames::to_local(s, id, &params)?;
    let _copy = crate::frames::copy_context(id, mapped.as_mut());
    let _local = crate::frames::local_geometry();
    s.execute(id, mapped.as_ref().unwrap_or(&params))
}

fn shrinkage(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "luggage.mold_shrinkage";
    let input: ShrinkageParams = decode(id, p)?;
    if input.body.trim().is_empty() || input.body.len() > 4096 {
        return Err(bad(id, "select one existing product body"));
    }
    if input.shrinkage_percent.iter().any(|v| !v.is_finite() || !(0.0..20.0).contains(v)) {
        return Err(bad(id, "each shrinkage must be finite and in 0..<20 percent"));
    }
    if input.origin_mm.iter().any(|v| !v.is_finite() || v.abs() >= 1e9) {
        return Err(bad(id, "origin must contain finite world millimeters below 1e9 in magnitude"));
    }
    // A root component provides an unambiguous world-aligned scale basis. Refuse
    // another active component instead of silently scaling in its rotated axes.
    if s.active_component != 0 {
        return Err(bad(id, "activate the root component before creating a world-axis mold copy"));
    }
    let state = s.world_state()?;
    let source = state.body(&input.body).ok_or_else(|| bad(id, "unknown product body"))?;
    if source.body.is_mesh() {
        return Err(bad(id, "mold shrinkage requires a CAD body, not a mesh"));
    }
    let copied = feature_in_world(s, "solid.move", json!({"bodies":[input.body],"copy":true,"translate":[0,0,0],"name":"Mold product reference"}))?;
    let mold_body = copied
        .get("created_bodies")
        .and_then(Value::as_array)
        .and_then(|b| b.first())
        .and_then(Value::as_str)
        .ok_or_else(|| bad(id, "copy did not return a mold body"))?
        .to_owned();
    let factors = input.shrinkage_percent.map(|rate| 1.0 / (1.0 - rate / 100.0));
    let expressions = input.shrinkage_percent.map(|rate| format!("1 / (1 - ({rate}) / 100)"));
    let scaled = feature_in_world(
        s,
        "solid.scale",
        json!({"bodies":[mold_body],"factors":expressions,"origin":input.origin_mm,"name":"Mold shrinkage compensation"}),
    )?;
    let feature_id = |v: &Value| v.get("feature").and_then(Value::as_u64).ok_or_else(|| bad(id, "missing generated feature identity"));
    reply(
        id,
        ShrinkageResult {
            source_body: input.body,
            mold_body,
            scale_factors: factors,
            copy_feature: feature_id(&copied)?,
            scale_feature: feature_id(&scaled)?,
            coordinate_basis: "world_xyz_mm".into(),
        },
    )
}

fn decode<T: serde::de::DeserializeOwned>(id: &str, p: &Value) -> Result<T> {
    serde_json::from_value(p.clone()).map_err(|e| bad(id, e.to_string()))
}
fn reply(id: &str, v: impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| bad(id, e.to_string()))
}
fn positive(id: &str, value: f64, name: &str, max: f64) -> Result<()> {
    if !value.is_finite() || value <= 0.0 || value > max {
        return Err(bad(id, format!("{name} must be finite, greater than zero and at most {max}")));
    }
    Ok(())
}
fn workspace(_: &mut Session, p: &Value) -> Result<Value> {
    let id = "luggage.workspace";
    let _: WorkspaceParams = decode(id, p)?;
    let formats = solidcraft_io::formats::FORMATS
        .iter()
        .map(|f| FormatInfo {
            name: f.label.into(),
            extensions: f.extensions.iter().map(|s| (*s).into()).collect(),
            read: f.readable(true),
            write: f.export.is_some() || f.id == "design",
        })
        .collect();
    reply(id, WorkspaceResult { formats, note: "Support is format-specific. STEP/IGES transfer geometry, not original feature history; meshes do not recover exact CAD surfaces. Native SolidCraft designs retain their editable history.".into() })
}
fn envelope(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "luggage.check_envelope";
    let input: EnvelopeParams = decode(id, p)?;
    for v in input.limits_mm {
        positive(id, v, "axis limit", 1e6)?;
    }
    positive(id, input.max_sum_mm, "max_sum_mm", 3e6)?;
    let state = s.world_state()?;
    if state.bodies.is_empty() {
        return Err(bad(id, "open or create a model first"));
    }
    if state.bodies.len() > 20_000 {
        return Err(bad(id, "at most 20000 bodies can be checked"));
    }
    let mut combined = Aabb3::EMPTY;
    let mut meshes = 0;
    for b in &state.bodies {
        if solidcraft_doc::progress::cancelled() {
            return Err(bad(id, "check cancelled"));
        }
        let bounds = b.body.exact_bounds()?;
        if bounds.is_empty() || !bounds.min.is_finite() || !bounds.max.is_finite() {
            return Err(bad(id, format!("invalid bounds for {}", b.name)));
        }
        combined = combined.union(&bounds);
        meshes += usize::from(b.body.is_mesh());
    }
    let size = combined.size();
    let size_mm = [size.x, size.y, size.z];
    let excess_mm = std::array::from_fn(|i| (size_mm[i] - input.limits_mm[i]).max(0.0));
    let sum_mm: f64 = size_mm.iter().sum();
    if !size.is_finite() || !sum_mm.is_finite() {
        return Err(bad(id, "model dimensions exceed the numeric range of this check"));
    }
    let passed = excess_mm.iter().all(|v| *v <= 1e-6) && sum_mm <= input.max_sum_mm + 1e-6;
    reply(id, EnvelopeResult { body_count: state.bodies.len(), mesh_body_count: meshes, size_mm,
        limits_mm: input.limits_mm, excess_mm, sum_mm, max_sum_mm: input.max_sum_mm, passed,
        note: "World X/Y/Z bounds include hidden bodies and current component positions. Retract handles and include wheels before checking. Mesh bounds describe only the imported mesh. This check does not certify geometry quality or carrier acceptance.".into() })
}
fn tooling(_: &mut Session, p: &Value) -> Result<Value> {
    let id = "luggage.estimate_tooling";
    let v: ToolingParams = decode(id, p)?;
    for (name, number, max) in [
        ("projected_area_mm2", v.projected_area_mm2, 1e9),
        ("cavity_pressure_mpa", v.cavity_pressure_mpa, 1000.0),
        ("safety_factor", v.safety_factor, 10.0),
        ("sheet_thickness_mm", v.sheet_thickness_mm, 1000.0),
        ("opening_area_mm2", v.opening_area_mm2, 1e9),
        ("formed_area_mm2", v.formed_area_mm2, 1e9),
    ] {
        positive(id, number, name, max)?;
    }
    if v.cavities == 0 || v.cavities > 1024 || v.safety_factor < 1.0 {
        return Err(bad(id, "cavities must be 1..1024 and safety_factor must be at least 1"));
    }
    if !v.runner_area_mm2.is_finite()
        || !(0.0..=1e9).contains(&v.runner_area_mm2)
        || !v.shrinkage_percent.is_finite()
        || !(0.0..20.0).contains(&v.shrinkage_percent)
    {
        return Err(bad(id, "runner area must be 0..1e9 mm2 and shrinkage must be 0..<20 percent"));
    }
    if v.formed_area_mm2 < v.opening_area_mm2 {
        return Err(bad(id, "formed area must be at least the opening area for this stretching estimate"));
    }
    let force_n = (v.projected_area_mm2 * f64::from(v.cavities) + v.runner_area_mm2) * v.cavity_pressure_mpa * v.safety_factor;
    let clamp_kn = force_n / 1000.0;
    let clamp_tf = force_n / 9806.65;
    let draw_area_ratio = v.formed_area_mm2 / v.opening_area_mm2;
    let estimated_average_thickness_mm = v.sheet_thickness_mm / draw_area_ratio;
    if [clamp_kn, clamp_tf, draw_area_ratio, estimated_average_thickness_mm].into_iter().any(|n| !n.is_finite() || n <= 0.0) {
        return Err(bad(id, "input magnitudes exceed the numeric range of this estimate"));
    }
    reply(id, ToolingResult { clamp_kn, clamp_tf,
        mold_scale: 1.0 / (1.0 - v.shrinkage_percent / 100.0),
        estimated_average_thickness_mm,
        draw_area_ratio,
        inputs: v,
        note: "Preliminary estimate using designer-supplied data. Runner area is total, shrinkage is relative to mold size, thickness is an average from area conservation. Minimum thickness, flow, cooling, tie-bar spacing and machine suitability require separate validation.".into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use solidcraft_geom::Vec3Geom;
    #[test]
    fn shrinkage_preserves_product_and_follows_parameters_as_one_undo_step() {
        let mut s = Session::default();
        s.execute("parameters.add", &json!({"name":"product_length","expression":"100 mm"})).unwrap();
        s.execute("solid.box", &json!({"length":"product_length","width":50,"height":20,"body_name":"Product","corner":[10,20,30]})).unwrap();
        let before = s.doc.clone();
        let original = s.world_state().unwrap().body("Product").unwrap().body.exact_bounds().unwrap();
        let result = s.execute("luggage.mold_shrinkage", &json!({"body":"Product","shrinkage_percent":[2,1,0],"origin_mm":[10,20,30]})).unwrap();
        let copy = result["mold_body"].as_str().unwrap();
        assert_ne!(copy, "Product");
        for session in [&s, &Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap())] {
            let state = session.world_state().unwrap();
            assert_eq!(state.bodies.len(), 2);
            assert_eq!(state.body("Product").unwrap().body.exact_bounds().unwrap(), original);
            let bounds = state.body(copy).unwrap().body.exact_bounds().unwrap();
            assert!(bounds.min.dist(original.min) < 1e-7);
            assert!((bounds.size().x - 100.0 / 0.98).abs() < 1e-7);
            assert!((bounds.size().y - 50.0 / 0.99).abs() < 1e-7);
            assert!((bounds.size().z - 20.0).abs() < 1e-7);
            let volume = solidcraft_kernel::measure(&state.body(copy).unwrap().body).unwrap().volume;
            assert!((volume - 100000.0 / (0.98 * 0.99)).abs() < 1e-4);
        }
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
        s.execute("edit.redo", &json!({})).unwrap();
        s.execute("parameters.change", &json!({"name":"product_length","expression":"120 mm"})).unwrap();
        let state = s.world_state().unwrap();
        assert!((state.body("Product").unwrap().body.exact_bounds().unwrap().size().x - 120.0).abs() < 1e-7);
        assert!((state.body(copy).unwrap().body.exact_bounds().unwrap().size().x - 120.0 / 0.98).abs() < 1e-7);
        s.execute("timeline.suppress", &json!({"feature":result["scale_feature"],"suppressed":true})).unwrap();
        let state = s.world_state().unwrap();
        assert!((state.body(copy).unwrap().body.exact_bounds().unwrap().size().x - 120.0).abs() < 1e-7);
        s.execute("timeline.suppress", &json!({"feature":result["scale_feature"],"suppressed":false})).unwrap();
        s.execute("timeline.edit", &json!({"feature":result["scale_feature"],"set":{"factors":["1.1","1","1"]}})).unwrap();
        let state = s.world_state().unwrap();
        assert!((state.body(copy).unwrap().body.exact_bounds().unwrap().size().x - 132.0).abs() < 1e-7);
        assert!((state.body("Product").unwrap().body.exact_bounds().unwrap().size().x - 120.0).abs() < 1e-7);
    }

    #[test]
    fn shrinkage_preserves_rotated_occurrences_and_rejects_invalid_inputs_atomically() {
        let mut s = Session::default();
        s.execute("component.create", &json!({"name":"ProductPart","grounded":false})).unwrap();
        s.execute("solid.box", &json!({"length":10,"width":20,"height":30,"body_name":"Product"})).unwrap();
        s.execute("occurrence.move", &json!({"occurrence":"ProductPart:1","translate":[50,10,20],"axis":[0,0,1],"angle":"90 deg"})).unwrap();
        let params = json!({"body":"Product","shrinkage_percent":[0,0,0],"origin_mm":[0,0,0]});
        let before = s.doc.clone();
        assert!(s.execute("luggage.mold_shrinkage", &params).is_err());
        assert_eq!(s.doc, before);
        s.execute("component.activate", &json!({"component":"root"})).unwrap();
        let original = s.world_state().unwrap().body("Product").unwrap().body.exact_bounds().unwrap();
        for (key, value) in [
            ("body", json!("missing")),
            ("shrinkage_percent", json!([-1, 0, 0])),
            ("shrinkage_percent", json!([20, 0, 0])),
            ("origin_mm", json!([1e9, 0, 0])),
        ] {
            let before = s.doc.clone();
            let mut invalid = params.clone();
            invalid[key] = value;
            assert!(s.execute("luggage.mold_shrinkage", &invalid).is_err());
            assert_eq!(s.doc, before);
        }
        let result = s.execute("luggage.mold_shrinkage", &params).unwrap();
        let copy = result["mold_body"].as_str().unwrap();
        for session in [&s, &Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap())] {
            let state = session.world_state().unwrap();
            assert_eq!(state.bodies.len(), 2);
            assert_eq!(state.body("Product").unwrap().body.exact_bounds().unwrap(), original);
            let bounds = state.body(copy).unwrap().body.exact_bounds().unwrap();
            assert!(bounds.min.dist(original.min) < 1e-7 && bounds.max.dist(original.max) < 1e-7);
        }
        s.execute("edit.undo", &json!({})).unwrap();
        let result = s.execute("luggage.mold_shrinkage", &json!({"body":"Product","shrinkage_percent":[2,1,0],"origin_mm":[20,-10,4]})).unwrap();
        let copy = result["mold_body"].as_str().unwrap();
        for session in [&s, &Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap())] {
            let state = session.world_state().unwrap();
            let bounds = state.body(copy).unwrap().body.exact_bounds().unwrap();
            assert_eq!(state.body("Product").unwrap().body.exact_bounds().unwrap(), original);
            for (actual, source) in [(bounds.min, original.min), (bounds.max, original.max)] {
                assert!((actual.x - (20.0 + (source.x - 20.0) / 0.98)).abs() < 1e-7);
                assert!((actual.y - (-10.0 + (source.y + 10.0) / 0.99)).abs() < 1e-7);
                assert!((actual.z - source.z).abs() < 1e-7);
            }
        }
    }
    #[test]
    fn tooling_units_and_shrinkage_are_explicit() {
        let mut s = Session::default();
        let input = json!({"projected_area_mm2":4676.0,"runner_area_mm2":100.0,"cavities":2,
            "cavity_pressure_mpa":40.0,"safety_factor":1.2,"shrinkage_percent":2.0,
            "sheet_thickness_mm":4.0,"opening_area_mm2":200000.0,"formed_area_mm2":360000.0});
        let r: ToolingResult = serde_json::from_value(tooling(&mut s, &input).unwrap()).unwrap();
        assert!((r.clamp_kn - 453.696).abs() < 1e-9);
        assert!((r.mold_scale - 1.0 / 0.98).abs() < 1e-12);
        assert!((r.estimated_average_thickness_mm - 20.0 / 9.0).abs() < 1e-12);
        for (key, bad_value) in [
            ("cavities", json!(0)),
            ("cavities", json!(1.5)),
            ("shrinkage_percent", json!(100)),
            ("safety_factor", json!(0.9)),
            ("formed_area_mm2", json!(1)),
            ("runner_area_mm2", json!(-1)),
            ("opening_area_mm2", json!(1e-320)),
        ] {
            let mut p = input.clone();
            p[key] = bad_value;
            assert!(tooling(&mut s, &p).is_err());
        }
        // Equal very small areas still have a well-defined ratio and preserve sheet thickness.
        let mut p = input.clone();
        p["opening_area_mm2"] = json!(1e-320);
        p["formed_area_mm2"] = json!(1e-320);
        let r = tooling(&mut s, &p).unwrap();
        assert_eq!(r["draw_area_ratio"], 1.0);
        assert_eq!(r["estimated_average_thickness_mm"], 4.0);
        p["sheet_thickness_mm"] = json!(f64::from_bits(1));
        p["formed_area_mm2"] = json!(1e9);
        p["opening_area_mm2"] = json!(200000.0);
        assert!(tooling(&mut s, &p).is_err());
    }
    #[test]
    fn envelope_includes_placed_bodies_and_rejects_empty_models() {
        let mut s = Session::default();
        let p = json!({"limits_mm":[80,50,6],"max_sum_mm":136});
        assert!(envelope(&mut s, &p).is_err());
        s.execute("solid.box", &json!({"length":80,"width":50,"height":6})).unwrap();
        let r: EnvelopeResult = serde_json::from_value(envelope(&mut s, &p).unwrap()).unwrap();
        assert_eq!(r.size_mm, [80.0, 50.0, 6.0]);
        assert!(r.passed);
        let r = envelope(&mut s, &json!({"limits_mm":[79,50,6],"max_sum_mm":136})).unwrap();
        assert_eq!(r["passed"], false);
        assert_eq!(r["excess_mm"][0], 1.0);
        assert!(envelope(&mut s, &json!({"limits_mm":[0,50,6],"max_sum_mm":136})).is_err());
        s.execute("solid.box", &json!({"corner":[100,0,0],"length":10,"width":10,"height":10})).unwrap();
        let r = envelope(&mut s, &p).unwrap();
        assert_eq!(r["body_count"], 2);
        assert_eq!(r["size_mm"][0], 110.0);
    }
}
