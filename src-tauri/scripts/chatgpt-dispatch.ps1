param([Parameter(Mandatory=$true)][string]$RequestPath)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)
function Result($status,$detail,$ref='') { @{status=$status;detail=$detail;windowRef=$ref;requestId=$req.requestId}|ConvertTo-Json -Compress;exit 0 }
# The same Windows application hosts both modes. Image identity never grants a Codex chat permission.
function Get-ChatGPTDesktopHostKind([string]$ImagePath) {
  if([string]::IsNullOrWhiteSpace($ImagePath) -or [IO.Path]::GetFileName($ImagePath) -ine 'ChatGPT.exe'){return 'unsupported'}
  if($ImagePath -match '(?i)(^|[\\/])(OpenAI\.Codex_[^\\/]+|Codex)([\\/]|$)'){return 'unified'}
  return 'independent'
}
function Get-ChatGPTModeFromName([string]$Name) {
  if($Name -in @('切换模式，当前模式：ChatGPT','Switch mode, current mode: ChatGPT')){return 'ChatGPT'}
  if($Name -in @('切换模式，当前模式：Codex','Switch mode, current mode: Codex')){return 'Codex'}
  return 'unknown'
}
function Get-ChatGPTModeSnapshot($Window) {
  $nodes=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
  $selectors=@($nodes|Where-Object { -not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and (Get-ChatGPTModeFromName $_.Current.Name) -ne 'unknown' })
  if($selectors.Count -ne 1){return @{mode='unknown';count=$selectors.Count;selector=$null}}
  return @{mode=(Get-ChatGPTModeFromName $selectors[0].Current.Name);count=1;selector=$selectors[0]}
}
function Confirm-ChatGPTAppMode($Window,[string]$HostKind,[string]$Action) {
  if($HostKind -eq 'web'){return @{ready=$true;detail='网页地址另行核对'}}
  if($HostKind -notin @('unified','independent')){return @{ready=$false;detail='无法确认 Windows 聊天宿主'}}
  $snapshot=Get-ChatGPTModeSnapshot $Window
  if($snapshot.mode -eq 'ChatGPT'){return @{ready=$true;detail='已核对左上角 ChatGPT 聊天模式'}}
  if($snapshot.count -eq 0 -and $HostKind -eq 'independent'){return @{ready=$true;detail='独立 ChatGPT 客户端'}}
  if($snapshot.mode -ne 'Codex'){return @{ready=$false;detail='统一桌面应用没有唯一可核对的模式按钮；未填入或发送'}}
  if($Action -ne 'stage'){return @{ready=$false;detail='当前处于 Codex 工作模式；只有任务准备阶段可切换 ChatGPT，未填入或发送'}}
  $selector=$snapshot.selector;$expand=$null
  if(-not $selector.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$expand)){return @{ready=$false;detail='模式菜单不支持可核对的展开；没有切换或发送'}}
  $opened=$false
  try {
    if($expand.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){return @{ready=$false;detail='模式菜单已被其他操作打开；未接管'}}
    $expand.Expand();$opened=$true;Start-Sleep -Milliseconds 250
    $items=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object { -not $_.Current.IsOffscreen -and $_.Current.IsEnabled -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::MenuItem -and $_.Current.Name -in @('ChatGPT 创建、学习和探索','ChatGPT Create, learn, and explore') })
    if($items.Count -ne 1){return @{ready=$false;detail='ChatGPT 模式菜单项不唯一或不存在；没有填入或发送'}}
    $invoke=$null
    if(-not $items[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){return @{ready=$false;detail='ChatGPT 模式项不可调用；没有填入或发送'}}
    $invoke.Invoke()
    for($i=0;$i -lt 10;$i++){
      Start-Sleep -Milliseconds 200
      if((Get-ChatGPTModeSnapshot $Window).mode -eq 'ChatGPT'){return @{ready=$true;detail='已切换并回读 ChatGPT 聊天模式'}}
    }
    return @{ready=$false;detail='切换后未回读到 ChatGPT 模式；没有填入或发送'}
  } finally {if($opened){try{if($expand.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){$expand.Collapse()}}catch{}}}
}

function Confirm-ChatGPTMode($Window,[string]$HostKind,[string]$Action) {
  $result=Confirm-ChatGPTAppMode $Window $HostKind $Action
  if(-not $result.ready -or $HostKind -ne 'unified'){return $result}
  $nodes=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
  $chat=@($nodes|Where-Object {-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in @('聊天','Chat')})
  $work=@($nodes|Where-Object {-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in @('工作','Work')})
  if($chat.Count -ne 1 -or $work.Count -ne 1){return @{ready=$false;detail='无法唯一核对聊天/工作选项；未填入或发送'}}
  $cp=$null;$wp=$null
  if(-not $chat[0].TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$cp) -or -not $work[0].TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$wp)){return @{ready=$false;detail='聊天/工作选中状态不可核对'}}
  if($cp.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::Off -and $wp.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On -and $Action -eq 'stage'){$cp.Toggle();Start-Sleep -Milliseconds 300}
  if($cp.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On -or $wp.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::Off){return @{ready=$false;detail='当前不是聊天选项；工作模式不允许派发'}}
  return @{ready=$true;detail='已核对 ChatGPT 模式及聊天选项'}
}
function Confirm-ChatGPTEntryMode($Window,[string]$HostKind,[string]$Action,[bool]$NewChat) {
  # A Work conversation can remain visible inside ChatGPT mode. Only preparation of a
  # new chat may defer the composer gate until after the unique New chat control.
  if($Action -eq 'stage' -and $NewChat){return Confirm-ChatGPTAppMode $Window $HostKind $Action}
  return Confirm-ChatGPTMode $Window $HostKind $Action
}
function Close-TaskOptionMenu($Window,$Trigger) {
  try {$p=$Trigger.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern);$p.Collapse();Start-Sleep -Milliseconds 80
    if($p.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){
      $hwnd=[IntPtr]$Window.Current.NativeWindowHandle
      if([ChatGPTMenuCleanup]::GetForegroundWindow() -ne $hwnd){throw '菜单关闭前窗口焦点改变'}
      [void][ChatGPTMenuCleanup]::PostMessage($hwnd,0x100,[IntPtr]0x1B,[IntPtr]0)
      [void][ChatGPTMenuCleanup]::PostMessage($hwnd,0x101,[IntPtr]0x1B,[IntPtr]0)
    }
  }catch{throw '无法确认已关闭本次选项菜单；没有发送'}
}
# This supported UI exposes a custom bar without RangeValue. User confirmed its right endpoint is Pro.
function Test-ProThumbAtMaximum($Bitmap) {
  $w=$Bitmap.Width;$h=$Bitmap.Height
  if($h -lt 20 -or $w -lt 4*$h -or $w -gt 12*$h){return $false}
  $count=0;$sx=0.0;$sy=0.0;$minX=$w;$maxX=0;$minY=$h;$maxY=0
  for($y=0;$y -lt $h;$y+=2){for($x=0;$x -lt $w;$x+=2){
    $c=$Bitmap.GetPixel($x,$y)
    if($c.R -ge 238 -and $c.G -ge 238 -and $c.B -ge 238){$count++;$sx+=$x;$sy+=$y;$minX=[Math]::Min($minX,$x);$maxX=[Math]::Max($maxX,$x);$minY=[Math]::Min($minY,$y);$maxY=[Math]::Max($maxY,$y)}
  }}
  if($count -eq 0){return $false}
  $area=4*$count/($h*$h);$cx=$sx/$count;$cy=$sy/$count
  return $area -ge 0.18 -and $area -le 0.9 -and [Math]::Abs($cx-($w-0.65*$h)) -le 0.1*$h -and [Math]::Abs($cy-$h/2) -lt 0.2*$h -and ($maxX-$minX) -ge 0.6*$h -and ($maxX-$minX) -le 1.2*$h -and ($maxY-$minY) -ge 0.6*$h -and ($maxY-$minY) -le 1.2*$h
}
function Read-ProSliderMaximum($Window,$Slider) {
  if($Slider.Current.ClassName -notmatch '_SliderKeyboardControl_'){return $false}
  if([ChatGPTMenuCleanup]::GetForegroundWindow() -ne [IntPtr]$Window.Current.NativeWindowHandle){return $false}
  $r=$Slider.Current.BoundingRectangle
  if($r.IsEmpty -or $r.Width -lt 4*$r.Height -or $r.Width -gt 12*$r.Height){return $false}
  $bitmap=New-Object System.Drawing.Bitmap ([int]$r.Width),([int]$r.Height)
  $g=[System.Drawing.Graphics]::FromImage($bitmap)
  try {$g.CopyFromScreen([int]$r.Left,[int]$r.Top,0,0,$bitmap.Size);return Test-ProThumbAtMaximum $bitmap}finally{$g.Dispose();$bitmap.Dispose()}
}
function Confirm-ProSlider($Window,$Slider,[bool]$Select) {
  if(Read-ProSliderMaximum $Window $Slider){return @{ready=$true;detail='已核对强度滑条最右端 Pro（用户确认的档位映射）'}}
  if(-not $Select){return @{ready=$false;detail='当前强度滑条未能核对为最右端 Pro；没有发送'}}
  $hwnd=[IntPtr]$Window.Current.NativeWindowHandle
  if([ChatGPTMenuCleanup]::GetForegroundWindow() -ne $hwnd){return @{ready=$false;detail='ChatGPT 不在前台，无法核对 Pro 滑条；请将其置于前台'}}
  if($Slider.Current.ClassName -notmatch '_SliderKeyboardControl_'){return @{ready=$false;detail='未知强度控件，不推测档位'}}
  $Slider.SetFocus()
  if(-not $Slider.Current.HasKeyboardFocus){return @{ready=$false;detail='强度控件焦点未确认；没有调整或发送'}}
  # Targeted navigation keys only; no text or Return, no guessed screen clicks.
  for($i=0;$i -lt 13;$i++){
    $mode=Confirm-ChatGPTMode $Window 'unified' 'verify'
    if(-not $mode.ready -or [ChatGPTMenuCleanup]::GetForegroundWindow() -ne $hwnd -or -not $Slider.Current.HasKeyboardFocus){return @{ready=$false;detail='调整强度时模式或焦点变化；没有发送'}}
    $key=if($i -eq 0){0x23}else{0x27}
    [void][ChatGPTMenuCleanup]::PostMessage($hwnd,0x100,[IntPtr]$key,[IntPtr]0)
    [void][ChatGPTMenuCleanup]::PostMessage($hwnd,0x101,[IntPtr]$key,[IntPtr]0)
    Start-Sleep -Milliseconds 120
    if(Read-ProSliderMaximum $Window $Slider){return @{ready=$true;detail='已设置并核对强度滑条最右端 Pro'}}
  }
  return @{ready=$false;detail='未能回读 Pro 滑条端点，停止；没有发送'}
}
function Confirm-TaskOption($Window,[string]$HostKind,[string]$Submenu,[string]$Option,[bool]$Select) {
  $mode=Confirm-ChatGPTMode $Window $HostKind 'verify'
  if(-not $mode.ready){return $mode}
  $nodes=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
  $triggers=@($nodes|Where-Object {-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in @('选择 ChatGPT 模型','Choose ChatGPT model')})
  if($triggers.Count -ne 1){return @{ready=$false;detail='模型与强度菜单不唯一'}}
  $expand=$null
  if(-not $triggers[0].TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$expand)){return @{ready=$false;detail='模型与强度菜单不可展开'}}
  if($expand.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){return @{ready=$false;detail='模型菜单已被其他操作打开；未接管'}}
  try {
    $expand.Expand();Start-Sleep -Milliseconds 300
    $nodes=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
    $items=@($nodes|Where-Object {-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::MenuItem -and $_.Current.Name -eq $Submenu})
    if($items.Count -ne 1){return @{ready=$false;detail="缺少唯一的 $Submenu 菜单"}}
    if($Submenu -eq '强度' -and $Option -eq 'Pro' -and $items[0].Current.ClassName -match '_SliderKeyboardControl_'){return Confirm-ProSlider $Window $items[0] $Select}
    $invoke=$null
    if(-not $items[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){return @{ready=$false;detail="$Submenu 菜单不可调用"}}
    $invoke.Invoke()
    $choices=@()
    for($i=0;$i -lt 10;$i++){
      Start-Sleep -Milliseconds 150
      $nodes=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
      $choices=@($nodes|Where-Object {-not $_.Current.IsOffscreen -and $_.Current.IsEnabled -and $_.Current.ControlType -in @([System.Windows.Automation.ControlType]::RadioButton,[System.Windows.Automation.ControlType]::MenuItem,[System.Windows.Automation.ControlType]::CheckBox) -and $_.Current.Name -eq $Option})
      if($choices.Count){break}
    }
    if($choices.Count -ne 1){return @{ready=$false;detail="无法唯一核对 $Submenu 中的 $Option；未填入或发送"}}
    $selected=$null;$toggle=$null
    if($choices[0].TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selected)){
      if($Select -and -not $selected.Current.IsSelected){$selected.Select();Start-Sleep -Milliseconds 150;Close-TaskOptionMenu $Window $triggers[0];return Confirm-TaskOption $Window $HostKind $Submenu $Option $false}
      return @{ready=[bool]$selected.Current.IsSelected;detail="$Submenu 的 $Option 选中状态已读取"}
    }
    if($choices[0].TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$toggle)){
      if($Select -and $toggle.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::Off){$toggle.Toggle();Start-Sleep -Milliseconds 150;Close-TaskOptionMenu $Window $triggers[0];return Confirm-TaskOption $Window $HostKind $Submenu $Option $false}
      return @{ready=($toggle.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On);detail="$Submenu 的 $Option 选中状态已读取"}
    }
    return @{ready=$false;detail="$Option 没有可独立回读的选中状态；没有发送"}
  } finally {Close-TaskOptionMenu $Window $triggers[0]}
}

function Test-WindowsMentionSnapshot([string]$Label,[string]$ParentKind,[string]$GrandId,[string]$ComposerId,$Icons) {
  return $Label -ceq 'GPTBridge Windows' -and $ParentKind -eq 'ControlType.Group' -and $GrandId -eq $ComposerId -and @($Icons|Where-Object {$_ -match '_SlotSizedIcon_'}).Count -eq 1
}
function Get-StructuredWindowsMention($Composer) {
  $labels=@($Composer.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text -and $_.Current.Name -ceq 'GPTBridge Windows'})
  if($labels.Count -ne 1){return $false}
  $parent=[System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($labels[0])
  if(-not $parent){return $false}
  $grand=[System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($parent)
  if(-not $grand){return $false}
  $icons=@($parent.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Image}|ForEach-Object {$_.Current.ClassName})
  return Test-WindowsMentionSnapshot $labels[0].Current.Name $parent.Current.ControlType.ProgrammaticName ($grand.GetRuntimeId()-join ',') ($Composer.GetRuntimeId()-join ',') $icons
}
function Ensure-StructuredWindowsMention($Window,$Composer,$Value) {
  if(Get-StructuredWindowsMention $Composer){return $true}
  if($Value.Current.Value.Trim()){return $false}
  $mode=Confirm-ChatGPTMode $Window 'unified' 'verify'
  if(-not $mode.ready -or [ChatGPTMenuCleanup]::GetForegroundWindow() -ne [IntPtr]$Window.Current.NativeWindowHandle){return $false}
  $Composer.SetFocus()
  if(-not $Composer.Current.HasKeyboardFocus){return $false}
  $Value.SetValue('@GPTBridge Windows')
  $choices=@();$cr=$Composer.Current.BoundingRectangle
  for($i=0;$i -lt 10;$i++){
    Start-Sleep -Milliseconds 200
    $choices=@($Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {
      $c=$_.Current
      -not $c.IsOffscreen -and $c.IsEnabled -and $c.ControlType -in @([System.Windows.Automation.ControlType]::Button,[System.Windows.Automation.ControlType]::MenuItem,[System.Windows.Automation.ControlType]::ListItem) -and $c.Name -match '^GPTBridge Windows(?:[ \r\n]|$)' -and $c.BoundingRectangle.Left -ge $cr.Left-20 -and $c.BoundingRectangle.Right -le $cr.Right+200 -and $c.BoundingRectangle.Bottom -le $cr.Bottom+100 -and $c.BoundingRectangle.Top -ge $cr.Top-800
    })
    if($choices.Count){break}
  }
  if($choices.Count -ne 1){return $false}
  $mode=Confirm-ChatGPTMode $Window 'unified' 'verify'
  if(-not $mode.ready){return $false}
  $invoke=$null;$selection=$null
  if($choices[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){$invoke.Invoke()}
  elseif($choices[0].TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selection)){$selection.Select()}else{return $false}
  for($i=0;$i -lt 10;$i++){Start-Sleep -Milliseconds 150;if(Get-StructuredWindowsMention $Composer){return $true}}
  return $false
}
function Append-TaskAfterMention($Window,$Composer,[string]$Text) {
  $Composer.SetFocus()
  $pattern=$null
  if(-not $Composer.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$pattern)){return $false}
  $range=$pattern.DocumentRange.Clone()
  $range.MoveEndpointByRange([System.Windows.Automation.TextPatternRangeEndpoint]::Start,$pattern.DocumentRange,[System.Windows.Automation.TextPatternRangeEndpoint]::End)
  $range.Select()
  for($offset=0;$offset -lt $Text.Length;$offset+=256){
    $mode=Confirm-ChatGPTMode $Window 'unified' 'verify'
    if(-not $mode.ready -or -not $Composer.Current.HasKeyboardFocus -or [ChatGPTMenuCleanup]::GetForegroundWindow() -ne [IntPtr]$Window.Current.NativeWindowHandle){return $false}
    $part=$Text.Substring($offset,[Math]::Min(256,$Text.Length-$offset))
    if(-not [ChatGPTTextInput]::AppendUnicode($part)){return $false}
  }
  return $true
}
function Get-PromptSha([string]$Text) {
  $sha=[Security.Cryptography.SHA256]::Create()
  try{return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
}
# END DESKTOP MODE HELPERS
try {
  $req=Get-Content -LiteralPath $RequestPath -Raw -Encoding UTF8|ConvertFrom-Json
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
  Add-Type -AssemblyName System.Drawing
  Add-Type @'
using System;using System.Runtime.InteropServices;public class ChatGPTMenuCleanup { [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);[DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();[DllImport("user32.dll")]public static extern bool SetProcessDPIAware();[DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr hwnd); }
'@
  # BEGIN UNICODE TEXT INPUT
  Add-Type @'
using System;using System.Runtime.InteropServices;
public class ChatGPTTextInput {
 [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {public ushort vk,scan;public uint flags,time;public IntPtr extra;}
 [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {public int dx,dy;public uint data,flags,time;public IntPtr extra;}
 [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION {[FieldOffset(0)]public KEYBDINPUT key;[FieldOffset(0)]public MOUSEINPUT mouse;}
 [StructLayout(LayoutKind.Sequential)] public struct INPUT {public uint type;public INPUTUNION data;}
 [DllImport("user32.dll",SetLastError=true)]static extern uint SendInput(uint count,INPUT[] inputs,int size);
 public static INPUT[] BuildUnicodeInputs(string text){var inputs=new INPUT[text.Length*2];for(int i=0;i<text.Length;i++){inputs[2*i].type=1;inputs[2*i].data.key.scan=text[i];inputs[2*i].data.key.flags=4;inputs[2*i+1]=inputs[2*i];inputs[2*i+1].data.key.flags=6;}return inputs;}
 public static bool AppendUnicode(string text){var inputs=BuildUnicodeInputs(text);return SendInput((uint)inputs.Length,inputs,Marshal.SizeOf(typeof(INPUT)))==(uint)inputs.Length;}
}
'@
  # END UNICODE TEXT INPUT
  [void][ChatGPTMenuCleanup]::SetProcessDPIAware()
  $root=[System.Windows.Automation.AutomationElement]::RootElement
  $windows=@($root.FindAll([System.Windows.Automation.TreeScope]::Children,[System.Windows.Automation.Condition]::TrueCondition))
  $targets=@();$desktopKinds=@{}
  foreach($w in $windows) {
    try {
      $proc=Get-Process -Id $w.Current.ProcessId
      if($req.client -eq 'desktop' -and $proc.ProcessName -eq 'ChatGPT' -and $w.Current.ControlType -eq [System.Windows.Automation.ControlType]::Window -and $w.Current.Name -and -not $w.Current.IsOffscreen){
        $kind=Get-ChatGPTDesktopHostKind $proc.Path
        if($kind -ne 'unsupported'){$targets+=,$w;$desktopKinds[[string]$w.Current.NativeWindowHandle]=$kind}
      }
      if($req.client -eq 'web' -and $proc.ProcessName -in @('msedge','chrome') -and $w.Current.Name -match 'ChatGPT') {
        $edits=$w.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Edit))
        foreach($edit in $edits) {
          $vp=$null
          if($edit.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$vp)) {
            $address=$vp.Current.Value
            if($address -match '^(https://)?chatgpt\.com(/|$)'){$targets+=,$w;break}
          }
        }
      }
    } catch { }
  }
  if($targets.Count -ne 1){Result 'blocked' '请只打开一个目标 ChatGPT 窗口；网页端还需要可核对 chatgpt.com 地址栏。'}
  $window=$targets[0];$hostKind=if($req.client -eq 'web'){'web'}else{$desktopKinds[[string]$window.Current.NativeWindowHandle]};$ref="$($window.Current.ProcessId):$($window.Current.NativeWindowHandle)"
  if($req.windowRef -and $req.windowRef -ne $ref){Result 'blocked' '目标窗口已变化，请重新检查并填入任务。' $ref}
  if($req.action -eq 'stage') {
    $busyButtons=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button))|Where-Object {$_.Current.Name -in @('Stop generating','Stop streaming','停止生成','停止响应','停止','停止任务') -and -not $_.Current.IsOffscreen})
    if($busyButtons.Count -gt 0){Result 'busy' 'ChatGPT 仍在处理上一条消息；未创建新聊天、未填入或发送。' $ref}
  }
  if($req.action -eq 'stage' -and $hostKind -eq 'unified' -and [ChatGPTMenuCleanup]::GetForegroundWindow() -ne [IntPtr]$window.Current.NativeWindowHandle){
    [void][ChatGPTMenuCleanup]::SetForegroundWindow([IntPtr]$window.Current.NativeWindowHandle)
    Start-Sleep -Milliseconds 150
    if([ChatGPTMenuCleanup]::GetForegroundWindow() -ne [IntPtr]$window.Current.NativeWindowHandle){Result 'blocked' '请将目标 ChatGPT 窗口置于前台；未填入或发送' $ref}
  }
  $mode=Confirm-ChatGPTEntryMode $window $hostKind $req.action ([bool]$req.newChat)
  if(-not $mode.ready){Result 'blocked' $mode.detail $ref}
  if($req.action -eq 'stage' -and $req.newChat) {
    $currentButtons=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button)))
    $new=@($currentButtons|Where-Object {$_.Current.Name -in @('New chat','新聊天','新建聊天') -and -not $_.Current.IsOffscreen})
    if($new.Count -ne 1){Result 'blocked' '无法核对唯一的新聊天入口，未填入或发送；请适配当前客户端名称。' $ref}
    $invoke=$null
    if(-not $new[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){Result 'blocked' '新聊天入口不可访问，没有发送。' $ref}
    $invoke.Invoke()
    Start-Sleep -Milliseconds 700
    $mode=Confirm-ChatGPTMode $window $hostKind 'stage'
    if(-not $mode.ready){Result 'blocked' $mode.detail $ref}
  }
  function Select-ExactOption($triggerNames,$optionName) {
    $guard=Confirm-ChatGPTMode $window $hostKind 'verify'
    if(-not $guard.ready){Result 'blocked' $guard.detail $ref}
    $controls=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
    $already=@($controls|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in @($optionName,"ChatGPT $optionName") -and -not $_.Current.IsOffscreen})
    if($already.Count -eq 1){return}
    $triggers=@($controls|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in $triggerNames -and -not $_.Current.IsOffscreen})
    if($triggers.Count -ne 1){return}
    $invoke=$null
    $expand=$null
    if($triggers[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){$invoke.Invoke()}
    elseif($triggers[0].TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$expand)){$expand.Expand()}else{return}
    Start-Sleep -Milliseconds 250
    $items=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.Name -eq $optionName -and -not $_.Current.IsOffscreen -and $_.Current.ControlType -in @([System.Windows.Automation.ControlType]::MenuItem,[System.Windows.Automation.ControlType]::Button,[System.Windows.Automation.ControlType]::CheckBox)})
    if($items.Count -ne 1){return}
    $selection=$null;$toggle=$null;$invoke=$null
    if($items[0].TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selection)){$selection.Select()}
    elseif($items[0].TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$toggle)){if($toggle.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On){$toggle.Toggle()}}
    elseif($items[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){$invoke.Invoke()}
    Start-Sleep -Milliseconds 250
  }
  $unifiedModelReady=$false;$unifiedEffortReady=$false
  if($hostKind -eq 'unified'){
    $modelChoice=Confirm-TaskOption $window $hostKind '选择模型' $req.model ($req.action -eq 'stage')
    if(-not $modelChoice.ready){Result 'blocked' $modelChoice.detail $ref}
    $effortChoice=Confirm-TaskOption $window $hostKind '强度' $req.effort ($req.action -eq 'stage')
    if(-not $effortChoice.ready){Result 'blocked' $effortChoice.detail $ref}
    $unifiedModelReady=$true;$unifiedEffortReady=$true
  }
  if($req.action -eq 'stage') {
    if($hostKind -ne 'unified'){Select-ExactOption @('Model selector','Choose model','选择模型','模型选择器') $req.model
    Select-ExactOption @('Thinking effort','思考强度','思考时长') $req.effort}
    $attached=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.Name -in @('Remove GPTBridge Windows','移除 GPTBridge Windows','删除 GPTBridge Windows') -and -not $_.Current.IsOffscreen})
    if($hostKind -ne 'unified' -and $attached.Count -ne 1){Select-ExactOption @('Tools','工具','Add files and more','添加文件和更多','添加文件等内容') 'GPTBridge Windows'}
  }
  $all=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
  $buttons=@($all|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and -not $_.Current.IsOffscreen})
  $model=@($buttons|Where-Object {$_.Current.Name -in @($req.model,"ChatGPT $($req.model)","模型，$($req.model)","Model, $($req.model)")})
  $effort=@($buttons|Where-Object {$_.Current.Name -eq $req.effort})
  # Only an attached removable chip counts, never a mention in old chat text or a picker result.
  $bridge=@($buttons|Where-Object {$_.Current.Name -in @('Remove GPTBridge Windows','移除 GPTBridge Windows','删除 GPTBridge Windows')})
  $composers=@($all|Where-Object { -not $_.Current.IsOffscreen -and $_.Current.IsEnabled -and $_.Current.ControlType -in @([System.Windows.Automation.ControlType]::Edit,[System.Windows.Automation.ControlType]::Document) -and $_.Current.Name -in @('Message ChatGPT','Ask anything','向 ChatGPT 发送消息','发送消息','询问任何问题','询问 ChatGPT','提示词','Prompt') })
  if($composers.Count -ne 1){Result 'blocked' '没有唯一可访问的输入框，请先打开一个新的空白聊天。' $ref}
  $mode=Confirm-ChatGPTMode $window $hostKind 'verify'
  if(-not $mode.ready){Result 'blocked' $mode.detail $ref}
  $composer=$composers[0];$value=$null
  if(-not $composer.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$value) -or $value.Current.IsReadOnly){Result 'blocked' '客户端输入框未提供可核对的 ValuePattern，已停止；不会用坐标或盲打发送。' $ref}
  $richMention=$false
  if($hostKind -eq 'unified'){
    if($req.action -eq 'stage'){$richMention=Ensure-StructuredWindowsMention $window $composer $value}else{$richMention=Get-StructuredWindowsMention $composer}
  }
  if(((-not $unifiedModelReady) -and $model.Count -ne 1) -or ((-not $unifiedEffortReady) -and $effort.Count -ne 1) -or ($bridge.Count -ne 1 -and -not $richMention)){Result 'blocked' '尚未核对模型、Pro 强度和真实附加的 GPTBridge Windows；未发送。' $ref}
  $expectedText=[string]$req.prompt
  $proofPath=Join-Path (Split-Path -Parent $RequestPath) ($req.requestId+'.prepared-ui.json')
  if($req.action -eq 'send' -and $richMention){
    if(-not(Test-Path -LiteralPath $proofPath)){Result 'blocked' '缺少本请求的结构化插件填入回执；没有发送' $ref}
    $proof=Get-Content -LiteralPath $proofPath -Raw -Encoding UTF8|ConvertFrom-Json
    if($proof.requestId -cne $req.requestId -or $proof.windowRef -cne $ref -or $proof.model -cne $req.model -or $proof.effort -cne $req.effort -or $proof.promptSha256 -cne (Get-PromptSha $req.prompt)){Result 'blocked' '插件填入回执与本请求不匹配；没有发送' $ref}
    $expectedText=[string]$proof.mentionPrefix+[string]$req.prompt
  }
  if($req.action -eq 'inspect'){Result 'ready' '已核对窗口、模型、思考强度、GPTBridge Windows 和输入框；尚未填入或发送。' $ref}
  if($req.action -eq 'stage') {
    if($richMention){
      $prefix=[string]$value.Current.Value
      if($prefix -notmatch '^\s*GPTBridge Windows\s*$'){Result 'blocked' '插件标记之外还有未发送内容，不覆盖草稿' $ref}
      $expectedText=$prefix+[string]$req.prompt
      if(-not(Append-TaskAfterMention $window $composer $req.prompt)){Result 'blocked' '填入期间模式或焦点变化，保留未发送草稿；没有发送' $ref}
      $deadline=[DateTime]::UtcNow.AddSeconds(3)
      while($value.Current.Value -cne $expectedText -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
      if(-not(Get-StructuredWindowsMention $composer) -or $value.Current.Value -cne $expectedText){Result 'blocked' '插件标记或完整输入内容回读不一致，没有发送' $ref}
      @{requestId=$req.requestId;windowRef=$ref;model=$req.model;effort=$req.effort;promptSha256=(Get-PromptSha $req.prompt);mentionPrefix=$prefix}|ConvertTo-Json -Compress|Set-Content -LiteralPath $proofPath -Encoding UTF8
    }else{
      if($value.Current.Value.Trim()){Result 'blocked' '输入框已有内容，请先在 ChatGPT 中处理；不会覆盖未发送草稿。' $ref}
      $value.SetValue([string]$req.prompt)
      if($value.Current.Value -cne $expectedText){Result 'blocked' '填入后内容回读不一致，没有发送。' $ref}
    }
    Result 'prepared' '任务已填入并逐字回读，尚未发送。请检查客户端内容后点击发送。' $ref
  }
  if($req.action -ne 'send'){Result 'blocked' '不支持的操作。' $ref}
  if($value.Current.Value -cne $expectedText){Result 'blocked' '输入内容已变化，请重新核对；没有发送。' $ref}
  $send=@($buttons|Where-Object {$_.Current.Name -in @('Send prompt','发送提示','发送消息','发送') -and $_.Current.IsEnabled})
  if($send.Count -ne 1){Result 'blocked' '没有唯一可用的发送按钮，没有发送。' $ref}
  $pattern=$null
  if(-not $send[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)){Result 'blocked' '发送按钮不支持可访问性调用，没有发送。' $ref}
  $mode=Confirm-ChatGPTMode $window $hostKind 'send'
  if(-not $mode.ready){Result 'blocked' $mode.detail $ref}
  $pattern.Invoke()
  # Observe the outgoing exact text outside the composer. No automatic retry or fallback.
  $deadline=[DateTime]::UtcNow.AddSeconds(10)
  while([DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 250
    $after=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
    $posted=@($after|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text -and $_.Current.Name -ceq $req.prompt})
    if($posted.Count -gt 0 -and -not $value.Current.Value.Trim()){Result 'sent' '已观察到本次完整消息出现在对话中；任务执行和验收尚未完成。' $ref}
  }
  Result 'unknown' '已调用一次发送按钮，但未能独立确认消息；请查询原对话，禁止自动重发或切换网页重发。' $ref
} catch {
  Result 'unknown' '客户端适配发生异常；保留本次请求并核对原窗口。没有自动重试。'
}
