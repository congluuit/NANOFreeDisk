; NANOFreeDisk NSIS hooks (wired in apps/nanofreedisk/src-tauri/tauri.conf.json).
; Uninstalling removes the per-user Explorer menu "Analyze with NANOFreeDisk" and
; "Find duplicates here" (A7):
; the same keys nfd_platform::shell_menu writes.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\NANOFreeDisk"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\NANOFreeDiskDupes"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\NANOFreeDiskDupes"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\NANOFreeDiskDupes"
!macroend
