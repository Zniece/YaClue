use crate::engine::{Engine, EngineError, EvalResult, Expr, RustEngine};

pub(crate) struct CountingEngine {
    inner: RustEngine,
    pub(crate) eval_calls: usize,
    pub(crate) batch_sizes: Vec<usize>,
}

impl CountingEngine {
    pub(crate) fn spawn() -> Self {
        Self {
            inner: RustEngine::spawn().expect("start Rust engine"),
            eval_calls: 0,
            batch_sizes: Vec::new(),
        }
    }

    pub(crate) fn reset_counts(&mut self) {
        self.eval_calls = 0;
        self.batch_sizes.clear();
    }
}

impl Engine for CountingEngine {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        self.eval_calls += 1;
        self.inner.eval(command)
    }

    fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
        self.eval_calls += 1;
        self.inner.eval_expr(command)
    }

    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        self.batch_sizes.push(expressions.len());
        self.inner.render_tex_batch(expressions)
    }
}
