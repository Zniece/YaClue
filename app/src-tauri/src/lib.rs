// GUI 壳:只做 Tauri 命令转发,逻辑在 processing(加工层 .rs 部分)
use processing::engine::{EngineError, RustEngineProxy};
use processing::steps::Step;
use std::sync::Mutex;

/// GUI → 加工层入口:生成分步求导过程(对变量 x)
#[tauri::command]
fn derive_steps(
    expr: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<Step>, String> {
    let mut engine = engine.lock().map_err(|e| e.to_string())?;
    processing::steps::derive_steps(&mut *engine, &expr, "x").map_err(|e: EngineError| e.to_string())
}

/// GUI → 加工层入口:生成分步积分过程(对变量 x)
#[tauri::command]
fn derive_integrals(
    expr: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<Step>, String> {
    let mut engine = engine.lock().map_err(|e| e.to_string())?;
    processing::steps::derive_integrals(&mut *engine, &expr, "x").map_err(|e: EngineError| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 引擎替换:Rust 进程内引擎(yacas-rs)经线程代理接入,不再依赖 C++ 子进程。
    // 脚本库路径可用 YACAS_SCRIPTS 覆盖(默认 <项目根>/yacas/scripts)。
    let engine = RustEngineProxy::spawn()
        .expect("无法初始化 Rust yacas 引擎(可用 YACAS_SCRIPTS 环境变量指定脚本库)");

    tauri::Builder::default()
        .manage(Mutex::new(engine))
        .invoke_handler(tauri::generate_handler![derive_steps, derive_integrals])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
