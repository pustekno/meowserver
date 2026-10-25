; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
; Additional terms under GPLv3 section 7(b) apply; see NOTICE.
;
; Windows installer (PRD §15), built by `cargo xtask dist` with Inno Setup 6.
; One installer does both jobs:
;
; - First install: per user, no administrator rights, default C:\MeowServer.
;   MeowServer keeps its data next to the program (PRD §12), so the folder
;   must be writable and Program Files is refused.
; - Update: run a newer installer. It finds the previous folder (same
;   AppId), quits MeowServer there, and replaces only the files listed in
;   [Files]. www\, data\, bin\, etc\, ssl\, logs\, and meow.toml are never
;   installed by Setup, so it never overwrites or removes them. A portable
;   folder can be updated the same way by choosing it on first install.
;
; Uninstall removes the program files and undoes system changes (F36), and
; leaves the user's projects and databases where they are.

#ifndef Source
  #error Pass /DSource=<dist folder>; use `cargo xtask dist`.
#endif
#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef NumericVersion
  #define NumericVersion Version
#endif
#ifndef Output
  #define Output "."
#endif
#ifndef BaseName
  #define BaseName "MeowServer-" + Version + "-windows-x86_64-setup"
#endif

[Setup]
; Never change AppId: it is how a newer installer recognises an install.
AppId={{51989D7A-89F6-40F1-A171-F4238A2B5550}
AppName=MeowServer
AppVersion={#Version}
AppVerName=MeowServer {#Version}
AppPublisher=Pustekno and Puskomedia
AppPublisherURL=https://pustekno.id
AppSupportURL=https://github.com/pustekno/meowserver/issues
AppUpdatesURL=https://github.com/pustekno/meowserver/releases
AppCopyright=Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
VersionInfoVersion={#NumericVersion}
DefaultDirName={sd}\MeowServer
DirExistsWarning=no
DisableDirPage=auto
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ChangesEnvironment=yes
CloseApplications=yes
RestartApplications=no
ShowLanguageDialog=no
WizardStyle=modern
SetupIconFile={#Source}\meowserver.ico
UninstallDisplayIcon={app}\meowserver.ico
UninstallDisplayName=MeowServer
OutputDir={#Output}
OutputBaseFilename={#BaseName}
Compression=lzma2/ultra64
SolidCompression=yes
SetupLogging=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
; Unofficial translation from the Inno Setup repository, unmodified.
Name: "id"; MessagesFile: "Indonesian.isl"

[CustomMessages]
en.AddToPath=Add the meow command to PATH (use it from any terminal)
id.AddToPath=Tambahkan perintah meow ke PATH (bisa dipakai dari terminal mana pun)
en.KeepsData=MeowServer is already in this folder and will be updated. Your projects (www), databases (data), settings (meow.toml, etc), and downloaded versions (bin) are kept.
id.KeepsData=MeowServer sudah ada di folder ini dan akan diperbarui. Proyek (www), database (data), pengaturan (meow.toml, etc), dan versi yang sudah diunduh (bin) tetap disimpan.
en.NotProgramFiles=MeowServer keeps your databases and downloads in its own folder, so it cannot be installed in Program Files. Choose a folder such as C:\MeowServer.
id.NotProgramFiles=MeowServer menyimpan database dan unduhan di foldernya sendiri, jadi tidak bisa dipasang di Program Files. Pilih folder seperti C:\MeowServer.
en.KeptData=Your projects, databases, and downloaded versions were kept in:%n%1%n%nDelete that folder yourself if you no longer need them.
id.KeptData=Proyek, database, dan versi yang sudah diunduh tetap disimpan di:%n%1%n%nHapus folder itu sendiri jika sudah tidak diperlukan.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "path"; Description: "{cm:AddToPath}"

[Files]
; Top-level files only: the three programs, the icon, and the legal and
; read-me files. Nothing the user creates lives at this level except
; meow.toml, which `cargo xtask dist` never ships.
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\www\README.txt"; DestDir: "{app}\www"; Flags: onlyifdoesntexist uninsneveruninstall

[Dirs]
Name: "{app}\www"; Flags: uninsneveruninstall

[Icons]
Name: "{autoprograms}\MeowServer"; Filename: "{app}\meowserver.exe"; IconFilename: "{app}\meowserver.ico"
Name: "{autodesktop}\MeowServer"; Filename: "{app}\meowserver.exe"; IconFilename: "{app}\meowserver.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\meowserver.exe"; Description: "{cm:LaunchProgram,MeowServer}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\meow.exe"; Parameters: "quit"; Flags: runhidden waituntilterminated; RunOnceId: "quit"
Filename: "{app}\meow.exe"; Parameters: "undo-system-changes"; Flags: runhidden waituntilterminated; RunOnceId: "undo"

[Code]
const
  EnvKey = 'Environment';

function SameDir(A, B: String): Boolean;
begin
  Result := CompareText(RemoveBackslashUnlessRoot(A), RemoveBackslashUnlessRoot(B)) = 0;
end;

// `Path` without any entry naming `Dir`; empty entries are dropped too.
function PathWithout(Path, Dir: String): String;
var
  Rest, Part: String;
  P: Integer;
begin
  Result := '';
  Rest := Path;
  while Rest <> '' do
  begin
    P := Pos(';', Rest);
    if P = 0 then
    begin
      Part := Rest;
      Rest := '';
    end else
    begin
      Part := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end;
    if (Trim(Part) <> '') and not SameDir(Trim(Part), Dir) then
    begin
      if Result <> '' then
        Result := Result + ';';
      Result := Result + Part;
    end;
  end;
end;

// Adds or removes `Dir` in the user's PATH; writes only when it changes.
procedure SetOnPath(Dir: String; Wanted: Boolean);
var
  Path, Updated: String;
begin
  if not RegQueryStringValue(HKCU, EnvKey, 'Path', Path) then
    Path := '';
  Updated := PathWithout(Path, Dir);
  if Wanted then
  begin
    if Updated <> '' then
      Updated := Updated + ';';
    Updated := Updated + Dir;
  end;
  if Updated <> Path then
    RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Updated);
end;

function InFolder(Dir, Parent: String): Boolean;
begin
  Result := Pos(Uppercase(AddBackslash(Parent)), Uppercase(AddBackslash(Dir))) = 1;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectDir) and
     (InFolder(WizardDirValue, ExpandConstant('{commonpf64}')) or
      InFolder(WizardDirValue, ExpandConstant('{commonpf32}'))) then
  begin
    MsgBox(CustomMessage('NotProgramFiles'), mbError, MB_OK);
    Result := False;
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if MemoTasksInfo <> '' then
    Result := Result + NewLine + NewLine + MemoTasksInfo;
  if FileExists(AddBackslash(WizardDirValue) + 'meowserver.exe') then
    Result := Result + NewLine + NewLine + CustomMessage('KeepsData');
end;

// Before files are replaced: stop services, `meowd`, and this folder's
// window. Anything still holding a file is then closed by the Restart
// Manager (CloseApplications).
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Meow: String;
  Code: Integer;
begin
  Result := '';
  Meow := ExpandConstant('{app}\meow.exe');
  if FileExists(Meow) then
    Exec(Meow, 'quit', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    SetOnPath(ExpandConstant('{app}'), WizardIsTaskSelected('path'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    SetOnPath(ExpandConstant('{app}'), False);
    if DirExists(ExpandConstant('{app}')) then
      SuppressibleMsgBox(FmtMessage(CustomMessage('KeptData'), [ExpandConstant('{app}')]),
        mbInformation, MB_OK, IDOK);
  end;
end;
