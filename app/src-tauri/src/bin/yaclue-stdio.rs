use app_lib::{process_expression_with_engine, ProcessExpressionRequest};
use processing::engine::{ErrorCode, ErrorResponse, RustEngineProxy};
use processing::{assumptions, assumptions::AssumptionFact};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, BufRead, Write};

#[derive(Serialize)]
struct StdioError<'a> {
    error: &'a ErrorResponse,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum ControlRequest {
    Assume {
        symbol: String,
        fact: AssumptionFact,
    },
    ClearAssumptions,
    ListAssumptions,
}

fn write_json(output: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = RustEngineProxy::spawn()
        .map_err(|error| io::Error::other(format!("引擎启动失败: {error}")))?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line?;
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        let is_json_request = input
            .strip_prefix('{')
            .is_some_and(|rest| rest.trim_start().starts_with('"'));
        if is_json_request {
            if let Ok(control) = serde_json::from_str::<ControlRequest>(input) {
                let response = match control {
                    ControlRequest::Assume { symbol, fact } => {
                        assumptions::assume(&mut engine, &symbol, fact)
                            .map_err(|error| error.response())
                    }
                    ControlRequest::ClearAssumptions => {
                        match assumptions::clear_assumptions(&mut engine) {
                            Ok(()) => {
                                write_json(&mut stdout, &json!({ "cleared": true }))?;
                                continue;
                            }
                            Err(error) => Err(error.response()),
                        }
                    }
                    ControlRequest::ListAssumptions => {
                        match assumptions::list_assumptions(&mut engine) {
                            Ok(assumptions) => {
                                write_json(&mut stdout, &json!({ "assumptions": assumptions }))?;
                                continue;
                            }
                            Err(error) => Err(error.response()),
                        }
                    }
                };
                match response {
                    Ok(state) => write_json(&mut stdout, &state)?,
                    Err(error) => write_json(&mut stdout, &StdioError { error: &error })?,
                }
                continue;
            }
        }
        let request = if is_json_request {
            match serde_json::from_str(input) {
                Ok(request) => request,
                Err(error) => {
                    let error = ErrorResponse {
                        code: ErrorCode::InvalidInput,
                        message: format!("无效的 JSON 请求: {error}"),
                        retryable: false,
                    };
                    write_json(&mut stdout, &StdioError { error: &error })?;
                    continue;
                }
            }
        } else {
            ProcessExpressionRequest {
                expression: input.to_string(),
                steps: true,
                verbosity: "standard".into(),
            }
        };
        match process_expression_with_engine(request, &mut engine) {
            Ok(result) => write_json(&mut stdout, &result)?,
            Err(error) => write_json(&mut stdout, &StdioError { error: &error })?,
        }
    }
    Ok(())
}
