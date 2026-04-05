#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

for target in \
  "./test/proc_macro_cfg_disabled_example_mutation_test" \
  "./test/proc_macro_cfg_enabled_example_mutation_test"
do
  set +e
  output="$(${target} 2>&1)"
  status=$?
  set -e

  if [[ "${status}" -ne 0 ]]; then
    echo "Expected proc-macro mutation target ${target} to succeed, got status ${status}."
    echo "${output}"
    exit 1
  fi

  if ! grep -q "Mutation Testing Summary" <<<"${output}"; then
    echo "Expected mutation summary output from ${target}."
    echo "${output}"
    exit 1
  fi
done
