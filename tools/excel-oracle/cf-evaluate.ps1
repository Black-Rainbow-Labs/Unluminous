# Ask Excel which formats its conditional formats give each cell.
#
# Every case is a sheet: some values in column A (and helper values in column B), one or more rules made
# through Excel's object model, each with a fill of its own, and then for every cell what
# `Range.DisplayFormat` says it looks like. The workbook is saved so the engine can read the rules back,
# and the answers are written as one JSON object a case:
#
#   {"case": "...", "today": 46318, "cells": [{"row": 1, "fill": 255, "font": null, "bold": false, "italic": false}]}
#
# Colours are Excel's own BGR integers. Icon sets and data bars cannot be read back this way.
#
#   pwsh tools/excel-oracle/cf-evaluate.ps1 -Workbook cf-evaluate.xlsx > cf-evaluate.jsonl
param([Parameter(Mandatory = $true)][string]$Workbook, [string]$Only)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

# One fill for each rule of a case, so the answer says which rule won.
$palette = @(0x0000FF, 0x00FF00, 0xFF0000, 0x00FFFF, 0xFF00FF, 0xFFFF00, 0x0080FF, 0x80FF00)

$errorValue = '#ERR'
$trueValue = '#TRUE'

# xlFormatConditionOperator
$op = @{ between = 1; notBetween = 2; equal = 3; notEqual = 4; greater = 5; less = 6; greaterEqual = 7; lessEqual = 8 }

function Add-Cell($rng, $fill) {
    $rng.Interior.Color = $fill
}

# Fill column A (or B) from a list of values. $null is empty, '#ERR' an error, '#TRUE' a logical, a
# hashtable with `date` a date serial, anything else is written as it is.
function Put-Values($sheet, $col, $values) {
    for ($i = 0; $i -lt $values.Count; $i++) {
        $v = $values[$i]
        $cell = $sheet.Cells.Item($i + 1, $col)
        if ($null -eq $v) { continue }
        elseif ($v -is [hashtable]) { $cell.Value2 = [double]$v.date; $cell.NumberFormat = 'yyyy-mm-dd' }
        elseif ($v -is [string] -and $v -eq $errorValue) { $cell.Formula = '=1/0' }
        elseif ($v -is [string] -and $v -eq $trueValue) { $cell.Value2 = $true }
        elseif ($v -is [string]) { $cell.NumberFormat = '@'; $cell.Value2 = $v }
        else { $cell.Value2 = [double]$v }
    }
}

$numbers = @(1, 5, 10, 'abc', 'ABC', $trueValue, $null, 5.5, -3, $errorValue, 'Apple', 0)
$ranked = @(10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 5, 1)
$ties = @(5, 9, 9, 7, 3, 9, 1, 7, 2, 8, $null, 'text')
$words = @('Apple', 'apricot', 'grape', 'AP', 'xapx', $null, 5, 'a?', 'a*e', 'Pineapple', 'banana', '~ap')

function Rule-Fills($fc, $index) { $fc.Interior.Color = $palette[$index] }

# Each case: values for column A, optionally column B, and a script block that makes the rules on A1:A<n>.
$cases = [ordered]@{}

function Add-Case($name, $values, $build, $helper = $null, $area = $null) { $script:cases[$name] = @{ values = $values; helper = $helper; build = $build; area = $area } }

# --- cell value rules
# A threshold that is a formula reads a helper cell. Excel refuses it when other colour scales were made first
# in the same workbook, so this case comes first.
Add-Case 'scale-formula' $ranked { param($r) ScaleWithFormula $r 2 @(4, 2) @('=$B$1', $null) @($low, $high) } @(20, 80)
Add-Case 'cellis-between' $numbers { param($r) $f = $r.FormatConditions.Add(1, 1, '=2', '=6'); Rule-Fills $f 0 }
Add-Case 'cellis-not-between' $numbers { param($r) $f = $r.FormatConditions.Add(1, 2, '=2', '=6'); Rule-Fills $f 0 }
Add-Case 'cellis-between-reversed' $numbers { param($r) $f = $r.FormatConditions.Add(1, 1, '=6', '=2'); Rule-Fills $f 0 }
Add-Case 'cellis-equal' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-not-equal' $numbers { param($r) $f = $r.FormatConditions.Add(1, 4, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-greater' $numbers { param($r) $f = $r.FormatConditions.Add(1, 5, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-less' $numbers { param($r) $f = $r.FormatConditions.Add(1, 6, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-greater-equal' $numbers { param($r) $f = $r.FormatConditions.Add(1, 7, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-less-equal' $numbers { param($r) $f = $r.FormatConditions.Add(1, 8, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-less-zero' $numbers { param($r) $f = $r.FormatConditions.Add(1, 6, '=0'); Rule-Fills $f 0 }
Add-Case 'cellis-equal-zero' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '=0'); Rule-Fills $f 0 }
Add-Case 'cellis-equal-text' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '="abc"'); Rule-Fills $f 0 }
Add-Case 'cellis-greater-text' $numbers { param($r) $f = $r.FormatConditions.Add(1, 5, '="b"'); Rule-Fills $f 0 }
Add-Case 'cellis-less-text' $numbers { param($r) $f = $r.FormatConditions.Add(1, 6, '="b"'); Rule-Fills $f 0 }
Add-Case 'cellis-equal-true' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '=TRUE'); Rule-Fills $f 0 }
Add-Case 'cellis-equal-cell' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '=$B$1'); Rule-Fills $f 0 } @(5)
Add-Case 'cellis-greater-relative' $numbers { param($r) $f = $r.FormatConditions.Add(1, 5, '=B1'); Rule-Fills $f 0 } @(4, 4, 11, 4, 4, 4, 4, 6, 4, 4, 4, 4)
Add-Case 'cellis-equal-formula' $numbers { param($r) $f = $r.FormatConditions.Add(1, 3, '=2+3'); Rule-Fills $f 0 }

# --- formula rules
Add-Case 'expression-greater' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1>5'); Rule-Fills $f 0 }
Add-Case 'expression-rows' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=MOD(ROW(),2)=0'); Rule-Fills $f 0 }
Add-Case 'expression-and' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=AND(A1>0,A1<10)'); Rule-Fills $f 0 }
Add-Case 'expression-isnumber' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=ISNUMBER(A1)'); Rule-Fills $f 0 }
Add-Case 'expression-number-result' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1'); Rule-Fills $f 0 }
Add-Case 'expression-other-column' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=$B1>A1'); Rule-Fills $f 0 } @(2, 9, 10, 1, 1, 1, 1, 6, 0, 0, 1, 1)
Add-Case 'expression-absolute' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1>$A$2'); Rule-Fills $f 0 }
Add-Case 'expression-text' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=LEN(A1)=3'); Rule-Fills $f 0 }

# --- text rules (xlTextString = 9; text operators contains 0, does not contain 1, begins 2, ends 3)
Add-Case 'text-contains' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'ap', 0); Rule-Fills $f 0 }
Add-Case 'text-not-contains' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'ap', 1); Rule-Fills $f 0 }
Add-Case 'text-begins' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'ap', 2); Rule-Fills $f 0 }
Add-Case 'text-ends' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'ap', 3); Rule-Fills $f 0 }
Add-Case 'text-contains-wildcard' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'a*e', 0); Rule-Fills $f 0 }
Add-Case 'text-contains-question' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', 'a?p', 0); Rule-Fills $f 0 }
Add-Case 'text-contains-tilde' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', '~ap', 0); Rule-Fills $f 0 }
Add-Case 'text-contains-number' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', '5', 0); Rule-Fills $f 0 }
Add-Case 'text-contains-empty' $words { param($r) $f = $r.FormatConditions.Add(9, 0, '', '', '', 0); Rule-Fills $f 0 }

# --- blanks and errors (xlBlanksCondition 10, xlNoBlanksCondition 13, xlErrorsCondition 16, xlNoErrorsCondition 17)
$mixed = @(1, $null, 'x', ' ', '', $errorValue, 0, $trueValue, 'abc', $null, 7, ' a ')
Add-Case 'blanks' $mixed { param($r) $f = $r.FormatConditions.Add(10); Rule-Fills $f 0 }
Add-Case 'no-blanks' $mixed { param($r) $f = $r.FormatConditions.Add(13); Rule-Fills $f 0 }
Add-Case 'errors' $mixed { param($r) $f = $r.FormatConditions.Add(16); Rule-Fills $f 0 }
Add-Case 'no-errors' $mixed { param($r) $f = $r.FormatConditions.Add(17); Rule-Fills $f 0 }

# --- top and bottom (xlTop10Top 1, xlTop10Bottom 0)
function Top($r, $where, $rank, $percent) { $f = $r.FormatConditions.AddTop10(); $f.TopBottom = $where; $f.Rank = $rank; $f.Percent = $percent; Rule-Fills $f 0 }
Add-Case 'top-3' $ranked { param($r) Top $r 1 3 $false }
Add-Case 'bottom-3' $ranked { param($r) Top $r 0 3 $false }
Add-Case 'top-20-percent' $ranked { param($r) Top $r 1 20 $true }
Add-Case 'bottom-25-percent' $ranked { param($r) Top $r 0 25 $true }
Add-Case 'top-1-percent' $ranked { param($r) Top $r 1 1 $true }
Add-Case 'top-50' $ranked { param($r) Top $r 1 50 $false }
Add-Case 'top-ties' $ties { param($r) Top $r 1 2 $false }
Add-Case 'bottom-ties' $ties { param($r) Top $r 0 3 $false }
Add-Case 'top-10-percent-of-ten' $ranked { param($r) Top $r 1 10 $true }
Add-Case 'top-15-percent' $ranked { param($r) Top $r 1 15 $true }
Add-Case 'top-33-percent' $ranked { param($r) Top $r 1 33 $true }
Add-Case 'top-with-text' $numbers { param($r) Top $r 1 2 $false }

# --- above and below average (xlAboveAverage 0, xlBelowAverage 1, xlEqualAboveAverage 2, xlEqualBelowAverage 3, xlAboveStdDev 4, xlBelowStdDev 5)
function Average($r, $kind, $deviations) { $f = $r.FormatConditions.AddAboveAverage(); $f.AboveBelow = $kind; if ($deviations) { $f.NumStdDev = $deviations }; Rule-Fills $f 0 }
$spread = @(2, 4, 4, 4, 5, 5, 7, 9, 5, 5, 3, 7)
Add-Case 'average-above' $spread { param($r) Average $r 0 0 }
Add-Case 'average-below' $spread { param($r) Average $r 1 0 }
Add-Case 'average-equal-above' $spread { param($r) Average $r 2 0 }
Add-Case 'average-equal-below' $spread { param($r) Average $r 3 0 }
Add-Case 'average-above-1sd' $spread { param($r) Average $r 4 1 }
Add-Case 'average-above-2sd' $spread { param($r) Average $r 4 2 }
Add-Case 'average-below-1sd' $spread { param($r) Average $r 5 1 }
Add-Case 'average-below-2sd' $spread { param($r) Average $r 5 2 }
Add-Case 'average-above-3sd' @(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 100) { param($r) Average $r 4 3 }
Add-Case 'average-with-text' $numbers { param($r) Average $r 0 0 }
Add-Case 'average-equal-on-mean' @(2, 4, 6, 4, 2, 6) { param($r) Average $r 2 0 }

# --- unique and duplicate (xlUnique 0, xlDuplicate 1)
function Dupes($r, $kind) { $f = $r.FormatConditions.AddUniqueValues(); $f.DupeUnique = $kind; Rule-Fills $f 0 }
$repeats = @(1, 2, 2, 'a', 'A', 'b', '1', $null, $null, 3, 3, 3)
Add-Case 'duplicates' $repeats { param($r) Dupes $r 1 }
Add-Case 'uniques' $repeats { param($r) Dupes $r 0 }
Add-Case 'duplicates-errors' @($errorValue, $errorValue, 1, 1, 'x', $trueValue, $trueValue, 0, 0.0, $null, 'x ', ' x') { param($r) Dupes $r 1 }
Add-Case 'uniques-numbers' @(1, 2, 3, 1, 2, 4, 5, 5, 6, 7, 8, 8) { param($r) Dupes $r 0 }

# --- colour scales
function Scale($r, $count, $types, $values, $colors) {
    $f = $r.FormatConditions.AddColorScale($count)
    for ($i = 1; $i -le $count; $i++) {
        $c = $f.ColorScaleCriteria.Item($i)
        $c.Type = $types[$i - 1]
        if ($null -ne $values[$i - 1]) { $c.Value = $values[$i - 1] }
        $c.FormatColor.Color = $colors[$i - 1]
    }
}
# A separate function, because the first value PowerShell sets through a call site fixes the type of every later one.
function ScaleWithFormula($r, $count, $types, $values, $colors) {
    $f = $r.FormatConditions.AddColorScale($count)
    for ($i = 1; $i -le $count; $i++) {
        $c = $f.ColorScaleCriteria.Item($i)
        $c.Type = $types[$i - 1]
        if ($null -ne $values[$i - 1]) { [void][System.__ComObject].InvokeMember("Value", [Reflection.BindingFlags]::SetProperty, $null, $c, @($values[$i - 1])) }
        $c.FormatColor.Color = $colors[$i - 1]
    }
}
$low = 0x0000FF; $mid = 0x00FFFF; $high = 0x00FF00
Add-Case 'scale-2-min-max' $ranked { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }
Add-Case 'scale-3-percentile' $ranked { param($r) Scale $r 3 @(1, 5, 2) @($null, 50, $null) @($low, $mid, $high) }
Add-Case 'scale-3-percent' $ranked { param($r) Scale $r 3 @(1, 3, 2) @($null, 25, $null) @($low, $mid, $high) }
Add-Case 'scale-3-number' $ranked { param($r) Scale $r 3 @(0, 0, 0) @(20, 55, 90) @($low, $mid, $high) }
Add-Case 'scale-number-outside' $ranked { param($r) Scale $r 2 @(0, 0) @(-100, 500) @($low, $high) }
Add-Case 'scale-percentile-90' $ranked { param($r) Scale $r 2 @(5, 5) @(10, 90) @($low, $high) }
Add-Case 'scale-percentile-extremes' $ranked { param($r) Scale $r 2 @(5, 5) @(0, 100) @($low, $high) }
Add-Case 'scale-equal-values' @(5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5) { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }
Add-Case 'scale-negative' @(-10, -5, 0, 5, 10, 15, -20, 20, 1, 2, 3, 4) { param($r) Scale $r 3 @(1, 0, 2) @($null, 0, $null) @($low, $mid, $high) }
Add-Case 'scale-with-text-and-blank' $numbers { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }
Add-Case 'scale-mid-below-min' $ranked { param($r) Scale $r 3 @(0, 0, 0) @(50, 20, 90) @($low, $mid, $high) }
Add-Case 'scale-two-rules' $ranked { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high); Scale $r 2 @(1, 2) @($null, $null) @($mid, $high) }
Add-Case 'scale-percent-100' $ranked { param($r) Scale $r 2 @(3, 3) @(0, 50) @($low, $high) }

# --- time periods (xlTimePeriod 11); the values are dates around today
$offsets = @(-40, -31, -15, -8, -7, -6, -3, -2, -1, 0, 1, 2, 3, 6, 7, 8, 15, 31, 40)
$periods = [ordered]@{ today = 0; yesterday = 1; last7Days = 2; thisWeek = 3; lastWeek = 4; lastMonth = 5; tomorrow = 6; nextWeek = 7; nextMonth = 8; thisMonth = 9 }

# --- a rule over cells that are not next to each other counts them together
Add-Case 'top-two-areas' $ranked { param($r) Top $r 1 3 $false } $null 'A1:A4,A9:A12'
Add-Case 'bottom-percent-two-areas' $ranked { param($r) Top $r 0 25 $true } $null 'A1:A4,A9:A12'
Add-Case 'scale-two-areas' $ranked { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) } $null 'A1:A4,A9:A12'
Add-Case 'duplicates-two-areas' @(1, 2, 3, 4, 9, 9, 9, 9, 1, 2, 7, 8) { param($r) Dupes $r 1 } $null 'A1:A4,A9:A12'
Add-Case 'average-two-areas' $ranked { param($r) Average $r 0 0 } $null 'A1:A4,A9:A12'
Add-Case 'expression-two-areas' $ranked { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1>A2'); Rule-Fills $f 0 } $null 'A1:A4,A9:A12'

# --- rules that name another sheet, which Excel stores in the extension

# --- more comparisons
Add-Case 'cellis-equal-text-number' @(5, '5', 'five', $null, 5.0, '5 ', 50) { param($r) $f = $r.FormatConditions.Add(1, 3, '="5"'); Rule-Fills $f 0 }
Add-Case 'cellis-equal-number-text' @(5, '5', 'five', $null, 5.0, '5 ', 50) { param($r) $f = $r.FormatConditions.Add(1, 3, '=5'); Rule-Fills $f 0 }
Add-Case 'cellis-greater-empty-string' $numbers { param($r) $f = $r.FormatConditions.Add(1, 5, '=""'); Rule-Fills $f 0 }
Add-Case 'cellis-less-true' $numbers { param($r) $f = $r.FormatConditions.Add(1, 6, '=TRUE'); Rule-Fills $f 0 }
Add-Case 'cellis-between-text' $numbers { param($r) $f = $r.FormatConditions.Add(1, 1, '="a"', '="b"'); Rule-Fills $f 0 }
Add-Case 'cellis-between-number-text' $numbers { param($r) $f = $r.FormatConditions.Add(1, 1, '=0', '="b"'); Rule-Fills $f 0 }
Add-Case 'expression-next-row' $ranked { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1>A2'); Rule-Fills $f 0 }
Add-Case 'expression-row-above' $ranked { param($r) $f = $r.FormatConditions.Add(2, 0, '=AND(ROW()>1,A1>OFFSET(A1,-1,0))'); Rule-Fills $f 0 }
Add-Case 'expression-error-result' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=1/A1'); Rule-Fills $f 0 }
Add-Case 'expression-text-result' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '="TRUE"'); Rule-Fills $f 0 }
Add-Case 'expression-text-true' $numbers { param($r) $f = $r.FormatConditions.Add(2, 0, '=A1&""'); Rule-Fills $f 0 }

# --- more top, bottom, average, duplicate and scale cases
Add-Case 'top-negative' @(-5, -1, -10, 0, -3, -20, 4, -7, -2, 3, -8, -6) { param($r) Top $r 1 4 $false }
Add-Case 'top-all-equal' @(7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7) { param($r) Top $r 1 3 $false }
Add-Case 'top-one-percent-small-list' @(3, 8, 5) { param($r) Top $r 1 1 $true }
Add-Case 'top-100-percent' $ranked { param($r) Top $r 1 100 $true }
Add-Case 'bottom-single' @(42) { param($r) Top $r 0 1 $false }
Add-Case 'average-single' @(42) { param($r) Average $r 0 0 }
Add-Case 'average-equal-single' @(42) { param($r) Average $r 2 0 }
Add-Case 'average-below-2sd' @(10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, -80) { param($r) Average $r 5 2 }
Add-Case 'duplicates-dates' @(@{ date = 46000 }, @{ date = 46000 }, @{ date = 46001 }, 46000, 46002, $null, $null, 'x', 'X', 0, 0, 0.5) { param($r) Dupes $r 1 }
Add-Case 'uniques-text-spaces' @('a', 'a ', ' a', 'A', 'b', 'B', 'c', 'c', '1', 1, '01', 1.0) { param($r) Dupes $r 0 }
Add-Case 'scale-one-number' @(5, 'x', $null, 'y') { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }
Add-Case 'scale-percent-negative' @(-100, -50, 0, 50, 100, 25, -25, 75, -75, 10, -10, 90) { param($r) Scale $r 3 @(1, 3, 2) @($null, 50, $null) @($low, $mid, $high) }
Add-Case 'scale-percentile-fraction' @(1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048) { param($r) Scale $r 3 @(1, 5, 2) @($null, 33, $null) @($low, $mid, $high) }
Add-Case 'scale-percentile-low-high' @(1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048) { param($r) Scale $r 2 @(5, 5) @(25, 75) @($low, $high) }
Add-Case 'scale-number-equal-stops' $ranked { param($r) Scale $r 3 @(0, 0, 0) @(40, 40, 70) @($low, $mid, $high) }
Add-Case 'scale-number-all-equal-stops' $ranked { param($r) Scale $r 2 @(0, 0) @(50, 50) @($low, $high) }
Add-Case 'scale-descending-stops' $ranked { param($r) Scale $r 2 @(0, 0) @(80, 20) @($low, $high) }
Add-Case 'scale-dates' @(@{ date = 46000 }, @{ date = 46010 }, @{ date = 46020 }, @{ date = 46030 }, @{ date = 46040 }) { param($r) Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }

# --- several rules on one range
Add-Case 'priority-first-wins' $numbers { param($r) $a = $r.FormatConditions.Add(1, 5, '=0'); Rule-Fills $a 0; $b = $r.FormatConditions.Add(1, 5, '=4'); Rule-Fills $b 1 }
Add-Case 'priority-set-first' $numbers { param($r) $a = $r.FormatConditions.Add(1, 5, '=0'); Rule-Fills $a 0; $b = $r.FormatConditions.Add(1, 5, '=4'); Rule-Fills $b 1; $b.SetFirstPriority() }
Add-Case 'stop-if-true' $numbers { param($r) $a = $r.FormatConditions.Add(1, 5, '=4'); Rule-Fills $a 0; $a.StopIfTrue = $true; $b = $r.FormatConditions.Add(1, 5, '=0'); Rule-Fills $b 1; $b.StopIfTrue = $false }
Add-Case 'merge-properties' $numbers { param($r) $a = $r.FormatConditions.Add(1, 5, '=4'); Rule-Fills $a 0; $b = $r.FormatConditions.Add(1, 5, '=0'); $b.Font.Bold = $true; $b.Font.Color = 0x00FF00 }
Add-Case 'merge-conflict' $numbers { param($r) $a = $r.FormatConditions.Add(1, 5, '=4'); $a.Font.Color = 0x0000FF; $b = $r.FormatConditions.Add(1, 5, '=0'); $b.Font.Color = 0x00FF00; $b.Font.Italic = $true; Rule-Fills $b 1 }
Add-Case 'scale-and-rule' $ranked { param($r) $a = $r.FormatConditions.Add(1, 5, '=50'); $a.Font.Bold = $true; Scale $r 2 @(1, 2) @($null, $null) @($low, $high) }
Add-Case 'rule-over-scale' $ranked { param($r) $a = $r.FormatConditions.Add(1, 5, '=50'); Rule-Fills $a 0; Scale $r 2 @(1, 2) @($null, $null) @($low, $high); $a.SetFirstPriority() }
Add-Case 'scale-over-rule' $ranked { param($r) $a = $r.FormatConditions.Add(1, 5, '=50'); Rule-Fills $a 0; Scale $r 2 @(1, 2) @($null, $null) @($low, $high); $r.FormatConditions.Item(2).SetFirstPriority() }
Add-Case 'top-and-average' $ranked { param($r) Top $r 1 3 $false; $b = $r.FormatConditions.AddAboveAverage(); $b.Interior.Color = $palette[1] }

$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $today = [int]$excel.Evaluate('TODAY()')
    $first = $true
    $everything = [ordered]@{}
    foreach ($k in $cases.Keys) { $everything[$k] = $cases[$k] }
    foreach ($p in $periods.Keys) {
        $dates = @($offsets | ForEach-Object { @{ date = $today + $_ } }) + @(@{ date = $today + 0.75 }, @{ date = $today - 1 + 0.25 })
        $everything["time-$p"] = @{ values = $dates; helper = $null; period = $periods[$p] }
    }
    foreach ($name in $everything.Keys) {
        if ($Only -and $name -ne $Only) { continue }
        $case = $everything[$name]
        if ($first) { $sheet = $book.Worksheets.Item(1); $first = $false } else { $sheet = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) }
        $sheet.Name = $name
        Put-Values $sheet 1 $case.values
        if ($case.helper) { Put-Values $sheet 2 $case.helper }
        $count = $case.values.Count
        $address = "A1:A$count"
        if ($case.area) { $address = $case.area }
        $range = $sheet.Range($address)
        try {
            if ($case.period -ne $null) {
                $f = $range.FormatConditions.Add(11, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, $case.period)
                $f.Interior.Color = $palette[0]
            } else {
                & $case.build $range
            }
        } catch {
            [pscustomobject]@{ case = $name; error = $_.Exception.Message; at = $_.InvocationInfo.PositionMessage } | ConvertTo-Json -Compress
            continue
        }
        $excel.Calculate()
        $cells = @()
        for ($row = 1; $row -le $count; $row++) {
            $df = $sheet.Cells.Item($row, 1).DisplayFormat
            $fill = if ($df.Interior.Pattern -eq -4142) { $null } else { [int]$df.Interior.Color }
            $font = if ($df.Font.ColorIndex -eq -4105) { $null } else { [int]$df.Font.Color }
            $cells += [pscustomobject]@{ row = $row; fill = $fill; font = $font; bold = [bool]($df.Font.Bold -eq $true); italic = [bool]($df.Font.Italic -eq $true) }
        }
        [pscustomobject]@{ case = $name; today = $today; cells = $cells } | ConvertTo-Json -Compress -Depth 5
    }
    $book.SaveAs($Workbook, 51)
    $book.Close($false)
} finally {
    [Console]::Error.WriteLine("quitting"); $excel.Quit(); [Console]::Error.WriteLine("quit")
    [Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
