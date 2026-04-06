$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
if ([System.Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'Package on Windows.' }
$targetTriple = 'x86_64-pc-windows-msvc'
$manifestText = Get-Content -LiteralPath Cargo.toml -Raw
$versionMatch = [regex]::Match($manifestText, '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"\r?$')
if (-not $versionMatch.Success) { throw 'Cannot read the package version.' }
$releaseVersion = $versionMatch.Groups[1].Value
& cargo --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]' build --locked --release --target $targetTriple --jobs 2 --target-dir target
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
New-Item -ItemType Directory -Force -Path dist | Out-Null
$assetPath = Join-Path (Get-Location) "dist/SlideshowMaker-v$releaseVersion.exe"
Copy-Item -LiteralPath "target/$targetTriple/release/slideshow-maker.exe" -Destination $assetPath
$assetHash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
"$assetHash  $([IO.Path]::GetFileName($assetPath))" | Set-Content -LiteralPath "$assetPath.sha256" -Encoding ascii
Copy-Item -LiteralPath LICENSE -Destination dist/LICENSE.txt
Copy-Item -LiteralPath docs/THIRD_PARTY.md -Destination dist/THIRD_PARTY.md
Copy-Item -LiteralPath docs/DEPENDENCY_LICENSES.md -Destination dist/DEPENDENCY_LICENSES.md
Copy-Item -LiteralPath docs/licenses -Destination dist -Recurse -Force
Write-Output "Packaged $assetPath (Windows x64). No application was launched; no publication was performed."
