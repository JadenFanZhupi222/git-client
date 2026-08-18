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

$runtimeTreeLines = & cargo tree -p agent-runtime -e normal --prefix none --depth 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo tree for agent-runtime failed with exit code $LASTEXITCODE"
}
$runtimeTree = $runtimeTreeLines -join [Environment]::NewLine
if ($runtimeTree -match "(?m)^agent-tools v") {
    throw "agent-runtime must not depend on the external adapter crate agent-tools"
}

$toolTreeLines = & cargo tree -p agent-tools -e normal --prefix none --depth 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo tree for agent-tools failed with exit code $LASTEXITCODE"
}
$toolTree = $toolTreeLines -join [Environment]::NewLine
if ($toolTree -notmatch "(?m)^agent-runtime v") {
    throw "agent-tools must use the provider-neutral agent-runtime contract"
}

Write-Output "Agent tool dependency boundary passed: adapters depend inward on agent-runtime only."

$sessionTreeLines = & cargo tree -p agent-session -e normal --prefix none --depth 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo tree for agent-session failed with exit code $LASTEXITCODE"
}
$sessionTree = $sessionTreeLines -join [Environment]::NewLine
if ($sessionTree -notmatch "(?m)^agent-runtime v") {
    throw "agent-session must use the provider-neutral agent-runtime contract"
}
if ($sessionTree -match "(?m)^(agent-tools|review-agent|app-service|ipc-types|app) v") {
    throw "agent-session production dependencies must not point to adapters or application crates"
}

Write-Output "Agent session dependency boundary passed: orchestration depends inward on agent-runtime only."
