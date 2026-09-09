#!/usr/bin/env bash
#
# Starts an Unluminous window and drives it **without ever taking the keyboard focus**, so that somebody
# can carry on working on their own machine while it is being tested.
#
# **Why this exists, and it is a correction rather than a feature.** A long session of driving the real
# window used `osascript -e 'tell application "System Events" to set frontmost of …'` before nearly every
# command, on the belief that a backgrounded window stops answering and cannot be photographed. Measured,
# both halves of that are false:
#
#   * Ten `status` calls in a row to a window that was not in front: all ten answered.
#   * A `window screenshot` of a window launched with `open -g`, which has never been frontmost at all:
#     wrote a correct 1938 by 1133 picture of the whole window.
#
# So the activation was never needed. What it did do is take the focus away from whatever the person was
# typing into, once per command, for an hour — which is the fault being fixed here.
#
# The two things that made it *look* necessary:
#
#   * `app::HEARTBEAT` means an idle window draws about twice a second, so a command can wait up to half a
#     second before the frame that answers it. That is a wait, not a failure, and the default timeout is
#     fifteen seconds.
#   * The window really does stop drawing sometimes, for minutes, with requests queued — the fault
#     `services::wake` records and which is still open. Activating it recovers that, which is what made
#     the habit look like it was working. The answer to *that* is the wake escalation, not the pointer.
#
# Usage:
#   tools/drive-a-window.sh <folder>                    # start one and print its process id
#   tools/drive-a-window.sh <folder> shot <file.png>     # start one, photograph it, leave it running
#   tools/drive-a-window.sh --pid                        # the id of a window this script started
#   tools/drive-a-window.sh --stop                       # close the ones it started
#
# Everything else is `unluminous-cli --instance <pid> …` as usual. Nothing here activates anything, and
# nothing here should be given a line that does.

set -uo pipefail

app="${UNLUMINOUS_APP:-/Applications/Unluminous.app}"
cli="$app/Contents/MacOS/unluminous-cli"
binary="$app/Contents/MacOS/unluminous"
marker="$HOME/.unluminous-driven-windows"

[ -x "$cli" ] || { echo "No Unluminous at $app. Build and install it first." >&2; exit 1; }

# The process ids of windows this script started, filtered to the ones still alive.
alive() {
    [ -f "$marker" ] || return 0
    while read -r pid; do
        [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && echo "$pid"
    done < "$marker"
}

case "${1:-}" in
    --pid)
        alive | head -1
        exit 0
        ;;
    --stop)
        for pid in $(alive); do
            kill "$pid" 2>/dev/null && echo "closed $pid"
        done
        : > "$marker"
        exit 0
        ;;
esac

folder="${1:-}"
[ -n "$folder" ] || { sed -n '3,36p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 2; }
[ -d "$folder" ] || { echo "$folder is not a folder." >&2; exit 1; }
folder="$(cd "$folder" && pwd)"

# **`open -g`, and that flag is the whole point.** It launches the application *without* bringing it to
# the front — `-n` makes it a new instance rather than activating one that is already running, which on
# its own would steal the focus. Running the binary directly also works and leaves the window unfocused,
# but it does not get a bundle identity, so the menu bar and the Dock icon are wrong and a screenshot of
# it is a screenshot of something that is not quite the application.
open -g -n -a "$app" --args "$folder" 2>/dev/null

# Waited for by asking rather than by sleeping a fixed time: a cold start is a second and a half on this
# machine and much longer on a busy one, and `instances` is the honest signal that the control channel is
# listening.
pid=""
for _ in $(seq 1 60); do
    pid="$("$cli" instances 2>/dev/null | grep -F " $folder" | awk '{print $2}' | head -1)"
    [ -n "$pid" ] && break
    sleep 0.5
done
if [ -z "$pid" ]; then
    echo "The window did not open, or its control channel never answered." >&2
    exit 1
fi
echo "$pid" >> "$marker"
echo "$pid"

if [ "${2:-}" = "shot" ]; then
    into="${3:-/tmp/unluminous.png}"
    # One frame is asked for and waited out by the command itself; nothing here needs to touch the window.
    "$cli" --instance "$pid" window screenshot "$into" >&2
fi
