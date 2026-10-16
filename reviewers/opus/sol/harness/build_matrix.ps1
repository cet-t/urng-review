# Build/feature/target matrix for an exported tree (run setup.ps1 first).
#   powershell -File build_matrix.ps1 -Tree _src|_base -Log out\build-matrix-<x>.log
param([string]$Tree = '_src', [string]$Log = 'out\build-matrix.log')
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here $Tree
$logPath = Join-Path $here $Log
Set-Location $src
"# build matrix for $src  ($(Get-Date -Format s))" | Out-File -Encoding utf8 $logPath
function Run([string]$label, [string[]]$cmd) {
    $out = & cargo @cmd 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $errs = ($out | Select-String -Pattern '^error(\[|:)').Count
    $warns = ($out | Select-String -Pattern '^warning:').Count
    "{0,-70} exit={1} errors={2} warnings={3}" -f $label, $code, $errs, $warns | Out-File -Append -Encoding utf8 $logPath
    if ($code -ne 0) {
        $out | Select-String -Pattern '^error' -Context 0, 3 | Select-Object -First 6 | ForEach-Object { "    $($_.Line)"; $_.Context.PostContext | ForEach-Object { "      $_" } } | Out-File -Append -Encoding utf8 $logPath
    }
}
$features = @('rand', 'cabi', 'wide', 'simd', 'sampler', 'seedgen')
for ($m = 0; $m -lt 64; $m++) {
    $sel = @(); for ($i = 0; $i -lt 6; $i++) { if ($m -band (1 -shl $i)) { $sel += $features[$i] } }
    $f = $sel -join ','
    if ($f) { Run "check --lib --features $f" @('check', '--lib', '--locked', '--no-default-features', '--features', $f) }
    else { Run "check --lib (no features)" @('check', '--lib', '--locked', '--no-default-features') }
}
Run 'test --doc (no features)' @('test', '--doc', '--locked', '--no-default-features')
Run 'test --doc --all-features' @('test', '--doc', '--locked', '--all-features')
Run 'build --all-targets --all-features' @('build', '--all-targets', '--locked', '--all-features')
Run 'doc --no-deps --all-features' @('doc', '--no-deps', '--locked', '--all-features')
# cross targets via nightly -Zbuild-std (no extra toolchain components needed)
foreach ($t in @('aarch64-unknown-linux-gnu', 'i686-pc-windows-msvc', 'powerpc64-unknown-linux-gnu')) {
    Run "+nightly check $t (no features)" @('+nightly', 'check', '--lib', '-Zbuild-std=core,std', '--target', $t, '--no-default-features')
    Run "+nightly check $t --features simd" @('+nightly', 'check', '--lib', '-Zbuild-std=core,std', '--target', $t, '--no-default-features', '--features', 'simd')
    Run "+nightly check $t --features wide,sampler,rand,seedgen,cabi" @('+nightly', 'check', '--lib', '-Zbuild-std=core,std', '--target', $t, '--no-default-features', '--features', 'wide,sampler,rand,seedgen,cabi')
    Run "+nightly check $t --all-features" @('+nightly', 'check', '--lib', '-Zbuild-std=core,std', '--target', $t, '--all-features')
}
"# done $(Get-Date -Format s)" | Out-File -Append -Encoding utf8 $logPath
