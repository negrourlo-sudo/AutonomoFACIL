; Instalador NSIS de AutónomoFácil, con asistente e imágenes propias.
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
;  · Unicode true: el .nsi va en UTF-8 y los acentos de "AutónomoFácil" salen bien.
;  · La página de aviso (aviso.txt) deja claro qué NO hace el programa, porque
;    un instalador que promete cumplimiento normativo y no lo tiene es peor que
;    uno feo.

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"

!define APP_NAME    "AutónomoFácil"
!define APP_AUTHOR  "Francisco Nicolás"
!define APP_EXE     "autonomofacil.exe"
!define APP_PROC    "autonomofacil"
; La versión llega desde `empaquetar-nsis.sh` (que la lee de src/version.rs).
; Si se compila a mano, aquí queda el valor por defecto.
!ifndef APP_VERSION
!define APP_VERSION "0.7.8.0"
!endif
; Windows quiere cuatro números en la versión del fichero.
!ifndef APP_VERSION4
!define APP_VERSION4 "0.7.8.0"
!endif
!define APP_KEY     "AutonomoFacil"
!define ORIGEN      "..\dist"
!define ARTE        "."
!define CLAVE       "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_KEY}"

Name "${APP_NAME}"
OutFile "${ORIGEN}\AutonomoFacil-NSIS.exe"
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
Var CarpetaCopia
Var RollbackActivo
Var InstalacionPrevia
Var VersionPrevia

; Propiedades del fichero
VIProductVersion "${APP_VERSION4}"
VIAddVersionKey "ProductName"     "${APP_NAME}"
VIAddVersionKey "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey "FileVersion"     "${APP_VERSION}"
VIAddVersionKey "FileDescription" "Instalador de ${APP_NAME}: presupuestos y facturas"
VIAddVersionKey "CompanyName"     "${APP_AUTHOR}"
VIAddVersionKey "Author"          "${APP_AUTHOR}"
VIAddVersionKey "LegalCopyright"  "© ${APP_AUTHOR}"

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
; MUI crea .onUserAbort y un.onUserAbort; le pedimos que llame a nuestro
; rollback antes de cerrar. Si no hay una instalación a medias, no hace nada.
!define MUI_CUSTOMFUNCTION_ABORT         "RestaurarCopia"

; ---- Textos de las páginas ------------------------------------------------
!define MUI_WELCOMEPAGE_TITLE "Instalar ${APP_NAME} ${APP_VERSION}"
!define MUI_WELCOMEPAGE_TEXT  "Presupuestos y facturas para España, con registro \
Verifactu encadenado, código QR y exportación FacturaE 3.2.2.$\r$\n$\r$\n\
Se instala solo para tu usuario: no hace falta administrador y no se toca \
ningún archivo del sistema.$\r$\n$\r$\nPulsa Siguiente para continuar."

!define MUI_LICENSEPAGE_TEXT_TOP  "Lee el aviso legal y la licencia antes de continuar."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "AutónomoFácil © 2026 de Francisco Nicolás está bajo la licencia CC BY-SA 4.0. Para ver una copia de esta licencia, visite https://creativecommons.org/licenses/by-sa/4.0/ Si necesitas un sistema conforme al RD 1007/2023, este programa todavía no lo es por sí solo."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Elige dónde instalar ${APP_NAME}. Por defecto va en tu carpeta de usuario, sin permisos especiales."

!define MUI_FINISHPAGE_TITLE       "${APP_NAME} ya está instalado"
!define MUI_FINISHPAGE_TEXT        "Puedes abrirlo desde el menú Inicio o el acceso directo del escritorio.$\r$\n$\r$\n\
Tus datos se guardan en:%$\r$\n%APPDATA%\AutonomoFacil$\r$\n$\r$\n\
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

; ---- Comprobaciones y copia de seguridad -----------------------------------
; Se instala como el usuario actual. Antes de tocar nada se comprueba que la
; aplicación esté cerrada y se guarda una copia del programa anterior. Si algo
; falla o el usuario cancela a mitad, se restaura esa copia.
Function .onInit
  StrCpy $RollbackActivo "0"
  StrCpy $InstalacionPrevia "0"
  StrCpy $VersionPrevia ""
  StrCpy $CarpetaCopia "$TEMP\AutonomoFacil-backup-${APP_VERSION4}"
FunctionEnd

Function ComprobarAppCerrada
  loop:
    ; PowerShell viene con Windows y evita falsos positivos de la salida
    ; localizada de tasklist. 0 = hay una instancia; 1 = no la hay.
    nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -Command "if (Get-Process -Name ${APP_PROC} -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }"'
    Pop $0
    Pop $1
    StrCmp $0 "0" app_abierta
    StrCmp $0 "1" app_cerrada
    ; Si no se pudo comprobar, es más seguro abortar que sobrescribir a ciegas.
    IfSilent app_deteccion_fallida
    MessageBox MB_ICONSTOP|MB_OK "No se pudo comprobar si ${APP_NAME} está abierto. Cierra la aplicación y vuelve a ejecutar el instalador."
    Abort
  app_deteccion_fallida:
    SetErrorLevel 1
    Abort
  app_cerrada:
    Return
  app_abierta:
    IfSilent app_abierta_silencioso
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "${APP_NAME} está abierto. Cierra la aplicación y pulsa Reintentar. Si sigues sin cerrarla no se tocarán sus datos, pero la instalación no puede continuar mientras el ejecutable esté en uso." IDRETRY loop
    Abort
  app_abierta_silencioso:
    SetErrorLevel 1
    Abort
FunctionEnd

Function un.ComprobarAppCerrada
  un_loop:
    nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -Command "if (Get-Process -Name ${APP_PROC} -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }"'
    Pop $0
    Pop $1
    StrCmp $0 "0" un_app_abierta
    StrCmp $0 "1" un_app_cerrada
    IfSilent un_deteccion_fallida
    MessageBox MB_ICONSTOP|MB_OK "No se pudo comprobar si ${APP_NAME} está abierto. Cierra la aplicación y vuelve a ejecutar el desinstalador."
    Abort
  un_deteccion_fallida:
    SetErrorLevel 1
    Abort
  un_app_cerrada:
    Return
  un_app_abierta:
    IfSilent un_app_abierta_silencioso
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "${APP_NAME} está abierto. Cierra la aplicación y pulsa Reintentar. Si sigues sin cerrarla no se borrarán sus datos, pero la desinstalación no puede continuar mientras el ejecutable esté en uso." IDRETRY un_loop
    Abort
  un_app_abierta_silencioso:
    SetErrorLevel 1
    Abort
FunctionEnd

Function PrepararCopia
  StrCpy $RollbackActivo "0"
  StrCpy $InstalacionPrevia "0"
  StrCpy $VersionPrevia ""
  RMDir /r "$CarpetaCopia"
  IfFileExists "$INSTDIR\${APP_EXE}" 0 copia_sin_app
  StrCpy $InstalacionPrevia "1"
  ReadRegStr $VersionPrevia HKCU "${CLAVE}" "DisplayVersion"

  copia_sin_app:
  CreateDirectory "$CarpetaCopia"
  IfErrors copia_fallida
  ClearErrors
  ; Sólo se copian los tres ficheros que este instalador puede sobrescribir.
  ; Así el rollback no toca ningún otro fichero de la carpeta elegida.
  IfFileExists "$INSTDIR\${APP_EXE}" 0 copia_icono
  CopyFiles /SILENT "$INSTDIR\${APP_EXE}" "$CarpetaCopia\${APP_EXE}"
  IfErrors copia_fallida
  copia_icono:
  IfFileExists "$INSTDIR\icono.ico" 0 copia_desinstalador
  CopyFiles /SILENT "$INSTDIR\icono.ico" "$CarpetaCopia\icono.ico"
  IfErrors copia_fallida
  copia_desinstalador:
  IfFileExists "$INSTDIR\Desinstalar.exe" 0 copia_lista
  CopyFiles /SILENT "$INSTDIR\Desinstalar.exe" "$CarpetaCopia\Desinstalar.exe"
  IfErrors copia_fallida
  copia_lista:
  StrCpy $RollbackActivo "1"
  DetailPrint "Copia de seguridad preparada en $CarpetaCopia"
  Return

  copia_fallida:
  RMDir /r "$CarpetaCopia"
  MessageBox MB_ICONSTOP|MB_OK "No se pudo crear la copia de seguridad de la instalación actual. No se ha modificado nada. Revisa el espacio libre y los permisos de $TEMP."
  SetErrorLevel 1
  Abort
FunctionEnd

Function RestaurarCopia
  StrCmp $RollbackActivo "1" 0 restaurar_fin
  StrCpy $RollbackActivo "0"
  DetailPrint "Restaurando la instalación anterior..."
  ; No se borra la carpeta entera: el usuario puede haber elegido una ruta
  ; compartida. Sólo se deshacen los tres ficheros que instala esta aplicación.
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\icono.ico"
  Delete "$INSTDIR\Desinstalar.exe"

  ; Si alguno de esos nombres ya existía (por ejemplo, en una carpeta
  ; compartida), se restauran los ficheros originales desde la copia.
  IfFileExists "$CarpetaCopia\*.*" 0 restaurar_registro
  CreateDirectory "$INSTDIR"
  IfErrors restaurar_error
  ClearErrors
  IfFileExists "$CarpetaCopia\${APP_EXE}" 0 restaurar_icono
  CopyFiles /SILENT "$CarpetaCopia\${APP_EXE}" "$INSTDIR\${APP_EXE}"
  IfErrors restaurar_error
  restaurar_icono:
  IfFileExists "$CarpetaCopia\icono.ico" 0 restaurar_desinstalador
  CopyFiles /SILENT "$CarpetaCopia\icono.ico" "$INSTDIR\icono.ico"
  IfErrors restaurar_error
  restaurar_desinstalador:
  IfFileExists "$CarpetaCopia\Desinstalar.exe" 0 restaurar_registro
  CopyFiles /SILENT "$CarpetaCopia\Desinstalar.exe" "$INSTDIR\Desinstalar.exe"
  IfErrors restaurar_error

  restaurar_registro:
  StrCmp $InstalacionPrevia "1" 0 restaurar_nueva
  StrCmp $VersionPrevia "" restaurar_limpiar
  WriteRegStr HKCU "${CLAVE}" "DisplayVersion" "$VersionPrevia"
  Goto restaurar_limpiar

  restaurar_nueva:
    ; La instalación era nueva: no debe quedar nada a medias, pero tampoco se
    ; tocan los datos de %APPDATA%.
    Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
    Delete "$SMPROGRAMS\${APP_NAME}\Desinstalar ${APP_NAME}.lnk"
    RMDir "$SMPROGRAMS\${APP_NAME}"
    Delete "$DESKTOP\${APP_NAME}.lnk"
    DeleteRegKey HKCU "${CLAVE}"
    DeleteRegKey HKCU "Software\${APP_KEY}"
    RMDir "$INSTDIR"

  restaurar_limpiar:
    RMDir /r "$CarpetaCopia"
    DetailPrint "Estado anterior restaurado."
    Return

  restaurar_error:
    MessageBox MB_ICONSTOP|MB_OK "No se pudo restaurar la copia anterior. La copia sigue en $CarpetaCopia para que puedas recuperarla a mano."
    Return

  restaurar_fin:
    Return
FunctionEnd

Function FalloInstalacion
  Call RestaurarCopia
  SetErrorLevel 1
  Abort
FunctionEnd

Function .onInstFailed
  Call RestaurarCopia
FunctionEnd

; ---- Instalación -----------------------------------------------------------
Section "Instalar ${APP_NAME}" SecPrincipal
  SetShellVarContext current
  Call ComprobarAppCerrada
  Call PrepararCopia
  ; Quita los accesos de la marca anterior, si existen.
  Delete "$SMPROGRAMS\AutónomoFACIL\AutónomoFACIL.lnk"
  Delete "$SMPROGRAMS\AutónomoFACIL\Desinstalar AutónomoFACIL.lnk"
  RMDir "$SMPROGRAMS\AutónomoFACIL"
  Delete "$DESKTOP\AutónomoFACIL.lnk"
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "${ORIGEN}\${APP_EXE}"
  IfErrors 0 +2
    Call FalloInstalacion

  ; El ejecutable lleva el icono incrustado por build.rs. Se conserva el .ico
  ; junto a la aplicación como respaldo para instalaciones antiguas y para el
  ; desinstalador.
  File "${ARTE}\icono.ico"
  IfErrors 0 +2
    Call FalloInstalacion

  WriteUninstaller "$INSTDIR\Desinstalar.exe"
  IfErrors 0 +2
    Call FalloInstalacion

  ; Accesos directos
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Desinstalar ${APP_NAME}.lnk" "$INSTDIR\Desinstalar.exe" "" "$INSTDIR\icono.ico" 0
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0

  ; Agregar o quitar programas (HKCU)
  WriteRegStr HKCU "${CLAVE}" "DisplayName"     "${APP_NAME}"
  WriteRegStr HKCU "${CLAVE}" "DisplayVersion"  "${APP_VERSION}"
  WriteRegStr HKCU "${CLAVE}" "Publisher"       "${APP_AUTHOR}"
  WriteRegStr HKCU "${CLAVE}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${CLAVE}" "DisplayIcon"     "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "${CLAVE}" "UninstallString" '"$INSTDIR\Desinstalar.exe"'
  WriteRegStr HKCU "${CLAVE}" "QuietUninstallString" '"$INSTDIR\Desinstalar.exe" /S'
  WriteRegDWORD HKCU "${CLAVE}" "NoModify" 1
  WriteRegDWORD HKCU "${CLAVE}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${CLAVE}" "EstimatedSize" $0

  WriteRegStr HKCU "Software\${APP_KEY}" "InstallDir" "$INSTDIR"

  ; Instalación completa: ya no hay nada que deshacer.
  StrCpy $RollbackActivo "0"
  RMDir /r "$CarpetaCopia"
SectionEnd

; ---- Página de datos del desinstalador -------------------------------------
Function un.PaginaDatos
  !insertmacro MUI_HEADER_TEXT "Datos y exportaciones" "Decide qué hacer con tu información antes de desinstalar."

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 46u "La aplicación guarda tus datos, exportaciones, nóminas y copias de seguridad en:%$\r$\n$APPDATA\AutonomoFacil%$\r$\nSi vienes de una versión anterior, también puede existir $APPDATA\Facturacion."
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
  Call un.ComprobarAppCerrada

  ; Quita también los accesos de la marca anterior.
  Delete "$SMPROGRAMS\AutónomoFACIL\AutónomoFACIL.lnk"
  Delete "$SMPROGRAMS\AutónomoFACIL\Desinstalar AutónomoFACIL.lnk"
  RMDir "$SMPROGRAMS\AutónomoFACIL"
  Delete "$DESKTOP\AutónomoFACIL.lnk"

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
    RMDir /r "$APPDATA\AutonomoFacil"
    RMDir /r "$APPDATA\Facturacion"
    ${If} ${FileExists} "$APPDATA\AutonomoFacil\*.*"
    ${OrIf} ${FileExists} "$APPDATA\Facturacion\*.*"
      MessageBox MB_ICONEXCLAMATION|MB_OK "No se pudieron borrar todos los datos. Cierra AutónomoFácil y vuelve a intentarlo, o borra a mano: $APPDATA\AutonomoFacil y $APPDATA\Facturacion"
    ${EndIf}
  ${EndIf}
SectionEnd
