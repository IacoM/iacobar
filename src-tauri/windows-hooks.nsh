; Before files are removed: give Windows its taskbar back and drop autostart.
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\iacobar.exe" --restore'
!macroend
