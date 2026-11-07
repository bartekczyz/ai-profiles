#!/usr/bin/env bash
#
# revert-migration.sh — undo first-run migration imports.
#
# Puts back every original install the imports moved into
# <app-data>/migration-backup-<ts>/ (Claude and ChatGPT/Codex alike), removes
# the launcher and CLI wrapper of every profile in profiles.json, then deletes
# the ai-profiles app-data dir.
#
# Everything is checked before anything is touched. The script refuses to run
# if any backup entry can't go back where it came from: two backups of the same
# install, an entry it doesn't recognise, or an original location that is in
# use again. The app-data dir (backups included) is deleted only once every
# backup has been emptied, so no backup is ever deleted unrestored.
#
# WARNING: this nukes ALL ai-profiles state, not just the imported profiles.
# Profiles created manually lose their launchers and data too. This script is
# intended for testing the migration flow.
#
# Written for macOS's stock bash 3.2 (no associative arrays).

set -euo pipefail
shopt -s nullglob dotglob

APP_DATA="$HOME/Library/Application Support/ai-profiles"
PROFILES_JSON="$APP_DATA/profiles.json"
SUPPORT_DIR="$HOME/Library/Application Support"

red() { printf '\033[31m%s\033[0m\n' "$1"; }
green() { printf '\033[32m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }

# Where a backup entry goes back to, or nothing for an entry an import never
# creates. Mirrors gui_support_dir_name / cli_stock_config_dir_name in
# app_kind.rs.
restore_target() {
    case "$1" in
        Claude | Codex) echo "$SUPPORT_DIR/$1" ;;
        .claude | .codex) echo "$HOME/$1" ;;
    esac
}

# Print the backup entry already planned to restore to $1; fail if none.
planned_source_for() {
    local restore
    for restore in "${restores[@]:-}"; do
        if [ -n "$restore" ] && [ "${restore#*$'\t'}" = "$1" ]; then
            echo "${restore%%$'\t'*}"
            return 0
        fi
    done
    return 1
}

if [ ! -d "$APP_DATA" ]; then
    yellow "Nothing to revert: $APP_DATA does not exist."
    exit 0
fi

# 1. Plan a restore for every entry of every backup, oldest backup first (glob
#    order is lexicographic and the unix-ms suffixes are fixed-width).
backups=("$APP_DATA"/migration-backup-*/)
restores=() # "<backup entry>\t<original location>"
problems=()
for backup in "${backups[@]:-}"; do
    [ -z "$backup" ] && continue
    for entry in "${backup%/}"/*; do
        name="${entry##*/}"
        [ "$name" = ".DS_Store" ] && continue
        target=$(restore_target "$name")
        if [ -z "$target" ]; then
            problems+=("Unrecognised backup entry: $entry")
        elif earlier=$(planned_source_for "$target"); then
            problems+=("Two backups of $target: $earlier and $entry")
        elif [ -e "$target" ] || [ -L "$target" ]; then
            problems+=("$target exists again, so $entry can't go back")
        else
            restores+=("$entry"$'\t'"$target")
        fi
    done
done

if [ "${#problems[@]}" -gt 0 ]; then
    red "Refusing to revert: some backups can't be restored in full."
    for problem in "${problems[@]}"; do
        echo "  - $problem"
    done
    echo
    yellow "Nothing was changed. Move the conflicting files aside, then rerun."
    exit 1
fi

# 2. Every generated launcher and CLI wrapper that exists. Mirrors
#    launcher_prefix, legacy_launcher_prefixes and cli_wrapper_prefix in
#    app_kind.rs; an unknown `app` fails here, before anything is touched.
launchers=()
if [ -f "$PROFILES_JSON" ]; then
    launcher_paths=$(python3 -c '
import json, os, sys
apps = {"claude": (["Claude"], "claude"), "codex": (["ChatGPT", "Codex"], "codex")}
home = os.path.expanduser("~")
for profile in json.load(open(sys.argv[1])).get("profiles", []):
    prefixes, wrapper = apps[profile.get("app", "claude")]
    name, slug = profile["name"], profile["slug"]
    for prefix in prefixes:
        print(f"/Applications/{prefix} ({name}).app")
    print(f"{home}/.local/bin/{wrapper}-{slug}")
' "$PROFILES_JSON")
    while IFS= read -r path; do
        if [ -n "$path" ] && { [ -e "$path" ] || [ -L "$path" ]; }; then
            launchers+=("$path")
        fi
    done <<<"$launcher_paths"
fi

echo "About to revert ai-profiles state."
echo
echo "App data dir:        $APP_DATA"
echo "Profiles registry:   $PROFILES_JSON"
if [ "${#restores[@]}" -eq 0 ]; then
    yellow "Backups to restore:  (none found — only state cleanup will run)"
fi
echo
for restore in "${restores[@]:-}"; do
    [ -z "$restore" ] && continue
    echo "Will restore:        ${restore%%$'\t'*} -> ${restore#*$'\t'}"
done
for launcher in "${launchers[@]:-}"; do
    [ -z "$launcher" ] && continue
    echo "Will remove:         $launcher"
done
echo "Will remove:         $APP_DATA"
echo

red "This is destructive. Type 'revert' to proceed:"
read -r confirmation
if [ "$confirmation" != "revert" ]; then
    yellow "Aborted."
    exit 1
fi

# 3. Restore the originals. The location is checked again because the prompt
#    above may have sat open while an app recreated it; `mv` onto an existing
#    dir would nest the backup inside it.
for restore in "${restores[@]:-}"; do
    [ -z "$restore" ] && continue
    from="${restore%%$'\t'*}"
    to="${restore#*$'\t'}"
    if [ -e "$to" ] || [ -L "$to" ]; then
        red "$to appeared after the check; stopping before the app-data dir is removed."
        yellow "Move it aside, then rerun."
        exit 1
    fi
    mv "$from" "$to"
    green "Restored: $to"
done

# Every backup must be empty now. `rmdir` refuses a non-empty dir, and
# `set -e` then stops the script before the app-data dir goes.
for backup in "${backups[@]:-}"; do
    [ -z "$backup" ] && continue
    rm -f "${backup%/}/.DS_Store"
    rmdir "$backup"
done

# 4. Remove launchers and wrappers.
for launcher in "${launchers[@]:-}"; do
    [ -z "$launcher" ] && continue
    rm -rf "$launcher"
    green "Removed: $launcher"
done

# 5. Nuke the app-data dir: the profiles registry and every per-profile data
#    dir under profiles/.
rm -rf "$APP_DATA"
green "Removed: $APP_DATA"

echo
green "Done. ai-profiles is back to its pre-import state."
