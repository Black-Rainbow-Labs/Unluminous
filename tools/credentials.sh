#!/usr/bin/env bash
#
# Says which GitHub account answers for which repository, and why it matters here.
#
# **This machine has two GitHub accounts and they are complementary.** `jasonmcaffee` owns the personal
# repositories — `unluminous`, `inillucent` — and its token is in the login keychain. `Jason-McAffee` is
# the work account, `gh` is logged in as it, and it is what reaches `allergan-data-labs`. Measured, each
# way round: the keychain token gets 200 on `jasonmcaffee/unluminous` and 404 on
# `allergan-data-labs/alle-experience-bfe`, and the `gh` token gets exactly the opposite. So neither can
# be the default for github.com as a whole.
#
# **`~/.gitconfig` routes them by repository owner**, which is the fix rather than a workaround:
#
#     [credential "https://github.com"]
#         helper =
#         helper = !/opt/homebrew/bin/gh auth git-credential
#
#     [credential "https://github.com/jasonmcaffee"]
#         helper =
#         helper = osxkeychain
#
# Git matches the longest `credential.<url>` section, so the entry with the owner on it wins for that
# owner and the bare host entry answers for everything else. The empty `helper =` clears the list
# inherited from the less specific section; without it the entries add up rather than replacing.
#
# **Why this was worth a file of its own.** A private repository the caller cannot see answers **404**,
# not 403, so git reads it as "no such repository" and stops rather than trying the next helper. With the
# `gh` account answering first for everything, `cargo build` failed with `revision 6eaa12d1… not found`
# and a release pushed its tag and then could not create the release — both of them a credential problem
# wearing the costume of a missing commit.
#
#     tools/credentials.sh              # check every repository this checkout needs
#     tools/credentials.sh <owner/repo> # check one
#
# It changes nothing and prints no secret.

set -uo pipefail

# The keychain helper is run by git with a stripped environment, and anything this script runs after it
# inherits that: without this, `curl` and `sed` are not found and every repository looks unreachable.
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:$PATH"

# The repositories this checkout actually needs, and which account should answer for each.
DEFAULT_REPOSITORIES=(
    "jasonmcaffee/unluminous:jasonmcaffee"
    "jasonmcaffee/inillucent:jasonmcaffee"
)

# What credential git would use for one repository, and what that credential can see. Prints the account
# name and the status code, never the token.
check_one() {
    local slug="$1" expected="${2:-}"
    local token who code
    # **The whole answer is read, and the account name comes from git rather than from the API.** The
    # helper reports the `username` it stored the token under, which is the account the routing chose —
    # and reading that is what makes this a check on the routing rather than on the token. Asking
    # `/user` as well would only say the same thing more slowly.
    #
    # `PATH` is set for the whole script, because `osxkeychain` runs as a child of git with a stripped
    # environment and a lookup that loses `curl` and `sed` reports every repository as unreachable.
    local answer
    answer="$(printf 'protocol=https\nhost=github.com\npath=%s.git\n\n' "$slug" | git credential fill 2>/dev/null)"
    token="$(printf '%s\n' "$answer" | sed -n 's/^password=//p' | head -1)"
    who="$(printf '%s\n' "$answer" | sed -n 's/^username=//p' | head -1)"
    if [ -z "$token" ]; then
        printf '  FAIL %-42s no credential at all\n' "$slug"
        return 1
    fi
    code="$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" \
        "https://api.github.com/repos/$slug" 2>/dev/null)"

    if [ "$code" != "200" ]; then
        printf '  FAIL %-42s %s cannot see it (HTTP %s)\n' "$slug" "${who:-an unknown account}" "$code"
        return 1
    fi
    if [ -n "$expected" ] && [ "$who" != "$expected" ]; then
        # It works, but through the account that was not meant to answer — worth saying, because it is
        # the state that breaks the moment that account's access changes.
        printf '  ok   %-42s %s (expected %s)\n' "$slug" "$who" "$expected"
        return 0
    fi
    printf '  ok   %-42s %s\n' "$slug" "$who"
}

failed=0

if [ "$#" -gt 0 ]; then
    for slug in "$@"; do
        check_one "$slug" || failed=1
    done
else
    echo "Which account answers for each repository"
    for entry in "${DEFAULT_REPOSITORIES[@]}"; do
        check_one "${entry%%:*}" "${entry##*:}" || failed=1
    done
    # A work repository as well, because the point of the routing is that both still work. Skipped
    # rather than failed when it is not checked out: not everybody has it.
    if [ -d "$HOME/dev/alle-experience-bfe" ]; then
        check_one "allergan-data-labs/alle-experience-bfe" "Jason-McAffee" || failed=1
    fi
fi

if [ "$failed" != 0 ]; then
    cat >&2 <<'BROKEN'

At least one repository is unreachable with the credential git would use for it.

`~/.gitconfig` should route github.com by repository owner — the comment at the top of this file has
the two sections it needs. Check that `gh auth status` is still logged in for the work repositories, and
that the login keychain still holds a token for the personal ones. A token that has expired looks
exactly like a repository that does not exist, because GitHub answers 404 either way.
BROKEN
    exit 1
fi
