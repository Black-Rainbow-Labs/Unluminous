#!/usr/bin/env bash
#
# Releases Unluminous on macOS: bump the version, build, install it on this machine, tag, push, and publish
# a GitHub release with the disk image on it.
#
# The other half of `tools/release.ps1`, which does the same on Windows and could never run here:
# it calls `installer\windows\build.ps1`, which needs Inno Setup, and this machine has no `pwsh` at
# all. So "finishing a task means releasing it" was an instruction nobody on a Mac could follow, and a
# release was something asked about rather than done. One command is the only form of that rule that
# gets followed.
#
# What it does, in the order it has to happen, stopping at the first thing that goes wrong:
#
#   1. Refuses to run on a dirty checkout. A release built from one is a release nobody can rebuild.
#   2. Bumps `version` under `[workspace.package]` in Cargo.toml, which is the one place the version is
#      written down. It reaches Info.plist, the About box and the disk image's name from there.
#   3. Runs installer/macos/build.sh --install --notarize: builds `unluminous` and `unluminous-cli`,
#      signs, notarises, staples, makes Unluminous.app and the disk image, and copies the bundle into
#      /Applications. The rebuild is what moves the build date the About box shows.
#
#      **Notarising is the default, because the file this step publishes is the one people download.**
#      A Developer ID signature on its own leaves the warning an unsigned build gets on macOS 10.15
#      and later; only the notarisation removes it. A release that stopped at signing therefore looked
#      finished and was not, which is what v0.38.2 shipped as. `--skip-notarize` is for a build that
#      never leaves this machine; anything published should not use it.
#   4. Copies the image into releases/.
#   5. Commits Cargo.toml and Cargo.lock on their own as `Unluminous <version>`, tags `v<version>`, and
#      pushes the branch and the tag.
#   6. Creates the GitHub release with the image attached.
#
# The task's own code is expected to be committed already: the version bump is a commit of its own so
# that the history stays greppable by ticket.
#
# Usage:
#   tools/release.sh                     # patch: 0.27.3 -> 0.27.4
#   tools/release.sh --part minor        # 0.27.3 -> 0.28.0
#   tools/release.sh --version 1.0.0     # exactly this version
#   tools/release.sh --notes "task-28: the handoff"
#   tools/release.sh --skip-install      # build the image but leave /Applications alone
#   tools/release.sh --skip-publish      # everything up to the tag, and stop before GitHub
#   tools/release.sh --skip-notarize     # sign but do not send it to Apple: a build for this machine only
#   tools/release.sh --dry-run           # say what would happen and change nothing
#   tools/release.sh --skip-tests        # for a release whose suite was just run by hand
#
# **No `gh` and no second credential.** `release.ps1` installs the GitHub CLI with winget the first
# time; the equivalent here would be a download, because this machine has no homebrew either. The
# release and the upload are two calls to api.github.com with `curl`, using the token
# `git credential fill` already holds for github.com — which is the credential that makes `git push`
# work, so a machine that can push can release. `GH_TOKEN` or `GITHUB_TOKEN` wins when set, which is
# how a machine with its own token keeps using it. Nothing is printed or written down.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
manifest="$repo/Cargo.toml"
releases="$repo/releases"
slug="jasonmcaffee/unluminous"

part=patch
version=""
notes=""
skip_install=0
skip_publish=0
skip_notarize=0
skip_tests=0
dry_run=0

# A while loop rather than `for argument in "$@"`, because the bash macOS ships is 3.2 and that one
# treats an empty "$@" as an unset variable under `set -u`.
while [ "$#" -gt 0 ]; do
    case "$1" in
        --part) part="${2:-}"; shift ;;
        --version) version="${2:-}"; shift ;;
        --notes) notes="${2:-}"; shift ;;
        --skip-install) skip_install=1 ;;
        --skip-publish) skip_publish=1 ;;
        --skip-notarize) skip_notarize=1 ;;
        --skip-tests) skip_tests=1 ;;
        --dry-run|--whatif) dry_run=1 ;;
        -h|--help) sed -n '3,49p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

case "$part" in
    patch|minor|major) ;;
    *) echo "--part is patch, minor or major, not $part" >&2; exit 2 ;;
esac

step() { printf '\n==> %s\n' "$1"; }
die() { echo "$1" >&2; exit 1; }

# The version in Cargo.toml, which is the one place a version is written down.
current="$(sed -nE 's/^[[:space:]]*version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' "$manifest" | head -1)"
[ -n "$current" ] || die "No version = \"x.y.z\" in $manifest."

# The ordinary semantic version rules: a minor bump zeroes the patch and a major bump zeroes both, so
# 0.27.3 --part minor is 0.28.0 rather than 0.28.3.
next_version() {
    local major minor patch
    IFS=. read -r major minor patch <<< "$current"
    case "$part" in
        major) echo "$((major + 1)).0.0" ;;
        minor) echo "$major.$((minor + 1)).0" ;;
        *) echo "$major.$minor.$((patch + 1))" ;;
    esac
}

next="${version:-$(next_version)}"
echo "$next" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' || die "$next is not a version of the form x.y.z."

# Compared part by part rather than as text, because 0.27.10 is above 0.27.9 and sorts below it.
higher() {
    local a b
    IFS=. read -r -a a <<< "$1"
    IFS=. read -r -a b <<< "$2"
    for index in 0 1 2; do
        if [ "${a[$index]}" -ne "${b[$index]}" ]; then
            [ "${a[$index]}" -gt "${b[$index]}" ]
            return
        fi
    done
    return 1
}
higher "$next" "$current" || die "$next is not higher than the version already released, $current."

cd "$repo"
branch="$(git rev-parse --abbrev-ref HEAD)"
image="$releases/unluminous-$next.dmg"
echo "Unluminous $current -> $next  on $branch"

if [ "$dry_run" = 1 ]; then
    echo
    echo "What would happen:"
    echo "  0. cargo test --workspace --exclude unluminous-app, then -p unluminous-app --lib --bins"
    echo "  1. Cargo.toml version -> $next"
    echo "  2. installer/macos/build.sh$([ "$skip_install" = 1 ] || echo ' --install')$([ "$skip_notarize" = 1 ] || echo ' --notarize')"
    echo "  3. $image"
    echo "  4. commit \"Unluminous $next\", tag v$next, push $branch"
    [ "$skip_publish" = 1 ] || echo "  5. a GitHub release v$next with the image attached"
    exit 0
fi

step 'Checking the working tree'
dirty="$(git status --porcelain)"
if [ -n "$dirty" ]; then
    echo "$dirty"
    die "The working tree is not clean. Commit the task's own work first: a release built from a dirty checkout is one nobody can rebuild."
fi

step 'Running the suite'
# `task-1922`: a release was tagged, installed and published before anything had run the tests. It is
# the whole gate now rather than a second opinion -- `task-1928` took the continuous integration out,
# so this is the only thing that runs the suite before a tag, and a release cannot be made from a
# workspace whose tests do not pass.
#
# The screenshot suite is deliberately not here. It needs a graphics card and it needs a person to
# open any image that changed, which is the one rule a script must not be allowed to satisfy on its
# own -- and with no continuous integration there is nowhere else it runs either. So a release says
# plainly that it did not run it, rather than leaving that unsaid.
if [ "$skip_tests" = 1 ]; then
    echo 'Skipped by --skip-tests.'
else
    cargo test --manifest-path "$repo/Cargo.toml" --workspace --exclude unluminous-app
    cargo test --manifest-path "$repo/Cargo.toml" -p unluminous-app --lib --bins
    echo 'The window through wgpu is not run here: it needs a graphics card and a person to look at'
    echo 'any image that changed. Run it by hand: cargo test -p unluminous-app --tests'
fi

# Everything GitHub needs is checked here, before anything is changed, so a missing credential cannot
# leave a pushed tag with no release behind it.
#
# **And the credential is *asked for* with the repository path on it.** `~/.gitconfig` routes github.com by
# repository owner — a longer `credential.<url>` section wins, so `credential "https://github.com/jasonmcaffee"`
# names the keychain and the bare `credential "https://github.com"` names `gh` for everything else. A
# `git credential fill` carrying only `host=github.com` cannot match the longer section, so it is answered by
# `gh`, which is logged in as the account that cannot see this repository. Measured: the same keychain, asked
# with `path=jasonmcaffee/unluminous`, answers with a token that gets **200** on the repository, and asked
# without it answers with one that gets **404** — which is why `git push` worked while this refused, and why
# the message said to set `GH_TOKEN` on a machine that already had a perfectly good credential. The pathless
# forms are still tried, last, for a machine whose routing is by host alone.
#
# **The credential is checked against this repository, not just against `/user`.** Asking `/user` only
# proves a token is a token. This machine has two GitHub accounts in play — `~/.gitconfig` routes
# github.com through `gh auth git-credential`, and that `gh` is logged in as an account which cannot see
# `jasonmcaffee/unluminous` — so `git credential fill` sometimes answers with a token that authenticates
# perfectly and gets 404 on the repository. Measured: `/user` 200 and `/repos/jasonmcaffee/unluminous`
# 404, from the same token, in the same second. A release using it would push the tag and then fail to
# create the release, which is the one outcome the check above exists to prevent.
token=""
if [ "$skip_publish" != 1 ]; then
    # Every credential worth trying, in order of preference: an explicit one, then the keychain asked
    # directly, then whatever the ordinary helper chain answers with. The keychain comes before the chain
    # because the chain is the thing that can answer with the wrong account.
    candidates=()
    [ -n "${GH_TOKEN:-}" ] && candidates+=("$GH_TOKEN")
    [ -n "${GITHUB_TOKEN:-}" ] && candidates+=("$GITHUB_TOKEN")
    from_keychain="$(printf 'protocol=https\nhost=github.com\npath=%s\n\n' "$slug" \
        | git -c credential.helper=osxkeychain credential fill 2>/dev/null \
        | sed -nE 's/^password=(.*)$/\1/p' | head -1)"
    [ -n "$from_keychain" ] && candidates+=("$from_keychain")
    from_chain="$(printf 'protocol=https\nhost=github.com\npath=%s\n\n' "$slug" \
        | git credential fill 2>/dev/null \
        | sed -nE 's/^password=(.*)$/\1/p' | head -1)"
    [ -n "$from_chain" ] && candidates+=("$from_chain")
    # And the same two without a path, last, so a machine whose routing is by host alone still works.
    for bare in \
        "$(printf 'protocol=https\nhost=github.com\n\n' \
            | git -c credential.helper=osxkeychain credential fill 2>/dev/null \
            | sed -nE 's/^password=(.*)$/\1/p' | head -1)" \
        "$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null \
            | sed -nE 's/^password=(.*)$/\1/p' | head -1)"
    do
        [ -n "$bare" ] && candidates+=("$bare")
    done

    [ "${#candidates[@]}" -gt 0 ] || die "No GitHub credential is stored for github.com. Push once, or set GH_TOKEN, and run this again."

    who=""
    for candidate in "${candidates[@]}"; do
        # The repository, because that is what the release is created on. A token that cannot read it
        # cannot write a release to it either.
        code="$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $candidate" \
            -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/$slug")"
        if [ "$code" = "200" ]; then
            token="$candidate"
            who="$(curl -sS -H "Authorization: Bearer $token" -H 'Accept: application/vnd.github+json' \
                https://api.github.com/user | sed -nE 's/.*"login"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)"
            break
        fi
    done

    if [ -z "$token" ]; then
        echo "None of the GitHub credentials on this machine can see $slug." >&2
        echo "Tried ${#candidates[@]}: the environment, the keychain, and the helper chain." >&2
        echo "\`gh auth status\` may be logged in as a different account — this repository is private to" >&2
        echo "the account that owns it. Set GH_TOKEN to a token with \`repo\` on $slug and run this again." >&2
        exit 1
    fi
    echo "GitHub: authenticated as $who, and $slug is visible to it"
fi

step "Setting the version to $next"
# Only the first `version = "x.y.z"` is touched. The workspace table is at the top and every crate
# inherits from it with `version.workspace = true`, so there is exactly one line to change and
# changing more than one would be a mistake rather than a thoroughness.
awk -v new="$next" '
    !done && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"/ {
        sub(/"[0-9]+\.[0-9]+\.[0-9]+"/, "\"" new "\"")
        done = 1
    }
    { print }
' "$manifest" > "$manifest.next"
grep -q "version = \"$next\"" "$manifest.next" || { rm -f "$manifest.next"; die "Could not write the version into $manifest."; }
mv "$manifest.next" "$manifest"
# Cargo.lock names every workspace member's version, so it has to move with the manifest. A metadata
# read is the cheapest thing that rewrites it, and it fails loudly if the edit was wrong.
cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" > /dev/null

step 'Building the disk image, and installing it'
build=("$repo/installer/macos/build.sh")
[ "$skip_install" = 1 ] || build+=(--install)
[ "$skip_notarize" = 1 ] || build+=(--notarize)
bash "${build[@]}"
[ -f "$image" ] || die "The disk image was not written to $image."
echo "Kept $image"

step "Committing and tagging v$next"
# Written from the history rather than kept by hand, so it cannot fall behind. See the same step in
# tools/release.ps1, and tools/changelog.mjs for why it is one script rather than two.
step "Writing CHANGELOG.md"
node "$repo/tools/changelog.mjs"

git add -- Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Unluminous $next" > /dev/null
git tag -a "v$next" -m "Unluminous $next"
git push origin "$branch"
git push origin "v$next"

if [ "$skip_publish" = 1 ]; then
    echo
    echo "Tagged and pushed v$next. Not published, because --skip-publish was given."
    exit 0
fi

step 'Publishing the GitHub release'
[ -n "$notes" ] || notes="$(git log -1 --pretty=%s "v$next^")"
body="$notes

macOS: download **unluminous-$next.dmg** below, open it and drag Unluminous to Applications. It is signed but not
notarised, so the first launch is a right click and Open.

\`Unluminous -> About Unluminous\` in the window says which build this is."

# The body goes through a file rather than being interpolated into JSON by hand, because a release
# note holding a quote or a newline would otherwise write a request GitHub refuses.
payload="$(mktemp)"
trap 'rm -f "$payload"' EXIT
python3 - "$next" "$body" > "$payload" <<'PY'
import json, sys
print(json.dumps({"tag_name": "v" + sys.argv[1], "name": "Unluminous " + sys.argv[1], "body": sys.argv[2]}))
PY
created="$(curl -sS -X POST \
    -H "Authorization: Bearer $token" -H 'Accept: application/vnd.github+json' \
    -d "@$payload" "https://api.github.com/repos/$slug/releases")"
upload="$(echo "$created" | sed -nE 's/.*"upload_url"[[:space:]]*:[[:space:]]*"([^"{]+).*/\1/p' | head -1)"
if [ -z "$upload" ]; then
    echo "$created" | head -20
    die "The tag v$next was pushed but the release was not created. The answer from GitHub is above."
fi

curl -sS -X POST \
    -H "Authorization: Bearer $token" -H 'Accept: application/vnd.github+json' \
    -H 'Content-Type: application/octet-stream' \
    --data-binary "@$image" "$upload?name=unluminous-$next.dmg" > /dev/null

page="$(echo "$created" | sed -nE 's/.*"html_url"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)"
echo
echo "Unluminous $next is released: $page"
[ "$skip_install" = 1 ] || echo "Installed at /Applications/Unluminous.app"
