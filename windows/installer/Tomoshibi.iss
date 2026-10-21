; The Windows app as it is given out: one setup program for an ARM machine and
; an Intel one alike, installing for the person in their own folder.
;
; Not an MSIX. The owner chose not to buy a code-signing certificate
; (2026-10-20), and Windows refuses outright to install a package whose
; signature it does not trust, where an ordinary program it has not seen is
; only stopped at SmartScreen, which the person may let through ("More info",
; then "Run anyway"). So this setup is unsigned, and is let through once; the
; app's own updates run it again with no mark of the web, and SmartScreen does
; not ask (Model/AppUpdates.cs, SetupInstaller).
;
; Built by windows\setup.ps1, which publishes the app for each architecture
; and passes Version, ArmDir, X64Dir and OutDir.

#ifndef Version
  #error Version is required, as in /DVersion=0.1.0.2
#endif

[Setup]
; Fixed for ever: it is how a later setup finds this one to replace.
AppId={{3B8E0E54-6C1E-4C2D-9A51-6E0C2D7F1A35}
AppName=Tomoshibi
AppVersion={#Version}
AppVerName=Tomoshibi {#Version}
AppPublisher=Tomoshibi
; What the app reads to tell its own setup from any other file it might have
; been handed (SetupInstaller).
VersionInfoVersion={#Version}
VersionInfoProductName=Tomoshibi
VersionInfoProductVersion={#Version}
VersionInfoProductTextVersion={#Version}
VersionInfoDescription=Tomoshibi Setup
; The person's own folder, so no administrator is asked for and nobody else
; on the machine is affected; the app's data is beside it, in
; %LOCALAPPDATA%\Tomoshibi, and outlives an uninstall.
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\Tomoshibi
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible or arm64
ArchitecturesInstallIn64BitMode=x64compatible or arm64
; What the app's manifest asks for (Windows 10 2004).
MinVersion=10.0.19041
OutputDir={#OutDir}
OutputBaseFilename=Tomoshibi-Setup-{#Version}
SetupIconFile=..\Tomoshibi\Assets\Tomoshibi.ico
UninstallDisplayIcon={app}\Tomoshibi.exe
UninstallDisplayName=Tomoshibi
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; An app still open is closed rather than left holding its files, and is
; opened again by [Run] rather than by the restart manager, so it opens once.
CloseApplications=force
RestartApplications=no
ShowLanguageDialog=auto

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
; The two Chinese translations are not shipped with Inno Setup; they are its
; own repository's unofficial ones, from the same release (is-6_7_3).
Name: "zhcn"; MessagesFile: "Languages\ChineseSimplified.isl"
Name: "zhtw"; MessagesFile: "Languages\ChineseTraditional.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[InstallDelete]
; The last version's files go before this one's arrive: a library that is no
; longer shipped would otherwise stay beside the new ones and be loaded.
Type: filesandordirs; Name: "{app}\*"

[Files]
Source: "{#ArmDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Check: IsArm64
Source: "{#X64Dir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Check: not IsArm64

[Icons]
Name: "{autoprograms}\Tomoshibi"; Filename: "{app}\Tomoshibi.exe"
Name: "{autodesktop}\Tomoshibi"; Filename: "{app}\Tomoshibi.exe"; Tasks: desktopicon

[Registry]
; tomoshibi:// links, which the invitation page opens the app with; a package
; registered these in its manifest, a program does it here, for this person.
Root: HKCU; Subkey: "Software\Classes\tomoshibi"; ValueType: string; ValueData: "URL:Tomoshibi"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\tomoshibi"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\tomoshibi\DefaultIcon"; ValueType: string; ValueData: """{app}\Tomoshibi.exe"",0"
Root: HKCU; Subkey: "Software\Classes\tomoshibi\shell\open\command"; ValueType: string; ValueData: """{app}\Tomoshibi.exe"" ""%1"""

[Run]
; Offered after an install by hand, and done after a silent one, which is how
; an update brings the app back.
Filename: "{app}\Tomoshibi.exe"; Description: "{cm:LaunchProgram,Tomoshibi}"; Flags: nowait postinstall
