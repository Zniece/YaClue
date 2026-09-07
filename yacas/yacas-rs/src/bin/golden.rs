//! Golden-file generator: feeds each line of `tests/probes_<name>.txt` to the
//! C++ reference binary (`-pc` mode) and writes `tests/golden_<name>.txt`.
//!
//! Usage: `cargo run --bin golden -- <name>` (e.g. `probes_l1`, `probes_l4`).
//! Two recording modes, chosen by the probe name:
//! - default (line): one output line per probe; record = `expr<TAB>out`.
//! - `l4` (sentinel): FullForm output can span multiple lines, so
//!   `Echo("###");` is appended after each probe as a sentinel; within a
//!   block the leading console status line (`True`) and the trailing
//!   InfixPrinter result line are dropped, keeping only the FullForm output.
//!
//! The reference binary defaults to the prebuilt `build-ref/cyacas` artifact
//! and can be overridden with `YACAS_BIN`; scripts live in `yacas/scripts`.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "l1".into());
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let probes = manifest.join(format!("tests/probes_{name}.txt"));
    // l1 keeps the historical name golden.txt; the rest are golden_<name>.txt.
    let golden_path = if name == "l1" {
        manifest.join("tests/golden.txt")
    } else {
        manifest.join(format!("tests/golden_{name}.txt"))
    };
    let sentinel = name.contains("l4") || name.contains("l5");
    let probes_text = std::fs::read_to_string(&probes).expect("read probes file");
    // Lines prefixed with `>` are setup lines (fed to the reference binary in
    // sentinel mode but not recorded); non-sentinel probe files have none.
    let raw_lines: Vec<&str> = probes_text
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let exprs: Vec<&str> = raw_lines
        .iter()
        .copied()
        .filter(|s| !s.starts_with('>'))
        .collect();

    let cyacas = std::env::var("YACAS_BIN").unwrap_or_else(|_| {
        repo.join("build-ref/cyacas/yacas/yacas")
            .to_string_lossy()
            .into_owned()
    });
    let scripts = std::env::var("YACAS_SCRIPTS")
        .unwrap_or_else(|_| repo.join("yacas/scripts").to_string_lossy().into_owned());
    let mut input: String = raw_lines
        .iter()
        .map(|e| {
            let e = e.strip_prefix('>').unwrap_or(e);
            match sentinel {
                true => format!("{e};\nEcho(\"###\");\n"),
                false => format!("{e};\n"),
            }
        })
        .collect();
    input.push('\n'); // trailing newline triggers the final evaluation in -pc mode
    let mut child = Command::new(&cyacas)
        .args(["-pc", "--rootdir", &scripts])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn cyacas ({cyacas}): {e}"));
    // wait_with_output drops stdin; write explicitly and close first,
    // otherwise the binary sees EOF and only prints its banner.
    child
        .stdin
        .take()
        .expect("cyacas stdin")
        .write_all(input.as_bytes())
        .expect("write cyacas stdin");
    let out = child.wait_with_output().expect("run cyacas");
    if !out.status.success() {
        panic!("cyacas exited with failure: {}", out.status);
    }
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let lines: Vec<&str> = stdout.lines().skip(12 /* startup banner */).collect();

    let mut golden = String::new();
    if sentinel {
        // Split on the `###` sentinel; each block = [status line, FullForm
        // output..., result line]; keep the middle FullForm segment.
        let mut i = 0usize;
        for raw in &raw_lines {
            let _e = raw.strip_prefix('>').unwrap_or(raw);
            let mut block: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i] != "###" {
                block.push(lines[i]);
                i += 1;
            }
            assert!(i < lines.len(), "probe {raw}: missing sentinel line");
            i += 1;
            if raw.starts_with('>') {
                continue; // setup line: fed but not recorded
            }
            assert!(block.len() >= 2, "probe {raw}: bad block {:?}", block);
            let mut middle: Vec<&str>;
            if name.contains("l5") {
                // Verification lines are plain expressions; the block is
                // [True?, result line]. Keep the trailing result, drop the
                // leading True status line.
                middle = block.to_vec();
                if middle.first() == Some(&"True") {
                    middle.remove(0);
                }
            } else {
                // Verification lines are FullForm(...) wrapped; the block is
                // [status line?, FullForm output..., result line]. Drop the
                // trailing result, then the leading True.
                middle = block[..block.len() - 1].to_vec();
                if middle.first() == Some(&"True") {
                    middle.remove(0);
                }
            }
            golden.push_str(raw);
            golden.push('\t');
            golden.push_str(&middle.join("\n"));
            golden.push('\n');
        }
    } else {
        assert_eq!(lines.len(), exprs.len(), "output line count != probe count");
        for (e, o) in exprs.iter().zip(lines.iter()) {
            golden.push_str(e);
            golden.push('\t');
            golden.push_str(o);
            golden.push('\n');
        }
    }
    std::fs::File::create(&golden_path)
        .expect("create golden")
        .write_all(golden.as_bytes())
        .expect("write golden");
    println!("wrote {} entries to {}", exprs.len(), golden_path.display());
}
