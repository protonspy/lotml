<#
.SYNOPSIS
Shows the LotML file icon on .lot and .lotml files in Windows Explorer, for the current user only
(specs/file-icons/ R2.1-R2.3).

.DESCRIPTION
Writes a LotML.Source file type under HKCU\Software\Classes, with the icon copied to
%LOCALAPPDATA%\LotML, and points .lot and .lotml at it. No administrator is needed. An extension
another type already claims, for the user or for the machine, is left as it is unless -Force is
given; the type -Force displaces is recorded. -Remove takes back exactly what this script wrote
and gives a displaced extension back to its type.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File editors\windows\register.ps1

.EXAMPLE
powershell -ExecutionPolicy Bypass -File editors\windows\register.ps1 -Remove
#>
param(
    [switch]$Remove,
    [switch]$Force,
    # Where the file type and the extensions are written; the tests point it at a throwaway key.
    [string]$Root = 'HKCU:\Software\Classes',
    # Where the machine's own associations are read, never written; the tests move it too.
    [string]$MachineRoot = 'HKLM:\Software\Classes',
    # Where the icon is copied; the tests point it at a scratch directory.
    [string]$IconHome = (Join-Path $env:LOCALAPPDATA 'LotML')
)

$ErrorActionPreference = 'Stop'
$Type = 'LotML.Source'
$Extensions = @('.lot', '.lotml')
$Icon = Join-Path $IconHome 'lotml-file.ico'
$Source = Join-Path $PSScriptRoot '..\icons\lotml-file.ico'

# The default value of a key, or $null when the key or the value is absent.
function Get-Default([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    return (Get-Item -LiteralPath $Path).GetValue('')
}

# Set a key's default value, creating the key, and only the key, when it is absent: New-Item -Force
# on an existing registry key would empty it.
function Set-Default([string]$Path, [string]$Value) {
    if (-not (Test-Path -LiteralPath $Path)) { New-Item -Path $Path -Force | Out-Null }
    Set-Item -LiteralPath $Path -Value $Value
}

# Tell Explorer the associations changed, so it draws the icons again.
function Update-Explorer {
    if (-not ('LotML.Shell' -as [type])) {
        Add-Type -Namespace LotML -Name Shell -MemberDefinition @'
[DllImport("shell32.dll")]
public static extern void SHChangeNotify(int eventId, uint flags, IntPtr item1, IntPtr item2);
'@
    }
    [LotML.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}

$TypeKey = Join-Path $Root $Type

if ($Remove) {
    foreach ($extension in $Extensions) {
        $key = Join-Path $Root $extension
        if ((Get-Default $key) -ne $Type) { continue }
        # A type -Force displaced was recorded on LotML's own type, and is given back.
        $displaced = if (Test-Path -LiteralPath $TypeKey) {
            (Get-Item -LiteralPath $TypeKey).GetValue("Displaced$extension")
        }
        if ($displaced) {
            Set-Item -LiteralPath $key -Value $displaced
            Write-Output "${extension}: given back to $displaced"
            continue
        }
        $writable = (Get-Item -LiteralPath $key).OpenSubKey('', $true)
        $writable.DeleteValue('')
        $writable.Close()
        $item = Get-Item -LiteralPath $key
        if ($item.SubKeyCount -eq 0 -and $item.ValueCount -eq 0) { Remove-Item -LiteralPath $key }
        Write-Output "${extension}: association removed"
    }
    if (Test-Path -LiteralPath $TypeKey) { Remove-Item -LiteralPath $TypeKey -Recurse }
    if (Test-Path -LiteralPath $Icon) { Remove-Item -LiteralPath $Icon }
    if ((Test-Path -LiteralPath $IconHome) -and -not (Get-ChildItem -LiteralPath $IconHome -Force)) {
        Remove-Item -LiteralPath $IconHome
    }
    Update-Explorer
    return
}

New-Item -ItemType Directory -Path $IconHome -Force | Out-Null
Copy-Item -LiteralPath $Source -Destination $Icon -Force
Set-Default $TypeKey 'LotML source file'
Set-Default (Join-Path $Root "$Type\DefaultIcon") $Icon
foreach ($extension in $Extensions) {
    $key = Join-Path $Root $extension
    $mine = Get-Default $key
    $owner = if ($mine) { $mine } else { Get-Default (Join-Path $MachineRoot $extension) }
    if ($owner -and $owner -ne $Type -and -not $Force) {
        Write-Warning "$extension belongs to $owner; left as it is, -Force gives it to LotML"
        continue
    }
    if ($mine -and $mine -ne $Type) {
        Set-ItemProperty -LiteralPath $TypeKey -Name "Displaced$extension" -Value $mine
    }
    Set-Default $key $Type
    $choice = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$extension\UserChoice"
    $chosen = if (Test-Path -LiteralPath $choice) { (Get-Item -LiteralPath $choice).GetValue('ProgId') }
    if ($chosen -and $chosen -ne $Type) {
        Write-Warning "$extension opens with $chosen, as chosen in Explorer, which shows that program's icon"
    }
    Write-Output "${extension}: LotML source file"
}
Update-Explorer
