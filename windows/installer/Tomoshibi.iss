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
; and passes Version, Numbers, Label, ArmDir, X64Dir and OutDir.
;
; Version is what the update check says and the deployment offers, the
; version alone for its first build and with the build as a fourth part
; after (0.1.4, 0.1.4.2: the core's version_with_build); Numbers is the four
; numbers of the version stamps, 0.1.4.1 for the first build too; Label is
; what people are told, 0.1.4 (2) (version_label).

#ifndef Version
  #error Version is required, as in /DVersion=0.1.4.2
#endif
#ifndef Numbers
  #define Numbers Version
#endif
#ifndef Label
  #define Label Version
#endif

[Setup]
; Fixed for ever: it is how a later setup finds this one to replace.
AppId={{3B8E0E54-6C1E-4C2D-9A51-6E0C2D7F1A35}
AppName=Tomoshibi
AppVersion={#Version}
AppVerName=Tomoshibi {#Label}
AppPublisher=Tomoshibi
; What the app reads to tell its own setup from any other file it might have
; been handed (SetupInstaller): the product's name, and its version as text,
; which must be the version the deployment offered exactly (0.1.4, 0.1.4.2),
; or every app out there refuses the setup as not the update it was offered.
VersionInfoVersion={#Numbers}
VersionInfoProductName=Tomoshibi
VersionInfoProductVersion={#Numbers}
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
//
// The bar is drawn at the apps' pace (Tomoshibi.Basics/Pace.cs), not where
// the install says it is: set from each report, it went across in leaps as
// each large file landed. What is drawn moves towards what the install has
// reached at one speed, the whole bar in no less than PaceWhole, never past
// it, and is drawn full before the window goes.
const
  PaceWhole = 1200;
  // A tick late after a stall moves the bar no further than this many
  // milliseconds would.
  PaceLongestTick = 50;
  // A tick about every frame.
  PaceEvery = 16;

var
  Shown: TSetupForm;
  Bar: TNewProgressBar;
  // The bar as drawn and as far as the install has got, both in milliseconds
  // of the bar's whole (its Max is PaceWhole), so that each millisecond of a
  // tick moves it one step.
  Drawn, Reached: Integer;
  LastMove: Int64;
  Ticker: Longword;
  Ticks: Integer;

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
function SetTimer(Window, Id, Every, Callback: Longword): Longword;
  external 'SetTimer@user32.dll stdcall';
function KillTimer(Window, Id: Longword): BOOL;
  external 'KillTimer@user32.dll stdcall';
function GetTickCount64: Int64;
  external 'GetTickCount64@kernel32.dll stdcall';

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
// rather than in blocks. The themed control would also ease every position
// it is given over a moment of its own, behind the pace below.
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

// Moves the bar drawn towards where the install has got, by the time since
// the last move (at most PaceLongestTick of it) and never past it; what has
// got back is gone to at once. Drawn again only when it has moved, and the
// bar alone: drawn with every call, the whole window flickered, its picture
// blinking out.
procedure Pace;
var
  Now, Since: Int64;
  Was: Integer;
begin
  Now := GetTickCount64;
  Since := Now - LastMove;
  LastMove := Now;
  if Since > PaceLongestTick then
    Since := PaceLongestTick;
  Was := Drawn;
  if Drawn < Reached then
  begin
    Drawn := Drawn + Since;
    if Drawn > Reached then
      Drawn := Reached;
  end
  else
    Drawn := Reached;
  if Drawn = Was then
    Exit;
  Bar.Position := Drawn;
  Bar.Update;
end;

// The timer's tick, about every frame, whenever the setup takes its messages.
procedure Tick(Window, Message, Id, Time: Longword);
begin
  Ticks := Ticks + 1;
  if Bar <> nil then
    Pace;
end;

procedure StopTicking;
begin
  if Ticker <> 0 then
    KillTimer(0, Ticker);
  Ticker := 0;
end;

procedure ShowUpdating;
var
  Logo: TBitmapImage;
  Title, Detail: TNewStaticText;
begin
  // Sized in the setup's own unscaled units: CreateCustomForm scales the
  // client size by the screen's DPI itself, and without KeepSizeX and
  // KeepSizeY stretches it by the wizard's size percent as well (120 for the
  // modern style). Handed ScaleX(420) and ScaleY(116), the window was scaled
  // twice and stretched, 2.56 times what its contents filled at 200 per cent
  // (2293 by 632 pixels around a picture, a title and a bar laid out for 895
  // by 247), and a fifth too wide even at 100. The contents are placed with
  // ScaleX and ScaleY, which is the scale the form is made at.
  Shown := CreateCustomForm(420, 116, True, True);
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
  Title.Caption := FmtMessage(CustomMessage('UpdatingTitle'), ['{#Label}']);
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
  Bar.Max := PaceWhole;
  Bar.Position := 0;
  Tint(Bar.Handle);

  DarkTitle(Shown.Handle);
  // Faded in and, in DeinitializeSetup, out (AW_BLEND), as everything the apps
  // show comes and goes.
  AnimateWindow(Shown.Handle, 200, $00080000);
  Shown.Show;
  Shown.Refresh;

  LastMove := GetTickCount64;
  Ticker := SetTimer(0, 0, PaceEvery, CreateCallback(@Tick));
end;

// The files in place: the bar is let finish at its pace before the app is
// opened again and the window goes, rather than cut from wherever it was.
// The ticks are not waited for here, since nothing takes the setup's
// messages until this returns; each turn moves it by its own time.
procedure FillBar;
var
  From: Int64;
  Behind: Integer;
begin
  Reached := PaceWhole;
  Behind := Reached - Drawn;
  From := GetTickCount64;
  while Drawn < Reached do
  begin
    Sleep(PaceEvery);
    Pace;
  end;
  StopTicking;
  Log(Format('Update window: the bar was %d ms behind and drawn full in %d ms, after %d ticks.', [Behind, Integer(GetTickCount64 - From), Ticks]));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssInstall) and VerySilent then
    ShowUpdating;
  if (CurStep = ssPostInstall) and (Shown <> nil) then
    FillBar;
end;

// Where the install has got; the bar is moved towards it here as well as by
// the ticks, by the time since its last move either way.
procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer);
var
  Part: Extended;
begin
  if (Shown = nil) or (MaxProgress <= 0) then
    Exit;
  Part := CurProgress;
  Part := Part * PaceWhole / MaxProgress;
  Reached := Round(Part);
  if Reached > PaceWhole then
    Reached := PaceWhole;
  Pace;
end;

procedure DeinitializeSetup;
begin
  StopTicking;
  // The app has been opened again by [Run] by now; the window goes with the setup.
  if Shown <> nil then
  begin
    AnimateWindow(Shown.Handle, 200, $00090000);
    Shown.Free;
  end;
end;
