; Extra steps for the Windows (NSIS) installer. Tauri includes this file through
; `bundle > windows > nsis > installerHooks` in tauri.conf.json.

; Since Windows 8 no installer can make itself the default app for a file type;
; only the user can, in Settings. So the finish page offers a link to PDFRivet's
; page in Settings > Default apps (Windows 11; Windows 10 opens the Default apps
; page). Tauri's finish page already uses both of its checkboxes, and this file is
; included before the pages are declared, so a link is what fits. The name after
; `registeredAppUser=` is the one written to RegisteredApplications below.
!define MUI_FINISHPAGE_LINK "Make PDFRivet your default PDF app"
!define MUI_FINISHPAGE_LINK_LOCATION "ms-settings:defaultapps?registeredAppUser=PDFRivet"

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri has just pointed `.pdf` at its file class, so read the class name back
  ; instead of assuming it. The uninstaller removes that whole class key, icon included.
  ReadRegStr $0 SHCTX "Software\Classes\.pdf" ""
  ${If} $0 != ""
    ; Tauri registers .pdf files with the app's own icon. Give them the PDF document
    ; icon instead (pdf-file.ico, installed next to the app as a bundle resource).
    WriteRegStr SHCTX "Software\Classes\$0\DefaultIcon" "" "$INSTDIR\pdf-file.ico"

    ; List PDFRivet under "Open with" and in Settings > Default apps, where the
    ; user can choose it for PDFs (Tauri only writes the class).
    WriteRegStr SHCTX "Software\Classes\.pdf\OpenWithProgids" "$0" ""
    WriteRegStr SHCTX "${MANUPRODUCTKEY}\Capabilities" "ApplicationName" "${PRODUCTNAME}"
    WriteRegStr SHCTX "${MANUPRODUCTKEY}\Capabilities" "ApplicationDescription" "Free, fast PDF reader and editor"
    WriteRegStr SHCTX "${MANUPRODUCTKEY}\Capabilities" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
    WriteRegStr SHCTX "${MANUPRODUCTKEY}\Capabilities\FileAssociations" ".pdf" "$0"
    WriteRegStr SHCTX "Software\RegisteredApplications" "${PRODUCTNAME}" "${MANUPRODUCTKEY}\Capabilities"

    ; Tell Explorer the associations changed, so it doesn't keep showing the cached icon.
    !insertmacro UPDATEFILEASSOC
  ${EndIf}
!macroend

; Undo the registration above. Runs before Tauri's own clean-up, which may delete
; ${MANUPRODUCTKEY} (where the class name is kept). An update reinstalls it all.
!macro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $0 SHCTX "${MANUPRODUCTKEY}\Capabilities\FileAssociations" ".pdf"
  ${If} $0 != ""
    DeleteRegValue SHCTX "Software\Classes\.pdf\OpenWithProgids" "$0"
  ${EndIf}
  DeleteRegValue SHCTX "Software\RegisteredApplications" "${PRODUCTNAME}"
  DeleteRegKey SHCTX "${MANUPRODUCTKEY}\Capabilities"
!macroend
