#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

disabled_params="test/proc_macro_cfg_disabled_example_mutation_test.rustc_params"
enabled_params="test/proc_macro_cfg_enabled_example_mutation_test.rustc_params"

for params in "${disabled_params}" "${enabled_params}"; do
  if [[ ! -f "${params}" ]]; then
    echo "Expected params file ${params} to exist."
    exit 1
  fi
done

disabled_cfg_count="$(grep -c '^--cfg=mutation_proc_macro_enabled$' "${disabled_params}" || true)"
enabled_cfg_count="$(grep -c '^--cfg=mutation_proc_macro_enabled$' "${enabled_params}" || true)"
disabled_test_count="$(grep -c '^--test$' "${disabled_params}" || true)"
enabled_test_count="$(grep -c '^--test$' "${enabled_params}" || true)"

if [[ "${disabled_cfg_count}" -ne 0 ]]; then
  echo "Expected disabled params to omit the cfg flag, found ${disabled_cfg_count} copies."
  cat "${disabled_params}"
  exit 1
fi

if [[ "${enabled_cfg_count}" -ne 1 ]]; then
  echo "Expected enabled params to contain exactly one cfg flag, found ${enabled_cfg_count}."
  cat "${enabled_params}"
  exit 1
fi

if [[ "${disabled_test_count}" -ne 1 || "${enabled_test_count}" -ne 1 ]]; then
  echo "Expected both params files to contain exactly one --test flag."
  echo "disabled count: ${disabled_test_count}"
  echo "enabled count: ${enabled_test_count}"
  exit 1
fi
