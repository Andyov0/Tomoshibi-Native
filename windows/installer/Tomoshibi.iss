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
  #error Version is required, as in /DVersion=0.1.2
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
; The picture in the window an update shows while it installs ([Code]).
Source: "UpdateLogo.bmp"; Flags: dontcopy
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

[CustomMessages]
en.UpdatingTitle=Updating Tomoshibi to %1
zhcn.UpdatingTitle=正在更新 Tomoshibi %1
zhtw.UpdatingTitle=正在更新 Tomoshibi %1
ja.UpdatingTitle=Tomoshibi %1 にアップデートしています
en.UpdatingDetail=It opens again by itself when it is done.
zhcn.UpdatingDetail=装好后会自动重新打开。
zhtw.UpdatingDetail=裝好後會自動重新開啟。
ja.UpdatingDetail=終わると自動でもう一度開きます。

[Run]
; Offered after an install by hand, and done after a silent one, which is how
; an update brings the app back.
Filename: "{app}\Tomoshibi.exe"; Description: "{cm:LaunchProgram,Tomoshibi}"; Flags: nowait postinstall

[Code]
// An update runs this setup with /VERYSILENT once the app has closed, and for
// the ten seconds or so it takes nothing was on the screen at all: the app
// was gone and came back. So a silent run shows a small window of its own, the
// app's picture with what is happening and a bar that follows the files as
// they are put in place, until the app has been opened again. A setup run by
// hand shows its own wizard, progress and all, and none of this.
var
  Shown: TSetupForm;
  Bar: TNewProgressBar;

function DwmSetWindowAttribute(Window: HWND; Attribute: Integer; var Value: Integer; Size: Integer): Integer;
  external 'DwmSetWindowAttribute@dwmapi.dll stdcall delayload';
function SetWindowTheme(Window: HWND; App, IdList: String): Integer;
  external 'SetWindowTheme@uxtheme.dll stdcall delayload';
function AnimateWindow(Window: HWND; Time: Longint; Flags: Longint): BOOL;
  external 'AnimateWindow@user32.dll stdcall';
function GetWindowLong(Window: HWND; Index: Integer): Longint;
  external 'GetWindowLongW@user32.dll stdcall';
function SetWindowLong(Window: HWND; Index: Integer; Value: Longint): Longint;
  external 'SetWindowLongW@user32.dll stdcall';

// The title bar dark, as the apps' are (DWMWA_USE_IMMERSIVE_DARK_MODE: 20,
// and 19 on Windows 10 before 20H1, where 20 is refused).
procedure DarkTitle(Window: HWND);
var
  On: Integer;
begin
  On := 1;
  try
    if DwmSetWindowAttribute(Window, 20, On, 4) <> 0 then
      DwmSetWindowAttribute(Window, 19, On, 4);
  except
  end;
end;

// The bar in the apps' tally on their raised grey rather than the system's
// green: drawn by the classic control, which takes colours, smooth (PBS_SMOOTH)
// rather than in blocks.
procedure Tint(Window: HWND);
begin
  try
    SetWindowTheme(Window, ' ', ' ');
  except
  end;
  SetWindowLong(Window, -16, GetWindowLong(Window, -16) or 1);
  SendMessage(Window, $0409, 0, $003BA2FF);
  SendMessage(Window, $2001, 0, $002E2C2C);
end;

function VerySilent: Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/VERYSILENT') = 0 then
      Result := True;
end;

procedure ShowUpdating;
var
  Logo: TBitmapImage;
  Title, Detail: TNewStaticText;
begin
  Shown := CreateCustomForm(ScaleX(420), ScaleY(116), False, False);
  Shown.Caption := 'Tomoshibi';
  Shown.BorderStyle := bsDialog;
  Shown.BorderIcons := [];
  Shown.Position := poScreenCenter;
  Shown.FormStyle := fsStayOnTop;
  // The apps' ground, the same dark as the picture's own, so that it sits in
  // the window rather than on it.
  Shown.Color := $000D0E0F;

  ExtractTemporaryFile('UpdateLogo.bmp');
  Logo := TBitmapImage.Create(Shown);
  Logo.Parent := Shown;
  Logo.Bitmap.LoadFromFile(ExpandConstant('{tmp}\UpdateLogo.bmp'));
  Logo.Stretch := True;
  Logo.SetBounds(ScaleX(20), ScaleY(26), ScaleX(64), ScaleY(64));

  Title := TNewStaticText.Create(Shown);
  Title.Parent := Shown;
  Title.Caption := FmtMessage(CustomMessage('UpdatingTitle'), ['{#Version}']);
  Title.Font.Size := 11;
  Title.Font.Style := [fsBold];
  Title.Font.Color := $00F7F5F5;
  Title.SetBounds(ScaleX(100), ScaleY(26), ScaleX(300), ScaleY(22));

  Detail := TNewStaticText.Create(Shown);
  Detail.Parent := Shown;
  Detail.Caption := CustomMessage('UpdatingDetail');
  Detail.Font.Color := $00A09A98;
  Detail.SetBounds(ScaleX(100), ScaleY(50), ScaleX(300), ScaleY(18));

  Bar := TNewProgressBar.Create(Shown);
  Bar.Parent := Shown;
  Bar.SetBounds(ScaleX(100), ScaleY(76), ScaleX(296), ScaleY(10));
  Bar.Min := 0;
  Bar.Max := 100;
  Bar.Position := 0;
  Tint(Bar.Handle);

  DarkTitle(Shown.Handle);
  // Faded in and, in DeinitializeSetup, out (AW_BLEND), as everything the apps
  // show comes and goes.
  AnimateWindow(Shown.Handle, 200, $00080000);
  Shown.Show;
  Shown.Refresh;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssInstall) and VerySilent then
    ShowUpdating;
  if (CurStep = ssPostInstall) and (Shown <> nil) then
  begin
    Bar.Position := Bar.Max;
    Bar.Update;
  end;
end;

// The bar alone, and only when it has moved by a hundredth: drawn again with
// every call, the whole window flickered, its picture blinking out.
procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer);
var
  Part: Integer;
begin
  if (Shown = nil) or (MaxProgress <= 0) then
    Exit;
  Part := (CurProgress * 100) div MaxProgress;
  if Part = Bar.Position then
    Exit;
  Bar.Position := Part;
  Bar.Update;
end;

procedure DeinitializeSetup;
begin
  // The app has been opened again by [Run] by now; the window goes with the setup.
  if Shown <> nil then
  begin
    AnimateWindow(Shown.Handle, 200, $00090000);
    Shown.Free;
  end;
end;
