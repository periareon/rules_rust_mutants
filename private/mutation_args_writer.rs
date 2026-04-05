//! Mutation rustc params writer.
//!
//! This binary turns canonical `construct_arguments(...).rustc_flags` output into a
//! runtime params file consumed by the mutation runner. The key jobs are:
//!
//! 1. Expand Bazel-generated `@params` indirections.
//! 2. Append rustc flag files produced by cargo build scripts.
//! 3. Normalize execroot paths into runfiles-relative paths.
//! 4. Replace crate root with a placeholder token that the runner swaps per mutant.
//! 5. Drop output-specific flags so the runner can set `-o <mutant-binary>` itself.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Options {
    output: PathBuf,
    crate_root_placeholder: String,
    strip_prefixes: Vec<String>,
    build_flags_files: Vec<PathBuf>,
    action_argv: Vec<String>,
}

fn parse_args() -> Options {
    let raw: Vec<String> = env::args().collect();
    let split = raw
        .iter()
        .position(|arg| arg == "--")
        .expect("Unable to find split marker `--`");

    let writer_args = &raw[1..split];
    let action_argv = raw[split + 1..].to_vec();

    let mut output = None;
    let mut crate_root_placeholder = None;
    let mut strip_prefixes = Vec::new();
    let mut build_flags_files = Vec::new();

    for arg in writer_args {
        if let Some(v) = arg.strip_prefix("--output=") {
            output = Some(PathBuf::from(v));
            continue;
        }
        if let Some(v) = arg.strip_prefix("--crate-root-placeholder=") {
            crate_root_placeholder = Some(v.to_owned());
            continue;
        }
        if let Some(v) = arg.strip_prefix("--strip-prefix=") {
            strip_prefixes.push(v.to_owned());
            continue;
        }
        if let Some(v) = arg.strip_prefix("--build-flags-file=") {
            build_flags_files.push(PathBuf::from(v));
            continue;
        }

        panic!("Unknown mutation args writer argument: {arg}");
    }

    Options {
        output: output.expect("Missing required --output=<path>"),
        crate_root_placeholder: crate_root_placeholder
            .expect("Missing required --crate-root-placeholder=<token>"),
        strip_prefixes,
        build_flags_files,
        action_argv,
    }
}

fn expand_arg_params(arg: &str) -> Result<Option<Vec<String>>, String> {
    let Some(path) = arg.strip_prefix('@') else {
        return Ok(None);
    };

    let path = Path::new(path);
    let file = fs::File::open(path)
        .map_err(|e| format!("Failed to open parameter file {}: {e}", path.display()))?;
    let reader = BufReader::new(file);

    let mut expanded = Vec::new();
    for line in reader.lines() {
        let line =
            line.map_err(|e| format!("Failed reading parameter file {}: {e}", path.display()))?;
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            expanded.push(trimmed.to_owned());
        }
    }

    Ok(Some(expanded))
}

fn expand_action_args(action_argv: &[String]) -> Result<Vec<String>, String> {
    let mut expanded = Vec::new();

    for arg in action_argv {
        if let Some(args_from_file) = expand_arg_params(arg)? {
            expanded.extend(args_from_file);
        } else {
            expanded.push(arg.to_owned());
        }
    }

    Ok(expanded)
}

fn read_build_flags_files(paths: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut flags = Vec::new();

    for path in paths {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read build flags file {}: {e}", path.display()))?;

        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                flags.push(trimmed.to_owned());
            }
        }
    }

    Ok(flags)
}

fn normalize_strip_prefixes(prefixes: &[String]) -> Vec<String> {
    let mut prefixes: Vec<String> = prefixes
        .iter()
        .filter_map(|prefix| {
            let trimmed = prefix.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.trim_end_matches('/').to_owned())
            }
        })
        .collect();

    // Prefer longer prefixes first to avoid partial stripping of nested paths.
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    prefixes.dedup();
    prefixes
}

fn strip_execroot_prefixes(arg: &str, strip_prefixes: &[String]) -> String {
    let mut out = arg.to_owned();
    for prefix in strip_prefixes {
        let with_sep = format!("{prefix}/");
        out = out.replace(&with_sep, "");
    }
    out
}

fn should_drop_arg(arg: &str) -> bool {
    // Runtime compilation supplies its own crate root and output path, so any
    // static output shaping from analysis-time args must be removed.
    arg.starts_with("--emit=") || arg.starts_with("--out-dir=")
}

fn build_runtime_params(options: &Options) -> Result<Vec<String>, String> {
    let strip_prefixes = normalize_strip_prefixes(&options.strip_prefixes);

    let mut args = expand_action_args(&options.action_argv)?;
    args.extend(read_build_flags_files(&options.build_flags_files)?);

    let mut runtime_args = Vec::new();
    let mut replaced_crate_root = false;

    for raw_arg in args {
        let arg = strip_execroot_prefixes(&raw_arg, &strip_prefixes);
        if arg.is_empty() {
            continue;
        }

        // In canonical rustc flags, the crate root is the first positional arg.
        // Replace it with a placeholder that the runtime runner swaps with the
        // per-mutant crate root path inside a temp directory.
        if !replaced_crate_root && !arg.starts_with('-') {
            runtime_args.push(options.crate_root_placeholder.clone());
            replaced_crate_root = true;
            continue;
        }

        if should_drop_arg(&arg) {
            continue;
        }

        runtime_args.push(arg);
    }

    if !replaced_crate_root {
        return Err("Failed to locate crate root positional argument in rustc flags".to_owned());
    }

    Ok(runtime_args)
}

fn main() {
    let options = parse_args();
    let params = build_runtime_params(&options).unwrap_or_else(|e| {
        panic!("Failed to build runtime rustc params: {e}");
    });

    fs::write(&options.output, params.join("\n")).unwrap_or_else(|e| {
        panic!(
            "Failed to write params file {}: {e}",
            options.output.display()
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{}_{}_{}", name, std::process::id(), nanos,));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn build_runtime_params_replaces_crate_root_and_drops_output_flags() {
        let options = Options {
            output: PathBuf::from("unused"),
            crate_root_placeholder: "__ROOT__".to_owned(),
            strip_prefixes: vec!["/execroot/bazel-out/k8-fastbuild/bin".to_owned()],
            build_flags_files: vec![],
            action_argv: vec![
                "/execroot/bazel-out/k8-fastbuild/bin/pkg/src/lib.rs".to_owned(),
                "--crate-name=my_crate".to_owned(),
                "--emit=link=/execroot/bazel-out/k8-fastbuild/bin/pkg/out".to_owned(),
                "--out-dir=/execroot/bazel-out/k8-fastbuild/bin/pkg/out-dir".to_owned(),
                "--extern=dep=/execroot/bazel-out/k8-fastbuild/bin/external/dep/libdep.rlib"
                    .to_owned(),
                "--cfg=test".to_owned(),
            ],
        };

        let args = build_runtime_params(&options).expect("params should build");
        assert_eq!(args[0], "__ROOT__");
        assert!(args.iter().any(|arg| arg == "--crate-name=my_crate"));
        assert!(args
            .iter()
            .any(|arg| arg == "--extern=dep=external/dep/libdep.rlib"));
        assert!(args.iter().any(|arg| arg == "--cfg=test"));
        assert!(!args.iter().any(|arg| arg.starts_with("--emit=")));
        assert!(!args.iter().any(|arg| arg.starts_with("--out-dir=")));
    }

    #[test]
    fn build_runtime_params_expands_param_files_and_appends_build_flags() {
        let tmp_dir = unique_temp_dir("mutation_args_writer");
        let params_file = tmp_dir.join("action.params");
        let build_flags_file = tmp_dir.join("build.flags");
        fs::write(
            &params_file,
            "--cfg=from_params
--extern=dep=/execroot/bazel-out/k8-fastbuild/bin/external/dep/libdep.rlib
",
        )
        .expect("params file should be written");
        fs::write(
            &build_flags_file,
            "--cfg=from_build_script
",
        )
        .expect("build flags file should be written");

        let options = Options {
            output: PathBuf::from("unused"),
            crate_root_placeholder: "__ROOT__".to_owned(),
            strip_prefixes: vec!["/execroot/bazel-out/k8-fastbuild/bin".to_owned()],
            build_flags_files: vec![build_flags_file.clone()],
            action_argv: vec![
                "/execroot/bazel-out/k8-fastbuild/bin/pkg/src/lib.rs".to_owned(),
                format!("@{}", params_file.display()),
                "--cfg=from_action".to_owned(),
            ],
        };

        let args = build_runtime_params(&options).expect("params should build");
        assert_eq!(
            args,
            vec![
                "__ROOT__".to_owned(),
                "--cfg=from_params".to_owned(),
                "--extern=dep=external/dep/libdep.rlib".to_owned(),
                "--cfg=from_action".to_owned(),
                "--cfg=from_build_script".to_owned(),
            ],
        );

        let _ = fs::remove_dir_all(tmp_dir);
    }

    #[test]
    fn build_runtime_params_errors_without_positional_root() {
        let options = Options {
            output: PathBuf::from("unused"),
            crate_root_placeholder: "__ROOT__".to_owned(),
            strip_prefixes: vec![],
            build_flags_files: vec![],
            action_argv: vec!["--crate-name=my_crate".to_owned()],
        };

        let err = build_runtime_params(&options).expect_err("should fail without crate root arg");
        assert!(err.contains("crate root positional"));
    }
}
