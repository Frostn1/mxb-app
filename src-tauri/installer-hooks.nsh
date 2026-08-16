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

!macro NSIS_HOOK_PREINSTALL
  !insertmacro CloseRunningApp
  !insertmacro CloseLegacyApp
  !insertmacro FreeMainBinary
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
