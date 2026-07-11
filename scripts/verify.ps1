# Local verification loop: format, lint, and tests for everything present.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

# cargo may live outside PATH when rustup was installed with --no-modify-path
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path (Join-Path $cargoBin "cargo.exe")) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

function Invoke-Step([string]$Name, [scriptblock]$Step) {
    Write-Host "== $Name"
    & $Step
    if ($LASTEXITCODE -ne 0) {
        Write-Host "verify: FAILED at '$Name'"
        exit 1
    }
}

if (Test-Path "Cargo.toml") {
    Invoke-Step "server: cargo fmt --check" { cargo fmt --all --check }
    Invoke-Step "server: cargo clippy (-D warnings)" { cargo clippy --workspace --all-targets -- -D warnings }
    Invoke-Step "server: cargo test" { cargo test --workspace }
} else {
    Write-Host "== server checks skipped (no Cargo.toml yet)"
}

if (Test-Path "web/package.json") {
    Push-Location web
    try {
        if (-not (Test-Path "node_modules")) {
            Invoke-Step "web: npm ci" { npm ci }
        }
        Invoke-Step "web: build" { npm run build }
        Invoke-Step "web: test" { npm test }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "== web checks skipped (no web/package.json yet)"
}

Write-Host "verify: OK"
