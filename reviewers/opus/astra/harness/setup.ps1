# Exports the reviewed tree and the urng 1.0.0 base into harness-local copies so
# that nothing is ever built inside the original submodule directories.
#   powershell -File setup.ps1 -Target sol|astra
param([Parameter(Mandatory = $true)][ValidateSet('sol', 'astra')][string]$Target)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = (Resolve-Path (Join-Path $here '..\..\..\..')).Path
$sub = Join-Path $repo "urng-review_$Target"
# base commits: Sol 386eb4d (1.0.0), Astra 19bc224 (1.0.0 + REVIEW.md); identical src/ trees
$baseRev = if ($Target -eq 'sol') { '386eb4d' } else { '19bc224' }
foreach ($pair in @(@('_src', 'HEAD'), @('_base', $baseRev))) {
    $dir = Join-Path $here $pair[0]
    $tar = "$dir.tar"
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    New-Item -ItemType Directory $dir | Out-Null
    git -C $sub archive -o $tar $pair[1]
    tar -xf $tar -C $dir
    Remove-Item $tar
    Write-Host "exported $Target $($pair[1]) -> $dir"
}
