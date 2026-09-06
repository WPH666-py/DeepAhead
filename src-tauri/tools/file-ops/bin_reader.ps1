param(
    [Parameter(Mandatory=$true)]
    [string]$Path,

    [int]$StartLine = 0,
    [int]$EndLine = 0
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

if (-not (Test-Path $Path)) {
    Write-Error "ERROR: file not found: $Path"
    exit 1
}

$ext = [IO.Path]::GetExtension($Path).ToLower()
$lines = @()

function Unescape-PdfLiteral([string]$text) {
    # 反转义 PDF 字符串中的 \n \r \t \( \) \\
    $result = $text
    $result = $result -replace '\\\\', [char]1
    $result = $result -replace '\\n', "`n"
    $result = $result -replace '\\r', "`r"
    $result = $result -replace '\\t', "`t"
    $result = $result -replace '\\\(', '('
    $result = $result -replace '\\\)', ')'
    $result = $result -replace [char]1, '\'
    return $result
}

function Decode-HexString([string]$hex) {
    $hex = $hex -replace '\s', ''
    if ($hex.Length -eq 0) { return '' }
    if ($hex.Length % 2 -eq 1) { $hex = $hex.Substring(0, $hex.Length - 1) }
    $bytes = @()
    for ($i = 0; $i -lt $hex.Length; $i += 2) {
        try { $bytes += [Convert]::ToByte($hex.Substring($i, 2), 16) } catch {}
    }
    if ($bytes.Count -eq 0) { return '' }
    try { return [Text.Encoding]::UTF8.GetString($bytes) } catch {}
    try { return [Text.Encoding]::GetEncoding('gb18030').GetString($bytes) } catch {}
    return ''
}

function Extract-PdfTextFromChunk([string]$chunk) {
    $textParts = @()

    # TJ 数组: [(string1)(string2)...] TJ
    $tjMatches = [regex]::Matches($chunk, '\[(.*?)\]\s*TJ', [Text.RegularExpressions.RegexOptions]::Singleline)
    foreach ($tm in $tjMatches) {
        $inner = $tm.Groups[1].Value
        # 提取括号字符串
        $innerMatches = [regex]::Matches($inner, '\(([^)]*)\)')
        foreach ($im in $innerMatches) {
            $textParts += Unescape-PdfLiteral $im.Groups[1].Value
        }
        # 提取十六进制字符串
        $hexMatches = [regex]::Matches($inner, '<([0-9A-Fa-f\s]+)>')
        foreach ($hm in $hexMatches) {
            $textParts += Decode-HexString $hm.Groups[1].Value
        }
    }

    # Tj 单字符串: (string) Tj
    $tjSingleMatches = [regex]::Matches($chunk, '\(([^)]*)\)\s*Tj')
    foreach ($sm in $tjSingleMatches) {
        $textParts += Unescape-PdfLiteral $sm.Groups[1].Value
    }

    # 十六进制 Tj: <hex> Tj
    $hexTjMatches = [regex]::Matches($chunk, '<([0-9A-Fa-f\s]+)>\s*Tj')
    foreach ($hm in $hexTjMatches) {
        $textParts += Decode-HexString $hm.Groups[1].Value
    }

    return ($textParts -join '')
}

function Extract-PdfText([string]$raw) {
    $textParts = @()

    # 1) 直接从 BT/ET 块中提取文本操作符
    $btMatches = [regex]::Matches($raw, 'BT\s*(.*?)\s*ET', [Text.RegularExpressions.RegexOptions]::Singleline)
    foreach ($btm in $btMatches) {
        $textParts += Extract-PdfTextFromChunk $btm.Groups[1].Value
    }

    # 2) 尝试解压 FlateDecode 流后再提取
    $streamRegex = 'stream\s*\r?\n(.*?)\r?\nendstream'
    $streamMatches = [regex]::Matches($raw, $streamRegex, [Text.RegularExpressions.RegexOptions]::Singleline)
    foreach ($m in $streamMatches) {
        $streamData = $m.Groups[1].Value.Trim()
        try {
            $compressedBytes = [Text.Encoding]::GetEncoding('ISO-8859-1').GetBytes($streamData)
            $memStream = New-Object IO.MemoryStream(, $compressedBytes)
            try {
                $deflateStream = New-Object IO.Compression.DeflateStream($memStream, [IO.Compression.CompressionMode]::Decompress)
                $reader = New-Object IO.StreamReader($deflateStream, [Text.Encoding]::UTF8)
                $decompressed = $reader.ReadToEnd()
                $reader.Close()
                $deflateStream.Close()
                if ($decompressed -ne $null -and $decompressed -ne '') {
                    $textParts += Extract-PdfTextFromChunk $decompressed
                }
            } catch {
                $memStream.Close()
            }
        } catch {}
    }

    return ($textParts -join ' ')
}

try {
    switch ($ext) {
        '.pdf' {
            $rawBytes = [IO.File]::ReadAllBytes($Path)
            $raw = [Text.Encoding]::GetEncoding('ISO-8859-1').GetString($rawBytes)

            $fullText = Extract-PdfText $raw

            if ($fullText -ne $null -and $fullText -ne '') {
                $lines = @($fullText -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
            }

            if (($lines -eq $null) -or ($lines.Count -eq 0)) {
                Write-Error "ERROR: Could not extract text from PDF. The PDF may be image-only (scanned) or use an unsupported font encoding. Consider installing pymupdf (pip install pymupdf) for better PDF support or OCR."
                exit 1
            }
        }

        '.xlsx' {
            try {
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $zip = [IO.Compression.ZipFile]::OpenRead($Path)

                $sharedStrings = @()
                $ssEntry = $zip.GetEntry('xl/sharedStrings.xml')
                if ($ssEntry) {
                    $reader = New-Object IO.StreamReader($ssEntry.Open(), [Text.Encoding]::UTF8)
                    $ssXml = $reader.ReadToEnd()
                    $reader.Close()
                    $ssMatches = [regex]::Matches($ssXml, '<t[^>]*>([^<]*)</t>')
                    $sharedStrings = $ssMatches | ForEach-Object { $_.Groups[1].Value }
                }

                $sheetEntries = @($zip.Entries | Where-Object { $_.FullName -match 'xl/worksheets/sheet\d+\.xml' } | Sort-Object FullName)
                $sheetNames = @()
                if ($sheetEntries.Count -eq 0) {
                    $wbEntry = $zip.GetEntry('xl/workbook.xml')
                    if ($wbEntry) {
                        $reader = New-Object IO.StreamReader($wbEntry.Open(), [Text.Encoding]::UTF8)
                        $wbXml = $reader.ReadToEnd()
                        $reader.Close()
                        $sheetMatches = [regex]::Matches($wbXml, 'name="([^"]*)"')
                        $sheetNames = $sheetMatches | ForEach-Object { $_.Groups[1].Value }
                    }
                    $sheetEntries = @($zip.Entries | Where-Object { $_.FullName -match 'xl/worksheets/sheet' })
                }

                $sheetNum = 0
                foreach ($entry in $sheetEntries) {
                    $sheetNum++
                    $reader = New-Object IO.StreamReader($entry.Open(), [Text.Encoding]::UTF8)
                    $xml = $reader.ReadToEnd()
                    $reader.Close()

                    $sheetName = if ($sheetNames -and $sheetNum -le $sheetNames.Count) { $sheetNames[$sheetNum-1] } else { "Sheet$sheetNum" }
                    $lines += "=== $sheetName ==="

                    $rowMatches = [regex]::Matches($xml, '<row[^>]*>(.*?)</row>', [Text.RegularExpressions.RegexOptions]::Singleline)
                    $rowNum = 0
                    $maxRows = 1000

                    foreach ($rm in $rowMatches) {
                        if ($rowNum -ge $maxRows) { break }
                        $rowXml = $rm.Groups[1].Value
                        $cells = @()

                        $cellMatches = [regex]::Matches($rowXml, '<c[^>]*>(.*?)</c>', [Text.RegularExpressions.RegexOptions]::Singleline)
                        foreach ($cm in $cellMatches) {
                            $cellXml = $cm.Groups[1].Value
                            $typeMatch = [regex]::Match($cm.Value, 't="([^"]*)"')
                            $cellType = if ($typeMatch.Success) { $typeMatch.Groups[1].Value } else { '' }
                            $val = ''

                            if ($cellType -eq 's') {
                                $numMatch = [regex]::Match($cellXml, '<v[^>]*>(\d+)</v>')
                                if ($numMatch.Success) {
                                    $idx = [int]$numMatch.Groups[1].Value
                                    $val = if ($idx -lt $sharedStrings.Count) { $sharedStrings[$idx] } else { '' }
                                }
                            } elseif ($cellType -eq 'inlineStr') {
                                $tMatch = [regex]::Match($cellXml, '<t[^>]*>([^<]*)</t>')
                                $val = if ($tMatch.Success) { $tMatch.Groups[1].Value } else { '' }
                            } else {
                                $vMatch = [regex]::Match($cellXml, '<v[^>]*>([^<]+)</v>')
                                $val = if ($vMatch.Success) { $vMatch.Groups[1].Value } else { '' }
                            }
                            $cells += $val
                        }

                        $line = ($cells | ForEach-Object { $_ }) -join "`t"
                        if ($line.Trim() -ne '') {
                            $lines += $line
                            $rowNum++
                        }
                    }
                }
                $zip.Dispose()
            } catch {
                Write-Error "ERROR: Failed to read Excel file: $_"
                exit 1
            }
        }

        '.docx' {
            try {
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $zip = [IO.Compression.ZipFile]::OpenRead($Path)

                $docEntry = $zip.GetEntry('word/document.xml')
                if (-not $docEntry) {
                    Write-Error "ERROR: document.xml not found in docx"
                    exit 1
                }

                $reader = New-Object IO.StreamReader($docEntry.Open(), [Text.Encoding]::UTF8)
                $xml = $reader.ReadToEnd()
                $reader.Close()
                $zip.Dispose()

                $paraMatches = [regex]::Matches($xml, '<w:p[ >](.*?)</w:p>', [Text.RegularExpressions.RegexOptions]::Singleline)
                if ($paraMatches.Count -eq 0) {
                    $paraMatches = [regex]::Matches($xml, '<w:p>(.*?)</w:p>', [Text.RegularExpressions.RegexOptions]::Singleline)
                }

                foreach ($pm in $paraMatches) {
                    $paraXml = $pm.Groups[1].Value
                    $tMatches = [regex]::Matches($paraXml, '<w:t[^>]*>([^<]*)</w:t>')
                    $paraText = ($tMatches | ForEach-Object { $_.Groups[1].Value }) -join ''
                    if ($paraText.Trim() -ne '') {
                        $lines += $paraText
                    }
                }

                $tableMatches = [regex]::Matches($xml, '<w:tbl>(.*?)</w:tbl>', [Text.RegularExpressions.RegexOptions]::Singleline)
                foreach ($tm in $tableMatches) {
                    $tableXml = $tm.Groups[1].Value
                    $rowMatches = [regex]::Matches($tableXml, '<w:tr>(.*?)</w:tr>', [Text.RegularExpressions.RegexOptions]::Singleline)
                    foreach ($rm in $rowMatches) {
                        $rowXml = $rm.Groups[1].Value
                        $tcells = [regex]::Matches($rowXml, '<w:t[^>]*>([^<]*)</w:t>')
                        $rowText = ($tcells | ForEach-Object { $_.Groups[1].Value }) -join "`t"
                        if ($rowText.Trim() -ne '') {
                            $lines += $rowText
                        }
                    }
                }

                if ($lines.Count -eq 0) {
                    Write-Error "ERROR: No text found in document"
                    exit 1
                }
            } catch {
                Write-Error "ERROR: Failed to read Word file: $_"
                exit 1
            }
        }

        '.pptx' {
            try {
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $zip = [IO.Compression.ZipFile]::OpenRead($Path)

                $slideEntries = @($zip.Entries | Where-Object { $_.FullName -match 'ppt/slides/slide\d+\.xml' } | Sort-Object FullName)

                $slideNum = 0
                foreach ($entry in $slideEntries) {
                    $slideNum++
                    $reader = New-Object IO.StreamReader($entry.Open(), [Text.Encoding]::UTF8)
                    $xml = $reader.ReadToEnd()
                    $reader.Close()

                    $lines += "--- Slide $slideNum ---"

                    $tMatches = [regex]::Matches($xml, '<a:t[^>]*>([^<]*)</a:t>')
                    $slideText = ($tMatches | ForEach-Object { $_.Groups[1].Value }) -join ' '
                    if ($slideText.Trim() -ne '') {
                        $lines += $slideText
                    }
                }
                $zip.Dispose()

                if ($lines.Count -eq 0) {
                    Write-Error "ERROR: No text found in presentation"
                    exit 1
                }
            } catch {
                Write-Error "ERROR: Failed to read PPT file: $_"
                exit 1
            }
        }

        default {
            Write-Error "ERROR: Unsupported file type: $ext"
            exit 1
        }
    }

    if ($lines.Count -eq 0) {
        Write-Output "[$ext file: no extractable content]"
        exit 0
    }

    $total = $lines.Count
    $s = if ($StartLine -gt 0) { $StartLine - 1 } else { 0 }
    $e = if ($EndLine -gt 0) { [Math]::Min($EndLine, $total) } else { $total }

    if ($s -ge $total) {
        Write-Error "ERROR: startLine > total lines"
        exit 1
    }

    for ($i = $s; $i -lt $e -and $i -lt $total; $i++) {
        Write-Output "$($i+1): $($lines[$i])"
    }

} catch {
    $line = $_.InvocationInfo.ScriptLineNumber
    $msg = $_.Exception.Message
    Write-Error "ERROR at line ${line}: ${msg}`nStack: $($_.ScriptStackTrace)"
    exit 1
}
