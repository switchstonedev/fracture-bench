#!/usr/bin/env bash
# ==============================================================================
# ACM / IEEE Artifact Evaluation Single-Entrypoint Reproduction Script (POSIX)
# Fracture Artifact Evaluation
# ==============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"
if [ ! -d "${SCRIPTS_DIR}" ] && [ -d "${SCRIPT_DIR}/Scripts" ]; then
    SCRIPTS_DIR="${SCRIPT_DIR}/Scripts"
fi
exec "${SCRIPTS_DIR}/run_all.sh" "$@"
