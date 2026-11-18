//! Current-document inspection, not a comparison with the original exchange file.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectBodiesParams {
    /// World body names. Empty means every placed body, including hidden bodies.
    #[serde(default)]
    pub bodies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    NotSupported,
    NotChecked,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Check {
    pub status: CheckStatus,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct KernelCounts {
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub shells: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BodyInspection {
    pub name: String,
    pub definition: String,
    pub component: u64,
    pub feature: u64,
    pub mesh: bool,
    /// Counts of the B-rep itself, never merged display topology. Null for mesh bodies.
    pub kernel: Option<KernelCounts>,
    pub closed: bool,
    pub bounds_mm: Option<Bounds>,
    pub record_check: Check,
    pub native_check: Check,
    /// A measured closed body's positive volume. Null on open/failed measurements.
    pub volume_mm3: Option<f64>,
    pub measurement: Check,
    pub notes: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FeatureDiagnostic {
    pub feature: u64,
    pub name: String,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub stale: bool,
    /// Original structured importer diagnostics, retained without reclassifying source defects.
    pub import_report: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InspectBodiesResult {
    pub document_id: String,
    pub revision: u64,
    pub document_units: String,
    pub measurement_units: String,
    pub body_count: usize,
    pub open_body_count: usize,
    /// True only for a nonempty selection of closed bodies with every check passing,
    /// and no active feature/parameter error, warning or stale evaluation.
    /// A clean import report is retained without failing. Not a manufacturing certification.
    pub passed: bool,
    pub bodies: Vec<BodyInspection>,
    pub feature_diagnostics: Vec<FeatureDiagnostic>,
    pub parameter_errors: std::collections::BTreeMap<String, String>,
    pub unchecked: Vec<String>,
}
