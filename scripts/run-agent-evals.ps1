$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

cargo test -p agent-session --test evaluation_baseline -- --nocapture
