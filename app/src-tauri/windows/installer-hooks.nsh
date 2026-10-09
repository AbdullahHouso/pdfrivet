; Extra steps for the Windows (NSIS) installer. Tauri includes this file through
; `bundle > windows > nsis > installerHooks` in tauri.conf.json.

; Tauri registers .pdf files with the app's own icon. Give them the PDF document
; icon instead (pdf-file.ico, installed next to the app as a bundle resource).
; Tauri has just pointed `.pdf` at its file class, so read the class name back
; instead of assuming it. The uninstaller removes that whole class key, icon included.
!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $0 SHCTX "Software\Classes\.pdf" ""
  ${If} $0 != ""
    WriteRegStr SHCTX "Software\Classes\$0\DefaultIcon" "" "$INSTDIR\pdf-file.ico"
    ; Tell Explorer the icon changed, so it doesn't keep showing the cached one.
    !insertmacro UPDATEFILEASSOC
  ${EndIf}
!macroend
