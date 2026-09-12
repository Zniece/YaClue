#![cfg(feature = "test-cli")]

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn json_lines_is_persistent_structured_and_recovers_after_bad_input() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yaclue-stdio"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start yaclue-stdio");
    let requests = [
        json!({"expression":"Limit(t,0)(Sin(t)/t+x^2)","steps":false,"verbosity":"standard"}),
        json!({"expression":"Limit(t,0)(Sin(t)/t+x^2)","steps":true,"verbosity":"standard"}),
        json!({"expression":"D(x)Limit(t,0)(Sin(t)/t+x^2)","steps":false,"verbosity":"standard"}),
        json!({"expression":"Lagrange(x+y,(x^2+y^2)^2,x,y)","steps":false,"verbosity":"standard"}),
        json!({"expression":"Extrema(x+y,x,y)","steps":false,"verbosity":"standard"}),
        json!({"expression":"Plot(D(x)(x^2),x,0,1)","steps":false,"verbosity":"standard"}),
        json!({"expression":"Gradient(x^2+y^2,{x,y},{1,-2})","steps":false,"verbosity":"standard"}),
        json!({"expression":"ScalarLineIntegral(x,{x,y},{t,0},t,0,1)","steps":false,"verbosity":"standard"}),
        json!({"expression":"ScalarSurfaceIntegral(1,{x,y,z},{u,v,0},{u,v},{0,0},{1,1})","steps":false,"verbosity":"standard"}),
    ];
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        for request in &requests[..3] {
            writeln!(stdin, "{request}").unwrap();
        }
        writeln!(stdin, "{{not-json").unwrap();
        for request in &requests[3..] {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().expect("collect yaclue-stdio");
    assert!(output.status.success());
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        requests.len() + 1,
        "one response per input line"
    );

    assert_eq!(rows[0]["expression"], rows[1]["expression"]);
    assert_eq!(rows[0]["semantic"], rows[1]["semantic"]);
    assert_eq!(rows[0]["outcome"], rows[1]["outcome"]);
    assert!(rows[0]["steps"].as_array().unwrap().is_empty());
    assert!(!rows[1]["steps"].as_array().unwrap().is_empty());
    let steps = rows[1]["steps"].as_array().unwrap();
    assert!(steps.iter().all(|step| {
        step["kind"] == "equivalent_transformation"
            && step["before_expr"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
            && step["before_tex"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    }));
    assert!(steps
        .windows(2)
        .all(|pair| pair[1]["before_expr"] == pair[0]["expr"]));
    assert_eq!(rows[2]["expression"], "2*x");
    assert_eq!(rows[3]["error"]["code"], "invalid_input");
    assert_eq!(rows[4]["outcome"]["resolution"], "unresolved");
    assert_eq!(rows[4]["status"], "unresolved");
    assert_eq!(rows[4]["semantic"]["kind"], "unevaluated");
    assert_eq!(rows[5]["outcome"]["resolution"], "no_result");
    assert_eq!(rows[5]["conclusions"][0]["kind"], "no_value");
    assert_eq!(rows[6]["effect_only"], true);
    assert_eq!(rows[7]["analysis"], json!([2]));
    assert_eq!(rows[8]["analysis"]["integrand_verified"], true);
    assert_eq!(rows[9]["analysis"]["integrand_verified"], true);
    assert!(rows.iter().all(|row| row.get("data").is_none()));
}
