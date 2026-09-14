use processing::assumptions::{AssumptionFact, AssumptionState};
use processing::engine::{EngineError, ErrorCode, ErrorResponse, RustEngineProxy};
#[cfg(test)]
use processing::protocol::{Condition, Conditionality};
#[cfg(test)]
use processing::semantic::ValueKind;
use std::sync::{Mutex, MutexGuard};
use tauri::Manager;

mod expression;
mod expression_protocol;

pub use expression::process_expression_with_engine;
#[cfg(test)]
use expression_protocol::ProcessExpressionDetails;
pub use expression_protocol::{ProcessExpressionRequest, ProcessExpressionResult};

fn lock_engine<'a>(
    state: &'a tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<MutexGuard<'a, RustEngineProxy>, ErrorResponse> {
    state.lock().map_err(|error| {
        ErrorResponse::keyed(
            ErrorCode::Internal,
            "errors.engine_state_unavailable",
            format!("引擎状态锁不可用: {error}"),
            true,
        )
    })
}

fn message(error: EngineError) -> ErrorResponse {
    let response = error.response();
    if response.code != ErrorCode::InvalidInput {
        eprintln!("[YaClue] backend diagnostic: {}", response.message);
    }
    response
}

fn parse_assumption_fact(value: &str) -> Result<AssumptionFact, ErrorResponse> {
    match value {
        "real" => Ok(AssumptionFact::Real),
        "integer" => Ok(AssumptionFact::Integer),
        "positive" => Ok(AssumptionFact::Positive),
        "negative" => Ok(AssumptionFact::Negative),
        "non_zero" => Ok(AssumptionFact::NonZero),
        _ => Err(ErrorResponse::keyed(
            ErrorCode::InvalidInput,
            "errors.unknown_assumption_fact",
            format!("未知假设性质: {value}"),
            false,
        )
        .arg("value", value)),
    }
}

#[tauri::command]
async fn set_assumption(
    symbol: String,
    fact: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<AssumptionState, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::assume(&mut *engine, &symbol, parse_assumption_fact(&fact)?)
        .map_err(message)
}

#[tauri::command]
async fn clear_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<(), ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::clear_assumptions(&mut *engine).map_err(message)
}

#[tauri::command]
async fn get_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<AssumptionState>, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::list_assumptions(&mut *engine).map_err(message)
}

#[tauri::command]
async fn process_expression(
    request: ProcessExpressionRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    process_expression_with_engine(request, &mut engine)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "android")]
            let resources = app
                .path()
                .app_data_dir()?
                .join("files")
                .join("bundled")
                .join(env!("CARGO_PKG_VERSION"));
            #[cfg(not(target_os = "android"))]
            let resources = app.path().resource_dir()?;
            let scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| {
                resources
                    .join("yacas/scripts")
                    .to_string_lossy()
                    .into_owned()
            });
            let steps = std::env::var("YACAS_STEPS_SCRIPTS").unwrap_or_else(|_| {
                resources
                    .join("processing/scripts")
                    .to_string_lossy()
                    .into_owned()
            });
            let engine = RustEngineProxy::spawn_with_scripts(scripts, steps)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(Mutex::new(engine));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_assumption,
            clear_assumptions,
            get_assumptions,
            process_expression,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
