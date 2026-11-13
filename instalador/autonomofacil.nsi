; Instalador NSIS de AutónomoFACIL, con asistente e imágenes propias.
;
;   bash empaquetar-nsis.sh
;
; o directamente:
;
;   cd instalador
;   makensis autonomofacil.nsi
;
; Los recursos (icono.ico, asistente.bmp, cabecera.bmp) los genera
; `imagenes.py`; `empaquetar-nsis.sh` lo hace antes de llamar a makensis.
;
; Decisiones:
;  · RequestExecutionLevel user -> sin administrador. Instala en
;    %LOCALAPPDATA%\Programs y registra el desinstalador en HKCU.
;  · Unicode true: el .nsi va en UTF-8 y los acentos de "AutónomoFACIL" salen bien.
;  · La página de aviso (aviso.txt) deja claro qué NO hace el programa, porque
;    un instalador que promete cumplimiento normativo y no lo tiene es peor que
;    uno feo.

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"

!define APP_NAME    "AutónomoFACIL"
!define APP_EXE     "autonomofacil.exe"
; La versión llega desde `empaquetar-nsis.sh` (que la lee de src/version.rs).
; Si se compila a mano, aquí queda el valor por defecto.
!ifndef APP_VERSION
!define APP_VERSION "0.1.0.1"
!endif
; Windows quiere cuatro números en la versión del fichero.
!ifndef APP_VERSION4
!define APP_VERSION4 "0.1.0.1"
!endif
!define APP_KEY     "AutonomoFACIL"
!define ORIGEN      "..\dist"
!define ARTE        "."
!define CLAVE       "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_KEY}"

Name "${APP_NAME}"
OutFile "${ORIGEN}\AutonomoFACIL-NSIS.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_KEY}"
InstallDirRegKey HKCU "Software\${APP_KEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
ShowInstDetails show
ShowUninstDetails show
BrandingText "${APP_NAME} ${APP_VERSION}  ·  instalación por usuario"

; Página propia del desinstalador: por defecto conserva los datos y sólo los
; borra si el usuario marca la casilla expresamente.
Var BorrarDatosCheck
Var BorrarDatos

; Propiedades del fichero
VIProductVersion "${APP_VERSION4}"
VIAddVersionKey "ProductName"     "${APP_NAME}"
VIAddVersionKey "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey "FileVersion"     "${APP_VERSION}"
VIAddVersionKey "FileDescription" "Instalador de ${APP_NAME}: presupuestos y facturas"
VIAddVersionKey "LegalCopyright"  ""

; ---- Arte ------------------------------------------------------------------
!define MUI_ICON                          "${ARTE}\icono.ico"
!define MUI_UNICON                        "${ARTE}\icono.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP      "${ARTE}\asistente.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP    "${ARTE}\asistente.bmp"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP            "${ARTE}\cabecera.bmp"
!define MUI_HEADERIMAGE_UNBITMAP          "${ARTE}\cabecera.bmp"
!define MUI_ABORTWARNING
!define MUI_ABORTWARNING_TEXT             "¿Seguro que quieres cancelar la instalación de ${APP_NAME}?"

; ---- Textos de las páginas ------------------------------------------------
!define MUI_WELCOMEPAGE_TITLE "Instalar ${APP_NAME} ${APP_VERSION}"
!define MUI_WELCOMEPAGE_TEXT  "Presupuestos y facturas para España, con registro \
Verifactu encadenado, código QR y exportación FacturaE 3.2.2.$\r$\n$\r$\n\
Se instala solo para tu usuario: no hace falta administrador y no se toca \
ningún archivo del sistema.$\r$\n$\r$\nPulsa Siguiente para continuar."

!define MUI_LICENSEPAGE_TEXT_TOP  "Lee esto antes de continuar: es lo que el programa hace y, sobre todo, lo que no hace."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Si necesitas un sistema conforme al RD 1007/2023, este programa todavía no lo es por sí solo."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Elige dónde instalar ${APP_NAME}. Por defecto va en tu carpeta de usuario, sin permisos especiales."

!define MUI_FINISHPAGE_TITLE       "${APP_NAME} ya está instalado"
!define MUI_FINISHPAGE_TEXT        "Puedes abrirlo desde el menú Inicio o el acceso directo del escritorio.$\r$\n$\r$\n\
Tus datos se guardan en:%$\r$\n%APPDATA%\AutonomoFACIL$\r$\n$\r$\n\
Al desinstalar podrás decidir si quieres borrarlos también."
!define MUI_FINISHPAGE_RUN         "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT    "Abrir ${APP_NAME}"
!define MUI_FINISHPAGE_NOREBOOTSUPPORT

; ---- Páginas ---------------------------------------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${ARTE}\aviso.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.PaginaDatos un.PaginaDatosSalir
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Spanish"

; ---- Instalación -----------------------------------------------------------
Section "Instalar ${APP_NAME}" SecPrincipal
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "${ORIGEN}\${APP_EXE}"

  ; El ejecutable de Rust no lleva icono incrustado, así que se instala el .ico
  ; al lado y los accesos directos apuntan a él: sin esto saldría el genérico.
  File "${ARTE}\icono.ico"

  WriteUninstaller "$INSTDIR\Desinstalar.exe"

  ; Accesos directos
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\icono.ico" 0
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Desinstalar ${APP_NAME}.lnk" "$INSTDIR\Desinstalar.exe" "" "$INSTDIR\icono.ico" 0
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\icono.ico" 0

  ; Agregar o quitar programas (HKCU)
  WriteRegStr HKCU "${CLAVE}" "DisplayName"     "${APP_NAME}"
  WriteRegStr HKCU "${CLAVE}" "DisplayVersion"  "${APP_VERSION}"
  WriteRegStr HKCU "${CLAVE}" "Publisher"       "${APP_NAME}"
  WriteRegStr HKCU "${CLAVE}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${CLAVE}" "DisplayIcon"     "$INSTDIR\icono.ico"
  WriteRegStr HKCU "${CLAVE}" "UninstallString" '"$INSTDIR\Desinstalar.exe"'
  WriteRegStr HKCU "${CLAVE}" "QuietUninstallString" '"$INSTDIR\Desinstalar.exe" /S'
  WriteRegDWORD HKCU "${CLAVE}" "NoModify" 1
  WriteRegDWORD HKCU "${CLAVE}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${CLAVE}" "EstimatedSize" $0

  WriteRegStr HKCU "Software\${APP_KEY}" "InstallDir" "$INSTDIR"
SectionEnd

; ---- Página de datos del desinstalador -------------------------------------
Function un.PaginaDatos
  !insertmacro MUI_HEADER_TEXT "Datos y exportaciones" "Decide qué hacer con tu información antes de desinstalar."

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 46u "La aplicación guarda tus datos, exportaciones, nóminas y copias de seguridad en:%$\r$\n$APPDATA\AutonomoFACIL%$\r$\nSi vienes de una versión anterior, también puede existir $APPDATA\Facturacion."
  Pop $0

  ${NSD_CreateCheckbox} 0 54u 100% 20u "Borrar también todos mis datos, exportaciones y copias de seguridad"
  Pop $BorrarDatosCheck
  ${NSD_SetState} $BorrarDatosCheck ${BST_UNCHECKED}

  ${NSD_CreateLabel} 0 84u 100% 58u "Déjalo sin marcar para conservar todo y poder recuperarlo si vuelves a instalar. Si lo marcas, se borran ambas carpetas de forma permanente y no se podrán recuperar."
  Pop $0

  nsDialogs::Show
FunctionEnd

Function un.PaginaDatosSalir
  ${NSD_GetState} $BorrarDatosCheck $BorrarDatos
FunctionEnd

; En una desinstalación silenciosa no se muestra la página; el valor queda a
; cero y los datos se conservan.
Function un.onInit
  StrCpy $BorrarDatos 0
FunctionEnd

; ---- Desinstalación --------------------------------------------------------
Section "Uninstall"
  SetShellVarContext current

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\icono.ico"
  Delete "$INSTDIR\Desinstalar.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Desinstalar ${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKCU "${CLAVE}"
  DeleteRegKey HKCU "Software\${APP_KEY}"

  ; Los datos son del usuario: sólo se borran si marcó la casilla. En una
  ; desinstalación silenciosa (/S) la página no se muestra y BorrarDatos queda
  ; a cero, así que también se conservan por defecto.
  ${If} $BorrarDatos == "1"
    DetailPrint "Borrando datos, exportaciones y copias de seguridad..."
    RMDir /r "$APPDATA\AutonomoFACIL"
    RMDir /r "$APPDATA\Facturacion"
    ${If} ${FileExists} "$APPDATA\AutonomoFACIL\*.*"
    ${OrIf} ${FileExists} "$APPDATA\Facturacion\*.*"
      MessageBox MB_ICONEXCLAMATION|MB_OK "No se pudieron borrar todos los datos. Cierra AutónomoFACIL y vuelve a intentarlo, o borra a mano: $APPDATA\AutonomoFACIL y $APPDATA\Facturacion"
    ${EndIf}
  ${EndIf}
SectionEnd
