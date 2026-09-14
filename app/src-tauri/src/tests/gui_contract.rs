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
}

#[test]
fn gui_examples_distinguish_equations_from_solving_them() {
    let javascript = include_str!("../../../src/main.js");
    let html = include_str!("../../../src/index.html");
    assert!(javascript.contains("Solve(x^2-5*x+6==0,x)"));
    assert!(!javascript.contains("[\"代数方程\", \"x^2-5*x+6==0\""));
    assert!(html.contains("==</code> 构造方程"));
    assert!(html.contains("显式使用 Solve 或 OdeSolve"));
}

#[test]
fn gui_uses_stable_locale_keys_with_compatibility_fallbacks() {
    let javascript = include_str!("../../../src/main.js");
    let i18n = include_str!("../../../src/i18n.js");
    let html = include_str!("../../../src/index.html");

    assert!(javascript.contains("hasTranslation(result.title_key)"));
    assert!(i18n.contains("\"zh-CN\""));
    assert!(i18n.contains("\"en-US\""));
    assert!(html.contains("id=\"locale\""));
    assert!(html.contains("data-i18n=\"tagline\""));
}
