# Run editing scenarios in Excel and write what Excel did, as JSON.
#
# A scenario file is a JSON array of cases. Each case builds cells on Sheet1 (and any other sheet named in an
# address, `Sheet2!A1`), performs steps through Excel's object model, and dumps the cells it asks for:
#
#   { "name": "...",
#     "sheets": ["Sheet1", "Sheet2"],                       (optional, extra sheets to make)
#     "cells": { "A1": 5, "B1": "text", "C1": { "f": "=A1*2", "nf": "0.00", "bold": true, "fill": 255 } },
#     "notes": { "A1": "a note" },
#     "validation": [ { "at": "A1:A3", "type": 1, "op": 1, "f1": "1", "f2": "10" } ],
#     "cf": [ { "at": "A1:A3", "op": 5, "f1": "3", "fill": 255 } ],
#     "widths": { "A": 20 },  "hidden_rows": [3],  "merge": ["A5:B5"],
#     "steps": [ { "do": "copy", "at": "A1:B3" }, { "do": "pastespecial", "at": "D1", "paste": -4104 } ],
#     "dump": ["A1:J12"] }
#
# Steps: copy, cut, paste, pastespecial (paste, op, skip, transpose), pastelink, insertcopied (shift),
# dataseries (at, rowcol, type, date, step, stop, trend), autofill (at, to, type), fillacross (at, sheets,
# type), justify, flashfill, filter (at, field, criteria), fill (at, dir), set (at, value).
#
# The answer is a JSON array with one object a case: `cells` (every cell with something in it), `widths`,
# `hidden_rows`, `error` (Excel's message when a step was refused).
#
#   pwsh tools/excel-oracle/edit-oracle.ps1 -Scenarios in.json -Out out.json
param(
    [Parameter(Mandatory = $true)][string]$Scenarios,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$cases = Get-Content -LiteralPath $Scenarios -Raw -Encoding UTF8 | ConvertFrom-Json
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$results = @()

function Get-Target($book, $address) {
    if ($address.Contains('!')) {
        $parts = $address.Split('!')
        return , $book.Worksheets.Item($parts[0]).Range($parts[1])
    }
    return , $book.Worksheets.Item(1).Range($address)
}

function Set-Cell($book, $address, $spec) {
    $cell = Get-Target $book $address
    $value = $spec
    if ($spec -is [pscustomobject]) {
        if ($spec.nf) { $cell.NumberFormat = $spec.nf }
        $value = if ($null -ne $spec.f) { $spec.f } else { $spec.v }
        if ($spec.bold) { $cell.Font.Bold = $true }
        if ($spec.italic) { $cell.Font.Italic = $true }
        if ($null -ne $spec.fill) { $cell.Interior.Color = [int]$spec.fill }
        if ($null -ne $spec.color) { $cell.Font.Color = [int]$spec.color }
        if ($spec.align) { $cell.HorizontalAlignment = [int]$spec.align }
        if ($spec.border) { $cell.Borders.Item(9).LineStyle = 1 }
    }
    if ($null -eq $value) { return }
    # PowerShell's COM binder keeps the type of the first value assigned to a property, so values go
    # through InvokeMember, which does not.
    $name = 'Value2'
    if ($value -is [string]) {
        if ($value.StartsWith('=')) { $name = 'Formula2' }
    } elseif ($value -isnot [bool]) {
        $value = [double]$value
    }
    [void][System.__ComObject].InvokeMember($name, [Reflection.BindingFlags]::SetProperty, $null, $cell, @($value))
    # Typing a formula makes Excel give the cell the number format of a cell it names. That is typing, not
    # what the scenarios are about, so a formula with no format of its own is put back to General.
    if ($name -eq 'Formula2' -and -not ($spec -is [pscustomobject] -and $spec.nf)) { $cell.NumberFormat = 'General' }
}

function Build-Case($book, $case) {
    foreach ($name in @($case.sheets)) {
        if ($name) {
            $sheet = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
            $sheet.Name = $name
        }
    }
    if ($case.cells) {
        foreach ($property in $case.cells.PSObject.Properties) { Set-Cell $book $property.Name $property.Value }
    }
    if ($case.notes) {
        foreach ($property in $case.notes.PSObject.Properties) {
            (Get-Target $book $property.Name).AddComment($property.Value) | Out-Null
        }
    }
    foreach ($v in @($case.validation)) {
        if (-not $v) { continue }
        $range = Get-Target $book $v.at
        $range.Validation.Delete()
        if ($v.f2) { $range.Validation.Add([int]$v.type, 1, [int]$v.op, [string]$v.f1, [string]$v.f2) | Out-Null }
        else { $range.Validation.Add([int]$v.type, 1, [int]$v.op, [string]$v.f1) | Out-Null }
    }
    foreach ($c in @($case.cf)) {
        if (-not $c) { continue }
        $range = Get-Target $book $c.at
        $condition = $range.FormatConditions.Add(1, [int]$c.op, [string]$c.f1)
        if ($null -ne $c.fill) { $condition.Interior.Color = [int]$c.fill }
    }
    if ($case.widths) {
        foreach ($property in $case.widths.PSObject.Properties) {
            $book.Worksheets.Item(1).Columns.Item($property.Name).ColumnWidth = [double]$property.Value
        }
    }
    foreach ($row in @($case.hidden_rows)) { if ($row) { $book.Worksheets.Item(1).Rows.Item([int]$row).Hidden = $true } }
    foreach ($m in @($case.merge)) { if ($m) { (Get-Target $book $m).Merge() | Out-Null } }
}

# Worksheet.Paste is a method with parameters; PowerShell reads it as a property, so it is invoked by name.
function Invoke-Paste($destination, [bool]$link) {
    $sheet = $destination.Worksheet
    $flags = [Reflection.BindingFlags]::InvokeMethod
    if ($link) {
        # Paste Link goes through the sheet's PasteSpecial on the selection.
        $sheet.Activate()
        $destination.Select() | Out-Null
        [void]$sheet.GetType().InvokeMember('Paste', $flags, $null, $sheet, @([Type]::Missing, $true))
    } else {
        $destination.PasteSpecial(-4104) | Out-Null
    }
}

function Invoke-Step($book, $step) {
    $at = if ($step.at) { Get-Target $book $step.at } else { $null }
    switch ($step.do) {
        'copy' { $at.Copy() | Out-Null }
        'cut' { $at.Cut() | Out-Null }
        'paste' { Invoke-Paste $at $false }
        'pastelink' { Invoke-Paste $at $true }
        'pastespecial' {
            $paste = if ($null -ne $step.paste) { [int]$step.paste } else { -4104 }
            $op = if ($null -ne $step.op) { [int]$step.op } else { -4142 }
            $skip = [bool]$step.skip
            $transpose = [bool]$step.transpose
            $at.PasteSpecial($paste, $op, $skip, $transpose) | Out-Null
        }
        'insertcopied' { $at.Insert([int]$step.shift) | Out-Null }
        'dataseries' {
            $rowcol = if ($null -ne $step.rowcol) { [int]$step.rowcol } else { 2 }
            $type = if ($null -ne $step.type) { [int]$step.type } else { -4132 }
            $date = if ($null -ne $step.date) { [int]$step.date } else { 1 }
            $stepValue = if ($null -ne $step.step) { [double]$step.step } else { 1.0 }
            $stop = if ($null -ne $step.stop) { [double]$step.stop } else { [Type]::Missing }
            $trend = [bool]$step.trend
            $at.DataSeries($rowcol, $type, $date, $stepValue, $stop, $trend) | Out-Null
        }
        'autofill' { $at.AutoFill((Get-Target $book $step.to), [int]$step.type) | Out-Null }
        'fillacross' {
            $names = @($step.sheets)
            $book.Worksheets.Item($names) | Out-Null
            $group = $book.Worksheets.Item($names)
            $group.Select()
            $group.FillAcrossSheets($at, [int]$step.type) | Out-Null
            $book.Worksheets.Item(1).Select()
        }
        'justify' { $at.Justify() | Out-Null }
        'flashfill' { $at.FlashFill() | Out-Null }
        'filter' {
            if ($step.criteria2) {
                $at.AutoFilter([int]$step.field, [string]$step.criteria, [int]$step.operator, [string]$step.criteria2) | Out-Null
            } else {
                $at.AutoFilter([int]$step.field, [string]$step.criteria) | Out-Null
            }
        }
        'fill' {
            switch ($step.dir) {
                'down' { $at.FillDown() | Out-Null }
                'right' { $at.FillRight() | Out-Null }
                'up' { $at.FillUp() | Out-Null }
                'left' { $at.FillLeft() | Out-Null }
            }
        }
        'set' { Set-Cell $book $step.at $step.value }
        default { throw "unknown step $($step.do)" }
    }
}

function Read-Cell($cell) {
    $value = $cell.Value2
    $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
    $shown = if ($kind -eq 'number') { $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } elseif ($kind -eq 'error') { $cell.Text } else { "$value" }
    $note = ''
    if ($null -ne $cell.Comment) { $note = $cell.Comment.Text() }
    $validation = -1
    try { $vt = $cell.Validation.Type; if ($null -ne $vt) { $validation = [int]$vt } } catch { }
    $fill = if ($cell.Interior.ColorIndex -eq -4142) { -1 } else { [int]$cell.Interior.Color }
    [pscustomobject]@{
        cell = $cell.Address($false, $false)
        kind = $kind
        value = $shown
        formula = if ($cell.HasFormula) { $cell.Formula2 } else { '' }
        nf = $cell.NumberFormat
        bold = [bool]$cell.Font.Bold
        italic = [bool]$cell.Font.Italic
        fill = $fill
        color = [int]$cell.Font.Color
        align = [int]$cell.HorizontalAlignment
        border = [int]$cell.Borders.Item(9).LineStyle
        note = $note
        validation = $validation
        cf = [int]$cell.FormatConditions.Count
        merged = [bool]$cell.MergeCells
    }
}

function Test-Notable($info) {
    return ($info.kind -ne 'empty') -or ($info.formula) -or ($info.nf -ne 'General') -or $info.bold -or $info.italic `
        -or ($info.fill -ne -1) -or ($info.color -ne 0) -or ($info.align -ne 1) -or ($info.border -ne -4142) `
        -or $info.note -or ($info.validation -ne -1) -or ($info.cf -gt 0) -or $info.merged
}

foreach ($case in $cases) {
    $book = $excel.Workbooks.Add()
    $error_text = ''
    try {
        Build-Case $book $case
        $excel.CutCopyMode = 0
        foreach ($step in @($case.steps)) {
            if (-not $step) { continue }
            try { Invoke-Step $book $step } catch { $error_text = $_.Exception.Message; break }
        }
        $excel.CutCopyMode = 0
        $dumped = @()
        foreach ($address in @($case.dump)) {
            $range = Get-Target $book $address
            $sheetName = $range.Worksheet.Name
            if ($case.formats -eq $false) {
                # Values and formulas only, read in bulk: much faster than one cell at a time.
                $values = $range.Value2
                $formulas = $range.Formula2
                $rows = $range.Rows.Count
                $cols = $range.Columns.Count
                $firstRow = $range.Row
                $firstCol = $range.Column
                for ($r = 1; $r -le $rows; $r++) {
                    for ($c = 1; $c -le $cols; $c++) {
                        if ($rows -eq 1 -and $cols -eq 1) { $value = $values; $formula = $formulas }
                        else { $value = $values[$r, $c]; $formula = $formulas[$r, $c] }
                        $hasFormula = ($formula -is [string]) -and $formula.StartsWith('=')
                        if ($null -eq $value -and -not $hasFormula) { continue }
                        $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
                        $shown = if ($kind -eq 'number') { $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } elseif ($kind -eq 'error') { $range.Cells.Item($r, $c).Text } else { "$value" }
                        $address = $range.Cells.Item($r, $c).Address($false, $false)
                        $dumped += [pscustomobject]@{
                            cell = $address; kind = $kind; value = $shown; formula = $(if ($hasFormula) { $formula } else { '' })
                            nf = 'General'; bold = $false; italic = $false; fill = -1; color = 0; align = 1; border = -4142
                            note = ''; validation = -1; cf = 0; merged = $false; sheet = $sheetName
                        }
                    }
                }
                continue
            }
            foreach ($cell in $range.Cells) {
                $info = Read-Cell $cell
                if (Test-Notable $info) {
                    $info | Add-Member -NotePropertyName sheet -NotePropertyValue $sheetName
                    $dumped += $info
                }
            }
        }
        $sheet = $book.Worksheets.Item(1)
        $widths = [ordered]@{}
        foreach ($letter in 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H') { $widths[$letter] = [double]$sheet.Columns.Item($letter).ColumnWidth }
        $hidden = @()
        for ($row = 1; $row -le 20; $row++) { if ($sheet.Rows.Item($row).Hidden) { $hidden += $row } }
        $results += [pscustomobject]@{ name = $case.name; error = $error_text; cells = $dumped; widths = $widths; hidden_rows = $hidden }
    } catch {
        $results += [pscustomobject]@{ name = $case.name; error = "HARNESS: $($_.Exception.Message) at $($_.ScriptStackTrace)"; cells = @(); widths = @{}; hidden_rows = @() }
    } finally {
        $book.Close($false)
    }
}
try { $excel.Quit() } catch { }
[System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
$json = ConvertTo-Json -InputObject @($results) -Depth 6
[IO.File]::WriteAllText($Out, $json, (New-Object Text.UTF8Encoding($false)))
