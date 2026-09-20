#!/usr/bin/env bash
#
# Removes superseded cargo build artifacts from this checkout's target directory.
#
# task-2011, and the POSIX half of tools/prune-target.ps1. The two exist for the reason the two
# release scripts do: the Mac this is developed on has no pwsh at all, so a rule that only ran on
# Windows would be a rule half the builds could not follow.
#
# Cargo never reclaims a target directory. Every build writes a fresh hash-suffixed copy of each
# artifact and leaves its predecessor exactly where it is, for ever. Measured on the Windows
# checkout, target had reached 153.7 GB of which 137.8 GB was copies cargo could no longer reach --
# each of the fourteen screenshot-test binaries had twenty of them, laid down over 1.6 days, at
# about 220 MB apiece.
#
# A cargo artifact is named <stem>-<hash>; cargo reads exactly one hash per stem, so every other
# hash is unreachable and removing it costs no rebuild at all. That is what makes this safe to run
# at the end of a build, which is where tools/release.sh calls it.
#
# What it never touches: anything outside target; anything in a profile root, which is where the
# binaries a person runs live; .fingerprint and build, which are cargo's record of what is already
# fresh rather than weight, and pruning which was measured to reclaim nothing and cost a rebuild of
# 219 crates; the newest --keep generations of every stem; anything written within --min-age-minutes;
# and any profile whose .cargo-lock is held, which is what a build in flight looks like.
#
# Usage:
#   tools/prune-target.sh [--path <checkout>] [--keep N] [--min-age-minutes N]
#                         [--skip-incremental] [--dry-run] [--quiet]

set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd -- "$here/.." && pwd)"
keep=2
min_age_minutes=60
skip_incremental=0
dry_run=0
quiet=0

while [ $# -gt 0 ]; do
    case "$1" in
        --path) root="$2"; shift 2 ;;
        --keep) keep="$2"; shift 2 ;;
        --min-age-minutes) min_age_minutes="$2"; shift 2 ;;
        --skip-incremental) skip_incremental=1; shift ;;
        --dry-run) dry_run=1; shift ;;
        --quiet) quiet=1; shift ;;
        -h|--help) sed -n '2,27p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "prune-target.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done

say() { [ "$quiet" -eq 1 ] && return 0; printf '%s\n' "$*"; }
always() { printf '%s\n' "$*"; }

# Bytes in a file or directory tree, portable between the BSD du on macOS and the GNU one.
size_of() { du -sk "$1" 2>/dev/null | awk '{print $1 * 1024}'; }

# The newest modification time anywhere in a path, as a unix timestamp.
newest_in() {
    if [ -d "$1" ]; then
        find "$1" -type f -exec stat -f '%m' {} + 2>/dev/null \
            || find "$1" -type f -printf '%T@\n' 2>/dev/null \
            || true
    else
        stat -f '%m' "$1" 2>/dev/null || stat -c '%Y' "$1" 2>/dev/null || echo 0
    fi | awk 'BEGIN{m=0} {t=int($1); if (t>m) m=t} END{print m}'
}

# True when cargo holds this profile's lock, meaning a build is in flight. Asked of the profile's
# own lock rather than of the process table, because this machine runs several builds at once and
# one repository's build is no reason to leave another's dead artifacts on the disk.
build_in_flight() {
    local lock="$1/.cargo-lock"
    [ -f "$lock" ] || return 1
    command -v flock >/dev/null 2>&1 || return 1
    ( flock -n 9 ) 9<"$lock" && return 1 || return 0
}

# Prunes one artifact directory, keeping the newest $keep generations of every stem. A generation is
# a hash rather than a file, because one build of one crate writes several files that share a hash --
# the rlib, the rmeta and the dep file -- and they live or die together.
prune_dir() {
    local dir="$1" freed=0 removed=0 cutoff
    [ -d "$dir" ] || return 0
    cutoff=$(( $(date +%s) - min_age_minutes * 60 ))

    # stem <tab> hash <tab> newest-mtime, one line per GENERATION rather than per file. The awk pass
    # is what makes that true, and it is not tidiness: one build of one crate writes several files
    # sharing a hash, so without it a generation is counted once per file it contains -- and a
    # newest generation of three files would fill all of --keep by itself and evict the live one
    # behind it. Each generation keeps the newest write among its files.
    local index
    index="$(
        find "$dir" -mindepth 1 -maxdepth 1 -print 2>/dev/null | while IFS= read -r entry; do
            local base name stem hash
            base="$(basename "$entry")"
            if [ -d "$entry" ]; then name="$base"; else name="${base%.*}"; fi
            # The stem match is greedy so a package whose own name carries a hyphen splits at the
            # last one. A name that does not parse is never printed and so is never removed, which
            # is the safe direction for a name this does not recognise.
            if ! printf '%s' "$name" | grep -Eq -- '-[0-9a-z]{8,32}$'; then continue; fi
            hash="${name##*-}"
            stem="${name%-*}"
            # A hash carries at least one digit. Without this an ordinary word at the end of a
            # hyphenated name reads as a hash, and two unrelated packages are filed as generations
            # of each other.
            case "$hash" in *[0-9]*) ;; *) continue ;; esac
            printf '%s\t%s\t%s\n' "$stem" "$hash" "$(newest_in "$entry")"
        done \
        | awk -F'\t' '{ k = $1 "\t" $2; if ($3 + 0 > m[k]) m[k] = $3 + 0 }
                      END { for (k in m) print k "\t" m[k] }' \
        | sort -t "$(printf '\t')" -k1,1 -k3,3nr
    )"
    [ -n "$index" ] || return 0

    local prev_stem='' seen=0
    while IFS="$(printf '\t')" read -r stem hash mtime; do
        [ -n "$stem" ] || continue
        if [ "$stem" != "$prev_stem" ]; then prev_stem="$stem"; seen=0; fi
        seen=$(( seen + 1 ))
        [ "$seen" -le "$keep" ] && continue
        [ "$mtime" -gt "$cutoff" ] && continue
        local path
        for path in "$dir/$stem-$hash" "$dir/$stem-$hash".*; do
            [ -e "$path" ] || continue
            freed=$(( freed + $(size_of "$path") ))
            if [ "$dry_run" -eq 0 ]; then rm -rf -- "$path"; fi
        done
        removed=$(( removed + 1 ))
    done <<< "$index"

    local verb='removed'
    [ "$dry_run" -eq 1 ] && verb='would remove'
    say "$(printf '  %-14s %8.2f GB %s from %4d generations' \
        "$(basename "$dir")" "$(awk -v b="$freed" 'BEGIN{print b/1073741824}')" "$verb" "$removed")"
    printf '%s\n' "$freed" >> "$tally"
}

target="$root/target"
if [ ! -d "$target" ]; then
    always "nothing to prune: $root has no target directory"
    exit 0
fi

tally="$(mktemp)"
trap 'rm -f -- "$tally"' EXIT

mode=''
[ "$dry_run" -eq 1 ] && mode='  [dry run]'
always "pruning $target  keep $keep generations$mode"

# A profile is recognised by what is inside it rather than by name: debug and release sit directly
# under target, and a cross-compiled profile sits one deeper under its triple.
for profile in "$target"/*/ "$target"/*/*/; do
    [ -d "$profile" ] || continue
    profile="${profile%/}"
    [ -d "$profile/deps" ] || [ -d "$profile/.fingerprint" ] || continue

    if build_in_flight "$profile"; then
        always "$profile: skipped, cargo holds its lock so a build is in flight"
        continue
    fi

    say "$profile"
    for name in deps examples incremental; do
        [ "$name" = 'incremental' ] && [ "$skip_incremental" -eq 1 ] && continue
        prune_dir "$profile/$name"
    done
done

total="$(awk 'BEGIN{s=0} {s+=$1} END{print s}' "$tally")"
verb='reclaimed'
[ "$dry_run" -eq 1 ] && verb='would reclaim'
always "$(awk -v v="$verb" -v b="$total" 'BEGIN{printf "%s %.2f GB\n", v, b/1073741824}')"
