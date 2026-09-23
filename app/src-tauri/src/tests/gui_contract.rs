#[test]
fn gui_keeps_analyses_steps_and_conclusions_semantically_separate() {
    let source = include_str!("../../../src/main.js");
    let rows = include_str!("../../../src/math-rows.js");

    assert!(source.contains("(result.analyses || []).map"));
    assert!(source.contains("(result.steps || []).map"));
    assert!(source.contains("(result.conclusions || []).map"));
    assert!(source.contains("[...analyses, ...calculationSteps, ...conclusions]"));
    assert!(source.contains("localizedMessage(analysis.message_ref, analysis.rule)"));
    assert!(source.contains("localizedMessage(step.message_ref, step.rule)"));
    assert!(source.contains("localizedMessage(conclusion.message_ref, conclusion.kind)"));
    assert!(!source.contains("Longrightarrow"));
    assert!(!rows.contains("Longrightarrow"));
}

#[test]
fn gui_keyboard_distinguishes_equations_from_solving_them() {
    let keyboard = include_str!("../../../src/keyboard.js");
    let i18n = include_str!("../../../src/i18n.js");

    assert!(keyboard.contains("equation:"));
    assert!(keyboard.contains("\\\\yaclueEquation{#@}{#0}"));
    assert!(keyboard.contains("solve:"));
    assert!(keyboard.contains("\\\\operatorname{Solve}\\\\left(#0,x\\\\right)"));
    assert!(i18n.contains("显式使用 Solve 或 OdeSolve"));
}

#[test]
fn gui_uses_stable_locale_keys() {
    let javascript = include_str!("../../../src/main.js");
    let i18n = include_str!("../../../src/i18n.js");

    assert!(javascript.contains("hasTranslation(reference.key)"));
    assert!(javascript.contains("t(reference.key, reference.args)"));
    assert!(i18n.contains("\"zh-CN\""));
    assert!(i18n.contains("\"en-US\""));
    assert!(i18n.contains("inconsistent keys"));
}

#[test]
fn changing_locale_renders_the_complete_existing_result_again() {
    let javascript = include_str!("../../../src/main.js");
    let locale_handler = javascript
        .split("document.addEventListener(\"localechange\"")
        .nth(1)
        .unwrap();

    assert!(locale_handler.contains("renderCalculation(lastCalculationResult)"));
    assert!(locale_handler.contains("renderCalculationError(lastCalculationError)"));
    assert!(javascript.contains("(result.analyses || []).map"));
    assert!(javascript.contains("(result.steps || []).map"));
    assert!(javascript.contains("(result.conclusions || []).map"));
}

#[test]
fn gui_foundation_exposes_status_focus_and_reduced_motion_contracts() {
    let html = include_str!("../../../src/index.html");
    let css = include_str!("../../../src/styles.css");

    assert!(html.contains("class=\"topbar\""));
    assert!(html.contains("id=\"math-input\""));
    assert!(html.contains("id=\"answer\" class=\"math-row answer-row\" aria-live=\"polite\""));
    assert!(css.contains("button:focus-visible"));
    assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
}

#[test]
fn keyboard_menu_scrolls_horizontally_and_survives_layer_switches() {
    let javascript = include_str!("../../../src/main.js");
    let css = include_str!("../../../src/styles.css");

    assert!(css.contains(".MLK__toolbar > .left"));
    assert!(css.contains("overflow-x: auto"));
    assert!(css.contains("touch-action: pan-x"));
    assert!(javascript.contains("keyboardMenuScrollLeft = scroller.scrollLeft"));
    assert!(javascript.contains("requestAnimationFrame(restoreKeyboardMenuScroll)"));
}

#[test]
fn editing_during_calculation_clears_the_stale_pending_state() {
    let javascript = include_str!("../../../src/main.js");

    assert!(javascript.contains("let calculationPending = false"));
    assert!(javascript.contains("if (!calculationPending) return"));
    assert!(javascript.contains("clearCalculationRows(steps, answer)"));
    assert!(javascript.contains("if (request === calculationRequest) renderCalculation(result)"));
}

#[test]
fn bodied_operators_take_the_existing_expression_as_their_operand() {
    let keyboard = include_str!("../../../src/keyboard.js");
    let mathlive = include_str!("../../../vendor/mathlive/mathlive-static.css");

    assert!(keyboard.contains("derivative:"));
    assert!(keyboard.contains("latex: '\\\\frac{\\\\mathrm{d}}{\\\\mathrm{d}x}'"));
    for template in [
        "\\\\yaclueNthDerivative{x}{#0}{#@}",
        "\\\\yaclueIntegral{x}{#@}",
        "\\\\yaclueDefiniteIntegral{x}{#0}{#0}{#@}",
        "\\\\yaclueLimit{x}{#0}{#@}",
        "\\\\yaclueSum{k}{#0}{#0}{#@}",
        "\\\\yaclueTaylor{x}{#0}{#0}{#@}",
        "\\\\yaclueSubstitute{x}{#0}{#@}",
    ] {
        assert!(keyboard.contains(template), "missing implicit operand: {template}");
    }
    assert!(mathlive.contains(".ML__yaclue-operand-slot.ML__placeholder"));
    assert!(mathlive.contains(".ML__yaclue-operand-slot .ML__placeholder"));
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
