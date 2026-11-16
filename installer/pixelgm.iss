; The installer of the Windows build, made by Inno Setup on the build runner:
;   ISCC /DAppVersion=<version> /DSourceDir=<folder with the game> installer\pixelgm.iss
; It installs for the current user, so it asks for no administrator's leave, and puts
; Steam's library beside the game: without it playing together through Steam is not there.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist"
#endif

[Setup]
AppId={{6C0E3C6B-5B0B-4C4E-9E55-7A1F3B0D9A21}
AppName=PixelGM
AppVersion={#AppVersion}
AppPublisher=PixelGM
DefaultDirName={localappdata}\Programs\PixelGM
DefaultGroupName=PixelGM
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..
OutputBaseFilename=PixelGM-setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\PixelGM.exe
; The game keeps saves and settings in the user's own folder; an update leaves them be.
CloseApplications=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceDir}\PixelGM.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\steam_api64.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\PixelGM"; Filename: "{app}\PixelGM.exe"
Name: "{group}\{cm:UninstallProgram,PixelGM}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\PixelGM"; Filename: "{app}\PixelGM.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\PixelGM.exe"; Description: "{cm:LaunchProgram,PixelGM}"; Flags: nowait postinstall skipifsilent
