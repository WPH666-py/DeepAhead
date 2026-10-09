@echo off
rem ============================================================================
rem  apply.bat - install dsh-rule-engine-client@0.1.0 into the "web" profile
rem
rem  What: the official dsh plugin subcommand path:
rem      dsh plugin --profile web add dsh-rule-engine-client@0.1.0
rem  The package is the client-half companion of dsh-rule-engine (same author,
rem  npm latest = 0.1.0). dsh-rule-engine's bundle patch references it by name
rem  as dsh-rule-engine-client; without it that row cannot resolve and the
rem  next boot would fail on it (the desktop logged exactly that during the
rem  hot-mount attempt).
rem
rem  Why this path: the desktop projection keeps real profile dependencies
rem  unchanged and only manages generation links, so a registry install via
rem  pnpm is the supported way to add this client-only package. It declares
rem  dsh.client and no dsh.bundle, so it stays a plain dependency and is
rem  shim-mounted by the market at the next boot.
rem
rem  Scope of change: profiles\web\package.json plus one dependency,
rem  profiles\web\pnpm-lock.yaml, profiles\web\node_modules\dsh-rule-engine-client.
rem  No bundle-order, cordis.patch.yml, harness, pool, or generation changes.
rem
rem  Idempotency: if the package is already installed at 0.1.0, apply exits 0
rem  with "nothing to do". If a snapshot already exists, apply aborts and
rem  asks for rollback first. On failure the snapshot is kept so rollback
rem  can fully restore.
rem ============================================================================
setlocal EnableExtensions EnableDelayedExpansion

set "DSH_HOME=C:\Users\admin\AppData\Roaming\dsh-desktop\harness"
set "PROFILE=web"
set "PROFDIR=%DSH_HOME%\profiles\%PROFILE%"
set "NODE=D:\DSH\DSH Desktop\resources\app\node_modules\node\bin\node.exe"
set "DSHCLI=D:\DSH\DSH Desktop\resources\app\node_modules\@deepseek-ai\dsh\lib\bin.js"
set "PKG=dsh-rule-engine-client"
set "VER=0.1.0"
set "SNAPDIR=%DSH_HOME%\profiles\.generations\apply-rule-engine-client"
set "SNAP=%SNAPDIR%\state.json"
set "TARGET=%PROFDIR%\node_modules\%PKG%"

echo [1/7] sanity checks
if not exist "%PROFDIR%\package.json" (
  echo ERROR profile manifest not found: %PROFDIR%\package.json
  exit /b 1
)
if not exist "%NODE%" (
  echo ERROR bundled node not found: %NODE%
  exit /b 1
)
if not exist "%DSHCLI%" (
  echo ERROR dsh CLI not found: %DSHCLI%
  exit /b 1
)
if exist "%TARGET%" (
  for /f "usebackq delims=" %%v in (`powershell -NoProfile -Command "(Get-Content '%TARGET%\package.json' -Raw | ConvertFrom-Json).version"`) do set "INSTALLED=%%v"
  if "!INSTALLED!"=="%VER%" (
    echo Already installed: %PKG%@%VER% at %TARGET%
    echo Nothing to do.
    exit /b 0
  )
  echo ERROR target exists with unexpected version !INSTALLED! - aborting.
  exit /b 1
)
if exist "%SNAP%" (
  echo ERROR snapshot already exists: %SNAP%
  echo An apply run seems to be in progress or already done. Run rollback.bat first.
  exit /b 1
)

echo [2/7] create snapshot
if not exist "%SNAPDIR%" mkdir "%SNAPDIR%"
copy /y "%PROFDIR%\package.json" "%SNAPDIR%\package.json.bak" >nul
if errorlevel 1 (
  echo ERROR could not back up package.json
  exit /b 1
)
if exist "%PROFDIR%\pnpm-lock.yaml" (
  copy /y "%PROFDIR%\pnpm-lock.yaml" "%SNAPDIR%\pnpm-lock.yaml.bak" >nul
  if errorlevel 1 (
    echo ERROR could not back up pnpm-lock.yaml
    exit /b 1
  )
)
> "%SNAP%" echo {"profile":"%PROFILE%","package":"%PKG%","version":"%VER%","status":"applying","at":"%DATE% %TIME%"}

echo [3/7] run dsh plugin add via desktop pnpm shim
set "PATH=%DSH_HOME%\.desktop-bin;%PATH%"
"%NODE%" "%DSHCLI%" plugin --profile %PROFILE% add %PKG%@%VER%
if errorlevel 1 (
  echo ERROR dsh plugin add failed. Snapshot kept: run rollback.bat to restore.
  exit /b 1
)

echo [4/7] verify installed package
if not exist "%TARGET%\package.json" (
  echo ERROR install incomplete - %TARGET%\package.json missing
  exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -Command "$p = Get-Content '%TARGET%\package.json' -Raw | ConvertFrom-Json; if ($p.name -ne '%PKG%' -or $p.version -ne '%VER%') { Write-Host ('FATAL unexpected package: ' + $p.name + '@' + $p.version); exit 1 } else { Write-Host ('verified: ' + $p.name + '@' + $p.version) }"
if errorlevel 1 (
  echo ERROR package verification failed. Snapshot kept: run rollback.bat to restore.
  exit /b 1
)

echo [5/7] verify profile manifest dependency
powershell -NoProfile -ExecutionPolicy Bypass -Command "$m = Get-Content '%PROFDIR%\package.json' -Raw | ConvertFrom-Json; if ($m.dependencies.'%PKG%' -ne '%VER%') { Write-Host ('FATAL dependency not recorded correctly: ' + $m.dependencies.'%PKG%'); exit 1 } else { Write-Host ('dependency recorded: ' + $m.dependencies.'%PKG%') }"
if errorlevel 1 (
  echo ERROR manifest check failed. Snapshot kept: run rollback.bat to restore.
  exit /b 1
)

echo [6/7] finalize snapshot
> "%SNAP%" echo {"profile":"%PROFILE%","package":"%PKG%","version":"%VER%","status":"applied","at":"%DATE% %TIME%"}

echo [7/7] done
echo.
echo ============================ DONE ============================
echo Installed %PKG%@%VER% into profile %PROFILE%.
echo dsh-rule-engine's bundle row dsh-rule-engine-client will now resolve at
echo the next boot, so the restart is safe. The package is client-only and
echo will be shim-mounted by the market at boot.
echo Verify version: 0.1.0
echo Roll back anytime with rollback.bat.
echo ============================================================
exit /b 0
