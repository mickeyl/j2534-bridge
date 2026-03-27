param(
    [ValidateSet("x64", "x86")]
    [string]$Arch = "x64"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repoRoot "native\\j2534_v5_can_test.c"
$outDir = Join-Path $repoRoot "target\\native\\$Arch"
$exe = Join-Path $outDir "j2534_v5_can_test.exe"
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"

if (-not (Test-Path $vcvars)) {
    throw "vcvarsall.bat not found at $vcvars"
}

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$cmd = @(
    'call "{0}" {1}' -f $vcvars, $Arch
    'cl /nologo /W4 /WX /Zi /std:c11 "{0}" /Fe:"{1}"' -f $source, $exe
) -join ' && '

cmd.exe /c $cmd
if ($LASTEXITCODE -ne 0) {
    throw "Build failed with exit code $LASTEXITCODE"
}

Write-Host $exe
