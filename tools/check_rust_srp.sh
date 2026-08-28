#!/usr/bin/env bash
set -euo pipefail

failed=0
review_manifest=tools/rust_srp_reviews.txt

shell_behavior() {
    rg -n '^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?((async|unsafe)[[:space:]]+)?(fn|struct|enum|trait)[[:space:]]|^[[:space:]]*impl([[:space:]<]|$)' "$1" || true
}

while IFS= read -r -d '' file; do
    [[ -f "$file" ]] || continue
    lines=$(wc -l < "$file")
    name=${file##*/}

    case "$name" in
        product.rs|common.rs|utils.rs|manager.rs)
            echo "ERROR forbidden owner: $file"
            failed=1
            ;;
    esac

    if (( lines >= 500 )); then
        echo "ERROR $lines lines: $file"
        failed=1
    elif (( lines >= 350 )); then
        if [[ -f "$review_manifest" ]] && awk -F '|' -v file="$file" '
            $1 == file && length($2) > 0 { found = 1 }
            END { exit(found ? 0 : 1) }
        ' "$review_manifest"; then
            echo "REVIEW $lines lines: $file (recorded)"
        else
            echo "ERROR unreviewed $lines-line owner: $file"
            failed=1
        fi
    fi

    case "$name" in
        lib.rs|mod.rs)
            if (( lines >= 100 )); then
                echo "ERROR entry shell is $lines lines: $file"
                failed=1
            fi
            behavior=$(shell_behavior "$file")
            if [[ -n "$behavior" ]]; then
                echo "ERROR implementation in entry shell: $file"
                echo "$behavior"
                failed=1
            fi
            ;;
        main.rs) ;;
    esac

    if [[ "$file" == */src/bin/*.rs || "$name" == main.rs ]]; then
        if (( lines >= 50 )); then
            echo "ERROR bootstrap is $lines lines: $file"
            failed=1
        fi
        behavior=$(shell_behavior "$file" | rg -v '^[0-9]+:[[:space:]]*fn main\(' || true)
        if [[ -n "$behavior" ]]; then
            echo "ERROR implementation in binary bootstrap: $file"
            echo "$behavior"
            failed=1
        fi
    fi
done < <(find crates tools -type f -name '*.rs' -print0)

exit "$failed"
