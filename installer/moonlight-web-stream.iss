#ifndef SourceDir
  #define SourceDir "package"
#endif

#ifndef AppVersion
  #define AppVersion "3.0.0-prerelease.5"
#endif

#define AppName "Moonlight Web Stream"
#define ServiceName "MoonlightWebStream"
#define WebUiFirewallRuleName "Moonlight Web Stream Web UI"
#define WebRtcFirewallRuleName "Moonlight Web Stream WebRTC"
#define WebRtcPortRange "50000-50020"

[Setup]
AppId={{5C51DA95-59C1-479A-9C95-CBB83BE2A265}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Moonlight Web Stream
DefaultDirName={autopf}\Moonlight Web Stream
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=output
OutputBaseFilename=moonlight-web-stream-setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no

[Files]
Source: "{#SourceDir}\web-server.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\static\*"; DestDir: "{app}\static"; Flags: ignoreversion recursesubdirs createallsubdirs

[Code]
const
  ServiceRegistryKey = 'SYSTEM\CurrentControlSet\Services\{#ServiceName}';

var
  WebUiPortPage: TInputQueryWizardPage;

procedure ExecSc(const Parameters: String; var ResultCode: Integer);
begin
  Exec(ExpandConstant('{sys}\sc.exe'), Parameters, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure ExecNetsh(const Parameters: String; var ResultCode: Integer);
begin
  Exec(ExpandConstant('{sys}\netsh.exe'), Parameters, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure ExecTaskkill(const Parameters: String; var ResultCode: Integer);
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), Parameters, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function ServiceExists(): Boolean;
var
  ResultCode: Integer;
begin
  ExecSc('query "{#ServiceName}"', ResultCode);
  Result := ResultCode = 0;
end;

procedure StopService();
var
  ResultCode: Integer;
begin
  if not ServiceExists() then
    exit;

  ExecSc('stop "{#ServiceName}"', ResultCode);
  Sleep(1500);
  { A damaged or older service binary can ignore SCM stop. Limit the fallback to this service. }
  ExecTaskkill('/f /fi "SERVICES eq {#ServiceName}"', ResultCode);
  Sleep(500);
end;

procedure RemoveService();
var
  ResultCode: Integer;
begin
  if not ServiceExists() then
    exit;

  StopService();
  ExecSc('delete "{#ServiceName}"', ResultCode);
  Sleep(500);
end;

procedure RemoveFirewallRules();
var
  ResultCode: Integer;
begin
  ExecNetsh('advfirewall firewall delete rule name="{#WebUiFirewallRuleName}"', ResultCode);
  ExecNetsh('advfirewall firewall delete rule name="{#WebRtcFirewallRuleName}"', ResultCode);
end;

procedure ConfigureFirewall(const ExecutablePath, WebUiPort: String);
var
  ResultCode: Integer;
begin
  RemoveFirewallRules();

  ExecNetsh('advfirewall firewall add rule name="{#WebUiFirewallRuleName}" dir=in action=allow program="' + ExecutablePath + '" enable=yes profile=any protocol=TCP localport=' + WebUiPort, ResultCode);
  if ResultCode <> 0 then
    RaiseException('Unable to configure the Windows Firewall rule. Error code: ' + IntToStr(ResultCode));

  ExecNetsh('advfirewall firewall add rule name="{#WebRtcFirewallRuleName}" dir=in action=allow program="' + ExecutablePath + '" enable=yes profile=any protocol=UDP localport={#WebRtcPortRange}', ResultCode);
  if ResultCode <> 0 then
    RaiseException('Unable to configure the Windows Firewall rule. Error code: ' + IntToStr(ResultCode));
end;

procedure InstallService();
var
  ExecutablePath: String;
  ImagePath: String;
  WebUiPort: String;
  ResultCode: Integer;
begin
  RemoveService();

  ExecutablePath := ExpandConstant('{app}\web-server.exe');
  ExecSc('create "{#ServiceName}" binPath= "' + ExecutablePath + '" start= auto DisplayName= "{#AppName}"', ResultCode);
  if ResultCode <> 0 then
    RaiseException('Unable to register the Moonlight Web Stream service. Error code: ' + IntToStr(ResultCode));

  WebUiPort := WebUiPortPage.Values[0];
  ImagePath := '"' + ExecutablePath + '" --service --bind-address 0.0.0.0:' + WebUiPort + ' --webrtc-port-range 50000:50020 --log-file server\service.log';
  if not RegWriteExpandStringValue(HKLM, ServiceRegistryKey, 'ImagePath', ImagePath) then
    RaiseException('Unable to configure the Moonlight Web Stream service.');

  ConfigureFirewall(ExecutablePath, WebUiPort);

  ExecSc('start "{#ServiceName}"', ResultCode);
  if ResultCode <> 0 then
    RaiseException('Unable to start the Moonlight Web Stream service. Error code: ' + IntToStr(ResultCode));
end;

procedure InitializeWizard();
var
  ImagePath: String;
  Marker: String;
  StartPosition: Integer;
  EndPosition: Integer;
begin
  WebUiPortPage := CreateInputQueryPage(
    wpSelectDir,
    'Web UI Port / Web UI 端口',
    'Choose the browser access port. 选择浏览器访问端口。',
    '');
  WebUiPortPage.Add('Web UI port / Web UI 端口:', False);
  WebUiPortPage.Values[0] := '8080';

  if RegQueryStringValue(HKLM, ServiceRegistryKey, 'ImagePath', ImagePath) then begin
    Marker := '--bind-address 0.0.0.0:';
    StartPosition := Pos(Marker, ImagePath);
    if StartPosition > 0 then begin
      StartPosition := StartPosition + Length(Marker);
      EndPosition := StartPosition;
      while (EndPosition <= Length(ImagePath)) and
        (ImagePath[EndPosition] >= '0') and (ImagePath[EndPosition] <= '9') do
        EndPosition := EndPosition + 1;
      WebUiPortPage.Values[0] := Copy(ImagePath, StartPosition, EndPosition - StartPosition);
    end;
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  WebUiPort: Integer;
begin
  Result := True;
  if CurPageID = wpReady then begin
    StopService();
    exit;
  end;

  if CurPageID <> WebUiPortPage.ID then
    exit;

  WebUiPort := StrToIntDef(WebUiPortPage.Values[0], 0);
  if (WebUiPort < 1) or (WebUiPort > 65535) then begin
    MsgBox('Enter a port number between 1 and 65535. 请输入 1 到 65535 之间的端口号。', mbError, MB_OK);
    Result := False;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    InstallService();
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then begin
    RemoveService();
    RemoveFirewallRules();
  end;
end;
