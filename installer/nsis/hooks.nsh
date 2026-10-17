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
;
; Install (the package is per user, so it can never write to Program Files):
; * the folder page disables Next and says why when the chosen folder cannot be
;   written without administrator rights (.onVerifyInstDir);
; * file errors offer Abort / Retry only: "Ignore" used to finish a broken install
;   that registered a folder holding nothing;
; * a failed or aborted fresh install is rolled back (.onInstFailed): its files,
;   the Apps & features entry and the saved install location, so the next run
;   starts clean and can use another folder;
; * an entry left by such a broken install (uninstaller missing) is dropped when
;   the installer starts, and the default folder is offered again.

; This file is included before the template's own !defines, and functions are
; compiled where they stand, so they use these copies (keep in sync with
; tauri.conf.json: productName, publisher, the main binary).
!define NFD_PRODUCT "NANOFreeDisk"
!define NFD_EXE "nanofreedisk.exe"
!define NFD_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\NANOFreeDisk"
!define NFD_MANUKEY "Software\NANO (nanodev.io.vn)"
!define NFD_PRODUCTKEY "Software\NANO (nanodev.io.vn)\NANOFreeDisk"

AllowSkipFiles off

Var NfdProbeDir   ; folder last probed (cache: .onVerifyInstDir runs per keystroke)
Var NfdProbeOk    ; 1 = $INSTDIR can be created and written
Var NfdDirText    ; the folder page's original description
Var NfdStarted    ; 1 once the Install section began
Var NfdHadExe     ; 1 = the app was already in $INSTDIR
Var NfdHadReg     ; 1 = a working installation was registered before

!define MUI_CUSTOMFUNCTION_GUIINIT NfdGuiInit

; $NfdState: "live" (registered and its uninstaller exists), "stale" (registered,
; nothing there) or "none". $NfdStaleDir: the saved location of a stale entry.
Var NfdState
Var NfdStaleDir
Function NfdReadState
  Push $0
  Push $1
  StrCpy $NfdState "none"
  StrCpy $NfdStaleDir ""
  ReadRegStr $0 SHCTX "${NFD_UNINSTKEY}" "UninstallString"
  ReadRegStr $1 SHCTX "${NFD_PRODUCTKEY}" ""
  ${If} $0 != ""
    ; "<dir>\uninstall.exe", maybe followed by arguments (the branded setup writes
    ; the same key with --uninstall). Anything else counts as live: never drop an
    ; entry this cannot read.
    StrCpy $NfdState "live"
    StrCpy $1 $0 1
    ${If} $1 == '"'
      StrCpy $0 $0 "" 1
      ${StrLoc} $1 $0 '"' ">"
      ${If} $1 != ""
        StrCpy $0 $0 $1
        ${IfNot} ${FileExists} "$0"
          StrCpy $NfdState "stale"
        ${EndIf}
      ${EndIf}
    ${EndIf}
    ReadRegStr $1 SHCTX "${NFD_PRODUCTKEY}" ""
  ${ElseIf} $1 != ""
    ${IfNot} ${FileExists} "$1\${NFD_EXE}"
      StrCpy $NfdState "stale"
    ${EndIf}
  ${EndIf}
  ${If} $NfdState == "stale"
    StrCpy $NfdStaleDir $1
  ${EndIf}
  Pop $1
  Pop $0
FunctionEnd

; Runs before the first page: forget an entry a broken install left behind, so the
; "already installed" page does not show and the folder page offers the default.
Function NfdGuiInit
  Call NfdReadState
  ${If} $NfdState == "stale"
    DeleteRegKey SHCTX "${NFD_UNINSTKEY}"
    DeleteRegValue SHCTX "${NFD_PRODUCTKEY}" ""
    ${If} $NfdStaleDir != ""
    ${AndIf} $INSTDIR == $NfdStaleDir
      StrCpy $INSTDIR "$LOCALAPPDATA\${NFD_PRODUCT}"
    ${EndIf}
  ${EndIf}
FunctionEnd

; Can $INSTDIR be created and written by this user? Probes the deepest folder that
; exists the way the install will use it: creates a sub-folder there and a file in
; it, then removes both. Result in $NfdProbeOk; cached per probed folder.
Function NfdProbeInstDir
  Push $0
  Push $1
  Push $2
  Push $3
  StrCpy $0 $INSTDIR
  ${Do}
    ${If} ${FileExists} "$0\*.*"
      ${Break}
    ${EndIf}
    ${GetParent} $0 $1
    ${If} $1 == ""
    ${OrIf} $1 == $0
      StrCpy $0 ""
      ${Break}
    ${EndIf}
    StrCpy $0 $1
  ${Loop}

  ${If} $0 != $NfdProbeDir
  ${OrIf} $0 == ""
    StrCpy $NfdProbeDir $0
    StrCpy $NfdProbeOk 0
    ${If} $0 != ""
      StrCpy $1 "$0\.nfd-setup-probe"
      StrCpy $3 0
      ${IfNot} ${FileExists} "$1\*.*"
        ClearErrors
        CreateDirectory $1
        ${IfNot} ${Errors}
          StrCpy $3 1
        ${EndIf}
      ${EndIf}
      ClearErrors
      FileOpen $2 "$1\probe.tmp" w
      ${IfNot} ${Errors}
        FileClose $2
        Delete "$1\probe.tmp"
        StrCpy $NfdProbeOk 1
      ${EndIf}
      ${If} $3 = 1
        RMDir $1
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Folder page: Next stays disabled for a folder this user cannot write to, and the
; description above the path says why.
Function .onVerifyInstDir
  Push $0
  Push $1
  Push $2
  Call NfdProbeInstDir
  FindWindow $0 "#32770" "" $HWNDPARENT
  GetDlgItem $1 $0 1019 ; the path box: only the folder page has it
  ${If} $1 <> 0
    GetDlgItem $1 $0 1006 ; the description
    ${If} $NfdDirText == ""
      System::Call 'user32::GetWindowText(p r1, t .r2, i ${NSIS_MAX_STRLEN})'
      StrCpy $NfdDirText $2
    ${EndIf}
    ${If} $NfdProbeOk = 1
      StrCpy $2 $NfdDirText
    ${ElseIf} $LANGUAGE == 1066
      StrCpy $2 "Không ghi được vào thư mục này nếu không có quyền quản trị (NANOFreeDisk chỉ cài cho tài khoản của bạn). Hãy chọn thư mục khác, ví dụ $LOCALAPPDATA\${NFD_PRODUCT}"
    ${Else}
      StrCpy $2 "This folder cannot be written without administrator rights (NANOFreeDisk installs for your account only). Choose another folder, for example $LOCALAPPDATA\${NFD_PRODUCT}"
    ${EndIf}
    SendMessage $1 ${WM_SETTEXT} 0 "STR:$2"
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
  ${If} $NfdProbeOk <> 1
    Abort
  ${EndIf}
FunctionEnd

; Undo a fresh install that failed or was aborted. Removes only what this run
; created: the files the package installs (never the folder's other content: the
; folder is removed only when empty), the registry entries and the shortcuts.
Function .onInstFailed
  ${If} $NfdStarted = 1
    ${If} $NfdHadExe <> 1
      Delete "$INSTDIR\${NFD_EXE}"
      Delete "$INSTDIR\nfd.exe"
      Delete "$INSTDIR\uninstall.exe"
      RMDir "$INSTDIR"
    ${EndIf}
    ${If} $NfdHadReg <> 1
      DeleteRegKey SHCTX "${NFD_UNINSTKEY}"
      DeleteRegKey SHCTX "${NFD_PRODUCTKEY}"
      DeleteRegKey /ifempty SHCTX "${NFD_MANUKEY}"
      ; no shortcuts to undo: the template makes them last, after every file, and
      ; a .lnk of the same name may belong to another package
    ${EndIf}
  ${EndIf}
FunctionEnd

; Runs right after the template's SetOutPath $INSTDIR, before any file is copied.
!macro NSIS_HOOK_PREINSTALL
  StrCpy $NfdStarted 1
  StrCpy $NfdHadExe 0
  ${If} ${FileExists} "$INSTDIR\${NFD_EXE}"
    StrCpy $NfdHadExe 1
  ${EndIf}
  Call NfdReadState
  StrCpy $NfdHadReg 0
  ${If} $NfdState == "live"
    StrCpy $NfdHadReg 1
  ${EndIf}

  ; silent / passive installs skip the folder page: check here as well
  StrCpy $NfdProbeDir "|"
  Call NfdProbeInstDir
  ${If} $NfdProbeOk <> 1
    ${If} $LANGUAGE == 1066
      StrCpy $R9 "Không thể ghi vào $INSTDIR.$\r$\n$\r$\nNANOFreeDisk chỉ cài cho tài khoản của bạn nên không dùng được thư mục cần quyền quản trị (như Program Files). Chưa có gì được cài. Hãy chạy lại trình cài đặt và chọn thư mục khác."
    ${Else}
      StrCpy $R9 "Cannot write to $INSTDIR.$\r$\n$\r$\nNANOFreeDisk installs for your account only, so it cannot use a folder that needs administrator rights (such as Program Files). Nothing was installed. Run the installer again and choose another folder."
    ${EndIf}
    DetailPrint "$R9"
    MessageBox MB_ICONSTOP|MB_OK "$R9" /SD IDOK
    Abort
  ${EndIf}
!macroend

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
