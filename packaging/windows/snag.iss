; Inno Setup script for the Snag Windows installer.
;
; Build with:
;   iscc /DAppVersion=0.2.0 packaging\windows\snag.iss
;
; It expects snag.exe at packaging\windows\snag.exe (the release workflow copies
; it there) and writes the installer into dist\.
;
; Note what is deliberately NOT here: yt-dlp and ffmpeg. Snag offers to fetch
; yt-dlp from its own project on first run, and ffmpeg is the user's to install,
; so bundling either would ship something stale and unasked for.

#define AppName "Snag"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define AppExe "snag.exe"
#define AppPublisher "EmreO33"
#define AppURL "https://github.com/EmreO33/snag"

[Setup]
AppId={{CF8CC397-F68D-4940-8897-10A39126914A}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
VersionInfoVersion={#AppVersion}

DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=no
AllowNoIcons=yes

; Let the user decide between an all-users install and a per-user one, rather
; than demanding a UAC prompt for a single small executable.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

OutputDir=..\..\dist
OutputBaseFilename=Snag-{#AppVersion}-windows-setup
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName} {#AppVersion}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "snag.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Snag writes nothing into its install folder, but a user who ran the installed
; copy in portable mode would have. Clean that up rather than leaving a stray
; folder behind.
Type: filesandordirs; Name: "{app}\data"

[Messages]
BeveledLabel={#AppName} {#AppVersion}
