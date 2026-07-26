$ErrorActionPreference = "Stop"

$treeLines = & cargo tree -p app-service -e normal --prefix none --depth 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo tree failed with exit code $LASTEXITCODE"
}

$tree = $treeLines -join [Environment]::NewLine
if ($tree -match "(?m)^git-engine v") {
    throw "app-service must not have a normal dependency on git-engine"
}

Write-Output "Dependency boundary check passed: app-service uses no git-engine production dependency."
