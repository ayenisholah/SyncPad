# One-time repository setup: init git if needed, set the repo-local identity.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

if (-not (Test-Path ".git")) {
    git init -b main
}

git config user.name "Shola Ayeni"
git config user.email "ayenisholah@yahoo.com"

$name = git config user.name
$email = git config user.email
Write-Host "setup: OK — commits will be authored by $name <$email>"
