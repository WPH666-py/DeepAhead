@echo off
rem ============================================================================
rem  apply.bat - fix for DSH Desktop profile "web" install-compat risk
rem
rem  Issue: desktop installer warned (warn level, event install-compat):
rem    "@shaoshi/dshscan: introduced host-compatibility risks -
rem     @deepseek-ai/dsh-tools@0.1.0-rc.6 vs 0.1.2-alpha.1"
rem  i.e. the plugin @shaoshi/dshscan@0.5.0 declares an EXACT peer
rem  @deepseek-ai/dsh-tools@0.1.0-rc.6, while the host singleton pool
rem  (profiles\node_modules) provides 0.1.2-alpha.1.
rem
rem  Fix: deliver @deepseek-ai/dsh-tools@0.1.0-rc.6 into the dshscan
rem  generation's node_modules\@deepseek-ai\dsh-tools - the exact path that
rem  pnpm's own pnpm-lock.yaml and .modules.yaml already record for this
rem  package. Node resolution from the plugin file finds this local copy
rem  FIRST (it sits above the shared pool in the walk-up chain), so the
rem  plugin loads the version it declares.
rem
rem  Scope: this script ONLY adds files inside
rem    profiles\.generations\live\shaoshi+dshscan+0.5.0+526732e5a1df\
rem  It does NOT touch the harness app, the shared pool, bundle order,
rem  cordis.patch.yml, or the profile package.json.
rem
rem  Idempotency: creates the snapshot only when none exists; if a snapshot
rem  already exists, apply ABORTS (run rollback.bat first). On partial
rem  failure the incomplete snapshot is removed so apply can be retried.
rem ============================================================================
setlocal EnableExtensions EnableDelayedExpansion

set "GEN=C:\Users\admin\AppData\Roaming\dsh-desktop\harness\profiles\.generations\live\shaoshi+dshscan+0.5.0+526732e5a1df"
set "SCOPE=%GEN%\node_modules\@deepseek-ai"
set "TARGET=%SCOPE%\dsh-tools"
set "SNAPDIR=C:\Users\admin\AppData\Roaming\dsh-desktop\harness\profiles\.generations\apply-dshscan-rc6"
set "SNAP=%SNAPDIR%\state.json"
set "WORK=%TEMP%\dshscan-rc6-fix"
set "URL_MIRROR=https://registry.npmmirror.com/@deepseek-ai/dsh-tools/-/dsh-tools-0.1.0-rc.6.tgz"
set "URL_NPM=https://registry.npmjs.org/@deepseek-ai/dsh-tools/-/dsh-tools-0.1.0-rc.6.tgz"

echo [1/6] sanity checks
if not exist "%GEN%\package.json" (
  echo ERROR: generation dir not found: %GEN%
  exit /b 1
)
if exist "%SNAP%" (
  echo ERROR: snapshot already exists: %SNAP%
  echo Apply appears to have run before. Run rollback.bat first, or delete the
  echo snapshot only if you are certain no changes are live.
  exit /b 1
)
if exist "%TARGET%" (
  echo ERROR: target already exists: %TARGET%
  echo Unexpected state - aborting without changing anything.
  exit /b 1
)

echo [2/6] create snapshot  ^(pre-state: %SCOPE% exists and is empty; only dsh-tools will be added^)
if not exist "%SNAPDIR%" mkdir "%SNAPDIR%"
> "%SNAP%" echo {"generationDir":"%GEN%","preState":"@deepseek-ai empty dir present","installed":["node_modules\@deepseek-ai\dsh-tools"],"status":"applying","at":"%DATE% %TIME%"}
if not exist "%SNAP%" (
  echo ERROR: could not write snapshot
  exit /b 1
)

echo [3/6] download dsh-tools@0.1.0-rc.6 tarball
if exist "%WORK%" rmdir /s /q "%WORK%"
mkdir "%WORK%"
powershell -NoProfile -ExecutionPolicy Bypass -Command "try { Invoke-WebRequest -UseBasicParsing '%URL_MIRROR%' -OutFile '%WORK%\pkg.tgz' -TimeoutSec 60; Write-Host 'download ok (npmmirror)'; exit 0 } catch { Write-Host ('npmmirror failed: ' + $_.Exception.Message); try { Invoke-WebRequest -UseBasicParsing '%URL_NPM%' -OutFile '%WORK%\pkg.tgz' -TimeoutSec 60; Write-Host 'download ok (npmjs fallback)'; exit 0 } catch { Write-Host ('FATAL download failed: ' + $_.Exception.Message); exit 1 } }"
if errorlevel 1 (
  echo ERROR: download failed
  goto :fail
)
if not exist "%WORK%\pkg.tgz" (
  echo ERROR: tarball missing after download
  goto :fail
)

echo [4/6] extract and verify package identity
tar -xzf "%WORK%\pkg.tgz" -C "%WORK%"
if errorlevel 1 (
  echo ERROR: tar extraction failed
  goto :fail
)
if not exist "%WORK%\package\package.json" (
  echo ERROR: tarball malformed - no package/package.json
  goto :fail
)
powershell -NoProfile -ExecutionPolicy Bypass -Command "$p = Get-Content '%WORK%\package\package.json' -Raw | ConvertFrom-Json; if ($p.name -ne '@deepseek-ai/dsh-tools' -or $p.version -ne '0.1.0-rc.6') { Write-Host ('FATAL unexpected package: ' + $p.name + '@' + $p.version); exit 1 } else { Write-Host ('verified: ' + $p.name + '@' + $p.version) }"
if errorlevel 1 (
  echo ERROR: package verification failed
  goto :fail
)

echo [5/6] install into generation node_modules
if not exist "%SCOPE%" mkdir "%SCOPE%"
move "%WORK%\package" "%TARGET%" >nul
if errorlevel 1 (
  echo ERROR: move to %TARGET% failed
  goto :fail
)
if not exist "%TARGET%\package.json" (
  echo ERROR: install incomplete - %TARGET%\package.json missing
  goto :fail
)

echo [6/6] finalize snapshot
> "%SNAP%" echo {"generationDir":"%GEN%","preState":"@deepseek-ai empty dir present","installed":["node_modules\@deepseek-ai\dsh-tools"],"status":"applied","at":"%DATE% %TIME%"}

echo.
echo =========================== DONE ===========================
echo Installed: %TARGET%
echo   Temporary files kept for inspection: %WORK%
echo   (safe to delete later)
echo.
echo The running harness is NOT affected by this change. It takes effect at the
echo NEXT harness start - restart the DSH Desktop app when convenient.
echo After restart, the dshscan plugin resolves the exact @deepseek-ai/dsh-tools
echo version it declares (0.1.0-rc.6) instead of the pool's 0.1.2-alpha.1.
echo.
echo Verify the installed copy now with:
echo   powershell -NoProfile -Command "(Get-Content '%TARGET%\package.json' -Raw ^| ConvertFrom-Json).version"
echo Expected output: 0.1.0-rc.6
echo Roll back anytime with rollback.bat.
echo ===========================================================
exit /b 0

:fail
echo.
echo APPLY FAILED - removing the incomplete snapshot so apply can be retried.
del "%SNAP%" >nul 2>&1
exit /b 1
