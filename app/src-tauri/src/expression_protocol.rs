use processing::protocol::ResultMetadata;
use processing::semantic::SemanticSummary;
use processing::steps::Step;
use serde::{Deserialize, Serialize};

pub(crate) type ProcessExpressionDetails = processing::composition::PartialProductDetails;

#[derive(Deserialize)]
pub struct ProcessExpressionRequest {
    pub expression: String,
    pub steps: bool,
    pub verbosity: String,
}

#[derive(Serialize)]
pub struct ProcessExpressionResult {
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) expression: String,
    pub(crate) tex: String,
    pub(crate) steps: Vec<Step>,
    pub(crate) analyses: Vec<processing::steps::MathematicalAnalysis>,
    pub(crate) conclusions: Vec<processing::steps::MathematicalConclusion>,
    pub(crate) status: Option<processing::composition::CompositionStatus>,
    pub(crate) operators: Vec<processing::semantic_core::OperatorId>,
    pub(crate) held: Option<processing::composition::HeldApplication>,
    pub(crate) sampled_data: Option<processing::semantic_core::SampledTrajectory>,
    pub(crate) plot: Option<processing::plot::PlotEffect>,
    pub(crate) effect_only: bool,
    pub(crate) analysis: Option<processing::semantic_core::ComputationAnalysis>,
    pub(crate) details: Option<ProcessExpressionDetails>,
    pub(crate) semantic: SemanticSummary,
    pub(crate) outcome: ResultMetadata,
}
