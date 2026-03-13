#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

set +e
output="$(./test/survivor_example_mutation_test 2>&1)"
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
  echo "Expected survivor example mutation target to fail when mutants survive."
  echo "${output}"
  exit 1
fi

if ! grep -q "SURVIVED" <<<"${output}"; then
  echo "Expected at least one survived mutant in output."
  echo "${output}"
  exit 1
fi

if ! grep -q "Survived mutations (test gaps):" <<<"${output}"; then
  echo "Expected survived mutation details to be printed."
  echo "${output}"
  exit 1
fi

if ! grep -q "Mutation testing failed:" <<<"${output}"; then
  echo "Expected strict survivor failure message."
  echo "${output}"
  exit 1
fi
