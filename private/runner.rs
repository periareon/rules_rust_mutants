//! Mutation test runner.
//!
//! Generates mutations from Rust source files, compiles each mutant using
//! rustc with provided parameters, runs the test binary, and reports results.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

// This must match `_CRATE_ROOT_PLACEHOLDER` in mutation_test.bzl.
const CRATE_ROOT_PLACEHOLDER: &str = "__MUTATION_CRATE_ROOT__";
const PASSTHROUGH_ENV_VARS: &[&str] = &[
    "RUNFILES_DIR",
    "RUNFILES_MANIFEST_FILE",
    "RUNFILES_MANIFEST_ONLY",
    "TEST_SRCDIR",
    "TEST_WORKSPACE",
    "TEST_TMPDIR",
    "TZ",
    "PATH",
    "PATHEXT",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
];

struct Args {
    rustc: PathBuf,
    params_file: PathBuf,
    crate_root: PathBuf,
    sources_file: Option<PathBuf>,
    sources: Vec<PathBuf>,
    inputs_file: Option<PathBuf>,
    inputs: Vec<PathBuf>,
    rustc_env_file: Option<PathBuf>,
    rustc_env_files_list: Option<PathBuf>,
    cargo: Option<PathBuf>,
    cargo_mutants: Option<PathBuf>,
    mutants_config: Option<PathBuf>,
    allow_survivors: bool,
}

fn next_arg_value(raw: &[String], i: &mut usize, flag: &str) -> String {
    *i += 1;
    if *i >= raw.len() {
        eprintln!("Missing value for {}", flag);
        std::process::exit(1);
    }
    raw[*i].clone()
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut rustc = None;
    let mut params_file = None;
    let mut crate_root = None;
    let mut sources_file = None;
    let mut sources = Vec::new();
    let mut inputs_file = None;
    let mut inputs = Vec::new();
    let mut rustc_env_file = None;
    let mut rustc_env_files_list = None;
    let mut cargo = None;
    let mut cargo_mutants = None;
    let mut mutants_config = None;
    let mut allow_survivors = false;

    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--rustc" => {
                rustc = Some(PathBuf::from(next_arg_value(&raw, &mut i, "--rustc")));
            }
            "--params" => {
                params_file = Some(PathBuf::from(next_arg_value(&raw, &mut i, "--params")));
            }
            "--crate-root" => {
                crate_root = Some(PathBuf::from(next_arg_value(&raw, &mut i, "--crate-root")));
            }
            "--source" => {
                sources.push(PathBuf::from(next_arg_value(&raw, &mut i, "--source")));
            }
            "--sources-file" => {
                sources_file = Some(PathBuf::from(next_arg_value(
                    &raw,
                    &mut i,
                    "--sources-file",
                )));
            }
            "--input" => {
                inputs.push(PathBuf::from(next_arg_value(&raw, &mut i, "--input")));
            }
            "--inputs-file" => {
                inputs_file = Some(PathBuf::from(next_arg_value(&raw, &mut i, "--inputs-file")));
            }
            "--rustc-env-file" => {
                rustc_env_file = Some(PathBuf::from(next_arg_value(
                    &raw,
                    &mut i,
                    "--rustc-env-file",
                )));
            }
            "--rustc-env-files-list" => {
                rustc_env_files_list = Some(PathBuf::from(next_arg_value(
                    &raw,
                    &mut i,
                    "--rustc-env-files-list",
                )));
            }
            "--cargo-mutants" => {
                cargo_mutants = Some(PathBuf::from(next_arg_value(
                    &raw,
                    &mut i,
                    "--cargo-mutants",
                )));
            }
            "--cargo" => {
                cargo = Some(PathBuf::from(next_arg_value(&raw, &mut i, "--cargo")));
            }
            "--mutants-config" => {
                mutants_config = Some(PathBuf::from(next_arg_value(
                    &raw,
                    &mut i,
                    "--mutants-config",
                )));
            }
            "--allow-survivors" => {
                allow_survivors = true;
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args {
        rustc: rustc.unwrap_or_else(|| {
            eprintln!("--rustc is required");
            std::process::exit(1);
        }),
        params_file: params_file.unwrap_or_else(|| {
            eprintln!("--params is required");
            std::process::exit(1);
        }),
        crate_root: crate_root.unwrap_or_else(|| {
            eprintln!("--crate-root is required");
            std::process::exit(1);
        }),
        sources_file,
        sources,
        inputs_file,
        inputs,
        rustc_env_file,
        rustc_env_files_list,
        cargo,
        cargo_mutants,
        mutants_config,
        allow_survivors,
    }
}

/// Counter for unique temp directories
static MUTANT_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunOutcome {
    Passed,
    CompileFailed(String),
    TestsFailed(String),
}

#[derive(Debug, Clone)]
struct FileMutant {
    source_path: PathBuf,
    name: String,
    mutated_source: String,
    diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MutantStatus {
    Caught,
    Survived,
}

#[derive(Debug, Clone)]
struct MutantResult {
    name: String,
    diff: String,
    status: MutantStatus,
}

#[derive(Debug, Clone)]
struct CampaignReport {
    results: Vec<MutantResult>,
}

impl CampaignReport {
    fn total(&self) -> usize {
        self.results.len()
    }

    fn caught(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.status == MutantStatus::Caught)
            .count()
    }

    fn survived(&self) -> impl Iterator<Item = &MutantResult> {
        self.results
            .iter()
            .filter(|result| result.status == MutantStatus::Survived)
    }
}

fn format_process_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => "no output".to_string(),
        (false, true) => format!("stdout:\n{}", stdout),
        (true, false) => format!("stderr:\n{}", stderr),
        (false, false) => format!("stdout:\n{}\nstderr:\n{}", stdout, stderr),
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        if seen.insert(path.clone()) {
            deduped.push(path);
        }
    }
    deduped
}

fn read_path_list_file(path: &Path) -> Result<Vec<PathBuf>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read list file {}: {}", path.display(), e))?;
    let mut paths = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            paths.push(PathBuf::from(trimmed));
        }
    }
    Ok(paths)
}

fn expand_pwd_placeholders(value: &str, cwd: &Path) -> String {
    value.replace("${pwd}", &cwd.display().to_string())
}

fn read_source_paths(args: &Args) -> Result<Vec<PathBuf>, String> {
    let mut source_paths = args.sources.clone();
    if let Some(sources_file) = &args.sources_file {
        source_paths.extend(read_path_list_file(sources_file)?);
    }

    if source_paths.is_empty() {
        return Err("No sources provided; pass --source or --sources-file".to_string());
    }

    Ok(dedupe_paths(source_paths))
}

fn read_input_paths(args: &Args, source_paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut input_paths = args.inputs.clone();
    if let Some(inputs_file) = &args.inputs_file {
        input_paths.extend(read_path_list_file(inputs_file)?);
    }

    if input_paths.is_empty() {
        input_paths = source_paths.to_vec();
    }

    Ok(dedupe_paths(input_paths))
}

fn parse_env_entry(
    line: &str,
    source: &Path,
    line_num: usize,
    cwd: &Path,
) -> Result<Option<(String, String)>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let (key, value) = trimmed.split_once('=').ok_or_else(|| {
        format!(
            "Invalid rustc env entry in {}:{}: expected KEY=VALUE",
            source.display(),
            line_num
        )
    })?;
    if key.is_empty() {
        return Err(format!(
            "Invalid rustc env entry in {}:{}: empty key",
            source.display(),
            line_num
        ));
    }
    let expanded = value.replace("${pwd}", &cwd.display().to_string());
    Ok(Some((key.to_string(), expanded)))
}

fn load_rustc_env(args: &Args) -> Result<HashMap<String, String>, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to read current directory for env expansion: {}", e))?;
    let mut env = HashMap::new();

    let mut env_files = Vec::new();
    if let Some(path) = &args.rustc_env_file {
        env_files.push(path.clone());
    }
    if let Some(list_file) = &args.rustc_env_files_list {
        env_files.extend(read_path_list_file(list_file)?);
    }

    for env_file in env_files {
        let content = fs::read_to_string(&env_file).map_err(|e| {
            format!(
                "Failed to read rustc env file {}: {}",
                env_file.display(),
                e
            )
        })?;
        for (line_num, line) in content.lines().enumerate() {
            if let Some((key, value)) = parse_env_entry(line, &env_file, line_num + 1, &cwd)? {
                env.insert(key, value);
            }
        }
    }

    Ok(env)
}

fn load_rustc_params(params_file: &Path) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to read current directory for param expansion: {}", e))?;
    let params_content = fs::read_to_string(params_file)
        .map_err(|e| format!("Failed to read params file {}: {}", params_file.display(), e))?;

    let mut params = Vec::new();
    for line in params_content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            params.push(expand_pwd_placeholders(trimmed, &cwd));
        }
    }
    Ok(params)
}

fn render_compile_params(params: &[String], mutated_root: &Path) -> Vec<String> {
    let mut rendered = Vec::with_capacity(params.len());
    for param in params {
        if param == CRATE_ROOT_PLACEHOLDER {
            rendered.push(mutated_root.display().to_string());
        } else {
            rendered.push(param.clone());
        }
    }
    rendered
}

fn read_input_files(input_paths: &[PathBuf]) -> Result<HashMap<PathBuf, Vec<u8>>, String> {
    let mut input_files = HashMap::new();
    for input_path in input_paths {
        let bytes = fs::read(input_path)
            .map_err(|e| format!("Failed to read input {}: {}", input_path.display(), e))?;
        input_files.insert(input_path.clone(), bytes);
    }
    Ok(input_files)
}

fn preferred_temp_dir() -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .or_else(|| std::env::var_os("TMP"))
        .or_else(|| std::env::var_os("TEMP"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

fn create_unique_temp_dir() -> Result<PathBuf, String> {
    let pid = std::process::id();
    let base = preferred_temp_dir();
    for attempt in 0..1000usize {
        let n = MUTANT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = base.join(format!("rust_mutant_{}_{}_{}", pid, n, attempt));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Failed to create temp dir {}: {}",
                    candidate.display(),
                    e
                ));
            }
        }
    }

    Err("Failed to allocate unique temp dir after 1000 attempts".to_string())
}

fn parse_hunk_old_start(line: &str) -> Option<usize> {
    let at = line.find("@@")?;
    let rest = &line[at + 2..];
    let dash = rest.find('-')?;
    let after_dash = &rest[dash + 1..];
    let digits: String = after_dash
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

fn strip_trailing_newline(line: &str) -> &str {
    line.strip_suffix('\n').unwrap_or(line)
}

fn apply_unified_diff(original: &str, diff: &str) -> Result<String, String> {
    let original_lines: Vec<&str> = original.split_inclusive('\n').collect();
    let mut output = String::new();
    let mut original_index = 0usize;

    let mut lines = diff.lines().peekable();
    let mut saw_hunk = false;

    while let Some(line) = lines.next() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if !line.starts_with("@@") {
            continue;
        }

        saw_hunk = true;
        let old_start = parse_hunk_old_start(line)
            .ok_or_else(|| format!("Invalid unified diff hunk header: {}", line))?;
        let target_index = old_start.saturating_sub(1);

        while original_index < target_index && original_index < original_lines.len() {
            output.push_str(original_lines[original_index]);
            original_index += 1;
        }

        while let Some(next) = lines.peek().copied() {
            if next.starts_with("@@") {
                break;
            }
            let patch_line = lines.next().expect("peeked item should be available");
            if patch_line == "\\ No newline at end of file" {
                continue;
            }
            let (prefix, content) = patch_line.split_at(1);
            let no_newline = matches!(lines.peek(), Some(&"\\ No newline at end of file"));
            if no_newline {
                lines.next();
            }

            match prefix {
                " " => {
                    let original_line = original_lines.get(original_index).ok_or_else(|| {
                        "Unified diff context line exceeded source length".to_string()
                    })?;
                    if strip_trailing_newline(original_line) != content {
                        return Err("Unified diff context line does not match source".to_string());
                    }
                    output.push_str(original_line);
                    original_index += 1;
                }
                "-" => {
                    let original_line = original_lines.get(original_index).ok_or_else(|| {
                        "Unified diff deletion line exceeded source length".to_string()
                    })?;
                    if strip_trailing_newline(original_line) != content {
                        return Err("Unified diff deletion line does not match source".to_string());
                    }
                    original_index += 1;
                }
                "+" => {
                    output.push_str(content);
                    if !no_newline {
                        output.push('\n');
                    }
                }
                _ => {
                    return Err(format!("Unsupported unified diff line prefix '{}'", prefix));
                }
            }
        }
    }

    if !saw_hunk {
        return Err("No unified diff hunks were found".to_string());
    }

    while original_index < original_lines.len() {
        output.push_str(original_lines[original_index]);
        original_index += 1;
    }

    Ok(output)
}

#[derive(Debug, Clone)]
struct CargoJsonMutant {
    name: String,
    diff: String,
}

fn collect_json_mutants_from_value(value: &Value, out: &mut Vec<CargoJsonMutant>) {
    match value {
        Value::Object(map) => {
            let diff = map.get("diff").and_then(Value::as_str);
            if let Some(diff) = diff {
                let name = map
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        map.get("genre")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or_else(|| "cargo-mutants mutant".to_string());
                out.push(CargoJsonMutant {
                    name,
                    diff: diff.to_owned(),
                });
            }
            for nested in map.values() {
                collect_json_mutants_from_value(nested, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json_mutants_from_value(item, out);
            }
        }
        _ => {}
    }
}

// Empty mutant lists are valid and mean "no mutations generated".
fn parse_cargo_mutants_json(output: &str) -> Result<Vec<CargoJsonMutant>, String> {
    let mut mutants = Vec::new();
    let trimmed_output = output.trim();

    if trimmed_output.is_empty() {
        return Ok(mutants);
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed_output) {
        collect_json_mutants_from_value(&value, &mut mutants);
        return Ok(mutants);
    } else {
        let mut parsed_any_json_line = false;
        for line in trimmed_output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                parsed_any_json_line = true;
                collect_json_mutants_from_value(&value, &mut mutants);
            }
        }
        if !parsed_any_json_line {
            return Err("Failed to parse cargo-mutants JSON output".to_string());
        }
    }

    Ok(mutants)
}

fn subprocess_base_env() -> Result<HashMap<OsString, OsString>, String> {
    let mut env = HashMap::new();
    for key in PASSTHROUGH_ENV_VARS {
        if let Some(value) = std::env::var_os(key) {
            env.insert(OsString::from(key), value);
        }
    }

    if let Some(test_tmpdir) = std::env::var_os("TEST_TMPDIR") {
        let test_tmpdir_path = PathBuf::from(&test_tmpdir);
        fs::create_dir_all(&test_tmpdir_path).map_err(|e| {
            format!(
                "Failed to create TEST_TMPDIR {}: {}",
                test_tmpdir_path.display(),
                e
            )
        })?;

        env.insert(OsString::from("HOME"), test_tmpdir.clone());
        env.insert(OsString::from("TMPDIR"), test_tmpdir.clone());
        env.insert(OsString::from("TMP"), test_tmpdir.clone());
        env.insert(OsString::from("TEMP"), test_tmpdir.clone());
        if cfg!(windows) {
            env.insert(OsString::from("USERPROFILE"), test_tmpdir.clone());
        }

        let cargo_home = test_tmpdir_path.join("cargo-home");
        fs::create_dir_all(&cargo_home).map_err(|e| {
            format!(
                "Failed to create isolated cargo home {}: {}",
                cargo_home.display(),
                e
            )
        })?;
        env.insert(OsString::from("CARGO_HOME"), cargo_home.into_os_string());
    }

    Ok(env)
}

fn configure_command_env<'a>(
    command: &'a mut Command,
    base_env: &HashMap<OsString, OsString>,
) -> &'a mut Command {
    command.env_clear();
    for (key, value) in base_env {
        command.env(key, value);
    }
    command
}

fn cargo_mutants_available(cargo_mutants: &Path, base_env: &HashMap<OsString, OsString>) -> bool {
    let mut command = Command::new(cargo_mutants);
    match configure_command_env(&mut command, base_env)
        .arg("mutants")
        .arg("--version")
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn collect_file_mutants_with_cargo_mutants(
    cargo_mutants: &Path,
    cargo: &Path,
    mutants_config: Option<&Path>,
    sources: &HashMap<PathBuf, String>,
    base_env: &HashMap<OsString, OsString>,
) -> Result<Vec<FileMutant>, String> {
    let mut source_paths: Vec<PathBuf> = sources.keys().cloned().collect();
    source_paths.sort();

    let mut file_mutants = Vec::new();
    for source_path in source_paths {
        let source = sources
            .get(&source_path)
            .ok_or_else(|| format!("Missing source content for {}", source_path.display()))?;

        let mut command = Command::new(cargo_mutants);
        configure_command_env(&mut command, base_env)
            .env("CARGO", cargo)
            .arg("mutants")
            .arg("--list")
            .arg("--json")
            .arg("--diff")
            .arg("--Zmutate-file")
            .arg(&source_path);
        if let Some(config) = mutants_config {
            command.arg("--config").arg(config);
        }

        let output = command.output().map_err(|e| {
            format!(
                "Failed to run cargo-mutants --Zmutate-file for {}: {}",
                source_path.display(),
                e
            )
        })?;

        if !output.status.success() {
            return Err(format!(
                "cargo-mutants --Zmutate-file failed for {}:\n{}",
                source_path.display(),
                format_process_output(&output)
            ));
        }

        let json = String::from_utf8_lossy(&output.stdout);
        let json_mutants = parse_cargo_mutants_json(&json)?;
        for json_mutant in json_mutants {
            let mutated_source = apply_unified_diff(source, &json_mutant.diff).map_err(|e| {
                format!(
                    "Failed to apply cargo-mutants diff for {} ({}): {}",
                    source_path.display(),
                    json_mutant.name,
                    e
                )
            })?;
            file_mutants.push(FileMutant {
                source_path: source_path.clone(),
                name: json_mutant.name,
                mutated_source,
                diff: json_mutant.diff,
            });
        }
    }

    Ok(file_mutants)
}

fn collect_file_mutants(
    sources: &HashMap<PathBuf, String>,
    cargo_mutants: &Path,
    cargo: &Path,
    mutants_config: Option<&Path>,
    base_env: &HashMap<OsString, OsString>,
) -> Result<Vec<FileMutant>, String> {
    if !cargo_mutants_available(cargo_mutants, base_env) {
        return Err(format!(
            "cargo-mutants executable was not available from '{}'.",
            cargo_mutants.display()
        ));
    }
    println!("Generating mutants with cargo-mutants...");
    collect_file_mutants_with_cargo_mutants(cargo_mutants, cargo, mutants_config, sources, base_env)
}

fn mutant_name(index: usize, file_mutant: &FileMutant) -> String {
    format!("mutant_{}_{}", index, file_mutant.name)
}

fn run_mutation_campaign<F>(
    file_mutants: &[FileMutant],
    mut execute_case: F,
) -> Result<CampaignReport, String>
where
    F: FnMut(Option<&FileMutant>) -> Result<RunOutcome, String>,
{
    match execute_case(None) {
        Ok(RunOutcome::Passed) => {}
        Ok(RunOutcome::CompileFailed(details)) => {
            return Err(format!("Baseline compile failed:\n{}", details));
        }
        Ok(RunOutcome::TestsFailed(details)) => {
            return Err(format!("Baseline tests failed:\n{}", details));
        }
        Err(e) => {
            return Err(format!("Baseline infrastructure error: {}", e));
        }
    }

    let mut results = Vec::with_capacity(file_mutants.len());
    for (index, file_mutant) in file_mutants.iter().enumerate() {
        let name = mutant_name(index, file_mutant);
        let outcome = match execute_case(Some(file_mutant)) {
            Ok(outcome) => outcome,
            Err(e) => {
                return Err(format!(
                    "Infrastructure error while evaluating {}: {}",
                    name, e
                ));
            }
        };
        let status = match outcome {
            RunOutcome::Passed => MutantStatus::Survived,
            RunOutcome::CompileFailed(_) | RunOutcome::TestsFailed(_) => MutantStatus::Caught,
        };

        results.push(MutantResult {
            name,
            diff: file_mutant.diff.clone(),
            status,
        });
    }

    Ok(CampaignReport { results })
}

fn compile_and_test_case(
    rustc: &Path,
    params: &[String],
    rustc_env: &HashMap<String, String>,
    crate_root: &Path,
    input_files: &HashMap<PathBuf, Vec<u8>>,
    mutation: Option<(&Path, &str)>,
    base_env: &HashMap<OsString, OsString>,
) -> Result<RunOutcome, String> {
    if let Some((source_path, _)) = mutation {
        if !input_files.contains_key(source_path) {
            return Err(format!(
                "Mutation source {} was not present in input map",
                source_path.display()
            ));
        }
    }

    let tmp_dir = create_unique_temp_dir()?;

    // Write all compile-time inputs, substituting the targeted source when a mutation is active.
    for (input_path, content) in input_files {
        let dest = tmp_dir.join(input_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
        }
        if let Some((mutation_path, mutated_source)) = mutation {
            if input_path.as_path() == mutation_path {
                fs::write(&dest, mutated_source.as_bytes())
                    .map_err(|e| format!("Failed to write mutated source: {}", e))?;
                continue;
            }
        }
        fs::write(&dest, content).map_err(|e| format!("Failed to write input: {}", e))?;
    }

    let result = (|| {
        let output_path = tmp_dir.join("test_binary");
        let mutated_root = tmp_dir.join(crate_root);
        let manifest_dir = crate_root
            .parent()
            .map(|p| tmp_dir.join(p))
            .unwrap_or_else(|| tmp_dir.clone());
        let compile_params = render_compile_params(params, &mutated_root);

        let mut compile_cmd = Command::new(rustc);
        configure_command_env(&mut compile_cmd, base_env)
            .args(&compile_params)
            .arg("-o")
            .arg(&output_path)
            .envs(rustc_env)
            .env("CARGO_MANIFEST_DIR", manifest_dir);

        let compile = compile_cmd
            .output()
            .map_err(|e| format!("Failed to run rustc: {}", e))?;

        if !compile.status.success() {
            return Ok(RunOutcome::CompileFailed(format_process_output(&compile)));
        }

        let mut test = Command::new(&output_path);
        let test = configure_command_env(&mut test, base_env)
            .arg("--test-threads=1")
            .output()
            .map_err(|e| format!("Failed to run test: {}", e))?;

        if !test.status.success() {
            return Ok(RunOutcome::TestsFailed(format_process_output(&test)));
        }

        Ok(RunOutcome::Passed)
    })();

    let _ = fs::remove_dir_all(&tmp_dir);

    result
}

fn main() {
    let args = parse_args();

    let params = match load_rustc_params(&args.params_file) {
        Ok(params) => params,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Read mutation source files.
    let mut sources: HashMap<PathBuf, String> = HashMap::new();
    let source_paths = match read_source_paths(&args) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let input_paths = match read_input_paths(&args, &source_paths) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    for src in &source_paths {
        let content = fs::read_to_string(src)
            .unwrap_or_else(|e| panic!("Failed to read mutation source {}: {}", src.display(), e));
        sources.insert(src.clone(), content);
    }
    let input_files = match read_input_files(&input_paths) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let rustc_env = match load_rustc_env(&args) {
        Ok(env) => env,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let subprocess_env = match subprocess_base_env() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    if !sources.contains_key(&args.crate_root) {
        panic!("Crate root {} not in sources", args.crate_root.display());
    }

    let cwd = std::env::current_dir().ok();
    let cargo = args.cargo.clone().unwrap_or_else(|| PathBuf::from("cargo"));
    let cargo = if cargo.is_absolute() {
        cargo
    } else if let Some(cwd) = &cwd {
        cwd.join(cargo)
    } else {
        PathBuf::from("cargo")
    };

    let cargo_mutants = args
        .cargo_mutants
        .clone()
        .or_else(|| args.cargo.clone())
        .unwrap_or_else(|| PathBuf::from("cargo-mutants"));
    let cargo_mutants = if cargo_mutants.is_absolute() {
        cargo_mutants
    } else if let Some(cwd) = &cwd {
        cwd.join(cargo_mutants)
    } else {
        PathBuf::from("cargo-mutants")
    };
    let mutants_config = args.mutants_config.clone().map(|config| {
        if config.is_absolute() {
            config
        } else if let Some(cwd) = &cwd {
            cwd.join(config)
        } else {
            config
        }
    });
    let file_mutants =
        match collect_file_mutants(
            &sources,
            &cargo_mutants,
            &cargo,
            mutants_config.as_deref(),
            &subprocess_env,
        ) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to generate mutants: {}", e);
                std::process::exit(1);
            }
        };

    println!("Running baseline compile+test...");
    let report = match run_mutation_campaign(&file_mutants, |mutation| match mutation {
        None => compile_and_test_case(
            &args.rustc,
            &params,
            &rustc_env,
            &args.crate_root,
            &input_files,
            None,
            &subprocess_env,
        ),
        Some(file_mutant) => compile_and_test_case(
            &args.rustc,
            &params,
            &rustc_env,
            &args.crate_root,
            &input_files,
            Some((&file_mutant.source_path, &file_mutant.mutated_source)),
            &subprocess_env,
        ),
    }) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    if report.total() == 0 {
        println!("No mutations generated.");
        return;
    }

    println!("Generated {} mutants", report.total());
    for result in &report.results {
        let status = match result.status {
            MutantStatus::Caught => "CAUGHT",
            MutantStatus::Survived => "SURVIVED",
        };
        println!("  {} - {}", result.name, status);
    }

    let total = report.total();
    let caught = report.caught();
    let survived = total - caught;

    println!();
    println!("Mutation Testing Summary");
    println!("========================");
    println!(
        "Total: {}  Caught: {} ({:.0}%)  Survived: {} ({:.0}%)",
        total,
        caught,
        caught as f64 / total as f64 * 100.0,
        survived,
        survived as f64 / total as f64 * 100.0,
    );

    let survived_mutants: Vec<&MutantResult> = report.survived().collect();
    let survived_mutants_count = survived_mutants.len();
    if !survived_mutants.is_empty() {
        println!();
        println!("Survived mutations (test gaps):");
        for result in &survived_mutants {
            println!("  - {}", result.name);
            for line in result.diff.lines() {
                println!("    {}", line);
            }
            println!();
        }
    }

    if survived_mutants_count > 0 && !args.allow_survivors {
        eprintln!(
            "Mutation testing failed: {} mutant(s) survived. \
Set `allow_survivors = True` on `rust_mutation_test` to opt out temporarily.",
            survived_mutants_count
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args() -> Args {
        Args {
            rustc: PathBuf::from("rustc"),
            params_file: PathBuf::from("params"),
            crate_root: PathBuf::from("src/lib.rs"),
            sources_file: None,
            sources: Vec::new(),
            inputs_file: None,
            inputs: Vec::new(),
            rustc_env_file: None,
            rustc_env_files_list: None,
            cargo: None,
            cargo_mutants: None,
            mutants_config: None,
            allow_survivors: false,
        }
    }

    fn sample_file_mutant(source_path: &str) -> FileMutant {
        FileMutant {
            source_path: PathBuf::from(source_path),
            name: "replace > with <=".to_string(),
            mutated_source: "fn f(x: i32) -> bool { x <= 0 }\n".to_string(),
            diff: "@@ -1,1 +1,1 @@\n-fn f(x: i32) -> bool { x > 0 }\n+fn f(x: i32) -> bool { x <= 0 }\n"
                .to_string(),
        }
    }

    #[test]
    fn read_input_paths_defaults_to_source_paths() {
        let mut args = make_args();
        let source_paths = vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/math.rs")];
        args.sources = source_paths.clone();

        let input_paths =
            read_input_paths(&args, &source_paths).expect("input path reading should succeed");
        assert_eq!(input_paths, source_paths);
    }

    #[test]
    fn load_rustc_env_reads_files_and_expands_pwd() {
        let tmp_dir = create_unique_temp_dir().expect("temp dir should be created");
        let cwd = std::env::current_dir().expect("cwd should be available");
        let direct_env = tmp_dir.join("direct.env");
        let from_list_env = tmp_dir.join("from_list.env");
        let env_files_list = tmp_dir.join("env_files.list");

        fs::write(
            &direct_env,
            "RUSTC_DIRECT=from_direct\nPWD_VALUE=${pwd}/x\n",
        )
        .expect("direct env file should be written");
        fs::write(&from_list_env, "RUSTC_FROM_LIST=from_list\n")
            .expect("list env file should be written");
        fs::write(&env_files_list, format!("{}\n", from_list_env.display()))
            .expect("env files list should be written");

        let mut args = make_args();
        args.rustc_env_file = Some(direct_env);
        args.rustc_env_files_list = Some(env_files_list);

        let rustc_env = load_rustc_env(&args).expect("rustc env should load");
        assert_eq!(
            rustc_env.get("RUSTC_DIRECT"),
            Some(&"from_direct".to_string())
        );
        assert_eq!(
            rustc_env.get("RUSTC_FROM_LIST"),
            Some(&"from_list".to_string())
        );
        assert_eq!(
            rustc_env.get("PWD_VALUE"),
            Some(&format!("{}/x", cwd.display()))
        );

        let _ = fs::remove_dir_all(tmp_dir);
    }

    #[test]
    fn load_rustc_params_expands_pwd_placeholders() {
        let tmp_dir = create_unique_temp_dir().expect("temp dir should be created");
        let params_file = tmp_dir.join("params");
        let cwd = std::env::current_dir().expect("cwd should be available");
        fs::write(
            &params_file,
            "__MUTATION_CRATE_ROOT__\n--cfg=test\n-Ldependency=${pwd}/external/dep\n",
        )
        .expect("params file should be written");

        let params = load_rustc_params(&params_file).expect("params should load");
        assert_eq!(params[0], "__MUTATION_CRATE_ROOT__");
        assert_eq!(params[1], "--cfg=test");
        assert_eq!(
            params[2],
            format!("-Ldependency={}/external/dep", cwd.display())
        );

        let _ = fs::remove_dir_all(tmp_dir);
    }

    #[test]
    fn render_compile_params_substitutes_crate_root_placeholder() {
        let params = vec![
            "__MUTATION_CRATE_ROOT__".to_owned(),
            "--crate-name=my_crate".to_owned(),
        ];
        let rendered = render_compile_params(&params, Path::new("/tmp/mutated/src/lib.rs"));
        assert_eq!(rendered[0], "/tmp/mutated/src/lib.rs");
        assert_eq!(rendered[1], "--crate-name=my_crate");
    }

    #[test]
    fn apply_unified_diff_replaces_content() {
        let original = "fn f(x: i32) -> bool { x > 0 }\n";
        let diff = "--- a\n+++ b\n@@ -1,1 +1,1 @@\n-fn f(x: i32) -> bool { x > 0 }\n+fn f(x: i32) -> bool { x <= 0 }\n";
        let mutated = apply_unified_diff(original, diff).expect("diff should apply");
        assert_eq!(mutated, "fn f(x: i32) -> bool { x <= 0 }\n");
    }

    #[test]
    fn parse_cargo_mutants_json_finds_mutants_array() {
        let payload = r#"[
            {"name":"replace > with <=","diff":"@@ -1,1 +1,1 @@\n-a\n+b\n"}
        ]"#;
        let parsed = parse_cargo_mutants_json(payload).expect("json should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "replace > with <=");
    }

    #[test]
    fn parse_cargo_mutants_json_accepts_entries_without_name() {
        let payload = r#"[
            {"genre":"FnValue","diff":"@@ -1,1 +1,1 @@\n-a\n+b\n"}
        ]"#;
        let parsed = parse_cargo_mutants_json(payload).expect("json should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "FnValue");
    }

    #[test]
    fn parse_cargo_mutants_json_accepts_empty_mutant_list() {
        let parsed = parse_cargo_mutants_json("[]").expect("empty mutant list should parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_cargo_mutants_json_rejects_non_json_output() {
        let err =
            parse_cargo_mutants_json("not-json").expect_err("non-json output should be rejected");
        assert!(err.contains("Failed to parse cargo-mutants JSON output"));
    }

    #[test]
    fn baseline_compile_failure_causes_campaign_failure() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let result = run_mutation_campaign(&file_mutants, |_| {
            Ok(RunOutcome::CompileFailed("compile error".to_string()))
        });
        let err = result.expect_err("campaign should fail on baseline compile failure");
        assert!(err.contains("Baseline compile failed"));
    }

    #[test]
    fn baseline_test_failure_causes_campaign_failure() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let result = run_mutation_campaign(&file_mutants, |_| {
            Ok(RunOutcome::TestsFailed("test error".to_string()))
        });
        let err = result.expect_err("campaign should fail on baseline test failure");
        assert!(err.contains("Baseline tests failed"));
    }

    #[test]
    fn mutant_compile_failure_is_counted_as_caught() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let mut outcomes = vec![
            Ok(RunOutcome::Passed),
            Ok(RunOutcome::CompileFailed("compile error".to_string())),
        ]
        .into_iter();

        let report = run_mutation_campaign(&file_mutants, |_| {
            outcomes.next().expect("expected outcome")
        })
        .expect("campaign should succeed");

        assert_eq!(report.total(), 1);
        assert_eq!(report.caught(), 1);
        assert_eq!(report.results[0].status, MutantStatus::Caught);
    }

    #[test]
    fn mutant_pass_is_counted_as_survived() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let mut outcomes = vec![Ok(RunOutcome::Passed), Ok(RunOutcome::Passed)].into_iter();

        let report = run_mutation_campaign(&file_mutants, |_| {
            outcomes.next().expect("expected outcome")
        })
        .expect("campaign should succeed");

        assert_eq!(report.total(), 1);
        assert_eq!(report.caught(), 0);
        assert_eq!(report.results[0].status, MutantStatus::Survived);
    }

    #[test]
    fn baseline_infrastructure_error_causes_campaign_failure() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let result = run_mutation_campaign(&file_mutants, |_| Err("spawn failure".to_string()));
        let err = result.expect_err("campaign should fail on baseline infrastructure errors");
        assert!(err.contains("Baseline infrastructure error"));
    }

    #[test]
    fn infrastructure_error_causes_campaign_failure() {
        let file_mutants = vec![sample_file_mutant("src/lib.rs")];
        let mut outcomes = vec![Ok(RunOutcome::Passed), Err("io failure".to_string())].into_iter();

        let result = run_mutation_campaign(&file_mutants, |_| {
            outcomes.next().expect("expected outcome")
        });
        let err = result.expect_err("campaign should fail on infrastructure errors");
        assert!(err.contains("Infrastructure error while evaluating"));
    }
}
