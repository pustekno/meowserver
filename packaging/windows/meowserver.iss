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
en.WebTitle=Web server
id.WebTitle=Web server
en.WebDesc=Which web server should serve your sites?
id.WebDesc=Web server mana yang melayani situs-situsmu?
en.WebSub=Caddy is the simplest choice. You can switch later in Settings.
id.WebSub=Caddy pilihan paling mudah. Bisa diganti nanti di Pengaturan.
en.Recommended=recommended
id.Recommended=disarankan
en.DbTitle=Database
id.DbTitle=Database
en.DbDesc=Which database server should MeowServer set up?
id.DbDesc=Database apa yang disiapkan MeowServer?
en.DbSub=MySQL 8.4 is what Laragon uses. Others can be added later on the Versions page.
id.DbSub=MySQL 8.4 sama dengan yang dipakai Laragon. Yang lain bisa ditambah nanti di halaman Versi.
en.LikeLaragon=like Laragon
id.LikeLaragon=seperti Laragon
en.NoDatabase=None for now
id.NoDatabase=Tidak usah dulu
en.Downloading=Downloading the web server, PHP, and database from their official sources
id.Downloading=Mengunduh web server, PHP, dan database dari sumber resminya
en.Preparing=Preparing MeowServer (unpacking PHP and the database)...
id.Preparing=Menyiapkan MeowServer (mengekstrak PHP dan database)...
en.Offline=MeowServer could not download its parts (%1).%n%nIt is installed anyway: open it and press Install recommended stack when you are online.
id.Offline=MeowServer tidak bisa mengunduh komponennya (%1).%n%nMeowServer tetap terpasang: buka lalu tekan Pasang paket yang disarankan saat online.
en.BringProjects=Bring my projects and databases from %1 (shown when MeowServer opens)
id.BringProjects=Pindahkan proyek dan database dari %1 (ditampilkan saat MeowServer dibuka)

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
; Through Explorer, not as Setup's child: Setup enforces Windows' RedirectionGuard
; on itself, and a child would inherit it and refuse the folder links (junctions)
; Laragon users keep their projects behind.
; Before the window opens, so it shows the import plan right away.
Filename: "{app}\meow.exe"; Parameters: "setup offer-import {code:ImportTool}"; Description: "{code:ImportText}"; Flags: postinstall runhidden skipifsilent; Check: ImportFound
Filename: "{win}\explorer.exe"; Parameters: """{app}\meowserver.exe"""; Description: "{cm:LaunchProgram,MeowServer}"; Flags: nowait postinstall skipifsilent
; An update started from MeowServer's window runs silently: open it again.
Filename: "{win}\explorer.exe"; Parameters: """{app}\meowserver.exe"""; Flags: nowait; Check: WizardSilent

[UninstallRun]
Filename: "{app}\meow.exe"; Parameters: "quit"; Flags: runhidden waituntilterminated; RunOnceId: "quit"
Filename: "{app}\meow.exe"; Parameters: "undo-system-changes"; Flags: runhidden waituntilterminated; RunOnceId: "undo"

[Code]
const
  EnvKey = 'Environment';

var
  WebPage, DbPage: TInputOptionWizardPage;
  DownloadPage: TDownloadWizardPage;
  // Components were downloaded and wait for `meow setup apply`.
  SetupReady: Boolean;
  ImportId, ImportName: String;

// MeowServer is already set up in the chosen folder: an update, which
// keeps what is installed and asks nothing about it.
function IsUpdate: Boolean;
begin
  Result := DirExists(AddBackslash(WizardDirValue) + 'bin\php');
end;

procedure InitializeWizard;
begin
  WebPage := CreateInputOptionPage(wpSelectTasks, CustomMessage('WebTitle'), CustomMessage('WebDesc'),
    CustomMessage('WebSub'), True, False);
  WebPage.Add('Caddy (' + CustomMessage('Recommended') + ')');
  WebPage.Add('Nginx');
  WebPage.Add('Apache');
  WebPage.SelectedValueIndex := 0;
  DbPage := CreateInputOptionPage(WebPage.ID, CustomMessage('DbTitle'), CustomMessage('DbDesc'),
    CustomMessage('DbSub'), True, False);
  DbPage.Add('MySQL 8.4 (' + CustomMessage('LikeLaragon') + ')');
  DbPage.Add('MariaDB');
  DbPage.Add('PostgreSQL');
  DbPage.Add(CustomMessage('NoDatabase'));
  DbPage.SelectedValueIndex := 0;
  DownloadPage := CreateDownloadPage(CustomMessage('Downloading'), '', nil);
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := ((PageID = WebPage.ID) or (PageID = DbPage.ID)) and IsUpdate;
end;

function WebChoice: String;
begin
  case WebPage.SelectedValueIndex of
    1: Result := 'nginx';
    2: Result := 'apache';
  else
    Result := 'caddy';
  end;
end;

function DbChoice: String;
begin
  case DbPage.SelectedValueIndex of
    1: Result := 'mariadb';
    2: Result := 'postgres';
    3: Result := 'none';
  else
    Result := 'mysql';
  end;
end;

// Asks meow.exe (unpacked to {tmp}) what to download, then downloads it
// on Setup's own page, which checks each file's SHA-256. Offline, Setup
// carries on and the window offers the same later.
function DownloadComponents: Boolean;
var
  Code, I, Tab1, Tab2: Integer;
  Lines: TArrayOfString;
  Line, Url, Name, Sha: String;
begin
  Result := True;
  ExtractTemporaryFile('meow.exe');
  if not Exec(ExpandConstant('{tmp}\meow.exe'),
      'setup plan --web ' + WebChoice + ' --db ' + DbChoice + ' --out "' + ExpandConstant('{tmp}') + '"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0)
      or not LoadStringsFromFile(ExpandConstant('{tmp}\downloads.txt'), Lines) then
  begin
    SuppressibleMsgBox(FmtMessage(CustomMessage('Offline'), ['meow setup plan']), mbInformation, MB_OK, IDOK);
    Exit;
  end;
  DownloadPage.Clear;
  for I := 0 to GetArrayLength(Lines) - 1 do
  begin
    // url <TAB> file name <TAB> sha256 (empty when upstream publishes none)
    Line := Lines[I];
    Tab1 := Pos(#9, Line);
    if Tab1 = 0 then
      Continue;
    Url := Copy(Line, 1, Tab1 - 1);
    Line := Copy(Line, Tab1 + 1, MaxInt);
    Tab2 := Pos(#9, Line);
    if Tab2 = 0 then
    begin
      Name := Line;
      Sha := '';
    end else
    begin
      Name := Copy(Line, 1, Tab2 - 1);
      Sha := Trim(Copy(Line, Tab2 + 1, MaxInt));
    end;
    DownloadPage.Add(Url, Name, Sha);
  end;
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
      SetupReady := True;
    except
      if DownloadPage.AbortedByUser then
        Log('Downloads cancelled by the user.')
      else
        SuppressibleMsgBox(FmtMessage(CustomMessage('Offline'), [GetExceptionMessage]), mbInformation, MB_OK, IDOK);
    end;
  finally
    DownloadPage.Hide;
  end;
end;

// The first other tool with projects or databases, as `meow import
// --detect` lists them: id <TAB> name <TAB> folder.
procedure DetectImport;
var
  Code, Tab1: Integer;
  Output: TExecOutput;
  Line: String;
begin
  ImportId := '';
  if not ExecAndCaptureOutput(ExpandConstant('{app}\meow.exe'), 'import --detect', '', SW_HIDE,
      ewWaitUntilTerminated, Code, Output) or (Code <> 0) or (GetArrayLength(Output.StdOut) = 0) then
    Exit;
  Line := Output.StdOut[0];
  Tab1 := Pos(#9, Line);
  if Tab1 = 0 then
    Exit;
  ImportId := Copy(Line, 1, Tab1 - 1);
  Line := Copy(Line, Tab1 + 1, MaxInt);
  if Pos(#9, Line) > 0 then
    ImportName := Copy(Line, 1, Pos(#9, Line) - 1)
  else
    ImportName := Line;
end;

function ImportFound: Boolean;
begin
  Result := ImportId <> '';
end;

function ImportTool(Param: String): String;
begin
  Result := ImportId;
end;

function ImportText(Param: String): String;
begin
  Result := FmtMessage(CustomMessage('BringProjects'), [ImportName]);
end;

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
  if (CurPageID = wpReady) and not IsUpdate and not WizardSilent then
    DownloadComponents;
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
var
  Code: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    SetOnPath(ExpandConstant('{app}'), WizardIsTaskSelected('path'));
    if SetupReady then
    begin
      WizardForm.StatusLabel.Caption := CustomMessage('Preparing');
      if not Exec(ExpandConstant('{app}\meow.exe'), 'setup apply --from "' + ExpandConstant('{tmp}') + '"', '',
          SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
        SuppressibleMsgBox(FmtMessage(CustomMessage('Offline'), ['meow setup apply']), mbInformation, MB_OK, IDOK);
    end;
    // Offer an import only on a first install.
    if not WizardSilent and (SetupReady or not IsUpdate) then
      DetectImport;
  end;
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
