#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

set +e
output="$(./test/survivor_allow_survivors_rust_test_mutation_test 2>&1)"
status=$?
set -e

if [[ "${status}" -ne 0 ]]; then
  echo "Expected rust_test-wrapped mutation target to succeed, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "SURVIVED" <<<"${output}"; then
  echo "Expected at least one survived mutant in rust_test-wrapped output."
  echo "${output}"
  exit 1
fi

if ! grep -q "Survived mutations (test gaps):" <<<"${output}"; then
  echo "Expected survived mutation details to be printed for rust_test-wrapped target."
  echo "${output}"
  exit 1
fi
