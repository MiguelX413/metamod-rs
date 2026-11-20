#Requires -Version 7.4
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

if ($env:VSCMD_ARG_TGT_ARCH -ne 'x86') {
    throw 'Run this script from an x86 Visual Studio Native Tools environment.'
}

$repo = Split-Path -Parent $PSScriptRoot
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$previousTarget = $env:CARGO_TARGET_DIR
New-Item -ItemType Directory -Path $work | Out-Null
Push-Location $repo
try {
    $env:CARGO_TARGET_DIR = Join-Path $work 'target'
    $target = 'i686-pc-windows-msvc'
    cargo build --locked --release --target $target -p hello-world -p engine-demo
    cargo build --locked --target $target -p metamod-sys --example layout

    $includes = @(
        '/Iupstream/metamod-hl1/metamod'
        '/Iupstream/hlsdk/dlls'
        '/Iupstream/hlsdk/engine'
        '/Iupstream/hlsdk/common'
        '/Iupstream/hlsdk/pm_shared'
        '/Iupstream/hlsdk/public'
    )
    $layout = Join-Path $work 'layout.exe'
    $loader = Join-Path $work 'load.exe'
    cl.exe /nologo /std:c++17 /EHsc @includes verify/layout.cpp "/Fo$work/layout.obj" "/Fe$layout"
    cl.exe /nologo /std:c++17 /EHsc @includes verify/load.cpp "/Fo$work/load.obj" "/Fe$loader"
    $cppLayout = & $layout
    $rustLayout = & (Join-Path $env:CARGO_TARGET_DIR "$target/debug/examples/layout.exe")
    if (($cppLayout -join "`n") -cne ($rustLayout -join "`n")) {
        Compare-Object $cppLayout $rustLayout | Out-Host
        throw 'Rust and C++ ABI layouts differ.'
    }
    foreach ($example in 'hello-world', 'engine-demo') {
        $dll = $example.Replace('-', '_') + '.dll'
        $plugin = Join-Path $env:CARGO_TARGET_DIR "$target/release/$dll"
        $pe = llvm-readobj --file-headers --coff-exports $plugin
        $pe | Write-Output
        $headers = $pe -join "`n"
        if ($headers -notmatch 'Machine: IMAGE_FILE_MACHINE_I386' -or $headers -notmatch 'IMAGE_FILE_DLL') {
            throw 'Expected an x86 PE DLL.'
        }
        $exports = @($pe | ForEach-Object {
            if ($_ -match '^\s+Name: (\S+)$') { $Matches[1] }
        } | Sort-Object)
        $expected = @('GiveFnptrsToDll', 'Meta_Attach', 'Meta_Detach', 'Meta_Query')
        if (($exports -join ',') -cne ($expected -join ',')) {
            throw "Unexpected DLL exports: $($exports -join ', ')"
        }
        & $loader $plugin $example
    }
    Write-Output 'Win32 ABI and loader checks passed.'
} finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $previousTarget
    Remove-Item -Recurse -Force $work
}
