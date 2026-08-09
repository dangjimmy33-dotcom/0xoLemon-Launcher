; Legacy releases changed the NSIS scope from current-user to per-machine.
; During an update, recover a valid older install location before NSIS falls
; back to Program Files. New launchers pass /OXO_PRESERVE_INSTALL_DIR together
; with a final /D argument, which is authoritative and bypasses this migration.
!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
    ClearErrors
    ${GetOptions} $CMDLINE "/OXO_PRESERVE_INSTALL_DIR" $R9
    ${If} ${Errors}
      ClearErrors
      ReadRegStr $R8 HKCU "${MANUPRODUCTKEY}" ""
      ${If} $R8 != ""
      ${AndIfNot} ${FileExists} "$R8\${MAINBINARYNAME}.exe"
        StrCpy $R8 ""
      ${EndIf}

      ${If} $R8 == ""
        ReadRegStr $R8 HKLM "${MANUPRODUCTKEY}" ""
        ${If} $R8 != ""
        ${AndIfNot} ${FileExists} "$R8\${MAINBINARYNAME}.exe"
          StrCpy $R8 ""
        ${EndIf}
      ${EndIf}

      ${If} $R8 != ""
        StrCpy $INSTDIR $R8
      ${EndIf}
    ${EndIf}

    ; Tauri invokes this hook after its initial `SetOutPath $INSTDIR`. When a
    ; legacy update changes $INSTDIR above, NSIS otherwise keeps writing every
    ; bundled File entry to the old $OUTDIR (usually Program Files), even though
    ; CreateDirectory commands already point at the recovered install path.
    SetOutPath "$INSTDIR"
  ${EndIf}
!macroend
