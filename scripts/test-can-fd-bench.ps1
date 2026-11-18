param(
    [Parameter(Mandatory = $true)][string]$Dll,
    [ValidateSet(32, 64)][int]$Bitness = 64,
    [string]$OpenName,
    [uint32]$NominalBitrate = 500000,
    [uint32]$DataBitrate = 2000000,
    [ValidateRange(1, 20)][int]$Cycles = 3,
    [string]$Peer,
    [switch]$Classic,
    [switch]$PeerFirst,
    [switch]$Transmit
)
$ErrorActionPreference = 'Stop'
if (-not $Transmit) { throw 'Use -Transmit only on an authorized isolated, terminated bench.' }
$crateRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $crateRoot 'Cargo.toml'
& cargo build --manifest-path $manifest --target x86_64-pc-windows-msvc --bins --example can-fd-bench
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$debugRoot = Join-Path $crateRoot 'target/x86_64-pc-windows-msvc/debug'
$exampleRoot = Join-Path $debugRoot 'examples'
Copy-Item -LiteralPath (Join-Path $debugRoot 'j2534-bridge.exe') -Destination (Join-Path $exampleRoot 'j2534-bridge-64.exe')
if ($Bitness -eq 32) {
    & cargo build --manifest-path $manifest --target i686-pc-windows-msvc --bin j2534-bridge
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Copy-Item -LiteralPath (Join-Path $crateRoot 'target/i686-pc-windows-msvc/debug/j2534-bridge.exe') -Destination (Join-Path $exampleRoot 'j2534-bridge-32.exe')
}
$benchArguments = @('--dll', $Dll, '--bitness', $Bitness, '--nominal-bitrate', $NominalBitrate, '--cycles', $Cycles, '--transmit')
if ($OpenName) { $benchArguments += @('--open-name', $OpenName) }
if (-not $Classic) { $benchArguments += @('--data-bitrate', $DataBitrate) }
if ($Peer) { $benchArguments += @('--peer', $Peer) }
if ($PeerFirst) { $benchArguments += '--peer-first' }
& (Join-Path $exampleRoot 'can-fd-bench.exe') @benchArguments
exit $LASTEXITCODE
