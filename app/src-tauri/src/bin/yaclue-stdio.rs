use app_lib::{process_expression_with_engine, ProcessExpressionRequest};
use processing::engine::{ErrorCode, ErrorResponse, RustEngineProxy};
use serde::Serialize;
use std::io::{self, BufRead, Write};

#[derive(Serialize)]
struct StdioError<'a> {
    error: &'a ErrorResponse,
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
