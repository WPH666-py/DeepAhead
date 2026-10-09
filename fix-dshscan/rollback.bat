@echo off
rem ============================================================================
rem  rollback.bat - fully undo apply.bat
rem
rem  Removes the @deepseek-ai\dsh-tools copy that apply.bat installed into the
rem  dshscan generation node_modules, then deletes the apply snapshot.
rem  The pre-existing (empty) @deepseek-ai scope directory is left intact,
rem  exactly matching the pre-apply state.
rem
rem  Safety:
rem    - no snapshot  -> does nothing, exits 0 (repeat rollback is safe)
rem    - apply failed halfway -> snapshot exists, target may be missing:
rem      rollback just removes the snapshot
rem    - never touches the shared pool, the harness app, bundle order,
rem      cordis.patch.yml, or the profile package.json
rem ============================================================================
setlocal EnableExtensions

set "GEN=C:\Users\admin\AppData\Roaming\dsh-desktop\harness\profiles\.generations\live\shaoshi+dshscan+0.5.0+526732e5a1df"
set "TARGET=%GEN%\node_modules\@deepseek-ai\dsh-tools"
set "SNAPDIR=C:\Users\admin\AppData\Roaming\dsh-desktop\harness\profiles\.generations\apply-dshscan-rc6"
set "SNAP=%SNAPDIR%\state.json"

if not exist "%SNAP%" (
  echo No snapshot found - nothing to roll back.  ^(repeat rollback is safe^)
  exit /b 0
)

echo [1/2] removing installed copy: %TARGET%
if exist "%TARGET%" (
  rmdir /s /q "%TARGET%"
  if errorlevel 1 (
    echo ERROR: could not remove %TARGET%
    exit /b 1
  )
) else (
  echo   ^(already absent - continuing^)
)

echo [2/2] removing snapshot
del /q "%SNAP%"
if errorlevel 1 (
  echo ERROR: could not remove snapshot %SNAP%
  exit /b 1
)
if exist "%SNAPDIR%" rmdir "%SNAPDIR%" >nul 2>&1

echo.
echo ============================ DONE ============================
echo Pre-apply state restored: the dsh-tools copy inside the dshscan
echo generation is gone; the empty @deepseek-ai scope dir remains.
echo The runtime falls back to the host pool version (0.1.2-alpha.1)
echo at the next harness start.
echo ============================================================
exit /b 0
