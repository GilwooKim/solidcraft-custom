//! Luggage envelope checks and preliminary tooling calculations. All dimensions are mm.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create a linked mold-side copy without changing the selected product body.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShrinkageParams {
    pub body: String,
    /// Percent relative to mold size, in world X/Y/Z. Equal values give isotropic shrinkage.
    pub shrinkage_percent: [f64; 3],
    /// Fixed point in world millimeters.
    pub origin_mm: [f64; 3],
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShrinkageResult {
    pub source_body: String,
    pub mold_body: String,
    pub scale_factors: [f64; 3],
    pub copy_feature: u64,
    pub scale_feature: u64,
    pub coordinate_basis: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceParams {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FormatInfo {
    pub name: String,
    pub extensions: Vec<String>,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WorkspaceResult {
    pub formats: Vec<FormatInfo>,
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeParams {
    /// Limits along the file's X, Y and Z axes. No automatic airline policy is implied.
    pub limits_mm: [f64; 3],
    pub max_sum_mm: f64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EnvelopeResult {
    pub body_count: usize,
    pub mesh_body_count: usize,
    pub size_mm: [f64; 3],
    pub limits_mm: [f64; 3],
    pub excess_mm: [f64; 3],
    pub sum_mm: f64,
    pub max_sum_mm: f64,
    pub passed: bool,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolingParams {
    /// Product projected area for one cavity, supplied by the designer, not bounding-box area.
    pub projected_area_mm2: f64,
    /// Total runner projected area across the entire mold.
    pub runner_area_mm2: f64,
    pub cavities: u32,
    pub cavity_pressure_mpa: f64,
    pub safety_factor: f64,
    /// Shrinkage relative to mold size: product = mold * (1 - shrinkage).
    pub shrinkage_percent: f64,
    pub sheet_thickness_mm: f64,
    pub opening_area_mm2: f64,
    pub formed_area_mm2: f64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ToolingResult {
    pub inputs: ToolingParams,
    pub clamp_kn: f64,
    pub clamp_tf: f64,
    pub mold_scale: f64,
    pub estimated_average_thickness_mm: f64,
    pub draw_area_ratio: f64,
    pub note: String,
}
