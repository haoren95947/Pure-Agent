# Builds Pure Agent plus its engine binaries and stages a runnable dist/.
#
#   powershell -ExecutionPolicy Bypass -File scripts/build.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Configuration debug
#
# dist/ contains:
#   pure-agent.exe        Pure Agent desktop client (window title: Pure Agent)
#   pure-backend.exe      ACP backend and kernel host in one binary
#                         (`rpc` serves ACP; `--mode rpc` is the session child)
#   extensions\*.dll      pure-goal, pure-todo, pure-subagents, pure-codemode,
#                         pure-permission-gate, pure-rtk, pure-plan-mode,
#                         pure-swarm (+ vendor\rtk binaries)
#   skills\               bundled skills seed for ~/.pure-agent/agent/skills

[CmdletBinding()]
param(
    [ValidateSet("release", "debug")]
    [string]$Configuration = "release",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"

$buildArgs = @()
if ($Configuration -eq "release") {
    $buildArgs += "--release"
}

function Invoke-Cargo {
    param([string]$Manifest, [string[]]$Packages)
    $cargoArgs = @("build", "--manifest-path", $Manifest) + $buildArgs
    foreach ($package in $Packages) {
        $cargoArgs += @("-p", $package)
    }
    Write-Host "cargo $($cargoArgs -join ' ')" -ForegroundColor Cyan
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed for $Manifest"
    }
}

$desktopTarget = Join-Path $root "target\$Configuration"
$backendTarget = Join-Path $root "target\$Configuration"

if (-not $SkipBuild) {
    Invoke-Cargo (Join-Path $root "Cargo.toml") @(
        "pure-desktop-app", "pure-backend", "pure-goal", "pure-todo",
        "pure-subagents", "pure-codemode", "pure-permission-gate", "pure-rtk",
        "pure-plan-mode", "pure-swarm"
    )
}

if (Test-Path $dist) {
    Remove-Item -Recurse -Force $dist
}
New-Item -ItemType Directory -Path $dist | Out-Null

Copy-Item (Join-Path $desktopTarget "pure-agent.exe") $dist
Copy-Item (Join-Path $backendTarget "pure-backend.exe") $dist

$extensions = Join-Path $dist "extensions"
New-Item -ItemType Directory -Path $extensions | Out-Null
foreach ($name in @("pure_goal", "pure_todo", "pure_subagents", "pure_codemode", "pure_permission_gate", "pure_rtk", "pure_plan_mode", "pure_swarm")) {
    Copy-Item (Join-Path $backendTarget "$name.dll") $extensions
}
# The RTK extension shells out to a per-platform binary beside the DLL.
Copy-Item -Recurse (Join-Path $root "crates\pure-extensions-packages\pure-rtk\vendor") (Join-Path $extensions "vendor")

$skills = Join-Path $dist "skills"
New-Item -ItemType Directory -Path $skills | Out-Null
Copy-Item -Recurse (Join-Path $root "skills\*") $skills
Copy-Item (Join-Path $root "README.md") $dist
Copy-Item (Join-Path $root "THIRD_PARTY_NOTICES.md") $dist

$required = @(
    "pure-agent.exe",
    "pure-backend.exe",
    "extensions\pure_goal.dll",
    "extensions\pure_todo.dll",
    "extensions\pure_subagents.dll",
    "extensions\pure_codemode.dll",
    "extensions\pure_permission_gate.dll",
    "extensions\pure_rtk.dll",
    "extensions\pure_plan_mode.dll",
    "extensions\pure_swarm.dll",
    "extensions\vendor\rtk\win32-x64\rtk.exe",
    "skills\pure-guide\SKILL.md",
    "skills\demo\SKILL.md"
)
$missing = @()
foreach ($item in $required) {
    if (-not (Test-Path (Join-Path $dist $item))) {
        $missing += $item
    }
}
if ($missing.Count -gt 0) {
    throw "dist is missing: $($missing -join ', ')"
}

Write-Host ""
Write-Host "dist staged at $dist" -ForegroundColor Green
Get-ChildItem -Recurse $dist | Where-Object { -not $_.PSIsContainer } |
    ForEach-Object { "{0,10}  {1}" -f $_.Length, $_.FullName.Substring($dist.Length + 1) }
