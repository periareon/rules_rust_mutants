#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

assert_has_line() {
  local file="$1"
  local expected="$2"
  if ! grep -Fxq -- "${expected}" "${file}"; then
    echo "Expected ${file} to contain line: ${expected}"
    cat "${file}"
    exit 1
  fi
}

assert_lacks_line() {
  local file="$1"
  local unexpected="$2"
  if grep -Fxq -- "${unexpected}" "${file}"; then
    echo "Did not expect ${file} to contain line: ${unexpected}"
    cat "${file}"
    exit 1
  fi
}

assert_has_substring() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq -- "${expected}" "${file}"; then
    echo "Expected ${file} to contain: ${expected}"
    cat "${file}"
    exit 1
  fi
}

example_args="test/example_mutation_test.mutation_args"
rustc_env_args="test/rustc_env_example_mutation_test.mutation_args"
mutants_args="test/mutants_config_example_mutation_test.mutation_args"
rustc_env_file="test/rustc_env_example_mutation_test.rustc_env"
rustc_env_files_list="test/rustc_env_files_example_mutation_test.rustc_env_files"

for file in \
  "${example_args}" \
  "${rustc_env_args}" \
  "${mutants_args}" \
  "${rustc_env_file}" \
  "${rustc_env_files_list}"
do
  if [[ ! -f "${file}" ]]; then
    echo "Expected file ${file} to exist."
    exit 1
  fi
done

assert_has_line "${example_args}" "--params"
assert_has_line "${example_args}" "test/example_mutation_test.rustc_params"
assert_has_line "${example_args}" "--rustc-env-file"
assert_has_line "${example_args}" "test/example_mutation_test.rustc_env"
assert_has_line "${example_args}" "--rustc-env-files-list"
assert_has_line "${example_args}" "test/example_mutation_test.rustc_env_files"
assert_lacks_line "${example_args}" "--allow-survivors"
assert_lacks_line "${example_args}" "--mutants-config"

assert_has_line "${rustc_env_args}" "--allow-survivors"
assert_has_line "${mutants_args}" "--mutants-config"
assert_has_substring "${mutants_args}" "mutants_config/exclude_all.toml"
assert_has_line "${rustc_env_file}" "MUTATION_EXPECTED_TOKEN=ok"
assert_has_substring "${rustc_env_files_list}" "rustc_env_file.env"
