; Installer hooks — close a running MXB App before the installer or uninstaller touches
; the install folder.
;
; MXB App hides to the tray when its window is closed and launches at login, so an
; installer a user started by hand nearly always finds it running. Tauri's own
; `CheckIfAppIsRunning` prompts and then kills the process by name, but two things go
; wrong from there:
;
;   * the prompt's Cancel — and a kill that doesn't take — `Abort` the uninstaller, which
;     makes the installer's `PageLeaveReinstall` bounce back to the "already installed"
;     page. Run it again and you land on the same page: an install loop with no
;     explanation, reported against v0.7.0-beta.1 upgrading from v0.6.3.
;   * it kills `MXB App.exe` alone, leaving the WebView2 child processes to exit in their
;     own time, so a file in the install folder can still be held when the copy starts.
;
; So close it ourselves first — whole process tree, no questions — and give Windows a
; moment to release the handles. By the time Tauri's check runs there's nothing to find,
; and no dialog appears. `/T` takes the WebView2 children with it; `/F` doesn't wait on a
; window that is only in the tray.
;
; PREUNINSTALL matters as much as PREINSTALL: it's the *installed* build's uninstaller
; that the next version's installer runs, so this is what stops the loop from recurring.

!macro CloseRunningApp
  nsExec::Exec 'taskkill /F /T /IM "${MAINBINARYNAME}.exe"'
  Pop $0 ; 0 = closed it, 128 = wasn't running. Either is the state we want.
  Sleep 700
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro CloseRunningApp
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CloseRunningApp
!macroend
