#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

set +e
output="$(./test/runtime_data_example_mutation_test 2>&1)"
status=$?
set -e

if [[ "${status}" -ne 0 ]]; then
  echo "Expected runtime-data mutation target to succeed, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "Mutation Testing Summary" <<<"${output}"; then
  echo "Expected mutation summary output."
  echo "${output}"
  exit 1
fi
