; Installer hooks — clear the way before the installer writes over the app it is replacing.
;
; MXB App hides to the tray when its window is closed and launches at login, so an
; installer a user started by hand nearly always finds it running; the in-app updater
; launches the installer from inside the app itself. Either way `$INSTDIR\MXB App.exe` — the
; app's own image — can still be held when the copy starts, and NSIS answers that with a
; blunt "error opening file for writing", reported against v0.8.1.
;
; Tauri's own `CheckIfAppIsRunning` is not enough on its own:
;
;   * its prompt's Cancel — and a kill that doesn't take — `Abort` the uninstaller, which
;     makes the installer's `PageLeaveReinstall` bounce back to the "already installed"
;     page. Run it again and you land on the same page: an install loop with no
;     explanation, reported against v0.7.0-beta.1 upgrading from v0.6.3.
;   * it only looks for a live process. A file can be held by something that is no longer
;     one — a process still tearing down, an antivirus reading the image it just watched
;     exit — and then the check finds nothing to close and the copy fails anyway. That is
;     the v0.8.1 report: the "app is running" prompt never appeared, yet the write failed.
;
; So don't rely on winning the race for the lock. Close the app, then clear the path by
; whatever means Windows allows: delete it if we can, and if we can't, *rename* it. An
; image that is mapped can't be deleted or overwritten, but it can be renamed on the same
; volume — that is how browsers replace themselves while running — which leaves the name
; free for `File` no matter who is still holding the old bytes.
;
; PREUNINSTALL matters as much as PREINSTALL: it's the *installed* build's uninstaller
; that the next version's installer runs, and its `Delete` of the main binary is what
; `PageLeaveReinstall` checks before deciding the uninstall failed. Freeing the name there
; is what stops the install loop from recurring.

; `x64.nsh` for `${RunningX64}` and the WOW64 redirection switches `EnsureVc140` needs.
; Guarded internally, so including it here is safe next to whatever the template pulls in.
!include x64.nsh

!macro CloseRunningApp
  ; `/F` because a window that only exists in the tray won't answer a polite close. No
  ; `/T`: the in-app updater launches this installer from inside the app, so the installer
  ; is a child of the process being killed and `/T` would take the installer down with it.
  ; The WebView2 children `/T` used to cover exit with their host anyway, and never hold
  ; the app's own image.
  nsExec::Exec 'taskkill /F /IM "${MAINBINARYNAME}.exe"'
  Pop $0 ; 0 = closed it, 128 = wasn't running. Either is the state we want.
!macroend

; Up to v0.9.2 the app shipped as `frost.exe`. `${MAINBINARYNAME}` no longer names it, so
; everything above walks straight past the build being replaced: an upgrade a user starts by
; hand leaves the old app running in the tray, and its image sits in `$INSTDIR` forever
; because `DropMovedBinaries` only sweeps the current name.
!macro CloseLegacyApp
  nsExec::Exec 'taskkill /F /IM "frost.exe"'
  Pop $0 ; 0 = closed it, 128 = wasn't running. Either is the state we want.
!macroend

!macro DropLegacyBinaries
  Delete "$INSTDIR\frost.exe"
  Delete "$INSTDIR\frost.exe.old*"
!macroend

; Drop the images past installs moved aside. Plain `Delete`, not `/REBOOTOK`: a leftover
; here is an orphan file, not a broken install, and `/REBOOTOK` would raise the reboot flag
; and put a "restart your computer" choice on the finish page over it.
!macro DropMovedBinaries
  Delete "$INSTDIR\${MAINBINARYNAME}.exe.old*"
!macroend

; Leave `$INSTDIR\${MAINBINARYNAME}.exe` free for the installer to write.
!macro FreeMainBinary
  !insertmacro DropMovedBinaries

  ; A process that was just killed can hold its image for a moment longer, so give the
  ; delete a few tries before concluding someone means to keep it. Two seconds all told,
  ; and only when it's actually held — the first pass carries the normal case.
  StrCpy $1 8
  ${Do}
    ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ${ExitDo}
    ${EndIf}

    Delete "$INSTDIR\${MAINBINARYNAME}.exe"
    ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ${ExitDo}
    ${EndIf}

    IntOp $1 $1 - 1
    ${If} $1 <= 0
      ${ExitDo}
    ${EndIf}
    Sleep 250
  ${Loop}

  ; Still there. Move it out of the name instead of fighting for it. The numbered suffix
  ; covers the one case a plain `.old` can't: a past install already parked a still-running
  ; app there and it has not been restarted since, so `DropMovedBinaries` couldn't clear
  ; it. Normally nothing is parked and this stops at `.old0`.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $2 0
    ${Do}
      StrCpy $3 "$INSTDIR\${MAINBINARYNAME}.exe.old$2"
      ${IfNot} ${FileExists} "$3"
        ${ExitDo}
      ${EndIf}
      IntOp $2 $2 + 1
    ${LoopUntil} $2 >= 10
    Delete "$3" ; only reachable with every slot taken; best effort, then take the name.
    Rename "$INSTDIR\${MAINBINARYNAME}.exe" "$3"
  ${EndIf}
!macroend

; The Visual C++ 2015-2022 x64 runtime, which the app cannot start without.
;
; `MXB App.exe` imports exactly two symbols from `MSVCP140.dll` — `std::_Xout_of_range` and
; `std::_Xlength_error`, the STL's throw helpers — by way of UnRAR's C++ sources, which
; `unrar_sys` builds against the dynamic CRT. `MSVCP140.dll` is not an inbox Windows file;
; it arrives only with the redistributable. On a machine that has never had it the first
; launch dies in the loader with
;
;   The application was unable to start correctly (0xc000007b)
;
; and nothing else — no window, no log line, nothing the app can report, because this
; happens before `main`. Every build since v0.3.2 has carried the import; it stays invisible
; because some other game nearly always brings the runtime in first. What exposes it is a
; clean Windows: reported by a player who had been running MXB App for weeks and hit this
; the day after resetting their PC.
;
; `crate::vcruntime` already detects and installs this exact package and cannot help here —
; it runs inside the process that can't start. The installer is the last thing that runs
; before the dependency matters, so the check belongs here.

!define VC140_URL "https://aka.ms/vs/17/release/vc_redist.x64.exe"

; Sets `$0` to 1 when the runtime is present, 0 when any part of it is missing.
;
; The same three files `VC140_DLLS` probes, not just the one we import: FrostMod needs the
; other two, one package carries all three, and this is the one moment we are already
; fetching it. `vcruntime140_1.dll` is the one that catches a machine still on the original
; 2015 package.
!macro ProbeVc140
  ; NSIS builds 32-bit installers, so under WOW64 every `$SYSDIR` read is redirected to
  ; `SysWOW64` — the *32-bit* runtime's home, and the wrong question. Left alone, this
  ; probe would report the x64 runtime missing on a machine that has it and present on a
  ; machine that only has the x86 package. Unredirected, `$SYSDIR` means what it says.
  ${DisableX64FSRedirection}
  StrCpy $0 1
  ${IfNot} ${FileExists} "$SYSDIR\vcruntime140.dll"
    StrCpy $0 0
  ${ElseIfNot} ${FileExists} "$SYSDIR\vcruntime140_1.dll"
    StrCpy $0 0
  ${ElseIfNot} ${FileExists} "$SYSDIR\msvcp140.dll"
    StrCpy $0 0
  ${EndIf}
  ${EnableX64FSRedirection}
!macroend

; Inserted once, from `NSIS_HOOK_PREINSTALL`. The label below makes a second insertion a
; compile error rather than a quiet duplicate-symbol surprise.
!macro EnsureVc140
  ; A 32-bit Windows can't run this app at all, so there is nothing to preflight there.
  ${If} ${RunningX64}
    !insertmacro ProbeVc140
    ${If} $0 == 0
      DetailPrint "Installing the Microsoft Visual C++ 2015-2022 (x64) runtime..."

      ; `NSISdl`, the downloader the template carries, speaks plain HTTP; this URL is HTTPS
      ; and redirects. PowerShell is inbox on every Windows the app supports. `$\'` is a
      ; literal quote — the command below reaches PowerShell as ordinary single-quoted
      ; arguments.
      ;
      ; `-TimeoutSec` because an install that hangs on a dead connection is worse than one
      ; that fails: the failure has a message box behind it and a user who can act on it.
      nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri $\'${VC140_URL}$\' -OutFile $\'$PLUGINSDIR\vc_redist.x64.exe$\'"'
      Pop $1

      ${If} ${FileExists} "$PLUGINSDIR\vc_redist.x64.exe"
        ; Burn bundle switches, which are not the 2008 package's — it treats a bare `/q` as
        ; unknown and does nothing.
        nsExec::ExecToLog '"$PLUGINSDIR\vc_redist.x64.exe" /install /quiet /norestart'
        Pop $1
      ${EndIf}

      ; Re-probe rather than read an exit code. A captive portal answers 200 with an HTML
      ; page that is not an installer, and the package itself exits 1638 when a newer build
      ; is already in — neither number means what it looks like. The files either exist now
      ; or they don't.
      !insertmacro ProbeVc140
      ${If} $0 == 0
        ; Never `Abort`. A half-installed folder helps nobody, and someone who fetches the
        ; runtime by hand afterwards should find the app already waiting for it.
        ;
        ; Silent is the in-app updater running this installer from inside the app. A modal
        ; there would hang an update with no window to answer it.
        ${IfNot} ${Silent}
          MessageBox MB_YESNO|MB_ICONEXCLAMATION "MXB App needs the Microsoft Visual C++ 2015-2022 (x64) runtime, and this PC doesn't have it. Installing it just now didn't work.$\r$\n$\r$\nMXB App is installed either way, but it will close on launch with error 0xc000007b until the runtime is in. Open Microsoft's download page?" IDNO vc140_declined
          ExecShell "open" "${VC140_URL}"
          vc140_declined:
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro CloseRunningApp
  !insertmacro CloseLegacyApp
  !insertmacro FreeMainBinary
  ; Last: freeing the binary is a race against a process that just died, while this can
  ; spend a minute on the wire.
  !insertmacro EnsureVc140
!macroend

; By now anything we moved aside is dead, so this is the pass that usually clears it and
; leaves the install folder with nothing but the build that was just written.
!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DropMovedBinaries
  !insertmacro DropLegacyBinaries
!macroend

; The uninstaller's own `RMDir "$INSTDIR"` runs before its POSTUNINSTALL hook, so the
; leftovers have to go here for the folder to come out with them.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CloseRunningApp
  !insertmacro CloseLegacyApp
  !insertmacro FreeMainBinary
  !insertmacro DropLegacyBinaries
!macroend
