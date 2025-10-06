#!/usr/bin/env bash
set -euo pipefail

# This is approximate and can be wrong.
# You were warned.

# Find all the public functions and structures in the scr directory, grab their names, and put them in pub_items
pub_items=$(rg -w fn\|struct src | grep -w pub  | sed -E 's/.*(fn|struct)[[:space:]]+([A-Za-z0-9_]+).*/\2/')

# For each of the public items, count the number of files it appears in. If the count is
# less than / equal to 1, then output that it's possibly unused.
for item in $pub_items; do
  hits=$(rg -lw $item src tests | sort -u | wc -l)

  if [[ $hits -le 1 ]]; then
    echo "🚨 Possibly unused: $item"
  fi
done
