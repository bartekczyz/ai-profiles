#!/usr/bin/env bash
#
# revert-migration.sh — undo a Phase 5 first-run migration import.
#
# Restores the most recent backup of ~/.claude and ~/Library/Application Support/Claude
# from <app-data>/migration-backup-<ts>/, removes every generated launcher for every
# profile listed in profiles.json, then deletes the ai-profiles app-data dir.
#
# WARNING: this nukes ALL ai-profiles state, not just the imported profile.
# If you've created additional profiles manually, their launchers and data will
# be removed too. This script is intended for testing the migration flow.

set -euo pipefail

APP_DATA="$HOME/Library/Application Support/ai-profiles"
PROFILES_JSON="$APP_DATA/profiles.json"
CLAUDE_DESKTOP="$HOME/Library/Application Support/Claude"
CLAUDE_CODE="$HOME/.claude"

red() { printf '\033[31m%s\033[0m\n' "$1"; }
green() { printf '\033[32m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }

if [ ! -d "$APP_DATA" ]; then
    yellow "Nothing to revert: $APP_DATA does not exist."
    exit 0
fi

# Most-recent backup directory (lexicographic sort works because timestamps are
# fixed-width unix-ms suffixes).
backup_dir=$(ls -1d "$APP_DATA"/migration-backup-* 2>/dev/null | sort -r | head -1 || true)

echo "About to revert ai-profiles state."
echo
echo "App data dir:        $APP_DATA"
echo "Profiles registry:   $PROFILES_JSON"
if [ -n "$backup_dir" ]; then
    green "Backup to restore:   $backup_dir"
else
    yellow "Backup to restore:   (none found — only state cleanup will run)"
fi
echo

profiles_to_remove=()
if [ -f "$PROFILES_JSON" ]; then
    while IFS=$'\t' read -r profile_name profile_slug; do
        gui_path="/Applications/Claude (${profile_name}).app"
        cli_path="$HOME/.local/bin/claude-${profile_slug}"
        echo "Will remove launcher: $gui_path"
        echo "Will remove wrapper:  $cli_path"
        profiles_to_remove+=("$profile_name"$'\t'"$profile_slug")
    done < <(python3 -c "
import json, sys
data = json.load(sys.stdin)
for profile in data.get('profiles', []):
    print(f\"{profile['name']}\t{profile['slug']}\")
" < "$PROFILES_JSON")
fi

if [ -n "$backup_dir" ]; then
    if [ -d "$backup_dir/Claude" ]; then
        echo "Will restore:         $backup_dir/Claude -> $CLAUDE_DESKTOP"
    fi
    if [ -d "$backup_dir/.claude" ]; then
        echo "Will restore:         $backup_dir/.claude -> $CLAUDE_CODE"
    fi
fi
echo "Will remove:          $APP_DATA"
echo

red "This is destructive. Type 'revert' to proceed:"
read -r confirmation
if [ "$confirmation" != "revert" ]; then
    yellow "Aborted."
    exit 1
fi

# 1. Remove launchers for every registered profile.
for entry in "${profiles_to_remove[@]:-}"; do
    [ -z "$entry" ] && continue
    profile_name="${entry%$'\t'*}"
    profile_slug="${entry#*$'\t'}"
    gui_path="/Applications/Claude (${profile_name}).app"
    cli_path="$HOME/.local/bin/claude-${profile_slug}"
    rm -rf "$gui_path"
    rm -f "$cli_path"
    green "Removed launcher: $gui_path"
    green "Removed wrapper:  $cli_path"
done

# 2. Restore originals from the most recent backup, if any.
if [ -n "$backup_dir" ]; then
    if [ -d "$backup_dir/Claude" ]; then
        rm -rf "$CLAUDE_DESKTOP"
        mv "$backup_dir/Claude" "$CLAUDE_DESKTOP"
        green "Restored: $CLAUDE_DESKTOP"
    fi
    if [ -d "$backup_dir/.claude" ]; then
        rm -rf "$CLAUDE_CODE"
        mv "$backup_dir/.claude" "$CLAUDE_CODE"
        green "Restored: $CLAUDE_CODE"
    fi
fi

# 3. Nuke the app-data dir (this also removes the now-empty backup dir, the
#    profiles registry, and every per-profile data dir under profiles/).
rm -rf "$APP_DATA"
green "Removed: $APP_DATA"

echo
green "Done. ai-profiles is back to its pre-import state."
