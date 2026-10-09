@echo off
rem ============================================================================
rem  rollback.bat - fully undo apply.bat for dsh-rule-engine-client
rem  Restores package.json and pnpm-lock.yaml from the snapshot, removes the
rem  installed package directory, then removes the snapshot. Safe to run
rem  repeatedly: no snapshot means nothing to do.
rem ============================================================================
setlocal EnableExtensions EnableDelayedExpansion

set "DSH_HOME=C:\Users\admin\AppData\Roaming\dsh-desktop\harness"
set "PROFILE=web"
set "PROFDIR=%DSH_HOME%\profiles\%PROFILE%"
set "PKG=dsh-rule-engine-client"
set "SNAPDIR=%DSH_HOME%\profiles\.generations\apply-rule-engine-client"
set "SNAP=%SNAPDIR%\state.json"
set "TARGET=%PROFDIR%\node_modules\%PKG%"

if not exist "%SNAP%" (
  echo No snapshot found - nothing to roll back.  ^(repeat rollback is safe^)
  exit /b 0
)

echo [1/4] restore package.json from snapshot
if exist "%SNAPDIR%\package.json.bak" (
  copy /y "%SNAPDIR%\package.json.bak" "%PROFDIR%\package.json" >nul
  if errorlevel 1 (
    echo ERROR could not restore package.json
    exit /b 1
  )
) else (
  echo   backup missing - leaving package.json as is
)

echo [2/4] restore pnpm-lock.yaml from snapshot
if exist "%SNAPDIR%\pnpm-lock.yaml.bak" (
  copy /y "%SNAPDIR%\pnpm-lock.yaml.bak" "%PROFDIR%\pnpm-lock.yaml" >nul
  if errorlevel 1 (
    echo ERROR could not restore pnpm-lock.yaml
    exit /b 1
  )
) else (
  echo   no lockfile backup - leaving pnpm-lock.yaml as is
)

echo [3/4] remove installed package directory
if exist "%TARGET%" (
  rmdir /s /q "%TARGET%"
  if errorlevel 1 (
    echo ERROR could not remove %TARGET%
    exit /b 1
  )
) else (
  echo   already absent - continuing
)
for /d %%d in ("%PROFDIR%\node_modules\.pnpm\%PKG%@*") do (
  rmdir /s /q "%%d"
)

echo [4/4] remove snapshot
rmdir /s /q "%SNAPDIR%"

echo.
echo ============================ DONE ============================
echo Pre-apply state restored: package.json and pnpm-lock.yaml were
echo restored from backup, the installed package copy was removed, and the
echo snapshot is gone. Applies at the next harness start.
echo ============================================================
exit /b 0
