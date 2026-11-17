#ifndef BuildVersion
  #define BuildVersion "0.44.0"
#endif
#ifndef StageDir
  #error StageDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif

[Setup]
AppId={{A36BBBD3-17B2-4F64-A29C-F2971B7995DE}
AppName=SolidCraft Custom
AppVersion={#BuildVersion}
AppPublisher=GilwooKim / SolidCraft contributors
AppPublisherURL=https://github.com/GilwooKim/solidcraft-custom
DefaultDirName={localappdata}\Programs\SolidCraft Custom
DefaultGroupName=SolidCraft Custom
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=solidcraft-{#BuildVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\..\assets\icons\app\windows\exports\solidcraft.ico
UninstallDisplayIcon={app}\solidcraft.exe
CloseApplications=yes
RestartApplications=no
DisableProgramGroupPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SolidCraft Custom"; Filename: "{app}\solidcraft.exe"
Name: "{autodesktop}\SolidCraft Custom"; Filename: "{app}\solidcraft.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\solidcraft.exe"; Description: "Launch SolidCraft Custom"; Flags: nowait postinstall skipifsilent
