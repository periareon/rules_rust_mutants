#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

output="$(./test/rustc_env_files_example_mutation_test 2>&1)"
status=$?

if [[ "${status}" -ne 0 ]]; then
  echo "Expected rustc-env-files mutation target to succeed, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "Mutation Testing Summary" <<<"${output}"; then
  echo "Expected mutation summary output."
  echo "${output}"
  exit 1
fi
