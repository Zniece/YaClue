//! Golden generator for the payload1 e2e suite: feeds each line of
//! `tests/e2e_payload/payload1.txt` to the C++ reference binary (`-pc` mode)
//! and writes `tests/e2e_payload/golden_p1.txt`. Lines prefixed with `>` are
//! setup lines (fed but not recorded).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let probes_path = manifest.join("tests/e2e_payload/payload1.txt");
    let probes = std::fs::read_to_string(&probes_path).expect("read payload1.txt");
    let mut input = String::new();
    for line in probes.lines().filter(|l| !l.trim().is_empty()) {
        let e = line.trim().trim_start_matches('>');
        input.push_str(e);
        input.push_str(";\n");
    }
    let cyacas = std::env::var("YACAS_BIN").unwrap_or_else(|_| {
        repo.join("build-ref/cyacas/yacas/yacas")
            .to_string_lossy()
            .into_owned()
    });
    let scripts = std::env::var("YACAS_SCRIPTS")
        .unwrap_or_else(|_| repo.join("yacas/scripts").to_string_lossy().into_owned());
    let mut child = Command::new(&cyacas)
        .args(["-pc", "--rootdir", &scripts])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn cyacas ({cyacas}): {e}"));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().skip(12 /* startup banner */).collect();
    let exprs: Vec<&str> = probes
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_start_matches('>'))
        .collect();
    assert_eq!(
        lines.len(),
        exprs.len(),
        "output lines {} != probes {}",
        lines.len(),
        exprs.len()
    );
    let mut golden = String::new();
    let mut outs = lines.iter();
    for raw in probes.lines().filter(|l| !l.trim().is_empty()) {
        let out = outs.next().expect("output line");
        if !raw.trim_start().starts_with('>') {
            golden.push_str(raw.trim());
            golden.push('\t');
            golden.push_str(out);
            golden.push('\n');
        }
    }
    let out_path = manifest.join("tests/e2e_payload/golden_p1.txt");
    std::fs::File::create(&out_path)
        .unwrap()
        .write_all(golden.as_bytes())
        .unwrap();
    println!("golden {} entries", golden.lines().count());
}
