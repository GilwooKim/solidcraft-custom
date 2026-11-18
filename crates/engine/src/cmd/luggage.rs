//! Luggage-specific checks, using world-space geometry and explicit project criteria.
use super::CommandSpec;
use crate::params::bad;
use crate::schema::luggage::*;
use crate::schema::{TypedContract, output_schema_for};
use crate::{Result, Session};
use serde_json::Value;
use solidcraft_geom::Aabb3;

const WORKSPACE: TypedContract<WorkspaceParams> = TypedContract::new();
const ENVELOPE: TypedContract<EnvelopeParams> = TypedContract::new();
const TOOLING: TypedContract<ToolingParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
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
