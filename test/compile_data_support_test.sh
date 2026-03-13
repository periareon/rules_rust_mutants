#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

output="$(./test/compile_data_example_mutation_test 2>&1)"
status=$?

if [[ "${status}" -ne 0 ]]; then
  echo "Expected compile-data mutation target to succeed, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "Mutation Testing Summary" <<<"${output}"; then
  echo "Expected mutation summary output."
  echo "${output}"
  exit 1
fi
