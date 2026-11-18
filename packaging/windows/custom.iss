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
ShowLanguageDialog=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "indonesian"; MessagesFile: "languages\Indonesian.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "vietnamese"; MessagesFile: "languages\Vietnamese.isl"

[CustomMessages]
english.DesktopShortcut=Create a desktop shortcut
indonesian.DesktopShortcut=Buat pintasan desktop
korean.DesktopShortcut=바탕 화면 바로 가기 만들기
vietnamese.DesktopShortcut=Tạo lối tắt trên màn hình nền
english.LaunchApp=Launch SolidCraft Custom
indonesian.LaunchApp=Jalankan SolidCraft Custom
korean.LaunchApp=SolidCraft Custom 실행
vietnamese.LaunchApp=Khởi chạy SolidCraft Custom

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopShortcut}"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SolidCraft Custom"; Filename: "{app}\solidcraft.exe"
Name: "{autodesktop}\SolidCraft Custom"; Filename: "{app}\solidcraft.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\solidcraft.exe"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\installer-language.txt"

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var
  Locale: String;
begin
  if CurStep = ssPostInstall then
  begin
    Locale := 'en';
    if ActiveLanguage = 'korean' then Locale := 'ko';
    if ActiveLanguage = 'indonesian' then Locale := 'id';
    if ActiveLanguage = 'vietnamese' then Locale := 'vi';
    if not SaveStringToFile(ExpandConstant('{app}\installer-language.txt'), Locale, False) then
      RaiseException('Unable to save the installation language.');
  end;
end;
