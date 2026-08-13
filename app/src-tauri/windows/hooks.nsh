; Squash context-menu verbs (docs/03 F6), wired in via Tauri's NSIS hook
; mechanism (bundle > windows > nsis > installerHooks).
;
; Registry verbs only — no shell-extension COM DLL (a signed IExplorerCommand
; extension for the Windows 11 modern menu is a documented follow-up). The
; verbs launch the GUI with the target as argv; the host routes archives to
; the extract sheet (S3) and everything else to the compress sheet (S2).
;
; Extract verbs go under SystemFileAssociations so they appear for the
; associated extensions without changing the user's default-handler choice.
; Compress verbs follow the 7-Zip precedent: all files (*) and folders
; (Directory). On Windows 11 these show under "Show more options".

!macro _SquashExtractVerb EXT
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\Squash.Extract" "" "Extract with Squash"
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\Squash.Extract" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\Squash.Extract\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro _SquashExtractVerbRemove EXT
  DeleteRegKey SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\Squash.Extract"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Extract verb for every extension Squash can open (mirrors
  ; bundle.fileAssociations in tauri.conf.json).
  !insertmacro _SquashExtractVerb "zip"
  !insertmacro _SquashExtractVerb "7z"
  !insertmacro _SquashExtractVerb "rar"
  !insertmacro _SquashExtractVerb "tar"
  !insertmacro _SquashExtractVerb "tar.gz"
  !insertmacro _SquashExtractVerb "tgz"
  !insertmacro _SquashExtractVerb "tar.bz2"
  !insertmacro _SquashExtractVerb "tbz2"
  !insertmacro _SquashExtractVerb "tar.xz"
  !insertmacro _SquashExtractVerb "txz"
  !insertmacro _SquashExtractVerb "tar.zst"
  !insertmacro _SquashExtractVerb "tzst"
  !insertmacro _SquashExtractVerb "gz"
  !insertmacro _SquashExtractVerb "xz"
  !insertmacro _SquashExtractVerb "zst"

  ; Compress verb on any file and on folders.
  WriteRegStr SHCTX "Software\Classes\*\shell\Squash.Compress" "" "Compress with Squash"
  WriteRegStr SHCTX "Software\Classes\*\shell\Squash.Compress" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\*\shell\Squash.Compress\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  WriteRegStr SHCTX "Software\Classes\Directory\shell\Squash.Compress" "" "Compress with Squash"
  WriteRegStr SHCTX "Software\Classes\Directory\shell\Squash.Compress" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\Directory\shell\Squash.Compress\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro _SquashExtractVerbRemove "zip"
  !insertmacro _SquashExtractVerbRemove "7z"
  !insertmacro _SquashExtractVerbRemove "rar"
  !insertmacro _SquashExtractVerbRemove "tar"
  !insertmacro _SquashExtractVerbRemove "tar.gz"
  !insertmacro _SquashExtractVerbRemove "tgz"
  !insertmacro _SquashExtractVerbRemove "tar.bz2"
  !insertmacro _SquashExtractVerbRemove "tbz2"
  !insertmacro _SquashExtractVerbRemove "tar.xz"
  !insertmacro _SquashExtractVerbRemove "txz"
  !insertmacro _SquashExtractVerbRemove "tar.zst"
  !insertmacro _SquashExtractVerbRemove "tzst"
  !insertmacro _SquashExtractVerbRemove "gz"
  !insertmacro _SquashExtractVerbRemove "xz"
  !insertmacro _SquashExtractVerbRemove "zst"

  DeleteRegKey SHCTX "Software\Classes\*\shell\Squash.Compress"
  DeleteRegKey SHCTX "Software\Classes\Directory\shell\Squash.Compress"
!macroend
