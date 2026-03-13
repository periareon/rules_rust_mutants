#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

set +e
output="$(./test/failing_baseline_mutation_test 2>&1)"
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
  echo "Expected failing baseline mutation target to fail, but it succeeded."
  echo "${output}"
  exit 1
fi

if ! grep -q "Baseline tests failed" <<<"${output}"; then
  echo "Expected baseline test failure message in output."
  echo "${output}"
  exit 1
fi
