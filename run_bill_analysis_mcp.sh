#!/usr/bin/env bash

# Actual escape-character colour codes (avoids \0 backreference bug in sed)
RED=$'\033[0;31m'
YELLOW=$'\033[0;33m'
GREEN=$'\033[0;32m'
CYAN=$'\033[0;36m'
BLUE=$'\033[0;34m'
BOLD=$'\033[1m'
RESET=$'\033[0m'

pretty_log() {
    while IFS= read -r line; do
        # Pass through non-JSON lines (cargo build output etc.) unchanged
        if ! echo "$line" | jq -e . >/dev/null 2>&1; then
            echo "$line"
            continue
        fi

        # Print raw JSON line first, then pretty output on the next line
        echo "$line" | jq -C -c .

        level=$(echo "$line" | jq -r '.level     // "INFO"')
        target=$(echo "$line" | jq -r '.target   // ""')
        msg=$(echo "$line"   | jq -r '.fields.message // ""')

        # Audit lines have upn/tool/bytes/run_msec but no message
        upn=$(echo "$line"      | jq -r '.fields.upn      // ""')
        tool=$(echo "$line"     | jq -r '.fields.tool     // ""')
        bytes=$(echo "$line"    | jq -r '.fields.bytes    // ""')
        run_msec=$(echo "$line" | jq -r '.fields.run_msec // ""')

        case "$level" in
            ERROR) lvl_color="$RED"    ;;
            WARN)  lvl_color="$YELLOW" ;;
            INFO)  lvl_color="$GREEN"  ;;
            DEBUG) lvl_color="$CYAN"   ;;
            *)     lvl_color="$RESET"  ;;
        esac

        if [ -n "$tool" ]; then
            # Structured audit line
            printf "%s%-5s%s %sAUDIT%s  upn=%s%s%s tool=%s%s%s bytes=%s %s%sms%s\n" \
                "$BOLD" "$level" "$lvl_color$RESET" \
                "$BLUE" "$RESET" \
                "$BOLD" "$upn" "$RESET" \
                "$CYAN" "$tool" "$RESET" \
                "$bytes" \
                "$YELLOW" "$run_msec" "$RESET"
        else
            # Regular log line — highlight keywords in the message
            highlighted=$(echo "$msg" \
                | sed "s/FAIL/${RED}FAIL${RESET}/g" \
                | sed "s/\bOK\b/${GREEN}OK${RESET}/g" \
                | sed "s/authenticated/${GREEN}authenticated${RESET}/g" \
                | sed "s/timeout/${YELLOW}timeout${RESET}/g" \
                | sed "s/upn=[^ ]*/${BOLD}&${RESET}/g" \
                | sed "s/session=[^ ]*/${CYAN}&${RESET}/g")
            short_target="${target##*::}"
            printf "%s%-5s%s %-20s  %s\n" \
                "$lvl_color" "$level" "$RESET" \
                "$BLUE$short_target$RESET" \
                "$highlighted"
        fi
    done
}

cd bill_analysis
cargo run --bin bill_analysis_mcp -- --data-dir ./csv_data --port 8091 2>&1 | pretty_log 2>&1
