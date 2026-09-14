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
