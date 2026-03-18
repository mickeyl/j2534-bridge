param(
    [string]$DllPath = "C:\Windows\SysWOW64\op20pt32.dll",
    [ValidateSet(32, 64)]
    [int]$Bitness = 32,
    [int]$ProtocolId = 3,
    [ValidateSet("none", "fast", "slow", "auto")]
    [string]$KlineInitMode = "slow",
    [string]$RequestHex = "0101",
    [int]$BaudRate = 10400,
    [string]$ConnectFlags = "0x1000",
    [switch]$UseNoChecksumFlag,
    [int]$RequestTimeoutMs = 1500,
    [int]$DurationSecs = 8,
    [int]$BatchSize = 64,
    [int]$MaxDrainReads = 8,
    [string]$TargetTriple = "i686-pc-windows-msvc",
    [int[]]$PostInitIdleMsValues = @(750, 1000, 1250, 1500),
    [int[]]$P1MinValues = @(),
    [int[]]$P1MaxValues = @(),
    [int[]]$P4MinValues = @(),
    [int[]]$P4MaxValues = @(),
    [int[]]$P3MinValues = @(),
    [int[]]$W1Values = @(),
    [int[]]$W2Values = @(),
    [int[]]$W3Values = @(),
    [int[]]$W4Values = @(),
    [int[]]$W5Values = @(),
    [int[]]$W0Values = @(),
    [int[]]$TIdleValues = @(),
    [int[]]$TWupValues = @(),
    [int[]]$FiveBaudModValues = @(),
    [int[]]$Iso9141NoChecksumValues = @(),
    [ValidateSet("all", "idle", "p1", "p4", "w", "tidle", "twup")]
    [string]$SweepFamily = "all",
    [switch]$StopOnFirstHit,
    [string]$OutputCsv
)

$ErrorActionPreference = "Stop"
if ($null -ne (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue)) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$dumpExe = Join-Path $repoRoot "target\$TargetTriple\debug\j2534-dump.exe"

$paramMap = [ordered]@{
    ISO9141_NO_CHECKSUM = 0x200
    P1_MIN = 0x06
    P1_MAX = 0x07
    P4_MIN = 0x0C
    P4_MAX = 0x0D
    P3_MIN = 0x0A
    W1 = 0x0E
    W2 = 0x0F
    W3 = 0x10
    W4 = 0x11
    W5 = 0x12
    TIDLE = 0x13
    TWUP = 0x15
    W0 = 0x19
    FIVE_BAUD_MOD = 0x21
}

$UnsetValue = -1

function Ensure-Array {
    param([int[]]$Values)
    if ($null -eq $Values -or $Values.Count -eq 0) {
        return @($UnsetValue)
    }
    return $Values
}

function Expand-Matrix {
    param([hashtable]$Matrix)

    $cases = New-Object 'System.Collections.Generic.List[hashtable]'
    $cases.Add(@{}) | Out-Null

    foreach ($key in $Matrix.Keys) {
        $expanded = New-Object 'System.Collections.Generic.List[hashtable]'
        foreach ($case in $cases) {
            foreach ($value in $Matrix[$key]) {
                $next = @{}
                foreach ($entry in $case.GetEnumerator()) {
                    $next[$entry.Key] = $entry.Value
                }
                $next[$key] = $value
                $expanded.Add($next) | Out-Null
            }
        }
        $cases = $expanded
    }

    return $cases
}

function Get-SetConfigArgs {
    param([hashtable]$Case)

    $args = @()
    foreach ($name in $paramMap.Keys) {
        $value = $Case[$name]
        if ($value -ne $UnsetValue) {
            $paramId = $paramMap[$name]
            $args += "--set-config"
            $args += ("0x{0:X}=0x{1:X}" -f $paramId, [int]$value)
        }
    }
    return $args
}

function Get-ResponseCount {
    param([string[]]$Lines)

    $postInitCount = 0
    foreach ($line in $Lines) {
        if ($line -match 'post-init responses=(\d+)') {
            return [int]$Matches[1]
        }
    }

    foreach ($line in $Lines) {
        if ($line -match '^\(' -and $line -notmatch '000#0808\b' -and $line -notmatch '000#9494\b') {
            $postInitCount += 1
        }
    }
    return $postInitCount
}

function Reached-PostInitTx {
    param([string[]]$Lines)
    foreach ($line in $Lines) {
        if ($line -match 'K-Line TX payload=') {
            return $true
        }
    }
    return $false
}

function Get-CaseExitStage {
    param([string[]]$Lines)
    foreach ($line in $Lines) {
        if ($line -match 'ERR_J2534_OPEN_FAILED') { return 'open_failed' }
        if ($line -match 'ERR_J2534_FIVE_BAUD_INIT_FAILED|ERR_J2534_FAST_INIT_FAILED') { return 'init_failed' }
        if ($line -match 'K-Line TX payload=') { return 'request_sent' }
    }
    return 'unknown'
}

function Get-CaseLabel {
    param([hashtable]$Case)

    $parts = @("idle=$($Case.PostInitIdleMs)")
    foreach ($name in $paramMap.Keys) {
        $value = $Case[$name]
        if ($value -ne $UnsetValue) {
            $parts += "$name=$value"
        }
    }
    return ($parts -join ", ")
}

function Build-Matrix {
    param()

    $matrix = [ordered]@{
        PostInitIdleMs = @($UnsetValue)
        ISO9141_NO_CHECKSUM = @($UnsetValue)
        P1_MIN = @($UnsetValue)
        P1_MAX = @($UnsetValue)
        P4_MIN = @($UnsetValue)
        P4_MAX = @($UnsetValue)
        P3_MIN = @($UnsetValue)
        W1 = @($UnsetValue)
        W2 = @($UnsetValue)
        W3 = @($UnsetValue)
        W4 = @($UnsetValue)
        W5 = @($UnsetValue)
        W0 = @($UnsetValue)
        TIDLE = @($UnsetValue)
        TWUP = @($UnsetValue)
        FIVE_BAUD_MOD = @($UnsetValue)
    }

    switch ($SweepFamily) {
        "all" {
            $matrix.PostInitIdleMs = (Ensure-Array $PostInitIdleMsValues)
            $matrix.ISO9141_NO_CHECKSUM = (Ensure-Array $Iso9141NoChecksumValues)
            $matrix.P1_MIN = (Ensure-Array $P1MinValues)
            $matrix.P1_MAX = (Ensure-Array $P1MaxValues)
            $matrix.P4_MIN = (Ensure-Array $P4MinValues)
            $matrix.P4_MAX = (Ensure-Array $P4MaxValues)
            $matrix.P3_MIN = (Ensure-Array $P3MinValues)
            $matrix.W1 = (Ensure-Array $W1Values)
            $matrix.W2 = (Ensure-Array $W2Values)
            $matrix.W3 = (Ensure-Array $W3Values)
            $matrix.W4 = (Ensure-Array $W4Values)
            $matrix.W5 = (Ensure-Array $W5Values)
            $matrix.W0 = (Ensure-Array $W0Values)
            $matrix.TIDLE = (Ensure-Array $TIdleValues)
            $matrix.TWUP = (Ensure-Array $TWupValues)
            $matrix.FIVE_BAUD_MOD = (Ensure-Array $FiveBaudModValues)
        }
        "idle" {
            $matrix.PostInitIdleMs = (Ensure-Array $PostInitIdleMsValues)
        }
        "p1" {
            $matrix.P1_MIN = (Ensure-Array $P1MinValues)
            $matrix.P1_MAX = (Ensure-Array $P1MaxValues)
        }
        "p4" {
            $matrix.P4_MIN = (Ensure-Array $P4MinValues)
            $matrix.P4_MAX = (Ensure-Array $P4MaxValues)
            $matrix.P3_MIN = (Ensure-Array $P3MinValues)
            $matrix.FIVE_BAUD_MOD = (Ensure-Array $FiveBaudModValues)
        }
        "w" {
            $matrix.W1 = (Ensure-Array $W1Values)
            $matrix.W2 = (Ensure-Array $W2Values)
            $matrix.W3 = (Ensure-Array $W3Values)
            $matrix.W4 = (Ensure-Array $W4Values)
            $matrix.W5 = (Ensure-Array $W5Values)
            $matrix.W0 = (Ensure-Array $W0Values)
        }
        "tidle" {
            $matrix.TIDLE = (Ensure-Array $TIdleValues)
        }
        "twup" {
            $matrix.TWUP = (Ensure-Array $TWupValues)
        }
    }

    if ($Iso9141NoChecksumValues.Count -gt 0) {
        $matrix.ISO9141_NO_CHECKSUM = (Ensure-Array $Iso9141NoChecksumValues)
    }

    return $matrix
}

Push-Location $repoRoot
try {
    Write-Host "Building j2534-dump for $TargetTriple..."
    & cargo build --bin j2534-dump --target $TargetTriple | Out-Host

    if (-not (Test-Path $dumpExe)) {
        throw "j2534-dump executable not found: $dumpExe"
    }

    $matrix = Build-Matrix

    $cases = @(Expand-Matrix -Matrix $matrix)
    $results = New-Object System.Collections.Generic.List[object]

    Write-Host ("Running {0} timing case(s)..." -f $cases.Count)

    $caseIndex = 0
    foreach ($case in $cases) {
    $caseIndex += 1
    $label = Get-CaseLabel -Case $case
    Write-Host ""
    Write-Host ("[{0}/{1}] {2}" -f $caseIndex, $cases.Count, $label)

    $args = @(
        "--dll-path", $DllPath,
        "--bitness", $Bitness,
        "--protocol-id", $ProtocolId,
        "--kline-init-mode", $KlineInitMode,
        "--kline-post-init-idle-ms", [string]$case.PostInitIdleMs,
        "--kline-request", $RequestHex,
        "--kline-request-timeout-ms", $RequestTimeoutMs,
        "--baud-rate", $BaudRate,
        "--connect-flags", (([int]$ConnectFlags) -bor ($(if ($UseNoChecksumFlag) { 0x200 } else { 0 }))),
        "--read-mode", "drain",
        "--timeout-ms", "100",
        "--batch-size", $BatchSize,
        "--max-drain-reads", $MaxDrainReads,
        "--duration-secs", $DurationSecs,
        "--raw-details",
        "--ascii",
        "--timestamp", "relative"
    )
    $args += Get-SetConfigArgs -Case $case

    $stdoutPath = Join-Path $env:TEMP ("j2534-dump-sweep-{0}-out.txt" -f [guid]::NewGuid().ToString("N"))
    $stderrPath = Join-Path $env:TEMP ("j2534-dump-sweep-{0}-err.txt" -f [guid]::NewGuid().ToString("N"))
    try {
        $proc = Start-Process -FilePath $dumpExe -ArgumentList $args -WorkingDirectory $repoRoot -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        $lines = @()
        if (Test-Path $stdoutPath) {
            $lines += Get-Content -Path $stdoutPath
        }
        if (Test-Path $stderrPath) {
            $lines += Get-Content -Path $stderrPath
        }
        if ($proc.ExitCode -ne 0) {
            Write-Host ("j2534-dump exited with code {0}" -f $proc.ExitCode)
        }
    }
    finally {
        if (Test-Path $stdoutPath) {
            Remove-Item $stdoutPath -Force
        }
        if (Test-Path $stderrPath) {
            Remove-Item $stderrPath -Force
        }
    }
    $responseCount = Get-ResponseCount -Lines $lines
    $reachedTx = Reached-PostInitTx -Lines $lines
    $stage = Get-CaseExitStage -Lines $lines
    $hit = $responseCount -ge 2

    foreach ($line in $lines) {
        Write-Host $line
    }

    $result = [pscustomobject][ordered]@{
        Case = $caseIndex
        Hit = $hit
        ResponseCount = $responseCount
        ReachedTx = $reachedTx
        Stage = $stage
        PostInitIdleMs = $case.PostInitIdleMs
        ISO9141_NO_CHECKSUM = $case.ISO9141_NO_CHECKSUM
        P1_MIN = $case.P1_MIN
        P1_MAX = $case.P1_MAX
        P4_MIN = $case.P4_MIN
        P4_MAX = $case.P4_MAX
        P3_MIN = $case.P3_MIN
        W1 = $case.W1
        W2 = $case.W2
        W3 = $case.W3
        W4 = $case.W4
        W5 = $case.W5
        W0 = $case.W0
        TIDLE = $case.TIDLE
        TWUP = $case.TWUP
        FIVE_BAUD_MOD = $case.FIVE_BAUD_MOD
    }
    $results.Add($result) | Out-Null

    Write-Host ("Result: hit={0} responses={1} reached_tx={2} stage={3}" -f $hit, $responseCount, $reachedTx, $stage)
    if ($hit -and $StopOnFirstHit) {
        break
    }
    }

    Write-Host ""
    Write-Host "Summary:"
    $results | Sort-Object -Property Hit, ResponseCount -Descending | Format-Table -AutoSize

    if ($OutputCsv) {
        $results | Export-Csv -Path $OutputCsv -NoTypeInformation
        Write-Host "Wrote CSV: $OutputCsv"
    }
}
finally {
    Pop-Location
}
