"""# rules_rust_mutation

Mutation testing for Rust crates built with rules_rust.

`rust_mutation_test` enumerates source-level mutations for a `rust_library`,
compiles each mutant with the same rustc configuration as `rust_test`, and
runs the crate's inline `#[cfg(test)]` tests against each mutant.

## Rules

- [rust_mutation_test](#rust_mutation_test)

## Setup

### Bzlmod

Add the following to your `MODULE.bazel` file:

```python
bazel_dep(name = "rules_rust_mutation", version = "{SEE_RELEASE_NOTES}")
```

### WORKSPACE

If you're using `WORKSPACE`, load repositories with:

```python
load("@rules_rust_mutation//:repositories.bzl", "rust_mutation_dependencies")

rust_mutation_dependencies()
```

### Usage

```python
load("@rules_rust_mutation//:defs.bzl", "rust_mutation_test")
load("@rules_rust//rust:defs.bzl", "rust_library")

rust_library(
    name = "my_lib",
    srcs = ["lib.rs"],
    edition = "2021",
)

rust_mutation_test(
    name = "my_lib_mutation_test",
    crate = ":my_lib",
)
```

Run with:
```
bazel test //:my_lib_mutation_test --test_output=all
```

## Behavior Notes

- Mutation generation uses `cargo-mutants` JSON output.
- Mutation enumeration uses
  `cargo mutants --list --json --diff --Zmutate-file ...`.
- Rustc params are generated from rules_rust's canonical argument-construction
  pipeline (`collect_inputs` + `construct_arguments`).
- `mutants_config` is forwarded as `cargo mutants --config <path>`.
- By default, survived mutants fail the Bazel test target.
- `allow_survivors = True` reports survivors without failing.
- If mutation generation produces zero mutants, the Bazel target succeeds and
  prints `No mutations generated.`.

## Tooling

- A hermetic `cargo-mutants` binary is built from source by this extension.
- A Cargo binary from the active Rust toolchain is used for `cargo-mutants`
  internals.

---
---
"""

load(
    "//private:mutation_test.bzl",
    _rust_mutation_test = "rust_mutation_test",
)

rust_mutation_test = _rust_mutation_test
