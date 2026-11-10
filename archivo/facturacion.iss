; Instalador con asistente para Windows (Inno Setup 6).
;
; Es la alternativa "clásica" al instalador propio. Requiere Inno Setup:
;   winget install JRSoftware.InnoSetup
; y después:
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" instalador\facturacion.iss
;
; Genera dist\Facturacion-Wizard.exe. Usa PrivilegesRequired=lowest, así que se
; instala por usuario igual que el instalador propio (sin UAC).

#define AppName "Facturación"
#define AppVersion "0.1.0.1"
#define AppExe "facturacion.exe"
#define Origen "..\dist"

[Setup]
AppId={{6F1C2A54-9D3E-4B77-9E21-5C0A8B7D4E13}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Facturación
DefaultDirName={autopf}\Facturacion
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir={#Origen}
OutputBaseFilename=Facturacion-Wizard
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"

[Files]
Source: "{#Origen}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Desinstalar {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Abrir {#AppName}"; Flags: nowait postinstall skipifsilent
