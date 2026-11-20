$d = Split-Path -Parent $MyInvocation.MyCommand.Path
$log = Join-Path $d 'out.txt'
function L($m) { ('{0} {1}' -f (Get-Date).ToString('s'), $m) | Out-File -Append -Encoding utf8 $log }
L 'start'
L ('os: ' + (Get-CimInstance Win32_OperatingSystem).Caption + ' build ' + (Get-CimInstance Win32_OperatingSystem).BuildNumber)
$wv = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
L ('webview2 runtime: ' + $wv.pv)
$exe = Get-ChildItem -Path $d -Filter '*setup.exe' | Select-Object -First 1
L ('installer: ' + $exe.Name)
$p = Start-Process -FilePath $exe.FullName -ArgumentList '/S' -Wait -PassThru
L ('installer exit code: ' + $p.ExitCode)
Start-Sleep 3
$app = Get-ChildItem -Path "$env:LOCALAPPDATA\AMI_Calculator","$env:ProgramFiles\AMI_Calculator" -Filter '*.exe' -Recurse -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'ami|AMI' -and $_.Name -notmatch 'uninst' } | Select-Object -First 1
L ('installed exe: ' + $app.FullName)
L ('start menu: ' + ((Get-ChildItem -Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs","$env:ProgramData\Microsoft\Windows\Start Menu\Programs" -Filter '*AMI*' -Recurse -ErrorAction SilentlyContinue | ForEach-Object FullName) -join '; '))
$un = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -match 'AMI' }
L ('uninstall entry: ' + $un.DisplayName + ' ' + $un.DisplayVersion)
if ($app) {
  Start-Process -FilePath $app.FullName
  Start-Sleep 25
  L ('process running: ' + [bool](Get-Process | Where-Object { $_.ProcessName -match 'ami' }))
  L ('db: ' + ((Get-ChildItem -Path "$env:APPDATA\com.seo1323.amicalculator","$env:LOCALAPPDATA\com.seo1323.amicalculator" -Recurse -Filter 'ami.db' -ErrorAction SilentlyContinue | ForEach-Object FullName) -join '; '))
}
L 'done'
