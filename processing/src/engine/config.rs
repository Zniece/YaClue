use std::path::PathBuf;

pub(super) fn default_scripts_dir() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_default();
    root.join("yacas/scripts").to_string_lossy().into_owned()
}

pub(super) fn default_steps_dir() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .to_string_lossy()
        .into_owned()
}

pub(super) fn steps_boot_cmds_from_dir(dir: &str) -> Vec<String> {
    let directory = yacas_directory_literal(dir);
    vec![
        format!("DefaultDirectory({directory})"),
        "Load(\"steps.rep/code.ys\")".to_string(),
    ]
}

pub(super) fn yacas_directory_literal(value: &str) -> String {
    let value = value.strip_prefix(r"\\?\").unwrap_or(value);
    let mut portable = value.replace('\\', "/");
    if !portable.ends_with('/') {
        portable.push('/');
    }
    yacas_string_literal(&portable)
}

fn yacas_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\t' => literal.push_str("\\t"),
            '\n' => literal.push_str("\\n"),
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_paths_are_escaped_as_yacas_strings() {
        assert_eq!(
            yacas_string_literal("a\\b\";SystemCall(\"bad\")\n"),
            "\"a\\\\b\\\";SystemCall(\\\"bad\\\")\\n\""
        );
        assert_eq!(
            steps_boot_cmds_from_dir("a\";SystemCall(\"bad\")"),
            [
                "DefaultDirectory(\"a\\\";SystemCall(\\\"bad\\\")/\")",
                "Load(\"steps.rep/code.ys\")"
            ]
        );
        assert_eq!(
            yacas_directory_literal(r"C:\Users\znie\Desktop\YaClue\yacas\scripts"),
            r#""C:/Users/znie/Desktop/YaClue/yacas/scripts/""#
        );
        assert_eq!(
            steps_boot_cmds_from_dir(r"\\?\C:\Program Files\YaClue\processing\scripts")[0],
            r#"DefaultDirectory("C:/Program Files/YaClue/processing/scripts/")"#
        );

        let mut env = yacas_rs::env::Environment::new();
        crate::engine::rust::eval_cmd(
            &mut env,
            r#"DefaultDirectory("C:/Users/znie/Desktop/YaClue/yacas/scripts/")"#,
        )
        .unwrap();
        assert_eq!(
            env.input_directories.last().map(String::as_str),
            Some("C:/Users/znie/Desktop/YaClue/yacas/scripts/")
        );
    }
}
