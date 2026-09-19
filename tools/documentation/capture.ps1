<#
.SYNOPSIS
  Takes every picture in `documentation/overview.md`, without touching the keyboard, the pointer or
  anything else on the machine.

.DESCRIPTION
  `documentation/taking-the-pictures.md` is the written half of this. The short version is that a
  capture used to be a photograph of the screen, which meant bringing the window to the front,
  pressing real keys and minimising everything else first — three things this repository now bans —
  and a fourth, a 3840 by 2160 screen, that the machine has to happen to have.

  None of it is needed, because **`unluminous-cli window screenshot` writes a PNG with a real alpha
  channel**. At `--opacity 0.83` the editing area comes back at `A=212` with straight,
  un-premultiplied colour, and every glyph at `A=255`. So the window's own capture already carries
  what a compositor needs, and

      out = window.rgb * a + backdrop.rgb * (1 - a)

  reproduces a screen copy exactly, given the same thing behind it. `backdrop.jpg` beside this script
  is that thing.

  What follows from it: the window is opened with `--background`, so neither the focus nor the
  virtual desktop moves; it is driven with `unluminous-cli` alone, which sends no operating system
  input at all; and nothing behind it is photographed, so nothing behind it matters.

  Two properties of the harness that came before this one are kept, because they are the difference
  between a picture of the product and a picture of one person's machine:

    * **The project is a fixture**, built under the temporary folder by `fixture.ps1`.
    * **The window is given a settings folder of its own**, through its own `APPDATA`, so the
      pictures carry the product's own defaults and taking them leaves nothing behind in the real
      settings.

  A few pictures are of a menu or a flyout, which are on no command because they belong to the
  pointer. Those are clicked with `unluminous-cli input`, which feeds the window the same
  `egui::Event` a real mouse produces, down the control channel — so it reaches the window that is
  being driven rather than whichever window happens to be in front. The positions those clicks use
  are in `$Menu`, `$FButton`, `$Flyout` and `$ExplorerRow`, and they are all for a window of exactly
  `$Width` by `$Height` points.

.PARAMETER Only
  Take these pictures and no others, by name — `04-code`, `27-base-of-infinite-space`. Everything by
  default. The window is opened once whatever is asked for.

.PARAMETER KeepOpen
  Leave the window running afterwards, for working out where something is.

.PARAMETER List
  Print the names of the pictures this script takes, and do nothing else.

.PARAMETER Chat
  Send a real question to the Agent-Chat pane for `28-agent-chat`, which runs the `claude` on this
  machine and costs what one short turn costs. Without it that picture is the pane as it opens.

.EXAMPLE
  pwsh tools/documentation/capture.ps1
  pwsh tools/documentation/capture.ps1 -Only 27-base-of-infinite-space,29-agent-tasks -KeepOpen
#>
[CmdletBinding()]
param(
    [string[]]$Only,
    [switch]$KeepOpen,
    [switch]$List,
    [switch]$Chat,
    [string]$Fixture = (Join-Path $env:TEMP 'unluminous-docs')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Images = Join-Path $Root 'documentation/images'
$Backdrop = Join-Path $PSScriptRoot 'backdrop.jpg'

$installed = Join-Path $env:LOCALAPPDATA 'Programs\Unluminous'
$App = if ($env:UNLUMINOUS_APP) { Join-Path $env:UNLUMINOUS_APP 'unluminous.exe' } else { Join-Path $installed 'unluminous.exe' }
$Cli = if ($env:UNLUMINOUS_APP) { Join-Path $env:UNLUMINOUS_APP 'unluminous-cli.exe' } else { Join-Path $installed 'unluminous-cli.exe' }

# The window, in points. Every position below is in these units, because that is what
# `unluminous-cli input` takes and what `window screenshot` writes out.
$Width = 1800
$Height = 1160
# How far outside the window the finished picture reaches. The margin is the whole reason the gallery
# exists: a picture cropped tight to the window cannot show that the colour in the editing area is
# the thing behind it rather than a shade somebody chose.
$Margin = 48
# The background opacity every picture is taken at, bar the two that are about the setting itself.
$Opacity = 0.83

# Where the things that are clicked rather than commanded sit, for a window of the size above.
$Menu = @{
    File    = @{ X = 129; Y = 18 }
    Edit    = @{ X = 179; Y = 18 }
    Code    = @{ X = 228; Y = 18 }
    Find    = @{ X = 279; Y = 18 }
    View    = @{ X = 329; Y = 18 }
    Run     = @{ X = 377; Y = 18 }
    Git     = @{ X = 420; Y = 18 }
    Plugins = @{ X = 478; Y = 18 }
}
$FButton = @{ X = 1465; Y = 18 }
$Flyout = @{
    Bold   = @{ X = 1551; Y = 57 }
    Italic = @{ X = 1583; Y = 57 }
    Red    = @{ X = 1575; Y = 91 }
    Green  = @{ X = 1601; Y = 91 }
    Blue   = @{ X = 1627; Y = 91 }
    Centre = @{ X = 1583; Y = 136 }
}
# The explorer's first row, and how far apart the rows are.
$ExplorerRow = @{ X = 110; Y = 123; Height = 34 }
# The Database pane's tree, at the width `Use-TheLibrary` gives it. Each level's disclosure is one
# indent further in and one row further down, and the rows are the same 34 points apart.
$DatabaseTree = @{
    X = 647; Y = 161; Height = 34; Indent = 12
    # Where a row's name is rather than its disclosure, for a right click; and where `Show DDL` lands
    # on the menu that opens.
    Label = 740
    ShowDdl = @{ X = 803; Y = 341 }
}
# A page a plugin contributed, in the Settings window's own list. `modal open settings --page` names
# the five Unluminous itself has; these are reached the way a person reaches them.
$SettingsPage = @{ Database = @{ X = 520; Y = 688 } }

# ---------------------------------------------------------------------------------------------
# Driving the window.
# ---------------------------------------------------------------------------------------------

$script:Window = $null

<#
.SYNOPSIS
  The pictures that want a window nobody has touched.
.DESCRIPTION
  `Reset-Window` puts back everything there is a command to put back, and the Database plugin's own
  workspace is the one thing there is not: a grid and a console are its own tabs and nothing closes
  them, so four pictures of one table in a row come out with four tabs called `album [library]`.
  Starting the window again is the honest answer. The board and the data source both survive it,
  because one lives in the project and the other in the settings folder.
#>
$Fresh = @('db-02-grid', 'db-03-console-select', 'db-04-pending-edits', 'db-05-after-submit')

<#
.SYNOPSIS
  One `unluminous-cli` command against the window this script started.
#>
function Q {
    $answer = & $Cli --instance $script:Window @args 2>&1
    if ($LASTEXITCODE -ne 0) { throw "unluminous-cli $($args -join ' ') failed: $answer" }
    return $answer
}

<#
.SYNOPSIS
  The same, for a command whose refusal means it had nothing to do.
.DESCRIPTION
  Putting the window back to a window nobody has touched asks for a good deal that is already true —
  `pane unsplit-all` on an editing area that is not split refuses with *"The editing area is not
  split"*, and so it should. A refusal there is the answer rather than a fault.
#>
function Try-Q { & $Cli --instance $script:Window @args *> $null }

<#
.SYNOPSIS
  Let the window settle. A command is answered at the top of a frame, so a picture taken in the same
  breath already shows what the command did; this is for the ones that then animate, stream or wait
  on a program.
#>
function Settle { param([int]$Milliseconds = 450) Start-Sleep -Milliseconds $Milliseconds }

<#
.SYNOPSIS
  Open a window on the fixture, sized, and wait until its control channel answers.
.DESCRIPTION
  `--background` is the whole point: the window opens without becoming the foreground window, so
  neither the keyboard nor the virtual desktop moves away from whoever is using this machine. It is
  waited for by **asking** rather than by sleeping a fixed time, because a cold start is about a
  second on this machine and much longer on a busy one.
#>
function Start-TheWindow {
    if ($script:Window) {
        & $Cli --instance $script:Window quit *> $null
        Start-Sleep -Seconds 2
    }
    $started = Start-Process -FilePath $App -PassThru `
        -ArgumentList '--background', '--opacity', $Opacity, $Fixture
    $script:Window = $started.Id

    for ($i = 0; $i -lt 60; $i++) {
        Start-Sleep -Milliseconds 500
        & $Cli --instance $script:Window status *> $null
        if ($LASTEXITCODE -eq 0) {
            Q window size --width $Width --height $Height | Out-Null
            Settle 1000
            Write-Output "  pid $($script:Window), $Width by $Height points"
            return
        }
    }
    throw 'The window did not open, or its control channel never answered.'
}

<#
.SYNOPSIS
  Put the window back to a window nobody has touched, so one picture cannot leak into the next.
.DESCRIPTION
  `terminal show` then `terminal hide` is how the run tile and the debug tile are put away: the
  bottom of the window holds one of the three and never two, so showing the terminal puts the other
  two away and hiding it leaves none of them.
#>
function Reset-Window {
    # A context menu and a flyout are `egui` popups rather than modals, so `modal cancel` does not
    # see them. Escape does, and without it one picture's right click menu is drawn over the next
    # twenty — which is what happened the first time this was run.
    Try-Q input key Escape
    Try-Q modal cancel
    if (-not (Q status --json | ConvertFrom-Json).result.editorShowing) { Try-Q action run toggle-editor }
    Try-Q debug stop
    Try-Q run stop
    # The run configuration the run and debug pictures add widens the widget at the right of the
    # title bar, which moves the `F` button — and the `F` button is clicked by position. Taking it
    # away again also keeps the title bar the same in every picture.
    Try-Q run remove numbers
    Try-Q debug breakpoint clear
    Try-Q pane unsplit-all
    # Every node, because a File Editor node holds a tab and a terminal node holds a shell, and a
    # canvas left over from one picture is a canvas the next one's `tab open` lands on.
    foreach ($view in (Q space list --json | ConvertFrom-Json).result.views) {
        foreach ($node in $view.nodes) { Try-Q space remove $node.id }
    }
    # `tab close` closes the one that is showing, and closing the last leaves an empty untitled tab.
    # It has to close that last one too: formatting a document is not a text change, so a readme left
    # open from one picture arrives at the next still bold and still centred.
    for ($i = 0; $i -lt 40; $i++) {
        $tabs = @((Q tab list --json | ConvertFrom-Json).result.tabs)
        if ($tabs.Count -eq 0) { break }
        if ($tabs.Count -eq 1 -and -not $tabs[0].path) { break }
        Try-Q tab close --discard
    }
    Try-Q space hide
    Try-Q plugins pane agent-chat/chat --hide
    Try-Q plugins pane agent-tasks/board --hide
    Try-Q plugins run database revert
    Try-Q plugins pane database/explorer --hide
    Try-Q terminal show
    Try-Q terminal hide
    Try-Q explorer show
    foreach ($folder in 'src', 'chapters', 'images') { Try-Q explorer collapse $folder }
    Try-Q panel reset
    Try-Q theme set unluminous/dark
    Try-Q settings set appearance.background.opacity $Opacity
    # The status bar keeps the last thing it was told, and what it was told during a reset is that
    # the panels went back where they started, which is true and is not what a picture is about.
    Try-Q window message
    Settle
}

<#
.SYNOPSIS
  Put the Database pane on the screen with the library connected and a grid's worth of room.
.DESCRIPTION
  The explorer is put away and the pane is widened, because a seven column grid in a 420 point column
  is a grid nobody can read. The source is added again each time rather than once at the start: a
  picture of the pane with nothing in it would be the one that showed what a fresh install looks like,
  and every one of these is about a database that is there.
#>
function Use-TheLibrary {
    Q explorer hide | Out-Null
    Q plugins pane database/explorer --show | Out-Null
    Settle 700
    Q panel size database/explorer --width 1180 | Out-Null
    Q plugins run database connect library | Out-Null
    Settle 1200
}

<#
.SYNOPSIS
  Open the tree out as far as one table's columns.
.DESCRIPTION
  There is no command for it, because the tree's disclosures are the one part of this pane that is
  only a pointer — so it is five clicks down the diagonal the levels make. Each level asks the
  database a question before its children appear, which is what the settle between them is for, and
  the table takes two because the first press on a row that has only just arrived chooses it.
#>
function Open-TheTree {
    for ($level = 0; $level -lt 4; $level++) {
        Q input click ($DatabaseTree.X + $DatabaseTree.Indent * $level) `
            ($DatabaseTree.Y + $DatabaseTree.Height * $level) | Out-Null
        Settle 1500
    }
}

<#
.SYNOPSIS
  Photograph the window and composite it onto the backdrop.
.DESCRIPTION
  The window is drawn over a crop of `backdrop.jpg` with `$Margin` points of it showing on every
  side. `CompositingMode.SourceOver` with straight alpha is the same arithmetic the desktop
  compositor does, which is what makes this a photograph of the window over a desktop rather than a
  picture of a window with a border drawn round it.
#>
function Save-Shot {
    param([string]$Name, [int]$Quality = 88)

    $raw = Join-Path $script:Shots "$Name.png"
    Q window screenshot $raw | Out-Null

    $window = [System.Drawing.Image]::FromFile($raw)
    $plate = [System.Drawing.Image]::FromFile($Backdrop)
    try {
        $w = $window.Width + $Margin * 2
        $h = $window.Height + $Margin * 2
        $out = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($out)
        $g.InterpolationMode = 'HighQualityBicubic'
        $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceOver
        $g.CompositingQuality = 'HighQuality'

        # The crop of the plate this picture is over: as much of it as fits the shape, centred, so
        # every picture is over the same part of the same thing.
        $scale = [Math]::Max($w / $plate.Width, $h / $plate.Height)
        $sw = [int][Math]::Ceiling($w / $scale)
        $sh = [int][Math]::Ceiling($h / $scale)
        $from = New-Object System.Drawing.Rectangle(
            [int](($plate.Width - $sw) / 2), [int](($plate.Height - $sh) / 2), $sw, $sh)
        $g.DrawImage($plate, (New-Object System.Drawing.Rectangle(0, 0, $w, $h)), $from,
            [System.Drawing.GraphicsUnit]::Pixel)

        $g.DrawImage($window, $Margin, $Margin, $window.Width, $window.Height)
        $g.Dispose()

        $codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() |
            Where-Object { $_.MimeType -eq 'image/jpeg' }
        $parameters = New-Object System.Drawing.Imaging.EncoderParameters(1)
        $parameters.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter(
            [System.Drawing.Imaging.Encoder]::Quality, [long]$Quality)
        $out.Save((Join-Path $Images "$Name.jpg"), $codec, $parameters)
        $out.Dispose()
    }
    finally { $window.Dispose(); $plate.Dispose() }
    Write-Output "  wrote $Name.jpg"
}

# ---------------------------------------------------------------------------------------------
# The pictures. Each is a name and what to do to the window before it is photographed.
# ---------------------------------------------------------------------------------------------

$Pictures = [ordered]@{

    # --- the window itself ---------------------------------------------------------------------

    '01-unluminous-window' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view side | Out-Null
    }

    '17-explorer' = {
        Q tab open src/query.ts --permanent | Out-Null
        Q explorer expand src | Out-Null
        Q explorer expand chapters | Out-Null
        Q explorer width 330 | Out-Null
    }

    '21-activity-bar' = {
        Q tab open src/layout.rs --permanent | Out-Null
        Q explorer hide | Out-Null
        Q terminal show | Out-Null
        Settle 1600
        Q terminal send 'git status' | Out-Null
        Settle 1600
    }

    '10-opacity-low' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q settings set appearance.background.opacity 0.15 | Out-Null
    }

    '11-opacity-full' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q settings set appearance.background.opacity 1.0 | Out-Null
    }

    # --- Markdown and diagrams -------------------------------------------------------------------

    '02-markdown-side-by-side' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view side | Out-Null
        Q editor scroll --line 30 | Out-Null
    }

    '03-markdown-preview' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view preview | Out-Null
    }

    '25-mermaid-diagram' = {
        Q tab open diagram.mmd --permanent | Out-Null
        Q editor view preview | Out-Null
    }

    '26-mermaid-in-markdown' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view preview | Out-Null
        Q editor scroll --preview --bottom | Out-Null
    }

    # --- prose, and the controls only a prose file has ---------------------------------------------

    '19-text-options' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q editor select --from-line 3 --to-line 5 | Out-Null
        Q input click $FButton.X $FButton.Y | Out-Null
        Settle 700
    }

    '22-picture' = {
        Q explorer expand images | Out-Null
        Q tab open images/aurora.jpg --permanent | Out-Null
        Settle 1200
    }

    # --- writing code ------------------------------------------------------------------------------

    '04-code' = {
        Q tab open src/theme.rs --permanent | Out-Null
        Q tab open src/query.ts --permanent | Out-Null
        Q tab open src/probe.py --permanent | Out-Null
        Q tab open src/site.css --permanent | Out-Null
        Q tab open src/layout.rs --permanent | Out-Null
        Q explorer expand src | Out-Null
    }

    '23-completion' = {
        Q explorer expand src | Out-Null
        Q tab open src/theme.rs --permanent | Out-Null
        Q editor caret --line 16 --column 1 | Out-Null
        Q editor insert '    let lifted = Color32::fro' | Out-Null
        # `Ctrl+Space` rather than waiting for the popup to open itself: `editor insert` puts the
        # whole stem in at once, and what opens the popup is typing.
        Q action run complete-word | Out-Null
        Settle 900
    }

    '35-split-view' = {
        Q tab open src/query.ts --permanent | Out-Null
        Q tab open src/layout.rs --permanent | Out-Null
        Q pane split | Out-Null
        Settle 700
        Q tab open src/theme.rs --permanent | Out-Null
        Q explorer expand src | Out-Null
    }

    '38-folding' = {
        Q tab open src/layout.rs --permanent | Out-Null
        Q explorer expand src | Out-Null
        # The struct and the loop inside the function, so the function itself is still readable and
        # the two badges are next to code rather than next to nothing.
        Q fold collapse --line 7 | Out-Null
        Q fold collapse --line 21 | Out-Null
    }

    '33-themes' = {
        Q tab open src/query.ts --permanent | Out-Null
        Q tab open src/theme.rs --permanent | Out-Null
        Q explorer expand src | Out-Null
        Q theme set themes-bundle-1/dracula | Out-Null
        Settle 800
    }

    # --- the menus -----------------------------------------------------------------------------------

    '18-file-menu' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q input click $Menu.File.X $Menu.File.Y | Out-Null
        Settle 700
    }

    '20-view-menu' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q input click $Menu.View.X $Menu.View.Y | Out-Null
        Settle 700
    }

    '06-git-menu' = {
        Q explorer expand src | Out-Null
        Q tab open src/version.ts --permanent | Out-Null
        Q input click $Menu.Git.X $Menu.Git.Y | Out-Null
        Settle 700
    }

    '05-explorer-menu' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        Q input click $ExplorerRow.X ($ExplorerRow.Y + $ExplorerRow.Height * 7) --right | Out-Null
        Settle 700
    }

    # --- the terminal ----------------------------------------------------------------------------------

    '07-terminal' = {
        Q tab open src/query.ts --permanent | Out-Null
        Q terminal show | Out-Null
        Settle 1800
        Q terminal send 'git log --oneline --graph --decorate --all' | Out-Null
        Settle 1800
        Q terminal new | Out-Null
        Settle 2000
        Q terminal send 'git status' | Out-Null
        Settle 2000
    }

    # --- git -------------------------------------------------------------------------------------------

    '15-git-commit' = {
        Q explorer expand src | Out-Null
        Q tab open src/version.ts --permanent | Out-Null
        Q git action commit --wait 10000 | Out-Null
        Settle 1200
    }

    '13-git-history' = {
        Q explorer expand src | Out-Null
        Q tab open src/query.ts --permanent | Out-Null
        Q git action show-history --wait 15000 | Out-Null
        Settle 1200
    }

    '14-git-diff' = {
        Q explorer expand src | Out-Null
        Q tab open src/version.ts --permanent | Out-Null
        Q git action show-diff --wait 15000 | Out-Null
        Settle 1200
    }

    '12-git-blame' = {
        Q explorer expand src | Out-Null
        Q tab open src/query.ts --permanent | Out-Null
        Q git action annotate --wait 15000 | Out-Null
        Settle 1500
    }

    # --- the settings -------------------------------------------------------------------------------------

    '08-settings-appearance' = {
        Q tab open readme.md --permanent | Out-Null
        Q modal open settings --page appearance | Out-Null
        Settle 900
    }

    '09-settings-plugins' = {
        Q tab open readme.md --permanent | Out-Null
        Q modal open settings --page plugins | Out-Null
        Settle 900
    }

    '36-settings-mcp' = {
        Q tab open readme.md --permanent | Out-Null
        Q modal open settings --page mcp | Out-Null
        Settle 900
    }

    # --- finding things -------------------------------------------------------------------------------------

    '31-command-palette' = {
        Q tab open src/layout.rs --permanent | Out-Null
        Q modal open command-palette --query fold | Out-Null
        Settle 900
    }

    '32-find-in-files' = {
        Q tab open readme.md --permanent | Out-Null
        Q modal open find-in-files --query export | Out-Null
        Settle 2000
    }

    '37-go-to-file' = {
        Q tab open readme.md --permanent | Out-Null
        Q modal open go-to-file --query ts | Out-Null
        Settle 900
    }

    # --- running and debugging --------------------------------------------------------------------------------

    '39-run' = {
        Q tab open app.js --permanent | Out-Null
        Try-Q run add numbers node app.js
        Q run start numbers | Out-Null
        Settle 4000
    }

    '30-debugger' = {
        Q tab open app.js --permanent | Out-Null
        Try-Q run add numbers node app.js
        Try-Q debug breakpoint add app.js 15
        Q debug start --wait-for-pause --timeout 90000 | Out-Null
        Settle 2500
    }

    # --- the canvas ---------------------------------------------------------------------------------------------

    '27-base-of-infinite-space' = {
        # The canvas is a panel like any other, so it is given the whole window by putting the two
        # that would share it away: the editing area, and the explorer.
        Q space show | Out-Null
        Q explorer hide | Out-Null
        Q action run toggle-editor | Out-Null
        Settle 1500
        $agent = (Q space add terminal --x 60 --y 60 --width 780 --height 470 --title 'An agent' --json | ConvertFrom-Json).result.node
        $editor = (Q space add editor --path src/theme.rs --x 880 --y 60 --width 780 --height 470 --json | ConvertFrom-Json).result.node
        $folder = (Q space add folder --x 60 --y 570 --width 480 --height 420 --json | ConvertFrom-Json).result.node
        $board = (Q space add tasks --x 580 --y 570 --width 1080 --height 420 --json | ConvertFrom-Json).result.node
        Q space connect $agent $editor | Out-Null
        Q space connect $agent $folder | Out-Null
        Q space connect $agent $board | Out-Null
        Settle 2500
        # What the terminal node prints is the canvas reading itself back, which is the whole of what
        # a connection is for: the agent in that node can act on the three it is wired to.
        Q space send $agent 'unluminous-cli space list' | Out-Null
        Settle 2500
        Q space camera --fit | Out-Null
        Settle 1500
    }

    # --- the agent panes -----------------------------------------------------------------------------------------

    '29-agent-tasks' = {
        Q tab open readme.md --permanent | Out-Null
        Q plugins pane agent-tasks/board --show | Out-Null
        Settle 900
        Q panel size agent-tasks/board --height 760 | Out-Null
        Settle 2000
    }

    # After every picture that shows the readme, because it is the one that changes the document it
    # is of. Formatting is not a text change, so a readme left formatted would arrive at the next
    # picture still bold, and the first run of this script proved it by putting a bold readme behind
    # twenty of them. `Reset-Window` closes that last tab now; this ordering is the second guard.
    '16-formatting' = {
        Q tab open readme.md --permanent | Out-Null
        Q editor view raw | Out-Null
        # Every one of these is a click on the same flyout, because the text options are on no menu
        # and have no action of their own: they belong to the file that is showing.
        Q editor select --from-line 1 --to-line 1 | Out-Null
        Q input click $FButton.X $FButton.Y | Out-Null; Settle 400
        Q input click $Flyout.Centre.X $Flyout.Centre.Y | Out-Null; Settle 300
        Q input click $Flyout.Blue.X $Flyout.Blue.Y | Out-Null; Settle 300
        Q input key Escape | Out-Null; Settle 300
        Q editor select --from-line 3 --to-line 5 | Out-Null
        Q input click $FButton.X $FButton.Y | Out-Null; Settle 400
        Q input click $Flyout.Bold.X $Flyout.Bold.Y | Out-Null; Settle 300
        Q input key Escape | Out-Null; Settle 300
        Q editor select --from-line 11 --to-line 14 | Out-Null
        Q input click $FButton.X $FButton.Y | Out-Null; Settle 400
        Q input click $Flyout.Green.X $Flyout.Green.Y | Out-Null; Settle 300
        Q input key Escape | Out-Null; Settle 400
        Q editor select --none | Out-Null
    }

    # --- the Database plugin ------------------------------------------------------------------------------------
    #
    # Nine pictures of a pane rather than of the editing area, so the explorer is put away and the
    # pane is given most of the window: a grid with seven columns in a 420 point column is a grid
    # nobody can read. `documentation/database.md` is the page they are on.

    'db-01-tree' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Open-TheTree
    }

    'db-02-grid' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q plugins run database open album | Out-Null
        Settle 1600
    }

    'db-03-console-select' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q plugins run database console library | Out-Null
        Settle 700
        Q plugins run database query "SELECT artist.name, artist.country, album.title, album.year, album.label FROM album JOIN artist ON artist.id = album.artist_id ORDER BY album.year DESC" | Out-Null
        Settle 2000
    }

    'db-04-pending-edits' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q plugins run database open album | Out-Null
        Settle 1600
        Q plugins run database set 2 note "remastered in 2024" | Out-Null
        Q plugins run database set 6 label "Sur Records" | Out-Null
        Q plugins run database delete-row 7 | Out-Null
        Settle 900
        Q plugins run database pending | Out-Null
        Settle 700
    }

    'db-05-after-submit' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q plugins run database open album | Out-Null
        Settle 1600
        Q plugins run database set 2 note "remastered in 2024" | Out-Null
        Q plugins run database set 6 label "Sur Records" | Out-Null
        Q plugins run database delete-row 7 | Out-Null
        Settle 700
        Q plugins run database submit | Out-Null
        Settle 2000
    }

    'db-06-ddl' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Open-TheTree
        # `plugins run database ddl` answers with the statement and opens nothing, which is right for
        # an agent asking a question. The modal is what a person gets, and it is on the tree's own
        # right click menu.
        Q input click $DatabaseTree.Label ($DatabaseTree.Y + $DatabaseTree.Height * 3) --right | Out-Null
        Settle 900
        Q input click $DatabaseTree.ShowDdl.X $DatabaseTree.ShowDdl.Y | Out-Null
        Settle 1200
    }

    'db-07-new-source' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q plugins run database add-source | Out-Null
        Settle 1200
    }

    'db-08-settings' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        # `modal open settings --page` names the five pages Unluminous itself has. A page a plugin
        # contributed is reached the way a person reaches it, by choosing it in the list.
        Q modal open settings --page plugins | Out-Null
        Settle 900
        Q input click $SettingsPage.Database.X $SettingsPage.Database.Y | Out-Null
        Settle 1200
    }

    'db-09-menu' = {
        Q tab open schema.sql --permanent | Out-Null
        Use-TheLibrary
        Q input click $Menu.Plugins.X $Menu.Plugins.Y | Out-Null
        Settle 700
    }

    # --- and last of all -----------------------------------------------------------------------------------
    #
    # **Last, because an agent leaves things in the folder it was started in.** The `claude` on this
    # machine is configured with an MCP server of its own, `inillucent-mcp --db app.rdb --root .`, so
    # a turn creates `app.rdb` and its log beside the project's own files — which is the pane doing
    # exactly what its page says it does, and two rows in the explorer no other picture has.

    '28-agent-chat' = {
        Q tab open src/theme.rs --permanent | Out-Null
        Q plugins pane agent-chat/chat --show | Out-Null
        Settle 1500
        Try-Q panel size agent-chat/chat --width 640
        if ($script:AskTheAgent) {
            Try-Q plugins run agent-chat send 'In one sentence, why would an editor keep its text buffer in a crate that cannot mention a user interface?'
            for ($i = 0; $i -lt 40; $i++) {
                Settle 2000
                if ((Q plugins run agent-chat state) -match 'finished|failed') { break }
            }
            # The answer is put in the status bar as it arrives, and a whole paragraph across the
            # bottom of the window is not what this picture is of.
            Try-Q window message
        }
        Settle 800
    }

}

# ---------------------------------------------------------------------------------------------

if ($List) { $Pictures.Keys | ForEach-Object { Write-Output $_ }; return }

if (-not (Test-Path $Cli)) { Write-Error "No Unluminous at $installed. Build and install it first, or set UNLUMINOUS_APP." }
if (-not (Test-Path $Backdrop)) { Write-Error "No backdrop at $Backdrop." }

$script:AskTheAgent = [bool]$Chat

Write-Output '==> the fixture'
& (Join-Path $PSScriptRoot 'fixture.ps1') -At $Fixture | Write-Output

$script:Library = Join-Path (Split-Path -Parent $Fixture) 'unluminous-docs-library.db'
$script:Shots = Join-Path $env:TEMP 'unluminous-docs-shots'
if (-not (Test-Path $script:Shots)) { New-Item -ItemType Directory -Path $script:Shots -Force | Out-Null }
if (-not (Test-Path $Images)) { New-Item -ItemType Directory -Path $Images -Force | Out-Null }

# A settings folder of its own, emptied first, so the window comes up in the product's own defaults
# and nothing is left behind in the real one.
$private = Join-Path $env:TEMP 'unluminous-docs-appdata'
if (Test-Path $private) { Remove-Item -Recurse -Force $private }
New-Item -ItemType Directory -Path (Join-Path $private 'Unluminous') -Force | Out-Null
$realAppData = $env:APPDATA
$env:APPDATA = $private

$failed = @()
try {
    Write-Output '==> the window'
    Start-TheWindow

    Write-Output '==> the board'
    # Filled once rather than by the picture that shows it, because the canvas has an Agent-Tasks node
    # on it as well and an empty board in either of them is a picture of nothing.
    $titles = @(
        'Read the passages nearest a question',
        'Give the retrieval branch a test of its own',
        'Colour a fenced block by the plugin that claims it',
        'Measure what one frame costs at 2 MB',
        'Write the migration down before running it')
    foreach ($title in $titles) { Try-Q plugins run agent-tasks new-task $title }
    Try-Q plugins run agent-tasks priority task-1 high
    Try-Q plugins run agent-tasks todo-add task-1 'Read the index back'
    Try-Q plugins run agent-tasks todo-add task-1 'Check the recall against an exhaustive scan'
    Try-Q plugins run agent-tasks todo-done task-1 1
    Try-Q plugins run agent-tasks move-task task-2 in_progress
    Try-Q plugins run agent-tasks todo-add task-2 'A scripted server rather than a real one'
    Try-Q plugins run agent-tasks move-task task-3 agent_done
    Try-Q plugins run agent-tasks move-task task-4 in_progress
    Try-Q plugins run agent-tasks priority task-5 low

    Write-Output '==> the library'
    # Added once rather than by each picture that needs it, because `add-source` adds another one
    # every time it is called and a tree holding `library 4` is a picture of this script rather than
    # of the product.
    Try-Q plugins run database add-source library $script:Library

    $wanted = if ($Only) { $Only } else { @($Pictures.Keys) }
    foreach ($name in $wanted) {
        if (-not $Pictures.Contains($name)) { throw "There is no picture called $name. -List prints them." }
        Write-Output "==> $name"
        try {
            if ($Fresh -contains $name) { Start-TheWindow }
            Reset-Window
            & $Pictures[$name]
            Settle
            Save-Shot $name
        }
        catch {
            # One picture that will not be taken is a picture that will not be taken. The rest are
            # still worth having, and the names of the ones that failed are printed at the end rather
            # than being lost in the scroll.
            Write-Warning "  $name failed: $($_.Exception.Message)"
            $failed += $name
        }
    }
}
finally {
    if ($script:Window -and -not $KeepOpen) {
        & $Cli --instance $script:Window quit *> $null
    }
    elseif ($script:Window) {
        Write-Output "The window is still running as $($script:Window). APPDATA for it is $private."
    }
    $env:APPDATA = $realAppData
}

if ($failed.Count -gt 0) {
    Write-Output ''
    Write-Output "$($failed.Count) did not come out: $($failed -join ', ')"
}
