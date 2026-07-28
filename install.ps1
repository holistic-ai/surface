<#
surface installer for Windows.

  irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex

Downloads the release archive, verifies its checksum against the release's
SHA256SUMS, extracts it to %LOCALAPPDATA%\Programs\surface and puts that on your
user PATH. No service, no registry keys beyond that PATH entry.

Overridable:
  $env:SURFACE_VERSION     = 'v0.1.0'   install a specific tag
  $env:SURFACE_INSTALL_DIR = 'C:\tools' install somewhere else
#>
$ErrorActionPreference = 'Stop'

# Windows PowerShell 5.1 still defaults to TLS 1.0, which github.com refuses.
# PowerShell 7 already negotiates TLS 1.2, so this is a no-op there.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
# Invoke-WebRequest's progress bar makes a 2 MB download take tens of seconds
# in 5.1, and it is redrawn per byte range.
$ProgressPreference = 'SilentlyContinue'

$repo = 'holistic-ai/surface'

if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
  Write-Host 'note: no arm64 build yet — installing the x86-64 binary, which runs emulated.'
}
$target = 'x86_64-pc-windows-msvc'

# ------------------------------------------------------------------- version

$tag = $env:SURFACE_VERSION
# Tags carry the `v`; accept $env:SURFACE_VERSION either way rather than failing
# on a download 404 several steps later.
if ($tag -match '^\d') { $tag = "v$tag" }
if (-not $tag) {
  try {
    $tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
  } catch {
    throw "no published release found — install with: cargo install surface-cli"
  }
}

$name = "surface-$tag-$target"
$base = "https://github.com/$repo/releases/download/$tag"

# --------------------------------------------------------- download + verify

$tmp = Join-Path $env:TEMP ("surface-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "surface $tag ($target)"
  Invoke-WebRequest "$base/$name.zip" -OutFile "$tmp\$name.zip" -UseBasicParsing
  $sums = (Invoke-WebRequest "$base/SHA256SUMS" -UseBasicParsing).Content

  $line = $sums -split "`n" | Where-Object { $_ -match [regex]::Escape("$name.zip") } | Select-Object -First 1
  if (-not $line) { throw "no checksum for $name.zip in SHA256SUMS" }
  $want = ($line -split '\s+')[0].ToLower()
  $got = (Get-FileHash "$tmp\$name.zip" -Algorithm SHA256).Hash.ToLower()
  if ($want -ne $got) { throw "checksum mismatch — refusing to install`n  expected $want`n  got      $got" }

  # --------------------------------------------------------------- install

  $dir = if ($env:SURFACE_INSTALL_DIR) { $env:SURFACE_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\surface" }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  Expand-Archive "$tmp\$name.zip" -DestinationPath $tmp -Force
  Copy-Item "$tmp\$name\*" $dir -Force -Recurse

  Write-Host "installed $dir\surface.exe"

  # Idempotent: appending a directory already on PATH would grow it every run.
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (($userPath -split ';') -notcontains $dir) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
    Write-Host "added $dir to your user PATH — reopen the terminal, then run: surface"
  } else {
    Write-Host 'run: surface'
  }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
