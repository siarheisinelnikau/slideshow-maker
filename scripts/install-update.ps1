param([Parameter(Mandatory)][string]$TargetPath,
      [Parameter(Mandatory)][string]$StagedPath,
      [Parameter(Mandatory)][int]$ParentId)
$ErrorActionPreference = 'Stop'
$backupPath = "$TargetPath.update-backup"
$failurePath = "$TargetPath.update-error.txt"
$movedOriginal = $false
try {
    $parentProcess = Get-Process -Id $ParentId -ErrorAction SilentlyContinue
    if ($null -ne $parentProcess) {
        if (-not $parentProcess.WaitForExit(120000)) { throw 'Application did not exit within two minutes.' }
    }
    if (Test-Path -LiteralPath $backupPath) { throw 'A previous update backup exists; recover it before retrying.' }
    Move-Item -LiteralPath $TargetPath -Destination $backupPath
    $movedOriginal = $true
    Move-Item -LiteralPath $StagedPath -Destination $TargetPath
    Start-Process -FilePath $TargetPath -WorkingDirectory (Split-Path -Parent $TargetPath)
    Remove-Item -LiteralPath $backupPath
    if (Test-Path -LiteralPath $failurePath) { Remove-Item -LiteralPath $failurePath }
} catch {
    if ($movedOriginal -and (Test-Path -LiteralPath $backupPath)) {
        if (Test-Path -LiteralPath $TargetPath) { Remove-Item -LiteralPath $TargetPath }
        Move-Item -LiteralPath $backupPath -Destination $TargetPath
        Start-Process -FilePath $TargetPath -WorkingDirectory (Split-Path -Parent $TargetPath)
    }
    $_.Exception.Message | Set-Content -LiteralPath $failurePath
} finally {
    if (Test-Path -LiteralPath $StagedPath) { Remove-Item -LiteralPath $StagedPath }
    Remove-Item -LiteralPath $PSCommandPath -ErrorAction SilentlyContinue
}
