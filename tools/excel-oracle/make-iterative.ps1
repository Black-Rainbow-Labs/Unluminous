# Builds the workbooks that check iterative calculation and precision as displayed, by letting Excel
# calculate them. Formulas are entered with iteration off, which gives each circular cell the value of one
# evaluation. Iteration is then switched on, the values every formula cell holds are written to a
# `.start.json` file beside the workbook, and the workbook is calculated once. Excel saves the result.
param([Parameter(Mandatory = $true)][string]$OutputFolder)
$ErrorActionPreference = 'Stop'
$xlOpenXMLWorkbook = 51
$xlCalculationManual = -4135
$xlCalculationAutomatic = -4105

# Enters the circular references. Each group is a different case of the order Excel calculates in.
function New-Cycles($sheet, $other) {
    $sheet.Range('A1').Formula = '=B1+1'
    $sheet.Range('B1').Formula = '=A1*0.5'
    $sheet.Range('D1').Formula = '=D2+1'
    $sheet.Range('D2').Formula = '=D1*0.5'
    $sheet.Range('F1').Formula = '=E2+1'
    $sheet.Range('E2').Formula = '=F1*0.5'
    $sheet.Range('H1').Formula = '=H1+1'
    $sheet.Range('I1').Formula = '=I1/2+1'
    $sheet.Range('K1').Formula = "='$($other.Name)'!A1*0.5"
    $other.Range('A1').Formula = "='$($sheet.Name)'!K1+1"
    $sheet.Range('L1').Formula = '=A1+B1'
    $sheet.Range('N1').Formula = '=MAX(O1,0)+1'
    $sheet.Range('O1').Formula = '=N1*0.9'
    $sheet.Range('Q1').Formula = '=R1+S1'
    $sheet.Range('R1').Formula = '=Q1*0.25+1'
    $sheet.Range('S1').Formula = '=Q1*0.25+2'
}

# The value of every formula cell before the calculation, which is where Excel starts iterating from.
function Get-Starts($book) {
    $starts = [ordered]@{}
    foreach ($sheet in $book.Worksheets) {
        foreach ($cell in $sheet.UsedRange.Cells) {
            if ($cell.HasFormula) { $starts["$($sheet.Name)!$($cell.Address($false, $false))"] = [double]$cell.Value2 }
        }
    }
    return $starts
}

function Save-Cycles($excel, $path, $count, $delta) {
    $book = $excel.Workbooks.Add()
    $excel.CalculateBeforeSave = $false
    $excel.Iteration = $false
    $excel.Calculation = $xlCalculationAutomatic
    $sheet = $book.Worksheets.Item(1)
    $sheet.Name = 'Loop'
    $other = $book.Worksheets.Add([Type]::Missing, $sheet)
    $other.Name = 'Other'
    New-Cycles $sheet $other
    $excel.Calculation = $xlCalculationManual
    $excel.Iteration = $true
    $excel.MaxIterations = $count
    $excel.MaxChange = $delta
    $starts = Get-Starts $book
    $excel.CalculateFull()
    $startFile = $path -replace '\.xlsx$', '.start.json'
    $starts | ConvertTo-Json | Set-Content -LiteralPath $startFile -Encoding UTF8
    $book.SaveAs($path, $xlOpenXMLWorkbook)
    $book.Close($false)
}

function Save-Precision($excel, $path) {
    $book = $excel.Workbooks.Add()
    $excel.Iteration = $false
    $excel.Calculation = $xlCalculationAutomatic
    $book.PrecisionAsDisplayed = $true
    $sheet = $book.Worksheets.Item(1)
    $sheet.Name = 'Shown'
    $cases = @(
        @('A1', '0.00', '1.23456'), @('A2', '0.0%', '0.12345'), @('A3', '#,##0', '1234.5'),
        @('A4', '0', '2.5'), @('A5', '0.000', '-2.0005'), @('A6', '#,##0.0,', '123456'),
        @('A7', '$#,##0.00', '9.995'), @('A8', 'General', '0.1234567890123456'),
        @('A9', '0.00E+00', '12345.6789'), @('A10', '0.00', '=1/3'), @('A11', '0%', '=2/3'),
        @('A12', '0.0', '=A1*3'), @('A13', '# ?/?', '0.3333'), @('A14', 'm/d/yyyy', '46023.75')
    )
    foreach ($case in $cases) {
        $cell = $sheet.Range($case[0])
        $cell.NumberFormat = $case[1]
        $cell.Formula = $case[2]
    }
    $book.SaveAs($path, $xlOpenXMLWorkbook)
    $book.Close($false)
}

$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    New-Item -ItemType Directory -Force -Path $OutputFolder | Out-Null
    $OutputFolder = (Resolve-Path -LiteralPath $OutputFolder).Path
    Save-Cycles $excel (Join-Path $OutputFolder 'iterative.xlsx') 100 0.001
    Save-Cycles $excel (Join-Path $OutputFolder 'iterative-few.xlsx') 5 0.0000001
    Save-Precision $excel (Join-Path $OutputFolder 'precision.xlsx')
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
