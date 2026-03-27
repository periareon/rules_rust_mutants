"""Mutation testing rule for Rust crates.

Generates mutations of a Rust library's source code, compiles each mutant
using the same rustc argument construction pipeline as `rust_test`, and
reports which mutations are caught by the test suite.
"""

# buildifier: disable=bzl-visibility
load("@rules_rust//rust/private:common.bzl", "rust_common")

# buildifier: disable=bzl-visibility
load("@rules_rust//rust/private:rust.bzl", "RUSTC_ATTRS")

# buildifier: disable=bzl-visibility
load(
    "@rules_rust//rust/private:rustc.bzl",
    "collect_deps",
    "collect_inputs",
    "construct_arguments",
)

# buildifier: disable=bzl-visibility
load(
    "@rules_rust//rust/private:utils.bzl",
    "determine_output_hash",
    "find_cc_toolchain",
    "find_toolchain",
)

# The runner uses this token in the params file and replaces it with the
# crate-root path inside the per-mutant temp directory.
_CRATE_ROOT_PLACEHOLDER = "__MUTATION_CRATE_ROOT__"

def _dedupe_files(files):
    seen = {}
    unique_files = []
    for f in files:
        key = f.short_path
        if key in seen:
            continue
        seen[key] = True
        unique_files.append(f)
    return unique_files

def _collect_root_prefixes(files):
    """Collect root prefixes that should be stripped from generated rustc args.

    `construct_arguments` emits file paths rooted at Bazel execroot locations.
    The mutation runner executes from runfiles, so those prefixes must be
    removed to recover runfiles-relative paths (`short_path` style).
    """
    seen = {}
    prefixes = []
    for f in files:
        root = f.root.path
        if root in seen:
            continue
        seen[root] = True
        prefixes.append(root)
    return prefixes

def _resolve_crate_under_test(ctx):
    """Resolve CrateInfo from a `rust_library` or wrapped test crate target."""
    if rust_common.crate_info in ctx.attr.crate:
        return ctx.attr.crate[rust_common.crate_info]
    if rust_common.test_crate_info in ctx.attr.crate:
        return ctx.attr.crate[rust_common.test_crate_info].crate
    fail("Target {} does not provide CrateInfo".format(ctx.attr.crate.label))

def _build_mutation_test_crate(ctx, crate, toolchain):
    """Create a test-mode CrateInfo used only for canonical arg generation.

    This mirrors the crate-level `rust_test(crate = ...)` shape closely enough
    for `collect_inputs + construct_arguments` to produce the same rustc
    compilation model while still allowing the runtime runner to substitute
    the crate root and binary output per mutant.
    """
    output_hash = determine_output_hash(crate.root, ctx.label)
    placeholder_output = ctx.actions.declare_file(
        "mutation-{hash}/{name}{ext}".format(
            hash = output_hash,
            name = ctx.label.name,
            ext = toolchain.binary_ext,
        ),
    )

    # `collect_inputs` may thread this output through transitive input sets.
    # Materialize a file so Bazel never sees an unresolved declared artifact.
    ctx.actions.write(placeholder_output, "")

    return rust_common.create_crate_info(
        name = crate.name,
        type = "bin",
        root = crate.root,
        srcs = crate.srcs,
        deps = crate.deps,
        proc_macro_deps = crate.proc_macro_deps,
        aliases = crate.aliases,
        output = placeholder_output,
        edition = crate.edition,
        rustc_env = dict(getattr(crate, "rustc_env", {})),
        rustc_env_files = getattr(crate, "rustc_env_files", []),
        is_test = True,
        data = getattr(crate, "data", depset([])),
        compile_data = crate.compile_data,
        compile_data_targets = crate.compile_data_targets,
        wrapped_crate_type = crate.type,
        owner = ctx.label,
        cfgs = getattr(crate, "cfgs", []),
    )

def _emit_rustc_params_file(ctx, rustc_flags, build_flags_files, compile_inputs, strip_prefixes):
    """Materialize canonical rustc flags into a runtime-ready params file.

    The writer action expands Bazel-generated parameter files and appends
    build-script rustc flag files (`--arg-file` inputs in process_wrapper mode),
    then normalizes paths to runfiles-relative form.
    """
    params_file = ctx.actions.declare_file(ctx.label.name + ".rustc_params")

    writer_args = ctx.actions.args()
    writer_args.add("--output={}".format(params_file.path))
    writer_args.add("--crate-root-placeholder={}".format(_CRATE_ROOT_PLACEHOLDER))
    writer_args.add_all(strip_prefixes, format_each = "--strip-prefix=%s")
    writer_args.add_all(build_flags_files, format_each = "--build-flags-file=%s")
    writer_args.add("--")

    writer_inputs = depset(transitive = [compile_inputs, build_flags_files])
    ctx.actions.run(
        executable = ctx.executable._mutation_args_writer,
        inputs = writer_inputs,
        outputs = [params_file],
        arguments = [writer_args, rustc_flags],
        mnemonic = "RustMutationWriteParams",
        progress_message = "Generating mutation rustc params for %{label}",
        toolchain = "@rules_rust//rust:toolchain_type",
    )

    return params_file

def _rust_mutation_test_impl(ctx):
    """Implementation of the rust_mutation_test rule."""
    toolchain = find_toolchain(ctx)
    crate = _resolve_crate_under_test(ctx)

    # Build a test-mode crate model and feed it through the same argument
    # constructors used by core rules_rust compile actions.
    mutation_test_crate = _build_mutation_test_crate(ctx, crate, toolchain)
    dep_info, build_info, linkstamps = collect_deps(
        deps = mutation_test_crate.deps.to_list(),
        proc_macro_deps = mutation_test_crate.proc_macro_deps.to_list(),
        aliases = mutation_test_crate.aliases,
    )
    cc_toolchain, feature_configuration = find_cc_toolchain(ctx)

    compile_inputs, out_dir, build_env_files, build_flags_files, linkstamp_outs, ambiguous_libs = collect_inputs(
        ctx = ctx,
        file = ctx.file,
        files = ctx.files,
        linkstamps = linkstamps,
        toolchain = toolchain,
        cc_toolchain = cc_toolchain,
        feature_configuration = feature_configuration,
        crate_info = mutation_test_crate,
        dep_info = dep_info,
        build_info = build_info,
        lint_files = [],
    )

    rustc_args, rustc_env = construct_arguments(
        ctx = ctx,
        attr = ctx.attr,
        file = ctx.file,
        toolchain = toolchain,
        tool_path = toolchain.rustc.path,
        cc_toolchain = cc_toolchain,
        feature_configuration = feature_configuration,
        crate_info = mutation_test_crate,
        dep_info = dep_info,
        linkstamp_outs = linkstamp_outs,
        ambiguous_libs = ambiguous_libs,
        output_hash = determine_output_hash(mutation_test_crate.root, ctx.label),
        rust_flags = ["--test"],
        out_dir = out_dir,
        build_env_files = build_env_files,
        build_flags_files = build_flags_files,
        emit = ["link"],
        skip_expanding_rustc_env = True,
    )

    # Collect mutation source files and compile-time inputs.
    all_src_files = mutation_test_crate.srcs.to_list()
    if mutation_test_crate.root not in all_src_files:
        all_src_files.append(mutation_test_crate.root)
    src_files = [f for f in all_src_files if f.path.endswith(".rs")]
    input_files = _dedupe_files(all_src_files + mutation_test_crate.compile_data.to_list())

    sources_file = ctx.actions.declare_file(ctx.label.name + ".sources")
    ctx.actions.write(
        output = sources_file,
        content = "\n".join([f.short_path for f in src_files]),
    )
    inputs_file = ctx.actions.declare_file(ctx.label.name + ".inputs")
    ctx.actions.write(
        output = inputs_file,
        content = "\n".join([f.short_path for f in input_files]),
    )

    # Persist rustc environment from canonical argument construction. This
    # includes toolchain defaults and crate-level rustc_env values.
    rustc_env_file = ctx.actions.declare_file(ctx.label.name + ".rustc_env")
    rustc_env_entries = sorted([
        "{}={}".format(key, value)
        for key, value in rustc_env.items()
    ])
    ctx.actions.write(
        output = rustc_env_file,
        content = "\n".join(rustc_env_entries),
    )

    # Build-script and crate rustc env files are loaded at runtime by the
    # runner to preserve rustc env parity with standard rules_rust actions.
    rustc_env_files_list = ctx.actions.declare_file(ctx.label.name + ".rustc_env_files")
    ctx.actions.write(
        output = rustc_env_files_list,
        content = "\n".join([f.short_path for f in build_env_files]),
    )

    compile_inputs_list = compile_inputs.to_list()
    strip_prefixes = _collect_root_prefixes(
        compile_inputs_list + [
            toolchain.rustc,
            toolchain.cargo,
        ],
    )
    params_file = _emit_rustc_params_file(
        ctx,
        rustc_args.rustc_flags,
        build_flags_files,
        compile_inputs,
        strip_prefixes,
    )

    # Generate the test runner wrapper.
    cargo_mutants_tool = ctx.executable._cargo_mutants
    is_windows = toolchain.rustc.basename.endswith(".exe")
    runner = ctx.actions.declare_file(ctx.label.name + (".bat" if is_windows else ""))
    runner_args = '--rustc "{rustc}" --cargo "{cargo}" --cargo-mutants "{cargo_mutants}" --params "{params}" --crate-root "{crate_root}" --sources-file "{sources_file}" --inputs-file "{inputs_file}" --rustc-env-file "{rustc_env_file}" --rustc-env-files-list "{rustc_env_files_list}"'.format(
        rustc = toolchain.rustc.short_path,
        cargo = toolchain.cargo.short_path,
        cargo_mutants = cargo_mutants_tool.short_path,
        params = params_file.short_path,
        crate_root = mutation_test_crate.root.short_path,
        sources_file = sources_file.short_path,
        inputs_file = inputs_file.short_path,
        rustc_env_file = rustc_env_file.short_path,
        rustc_env_files_list = rustc_env_files_list.short_path,
    )
    if ctx.file.mutants_config:
        runner_args += ' --mutants-config "{mutants_config}"'.format(
            mutants_config = ctx.file.mutants_config.short_path,
        )
    if ctx.attr.allow_survivors:
        runner_args += " --allow-survivors"

    if is_windows:
        content = '@echo off\r\npowershell.exe -c "if (!(Test-Path .\\external)) { New-Item -Path .\\external -ItemType SymbolicLink -Value ..\\ }" >NUL 2>NUL\r\n"{runner}" {args}\r\n'.format(
            runner = ctx.executable._mutation_runner.short_path,
            args = runner_args,
        )
    else:
        content = "#!/bin/bash\n" + "set -euo pipefail\n" + "if [[ ! -e external ]]; then ln -s ../ external; fi\n" + 'exec "{runner}" {args}\n'.format(
            runner = ctx.executable._mutation_runner.short_path,
            args = runner_args,
        )
    ctx.actions.write(runner, content, is_executable = True)

    # Collect all files needed at test time.
    runfiles_files = _dedupe_files(
        [
            params_file,
            sources_file,
            inputs_file,
            rustc_env_file,
            rustc_env_files_list,
            toolchain.rustc,
            toolchain.cargo,
            cargo_mutants_tool,
        ] + compile_inputs_list + ([ctx.file.mutants_config] if ctx.file.mutants_config else []),
    )

    runfiles = ctx.runfiles(files = runfiles_files)
    runfiles = runfiles.merge(ctx.attr._mutation_runner[DefaultInfo].default_runfiles)
    runfiles = runfiles.merge(ctx.attr._cargo_mutants[DefaultInfo].default_runfiles)
    crate_default_info = ctx.attr.crate[DefaultInfo]
    if crate_default_info.default_runfiles:
        runfiles = runfiles.merge(crate_default_info.default_runfiles)
    if crate_default_info.data_runfiles:
        runfiles = runfiles.merge(crate_default_info.data_runfiles)

    return [DefaultInfo(
        executable = runner,
        runfiles = runfiles,
    )]

rust_mutation_test = rule(
    implementation = _rust_mutation_test_impl,
    doc = """\
Mutation testing for a Rust library crate.

`rust_mutation_test`:

1. Enumerates source-level mutants with `cargo-mutants`.
2. Builds rustc params from rules_rust's canonical argument construction
   pipeline (`collect_inputs + construct_arguments`).
3. Runs baseline and per-mutant compile+test cycles against inline `#[cfg(test)]`
   tests from the crate.
4. Fails if any mutant survives (unless `allow_survivors = True`).
5. Succeeds and prints `No mutations generated.` when enumeration yields zero
   mutants.

Mutation enumeration mode:

- Uses `cargo mutants --list --json --diff --Zmutate-file ...`.

Example:
```python
load("@rules_rust_mutation//:defs.bzl", "rust_mutation_test")

rust_library(
    name = "my_lib",
    srcs = ["lib.rs"],
)

rust_mutation_test(
    name = "my_lib_mutation_test",
    crate = ":my_lib",
)
```

Run with: `bazel test //:my_lib_mutation_test --test_output=all`
""",
    attrs = {
        "allow_survivors": attr.bool(
            default = False,
            doc = "If True, survived mutants are reported but do not fail the test. " +
                  "Default is False (survivors fail).",
        ),
        "crate": attr.label(
            mandatory = True,
            doc = "The `rust_library` crate to mutation-test. " +
                  "The crate's inline `#[cfg(test)]` tests are compiled " +
                  "and run against each mutation.",
        ),
        "mutants_config": attr.label(
            allow_single_file = [".toml"],
            doc = "Optional cargo-mutants configuration file. " +
                  "It is forwarded as `cargo mutants --config <path>`. " +
                  "If all mutants are filtered out, the target succeeds and reports no mutants.",
        ),
        "_cargo_mutants": attr.label(
            default = Label("//private:cargo_mutants"),
            executable = True,
            cfg = "exec",
            allow_files = True,
        ),
        "_mutation_args_writer": attr.label(
            default = Label("//private:mutation_args_writer"),
            executable = True,
            cfg = "exec",
        ),
        "_mutation_runner": attr.label(
            default = Label("//private:runner"),
            executable = True,
            cfg = "exec",
        ),
    } | RUSTC_ATTRS,
    test = True,
    executable = True,
    fragments = ["cpp"],
    toolchains = [
        str(Label("@rules_rust//rust:toolchain_type")),
        config_common.toolchain_type("@bazel_tools//tools/cpp:toolchain_type", mandatory = False),
    ],
)
