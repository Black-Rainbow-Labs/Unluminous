#!/usr/bin/env bash
#
# Makes git reach the private `inillucent` repository on a machine with two GitHub accounts.
#
# **The problem, measured rather than guessed.** `~/.gitconfig` on this machine routes github.com through
# `gh auth git-credential`, and that `gh` is logged in as an account which cannot see
# `jasonmcaffee/inillucent`. GitHub answers **404** for a private repository the caller cannot see — not
# 403 — so git treats it as "no such repository" and stops without trying the next helper in the chain,
# which is the keychain, where the token that *can* see it lives. Both of these were measured from the
# same shell in the same second:
#
#     /user with the chain's token                      -> 200
#     /repos/jasonmcaffee/unluminous with that token     -> 404
#     the same path with the keychain's token            -> 200
#
# It shows up as `cargo build` failing with `revision 6eaa12d1… not found`, and as `release.sh` failing to
# create a release after pushing the tag.
#
# **Two ways to fix it, and this file is the temporary one.** Sourced, it names the keychain for the
# current shell only and writes nothing:
#
#     . tools/credentials.sh
#     cargo build --release
#
# The permanent fix is one line in `~/.gitconfig`, which is a person's own file so this script does not
# edit it. Under the existing section, before the `gh` helper:
#
#     [credential "https://github.com"]
#         helper = osxkeychain
#         helper = !/opt/homebrew/bin/gh auth git-credential
#
# `--check` says which credentials this machine has and which of them can see the repository, without
# changing anything.

# `GIT_CONFIG_*` rather than `git config`, because it applies to this process and its children and leaves
# no trace. The empty value first is what clears the inherited list: a helper is a multi-valued key, so
# adding one without clearing would put the keychain *after* the helper that already answers wrongly.
set_the_keychain_first() {
    export GIT_CONFIG_COUNT=2
    export GIT_CONFIG_KEY_0="credential.https://github.com.helper"
    export GIT_CONFIG_VALUE_0=""
    export GIT_CONFIG_KEY_1="credential.https://github.com.helper"
    export GIT_CONFIG_VALUE_1="osxkeychain"
    # Cargo's own git client cannot authenticate to a private repository at all, so the fetch has to be
    # `git`. `.cargo/config.toml` says this too, for anybody who does not source this file.
    export CARGO_NET_GIT_FETCH_WITH_CLI=true
}

# What each credential source answers, and whether the repository is visible to it. Prints no secret.
report() {
    local slug="${1:-jasonmcaffee/unluminous}"
    printf 'Which credentials can see %s\n\n' "$slug"
    local names=("the helper chain" "the keychain, asked directly")
    local index=0
    for source in chain keychain; do
        local token
        case "$source" in
            chain) token="$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null \
                | sed -nE 's/^password=(.*)$/\1/p' | head -1)" ;;
            keychain) token="$(printf 'protocol=https\nhost=github.com\n\n' \
                | git -c credential.helper=osxkeychain credential fill 2>/dev/null \
                | sed -nE 's/^password=(.*)$/\1/p' | head -1)" ;;
        esac
        if [ -z "$token" ]; then
            printf '  %-28s no credential\n' "${names[$index]}:"
        else
            local who code
            who="$(curl -sS -H "Authorization: Bearer $token" https://api.github.com/user \
                | sed -nE 's/.*"login"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)"
            code="$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" \
                "https://api.github.com/repos/$slug")"
            local verdict="cannot see it (HTTP $code)"
            [ "$code" = "200" ] && verdict="can see it"
            printf '  %-28s %s — %s\n' "${names[$index]}:" "${who:-unknown account}" "$verdict"
        fi
        index=$((index + 1))
    done
    printf '\nSource this file to use the keychain for one shell, or add `helper = osxkeychain` to the\n'
    printf '[credential "https://github.com"] section of ~/.gitconfig, above the gh one, to fix it for good.\n'
}

case "${1:-}" in
    --check) report "${2:-}" ;;
    *) set_the_keychain_first ;;
esac
