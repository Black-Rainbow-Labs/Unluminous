# Put each of Excel's error codes into a cell with CVErr and print what Excel shows and what ERROR.TYPE answers.
#
# A formula cannot hold the newer errors as literals, so the cell is given the error value directly. The codes
# are the xlCVError values: 2000 #NULL!, 2007 #DIV/0!, 2015 #VALUE!, 2023 #REF!, 2029 #NAME?, 2036 #NUM!,
# 2042 #N/A, 2043 #GETTING_DATA, 2045 #SPILL!, 2046 #CONNECT!, 2047 #BLOCKED!, 2048 #UNKNOWN!, 2049 #FIELD!,
# 2050 #CALC!. Codes past those are tried as well.
#
#   pwsh tools/excel-oracle/error-types.ps1
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    foreach ($code in ((2000, 2007, 2015, 2023, 2029, 2036, 2042, 2043, 2045, 2046, 2047, 2048, 2049, 2050) + (2051..2080))) {
        try {
            # CVErr(n) is the HRESULT 0x800A0000 + n.
            $sheet.Range('A1').Value2 = [System.Runtime.InteropServices.ErrorWrapper]::new([int](-2146828288 + $code))
            $sheet.Range('B1').Formula2 = '=ERROR.TYPE(A1)'
            $sheet.Range('C1').Formula2 = '=ISERROR(A1)'
            $sheet.Range('D1').Formula2 = '=ISNA(A1)'
            $excel.Calculate()
            [pscustomobject]@{
                code = $code
                text = $sheet.Range('A1').Text
                errorType = "$($sheet.Range('B1').Value2)"
                isError = "$($sheet.Range('C1').Value2)"
                isNa = "$($sheet.Range('D1').Value2)"
            } | ConvertTo-Json -Compress
        } catch {
            [pscustomobject]@{ code = $code; refused = $_.Exception.Message } | ConvertTo-Json -Compress
        }
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
