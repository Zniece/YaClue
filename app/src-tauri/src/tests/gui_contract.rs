#[test]
fn gui_renders_analysis_without_an_equivalence_arrow() {
    let source = include_str!("../../../src/main.js");
    let analysis_renderer = source
        .split("function renderAnalyses")
        .nth(1)
        .unwrap()
        .split("function renderPlot")
        .next()
        .unwrap();
    assert!(!analysis_renderer.contains("Longrightarrow"));
    assert!(source.contains("renderAnalyses(result.analyses || [])"));
    assert!(source.contains("renderSteps(result.steps || [])"));
    assert!(source.contains("localizedMessage(step.message_ref, step.why)"));
    assert!(source.contains("localizedMessage(analysis.message_ref, analysis.message)"));
    assert!(source.contains("localizedMessage(conclusion.message_ref, conclusion.message)"));
}

#[test]
fn gui_examples_distinguish_equations_from_solving_them() {
    let javascript = include_str!("../../../src/main.js");
    let html = include_str!("../../../src/index.html");
    let i18n = include_str!("../../../src/i18n.js");
    assert!(javascript.contains("Solve(x^2-5*x+6==0,x)"));
    assert!(!javascript.contains("[\"代数方程\", \"x^2-5*x+6==0\""));
    assert!(html.contains("data-i18n=\"constructEquation\""));
    assert!(html.contains("data-i18n=\"equationHelp\""));
    assert!(i18n.contains("显式使用 Solve 或 OdeSolve"));
}

#[test]
fn gui_uses_stable_locale_keys() {
    let javascript = include_str!("../../../src/main.js");
    let i18n = include_str!("../../../src/i18n.js");
    let html = include_str!("../../../src/index.html");

    assert!(javascript.contains("hasTranslation(result.title_key)"));
    assert!(i18n.contains("\"zh-CN\""));
    assert!(i18n.contains("\"en-US\""));
    assert!(html.contains("id=\"locale\""));
    assert!(html.contains("data-i18n=\"tagline\""));
    assert!(i18n.contains("inconsistent keys"));
    assert!(javascript.contains("t(\"missingTranslation\", { key: messageRef.key })"));
    assert!(!javascript
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)));
}

#[test]
fn changing_locale_renders_the_complete_existing_result_again() {
    let javascript = include_str!("../../../src/main.js");
    let locale_handler = javascript
        .split("document.addEventListener(\"localechange\"")
        .nth(1)
        .unwrap()
        .split("exprEl.addEventListener")
        .next()
        .unwrap();
    let result_renderer = javascript
        .split("function renderResult(result)")
        .nth(1)
        .unwrap()
        .split("async function calculate")
        .next()
        .unwrap();

    assert!(locale_handler.contains("renderResult(lastResult)"));
    assert!(locale_handler.contains("showError(lastError)"));
    assert!(result_renderer.contains("renderAnalyses(result.analyses || [])"));
    assert!(result_renderer.contains("renderSteps(result.steps || [])"));
    assert!(result_renderer.contains("renderConclusions(result.conclusions || [])"));
}

#[test]
fn gui_foundation_exposes_status_focus_and_reduced_motion_contracts() {
    let html = include_str!("../../../src/index.html");
    let css = include_str!("../../../src/styles.css");

    assert!(html.contains("class=\"header-tools\""));
    assert!(html.contains("role=\"status\" aria-live=\"polite\""));
    assert!(css.contains("button:focus-visible"));
    assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
}

#[test]
fn core_branch_step_keys_exist_in_every_locale() {
    let i18n = include_str!("../../../src/i18n.js");
    for key in [
        "steps.limit-one-sided-approach",
        "steps.derivative-known-formal-function",
        "steps.equation-system-eliminate",
        "steps.equation-system-verify",
        "analyses.ode-method-bernoulli",
        "steps.ode-variation-wronskian",
        "steps.ode-euler-characteristic-equation",
    ] {
        assert_eq!(i18n.matches(&format!("\"{key}\"")).count(), 2, "{key}");
    }
}
