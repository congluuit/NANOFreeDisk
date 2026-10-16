; NANOFreeDisk NSIS hooks (wired in apps/nanofreedisk/src-tauri/tauri.conf.json).
; See docs/PHASE8_INSTALLERS.md §2.1 / §2.3.
;
; Uninstall:
; * removes the per-user Explorer menu "Analyze with NANOFreeDisk" and "Find
;   duplicates here" (A7): the same keys nfd_platform::shell_menu writes;
; * runs `nanofreedisk.exe --uninstall-cleanup` BEFORE Tauri's template deletes the
;   data folders, so files still in the quarantine are restored or deleted only
;   after a clear question (never silently). The cleanup removes the data itself
;   when "Delete the application data" is ticked (or /REMOVEDATA is passed) and
;   keeps a quarantine that still holds files, so the template's own RmDir is
;   switched off;
; * removes HKCU\Software\<publisher>\NANOFreeDisk (install location, installer
;   language) that the template only removes together with the data.
; Updates (/UPDATE) touch none of this.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\NANOFreeDiskDupes"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\NANOFreeDiskDupes"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\NANOFreeDiskDupes"

  ${If} $UpdateMode <> 1
    ; /REMOVEDATA on the command line = tick "Delete the application data" (silent)
    ClearErrors
    ${GetOptions} $CMDLINE "/REMOVEDATA" $R9
    ${IfNot} ${Errors}
      StrCpy $DeleteAppDataCheckboxState 1
    ${EndIf}

    StrCpy $R8 "--uninstall-cleanup"
    ${If} $LANGUAGE == 1066
      StrCpy $R8 "$R8 --lang vi"
    ${Else}
      StrCpy $R8 "$R8 --lang en"
    ${EndIf}
    ${If} ${Silent}
    ${OrIf} $PassiveMode = 1
      StrCpy $R8 "$R8 --silent"
    ${EndIf}
    ${If} $DeleteAppDataCheckboxState = 1
      StrCpy $R8 "$R8 --remove-data"
    ${EndIf}
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ; the data folders are per user: run as the user even from an elevated uninstaller
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" $R8'
    ${EndIf}
    ; the cleanup removed the data (and kept a quarantine that still holds files)
    StrCpy $DeleteAppDataCheckboxState 0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; install location + installer language: only NANOFreeDisk writes these keys
    DeleteRegKey HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"
  ${EndIf}
!macroend
