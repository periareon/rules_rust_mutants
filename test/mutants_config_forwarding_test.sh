#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

output="$(./test/mutants_config_example_mutation_test 2>&1)"
status=$?

if [[ "${status}" -ne 0 ]]; then
  echo "Expected mutants-config mutation target to succeed with zero generated mutants, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "No mutations generated." <<<"${output}"; then
  echo "Expected no-mutants message in output when mutants_config filters all mutants."
  echo "${output}"
  exit 1
fi
