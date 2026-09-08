use std::sync::mpsc;
use std::thread;

use super::{Engine, EngineError, EvalResult, Expr, RustEngine};

enum EngineRequest {
    Eval(String),
    EvalExpr(String),
    RenderTex(Vec<String>),
}

enum EngineResponse {
    Eval(Result<EvalResult, EngineError>),
    EvalExpr(Result<Expr, EngineError>),
    RenderTex(Result<Vec<String>, EngineError>),
}

pub struct RustEngineProxy {
    tx: Option<mpsc::Sender<EngineRequest>>,
    rx: mpsc::Receiver<EngineResponse>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RustEngineProxy {
    pub fn spawn() -> Result<Self, EngineError> {
        Self::spawn_with_initializer(RustEngine::spawn)
    }

    pub fn spawn_with_scripts(scripts: String, steps: String) -> Result<Self, EngineError> {
        Self::spawn_with_initializer(move || RustEngine::spawn_with_scripts(scripts, steps))
    }

    pub(super) fn spawn_with_initializer(
        initialize: impl FnOnce() -> Result<RustEngine, EngineError> + Send + 'static,
    ) -> Result<Self, EngineError> {
        let (tx, cmd_rx) = mpsc::channel::<EngineRequest>();
        let (res_tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("yacas-engine".into())
            .spawn(move || {
                let mut engine = match initialize() {
                    Ok(engine) => engine,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }
                for request in cmd_rx {
                    let response = match request {
                        EngineRequest::Eval(command) => EngineResponse::Eval(engine.eval(&command)),
                        EngineRequest::EvalExpr(command) => {
                            EngineResponse::EvalExpr(engine.eval_expr(&command))
                        }
                        EngineRequest::RenderTex(expressions) => {
                            EngineResponse::RenderTex(engine.render_tex_batch(&expressions))
                        }
                    };
                    if res_tx.send(response).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| EngineError::Spawn(format!("无法启动引擎线程: {e}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(RustEngineProxy {
                tx: Some(tx),
                rx,
                handle: Some(handle),
            }),
            startup => {
                // Close commands before joining, including a worker that exits
                // without a startup response. No unusable proxy escapes.
                drop(tx);
                let _ = handle.join();
                Err(match startup {
                    Ok(Err(error)) => error,
                    Err(error) => EngineError::Spawn(format!("引擎线程初始化期间退出: {error}")),
                    Ok(Ok(())) => unreachable!(),
                })
            }
        }
    }
}

impl Drop for RustEngineProxy {
    fn drop(&mut self) {
        drop(self.tx.take()); // 关闭命令通道 → 引擎线程 for 循环结束 → join 可返回
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Engine for RustEngineProxy {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::Eval(command.to_string()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::Eval(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }

    fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::EvalExpr(command.to_string()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::EvalExpr(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }

    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::RenderTex(expressions.to_vec()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::RenderTex(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }
}
