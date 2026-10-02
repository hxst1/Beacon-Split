; Installer hooks for Windows, included by Tauri's NSIS template.
;
; Beacon's daemon outlives its window, and every Claude session runs the same
; file again as its MCP server, so when an update arrives beacon-daemon.exe is
; usually running — often several times over. The installer closes the window
; and nothing else, and Windows will not overwrite a program that is running:
; the old daemon used to stay on disk beside a new window, which replaced it
; over the protocol with the same old file, over and over. See ADR-080.
;
; Windows will rename a running program, though. So the old one is moved
; aside, the new one is written in its place, and the new window replaces the
; old daemon over the protocol exactly as on macOS — where a build replaces a
; running binary without anybody noticing.

!macro BEACON_DAEMON_ASIDE
  ; What earlier updates moved aside, once nothing runs from it any more.
  ; Anything still running is left for next time.
  Delete "$INSTDIR\beacon-daemon.*.old"

  ${If} ${FileExists} "$INSTDIR\beacon-daemon.exe"
    ; The template's own registers are left as they were found.
    Push $R9
    ; A name of its own, in case one from a previous update is still in use.
    System::Call 'kernel32::GetTickCount() i .s'
    Pop $R9
    Rename "$INSTDIR\beacon-daemon.exe" "$INSTDIR\beacon-daemon.$R9.old"
    ; Gone at once when nothing was running it.
    Delete "$INSTDIR\beacon-daemon.$R9.old"
    Pop $R9
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro BEACON_DAEMON_ASIDE
!macroend

; Uninstalling has the same problem: a running daemon cannot be deleted. Moved
; aside, the uninstaller can at least remove the rest, and the next install
; into the same folder clears what is left.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro BEACON_DAEMON_ASIDE
!macroend
