# Shell and configuration

## Bash

```bash
#!/usr/bin/env bash
set -euo pipefail

readonly ROOT="${1:-$(pwd)}"
declare -a targets=()

usage() {
  cat <<'USAGE'
usage: build.sh [root]
USAGE
}

for file in "$ROOT"/*.toml; do
  [[ -f "$file" ]] || continue
  targets+=("$(basename "$file" .toml)")
done

if (( ${#targets[@]} == 0 )); then
  usage >&2
  exit 1
fi

printf '%s\n' "${targets[@]}" | sort -u
```

## PowerShell

> [!NOTE]
> `ps1` also works as an alias: ```ps1`Get-ChildItem -Recurse | Measure-Object```.

```powershell
#Requires -Version 7.0
using namespace System.Collections.Generic

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

<#
.SYNOPSIS
    Counts files by extension under a root.
.EXAMPLE
    Measure-Extension -Root . -Minimum 2
#>
function Measure-Extension {
    [CmdletBinding()]
    [OutputType([hashtable])]
    param(
        [Parameter(Mandatory, ValueFromPipeline)]
        [ValidateNotNullOrEmpty()]
        [string] $Root,

        [ValidateRange(1, [int]::MaxValue)]
        [int] $Minimum = 1
    )

    begin {
        $counts = [Dictionary[string, int]]::new()
    }

    process {
        Get-ChildItem -Path $Root -File -Recurse | ForEach-Object {
            $key = if ($_.Extension) { $_.Extension } else { '(none)' }
            $counts[$key] = 1 + ($counts[$key] ?? 0)
        }
    }

    end {
        $counts.GetEnumerator() |
            Where-Object { $_.Value -ge $Minimum } |
            Sort-Object -Property Value -Descending
    }
}
```


## TOML

```toml
[package]
name = "example"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }

[[bin]]
name = "example"
path = "src/main.rs"

[profile.release]
lto = true
codegen-units = 1
```

## YAML

```yaml
name: ci
on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: |
          cargo build --release
          cargo test
    env:
      RUST_BACKTRACE: "1"
```

## JSON

```json
{
  "name": "example",
  "version": "0.1.0",
  "private": true,
  "scripts": { "build": "tsc -p ." },
  "numbers": [1, -2, 3.5e10],
  "nested": { "ok": true, "missing": null }
}
```

## Lua

```lua
local M = {}

---@param items string[]
---@return table<string, integer>
function M.index(items)
  local out = {}
  for i, item in ipairs(items) do
    out[item] = i
  end
  return out
end

function M.greet(name)
  name = name or "world"
  return ("hello, %s"):format(name)
end

return M
```
