//! Bounded, three-valued equivalence verification and transformation certificates.
//!
//! This module is opt-in. Ordinary evaluation does not pay for proof work.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::binding;
use crate::engine::{Engine, EngineError, Expr};
use crate::protocol::ConditionSet;
use crate::semantic;
use crate::steps::{Step, StepImportance};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Equivalence {
    ProvenEqual,
    ProvenDifferent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    IdenticalAst,
    AlphaEquivalent,
    DecidableNormalForm,
    BoundedRewrite,
    ResidualProof,
    NumericCounterexample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    NotProven,
    BudgetExhausted,
    UnsupportedNumericDomain,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Counterexample {
    pub assignments: Vec<RuleBinding>,
    pub left_value: f64,
    pub right_value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleBinding {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProofBudget {
    pub max_steps: usize,
    pub max_branches: usize,
    pub max_nodes: usize,
    pub max_node_growth: usize,
    pub max_cas_requests: usize,
    pub max_visited_expressions: usize,
    pub max_millis: u64,
}

impl Default for ProofBudget {
    fn default() -> Self {
        Self {
            max_steps: 8,
            max_branches: 8,
            max_nodes: 4_096,
            max_node_growth: 4,
            max_cas_requests: 16,
            max_visited_expressions: 32,
            max_millis: 2_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ProofUsage {
    pub steps: usize,
    pub branches: usize,
    pub peak_nodes: usize,
    pub cas_requests: usize,
    pub visited_expressions: usize,
    pub elapsed_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EquivalenceReport {
    pub conclusion: Equivalence,
    pub method: Option<VerificationMethod>,
    pub unknown_reason: Option<UnknownReason>,
    pub counterexample: Option<Counterexample>,
    pub usage: ProofUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransformationCertificate {
    pub rule: String,
    pub before: String,
    pub after: String,
    pub position: Vec<usize>,
    pub bindings: Vec<RuleBinding>,
    pub conditions: ConditionSet,
    pub verification: VerificationMethod,
}

impl TransformationCertificate {
    pub fn project_step(&self, tex: String) -> Step {
        Step {
            rule: self.rule.clone(),
            expr: self.after.clone(),
            why: format!("应用经 {:?} 验证的有限变换。", self.verification),
            tex,
            importance: StepImportance::Key,
        }
    }
}

struct ProofContext {
    budget: ProofBudget,
    usage: ProofUsage,
    started: Instant,
    initial_nodes: usize,
    visited: BTreeSet<String>,
}

impl ProofContext {
    fn new(budget: ProofBudget, left: &str, right: &str) -> Self {
        let initial_nodes = node_estimate(left).max(node_estimate(right)).max(1);
        Self {
            budget,
            usage: ProofUsage {
                peak_nodes: initial_nodes,
                ..ProofUsage::default()
            },
            started: Instant::now(),
            initial_nodes,
            visited: BTreeSet::new(),
        }
    }

    fn visit(&mut self, expression: &str) -> bool {
        self.usage.steps += 1;
        let nodes = node_estimate(expression);
        self.usage.peak_nodes = self.usage.peak_nodes.max(nodes);
        self.visited.insert(expression.to_string());
        self.sync();
        !self.exhausted()
    }

    fn branch(&mut self) -> bool {
        self.usage.branches += 1;
        self.sync();
        !self.exhausted()
    }

    fn allow_cas(&mut self) -> bool {
        self.sync();
        if self.exhausted() || self.usage.cas_requests >= self.budget.max_cas_requests {
            return false;
        }
        self.usage.cas_requests += 1;
        true
    }

    fn sync(&mut self) {
        self.usage.visited_expressions = self.visited.len();
        self.usage.elapsed_millis = self.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    }

    fn exhausted(&self) -> bool {
        self.usage.steps > self.budget.max_steps
            || self.usage.branches > self.budget.max_branches
            || self.usage.peak_nodes > self.budget.max_nodes
            || self.usage.peak_nodes
                > self
                    .initial_nodes
                    .saturating_mul(self.budget.max_node_growth)
            || self.usage.cas_requests > self.budget.max_cas_requests
            || self.usage.visited_expressions > self.budget.max_visited_expressions
            || self.started.elapsed() > Duration::from_millis(self.budget.max_millis)
    }

    fn finish(
        mut self,
        conclusion: Equivalence,
        method: Option<VerificationMethod>,
        unknown_reason: Option<UnknownReason>,
        counterexample: Option<Counterexample>,
    ) -> EquivalenceReport {
        self.sync();
        EquivalenceReport {
            conclusion,
            method,
            unknown_reason,
            counterexample,
            usage: self.usage,
        }
    }
}

pub fn verify(
    engine: &mut dyn Engine,
    left: &str,
    right: &str,
    budget: ProofBudget,
) -> Result<EquivalenceReport, EngineError> {
    let left_analysis = semantic::analyze_input(left, "左表达式")?;
    let right_analysis = semantic::analyze_input(right, "右表达式")?;
    let mut context = ProofContext::new(budget, left, right);
    if !context.visit(left) || !context.visit(right) {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }

    if binding::structurally_equal(left, right)? {
        return Ok(context.finish(
            Equivalence::ProvenEqual,
            Some(VerificationMethod::IdenticalAst),
            None,
            None,
        ));
    }
    if !context.branch() {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }
    if binding::alpha_equivalent(left, right)? {
        return Ok(context.finish(
            Equivalence::ProvenEqual,
            Some(VerificationMethod::AlphaEquivalent),
            None,
            None,
        ));
    }

    if compare_cas_forms(engine, &mut context, left, right, "NormalForm")? {
        return Ok(context.finish(
            Equivalence::ProvenEqual,
            Some(VerificationMethod::DecidableNormalForm),
            None,
            None,
        ));
    }
    if context.exhausted() {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }
    if compare_cas_forms(engine, &mut context, left, right, "Simplify")? {
        return Ok(context.finish(
            Equivalence::ProvenEqual,
            Some(VerificationMethod::BoundedRewrite),
            None,
            None,
        ));
    }
    if context.exhausted() {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }
    if !context.branch() || !context.allow_cas() {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }
    let residual = engine.eval_expr(&format!("Simplify(({left})-({right}))"))?;
    let residual_text = residual.to_string();
    if !context.visit(&residual_text) {
        return Ok(unknown(context, UnknownReason::BudgetExhausted));
    }
    if exact_zero(&residual) {
        return Ok(context.finish(
            Equivalence::ProvenEqual,
            Some(VerificationMethod::ResidualProof),
            None,
            None,
        ));
    }
    if let Some(value) = numeric_value(&residual) {
        if value != 0.0 {
            return Ok(context.finish(
                Equivalence::ProvenDifferent,
                Some(VerificationMethod::ResidualProof),
                None,
                None,
            ));
        }
    }

    let symbols = left_analysis
        .semantic
        .symbols
        .into_iter()
        .chain(right_analysis.semantic.symbols)
        .collect::<BTreeSet<_>>();
    if symbols.is_empty() {
        return Ok(unknown(context, UnknownReason::UnsupportedNumericDomain));
    }
    if let Some(counterexample) =
        numeric_counterexample(engine, &mut context, left, right, &symbols)?
    {
        return Ok(context.finish(
            Equivalence::ProvenDifferent,
            Some(VerificationMethod::NumericCounterexample),
            None,
            Some(counterexample),
        ));
    }
    let reason = if context.exhausted() {
        UnknownReason::BudgetExhausted
    } else {
        UnknownReason::NotProven
    };
    Ok(unknown(context, reason))
}

fn compare_cas_forms(
    engine: &mut dyn Engine,
    context: &mut ProofContext,
    left: &str,
    right: &str,
    operator: &str,
) -> Result<bool, EngineError> {
    if !context.branch() || !context.allow_cas() {
        return Ok(false);
    }
    let left_form = engine
        .eval_expr(&format!("{operator}({left})"))?
        .to_string();
    if !context.visit(&left_form) || !context.allow_cas() {
        return Ok(false);
    }
    let right_form = engine
        .eval_expr(&format!("{operator}({right})"))?
        .to_string();
    if !context.visit(&right_form) {
        return Ok(false);
    }
    Ok(left_form == right_form)
}

fn numeric_counterexample(
    engine: &mut dyn Engine,
    context: &mut ProofContext,
    left: &str,
    right: &str,
    symbols: &BTreeSet<String>,
) -> Result<Option<Counterexample>, EngineError> {
    const SAMPLES: &[i32] = &[-2, -1, 1, 2];
    for sample in SAMPLES {
        if !context.branch() {
            return Ok(None);
        }
        let assignments = symbols
            .iter()
            .map(|name| RuleBinding {
                name: name.clone(),
                value: sample.to_string(),
            })
            .collect::<Vec<_>>();
        let left_command = substitute_numeric(left, &assignments);
        let right_command = substitute_numeric(right, &assignments);
        if !context.allow_cas() {
            return Ok(None);
        }
        let left_value = numeric_value(&engine.eval_expr(&left_command)?);
        if !context.allow_cas() {
            return Ok(None);
        }
        let right_value = numeric_value(&engine.eval_expr(&right_command)?);
        if let (Some(left_value), Some(right_value)) = (left_value, right_value) {
            let tolerance = 1e-10 * left_value.abs().max(right_value.abs()).max(1.0);
            if (left_value - right_value).abs() > tolerance {
                return Ok(Some(Counterexample {
                    assignments,
                    left_value,
                    right_value,
                }));
            }
        }
    }
    Ok(None)
}

fn substitute_numeric(expression: &str, assignments: &[RuleBinding]) -> String {
    let substituted = assignments
        .iter()
        .fold(expression.to_string(), |current, binding| {
            format!(
                "Eval(ApplyPure(\"Subst\",{{{},{},{current}}}))",
                binding.name, binding.value
            )
        });
    format!("N({substituted},30)")
}

fn numeric_value(expression: &Expr) -> Option<f64> {
    match expression {
        Expr::Number(value) => value.parse().ok().filter(|value: &f64| value.is_finite()),
        Expr::Call { head, args } if head == "-" && args.len() == 1 => {
            numeric_value(&args[0]).map(|value| -value)
        }
        Expr::Call { head, args } if head == "/" && args.len() == 2 => {
            let denominator = numeric_value(&args[1])?;
            (denominator != 0.0)
                .then(|| numeric_value(&args[0]).map(|value| value / denominator))
                .flatten()
        }
        _ => None,
    }
}

fn exact_zero(expression: &Expr) -> bool {
    matches!(expression, Expr::Number(value) if value == "0")
}

fn node_estimate(expression: &str) -> usize {
    expression
        .chars()
        .filter(|character| matches!(character, '(' | ',' | '+' | '-' | '*' | '/' | '^'))
        .count()
        + 1
}

fn unknown(context: ProofContext, reason: UnknownReason) -> EquivalenceReport {
    context.finish(Equivalence::Unknown, None, Some(reason), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn distinguishes_proof_counterexample_and_unknown() {
        let mut engine = RustEngine::spawn().unwrap();
        let equal = verify(&mut engine, "(x+1)^2", "x^2+2*x+1", ProofBudget::default()).unwrap();
        assert_eq!(equal.conclusion, Equivalence::ProvenEqual);

        let different = verify(&mut engine, "x+1", "x+2", ProofBudget::default()).unwrap();
        assert_eq!(different.conclusion, Equivalence::ProvenDifferent);

        let unknown = verify(
            &mut engine,
            "Sin(x)",
            "Cos(x)",
            ProofBudget {
                max_cas_requests: 0,
                ..ProofBudget::default()
            },
        )
        .unwrap();
        assert_eq!(unknown.conclusion, Equivalence::Unknown);
        assert_eq!(unknown.unknown_reason, Some(UnknownReason::BudgetExhausted));
    }

    #[test]
    fn recognizes_alpha_equivalence_before_cas() {
        let mut engine = RustEngine::spawn().unwrap();
        let report = verify(
            &mut engine,
            "Integrate(t,0,X)Sin(t^2)",
            "Integrate(u,0,X)Sin(u^2)",
            ProofBudget::default(),
        )
        .unwrap();
        assert_eq!(report.method, Some(VerificationMethod::AlphaEquivalent));
        assert_eq!(report.usage.cas_requests, 0);
    }

    #[test]
    fn numeric_samples_never_prove_equality() {
        let mut engine = RustEngine::spawn().unwrap();
        let report = verify(
            &mut engine,
            "UnknownF(x)",
            "UnknownG(x)",
            ProofBudget::default(),
        )
        .unwrap();
        assert_ne!(
            report.method,
            Some(VerificationMethod::NumericCounterexample)
        );
        assert_ne!(report.conclusion, Equivalence::ProvenEqual);
    }

    #[test]
    fn certificate_projects_to_a_step() {
        let certificate = TransformationCertificate {
            rule: "factor-common-term".into(),
            before: "a*x+a*y".into(),
            after: "a*(x+y)".into(),
            position: vec![],
            bindings: vec![RuleBinding {
                name: "factor".into(),
                value: "a".into(),
            }],
            conditions: ConditionSet::empty(),
            verification: VerificationMethod::BoundedRewrite,
        };
        let step = certificate.project_step("a(x+y)".into());
        assert_eq!(step.expr, certificate.after);
    }
}
