#ifndef AppVersion
  #error AppVersion must be provided by the packaging script.
#endif
#ifndef PayloadDir
  #error PayloadDir must be provided by the packaging script.
#endif

[Setup]
AppId={{8BCB69DA-91AA-49AF-A3D6-38796A4768A5}
AppName=Rovar
AppVersion={#AppVersion}
AppPublisher=cradiy
DefaultDirName={localappdata}\Programs\Rovar
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
UninstallDisplayIcon={app}\rovar.exe
SetupIconFile=..\..\assets\rovar-icon.ico
LicenseFile={#PayloadDir}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Files]
Source: "{#PayloadDir}\rovar.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Rovar"; Filename: "{app}\rovar.exe"

[Run]
Filename: "{app}\rovar.exe"; Description: "Launch Rovar"; Flags: nowait postinstall skipifsilent
