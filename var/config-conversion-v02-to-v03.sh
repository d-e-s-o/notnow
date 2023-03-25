#!/bin/bash

# This script may be used to convert notnow configuration (including
# tasks) from the format as used by version 0.2.* to that of version
# 0.3.
#
# It requires a full checkout of the notnow source code (including
# history), as well as a functional 1.65.0 toolchain and the neovim
# editor.
#
# Creating a backup before usage is highly recommended. Use at your own
# risk.

set -e -u -o pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: ${0} <path-to-notnow-source-directory>"
  exit 1
fi

NOTNOW_CFG_DIR="$(readlink -e ${XDG_CONFIG_HOME:-~/.config/notnow})"
NOTNOW_SRC_DIR="$1"

sed -i 's@{\n[ ]\+"rgb": \(\[\n.*\n.*\n.*\n.*\]\)\n.*}@\1@g' "${NOTNOW_CFG_DIR}/notnow.json"
nvim "${NOTNOW_CFG_DIR}/notnow.json" -c '%s!{\n[ ]\+"rgb": \(\[\n.*\n.*\n.*\n.*\]\)\n.*}!\1!g' -c 'wq'

nvim "${NOTNOW_CFG_DIR}/notnow.json" -c '%s!{\n[ ]\+"id": \([0-9]\+\)\n[ ]\+}!\1!g' -c 'wq'
nvim "${NOTNOW_CFG_DIR}/tasks.json" -c '%s!{\n[ ]\+"id": \([0-9]\+\)\n[ ]\+}!\1!g' -c 'wq'

nvim "${NOTNOW_CFG_DIR}/notnow.json" -c '%s!_query_\([fb]g\)"!_tab_\1"!g' -c 'wq'
nvim "${NOTNOW_CFG_DIR}/notnow.json" -c '%s!"queries": \[!"views": \[!g' -c 'wq'

cd "${NOTNOW_SRC_DIR}"

git checkout f606dc755d82da062cfcb3b80c8dc4510d76a4dc
echo 'wq' | cargo run
rm "${NOTNOW_CFG_DIR}/tasks.json"

git checkout f3def829a68cfd95ac089e1bd04d3ccac45a5e46
echo 'wq' | cargo run

git checkout 762ec09b91707b5e52b9f1f1d837e42087cd755c
echo 'wq' | cargo run
