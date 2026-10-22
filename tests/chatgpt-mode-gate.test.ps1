# Offline fixtures only: no live windows, process discovery, new chat, composer or sending.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
$source=[IO.File]::ReadAllText((Join-Path $PSScriptRoot '../src-tauri/scripts/chatgpt-dispatch.ps1'))
$start=$source.IndexOf('function Get-ChatGPTDesktopHostKind')
$end=$source.IndexOf('# END DESKTOP MODE HELPERS')
if($start -lt 0 -or $end -le $start){throw 'Missing mode helpers'}
Invoke-Expression $source.Substring($start,$end-$start)
function Assert($condition,$detail){if(-not $condition){throw $detail}}
function Reset-Fixture([string]$mode='Codex'){
 $script:fixture=@{mode=$mode;menuOpen=$false;switches=0;reflect=$true;selectors=1;items=1;chat=$true;conversationControls=$true}
 $script:expand=[pscustomobject]@{Current=[pscustomobject]@{ExpandCollapseState=[Windows.Automation.ExpandCollapseState]::Collapsed}}
 $script:expand|Add-Member ScriptMethod Expand {$this.Current.ExpandCollapseState=[Windows.Automation.ExpandCollapseState]::Expanded;$script:fixture.menuOpen=$true}
 $script:expand|Add-Member ScriptMethod Collapse {$this.Current.ExpandCollapseState=[Windows.Automation.ExpandCollapseState]::Collapsed;$script:fixture.menuOpen=$false}
 $script:invoker=[pscustomobject]@{}
 $script:invoker|Add-Member ScriptMethod Invoke {$script:fixture.switches++;if($script:fixture.reflect){$script:fixture.mode='ChatGPT'}}
 $script:window=[pscustomobject]@{}
 $script:window|Add-Member ScriptMethod FindAll {
   param($scope,$condition)
   $items=@()
   for($n=0;$n -lt $script:fixture.selectors;$n++){
     $selector=[pscustomobject]@{Current=[pscustomobject]@{Name=('切换模式，当前模式：'+$script:fixture.mode);ControlType=[Windows.Automation.ControlType]::Button;IsOffscreen=$false;IsEnabled=$true}}
     $selector|Add-Member ScriptMethod TryGetCurrentPattern {param($pattern,$result);$result.Value=$script:expand;return $true}
     $items+=,$selector
   }
   if($script:fixture.menuOpen){for($n=0;$n -lt $script:fixture.items;$n++){
     $item=[pscustomobject]@{Current=[pscustomobject]@{Name='ChatGPT 创建、学习和探索';ControlType=[Windows.Automation.ControlType]::MenuItem;IsOffscreen=$false;IsEnabled=$true}}
     $item|Add-Member ScriptMethod TryGetCurrentPattern {param($pattern,$result);$result.Value=$script:invoker;return $true}
     $items+=,$item
   }}
   if($script:fixture.conversationControls){
    $script:chatToggle=[pscustomobject]@{Current=[pscustomobject]@{ToggleState=$(if($script:fixture.chat){[Windows.Automation.ToggleState]::On}else{[Windows.Automation.ToggleState]::Off})}}
    $script:workToggle=[pscustomobject]@{Current=[pscustomobject]@{ToggleState=$(if($script:fixture.chat){[Windows.Automation.ToggleState]::Off}else{[Windows.Automation.ToggleState]::On})}}
    $script:chatToggle|Add-Member ScriptMethod Toggle {$script:fixture.chat=$true;$this.Current.ToggleState=[Windows.Automation.ToggleState]::On;$script:workToggle.Current.ToggleState=[Windows.Automation.ToggleState]::Off}
    $chat=[pscustomobject]@{Current=[pscustomobject]@{Name='聊天';ControlType=[Windows.Automation.ControlType]::Button;IsOffscreen=$false;IsEnabled=$true}}
    $work=[pscustomobject]@{Current=[pscustomobject]@{Name='工作';ControlType=[Windows.Automation.ControlType]::Button;IsOffscreen=$false;IsEnabled=$true}}
    $chat|Add-Member ScriptMethod TryGetCurrentPattern {param($pattern,$result);$result.Value=$script:chatToggle;return $true}
    $work|Add-Member ScriptMethod TryGetCurrentPattern {param($pattern,$result);$result.Value=$script:workToggle;return $true}
    $items+=,$chat;$items+=,$work
   }
   return $items
 }
}
Assert ((Get-ChatGPTDesktopHostKind 'C:\Program Files\WindowsApps\OpenAI.Codex_26_x64__id\app\ChatGPT.exe') -eq 'unified') 'Unified app misclassified'
Assert ((Get-ChatGPTDesktopHostKind 'C:\Apps\ChatGPT\ChatGPT.exe') -eq 'independent') 'Independent app misclassified'
Assert ((Get-ChatGPTDesktopHostKind '') -eq 'unsupported') 'Empty image accepted'
Assert ((Get-ChatGPTDesktopHostKind 'C:\Apps\Other.exe') -eq 'unsupported') 'Wrong executable accepted'
Assert ((Get-ChatGPTModeFromName 'ChatGPT') -eq 'unknown') 'Generic chat text trusted as a mode'
Reset-Fixture
$r=Confirm-ChatGPTMode $window 'unified' 'inspect'
Assert (-not $r.ready -and $fixture.switches -eq 0) 'Inspect changed Codex mode'
Reset-Fixture
$r=Confirm-ChatGPTMode $window 'unified' 'stage'
Assert ($r.ready -and $fixture.mode -eq 'ChatGPT' -and $fixture.switches -eq 1) 'Stage did not select and verify ChatGPT'
Reset-Fixture 'ChatGPT'
$r=Confirm-ChatGPTMode $window 'unified' 'send'
Assert ($r.ready -and $fixture.switches -eq 0) 'ChatGPT send gate failed'
Reset-Fixture
$r=Confirm-ChatGPTMode $window 'unified' 'send'
Assert (-not $r.ready -and $fixture.switches -eq 0) 'Send switched or accepted Codex'
Reset-Fixture 'ChatGPT'
Assert ((Confirm-ChatGPTMode $window 'unified' 'stage').ready) 'Prepared ChatGPT rejected'
$fixture.mode='Codex'
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'send').ready) 'Mode change after stage bypassed send gate'
Reset-Fixture;$fixture.selectors=0
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready) 'Unified app with no mode accepted'
Reset-Fixture;$fixture.selectors=2
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready) 'Ambiguous mode accepted'
Reset-Fixture;$fixture.items=0
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.switches -eq 0 -and -not $fixture.menuOpen) 'Missing menu did not fail closed'
Reset-Fixture;$fixture.items=2
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.switches -eq 0) 'Duplicate menu item accepted'
Reset-Fixture;$fixture.reflect=$false
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.switches -eq 1) 'Unconfirmed mode switch accepted'
Reset-Fixture;$fixture.selectors=0
Assert ((Confirm-ChatGPTMode $window 'independent' 'stage').ready) 'Independent client without mode switch rejected'
Reset-Fixture;$expand.Current.ExpandCollapseState=[Windows.Automation.ExpandCollapseState]::Expanded
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.switches -eq 0) 'User-opened menu was taken over'
Reset-Fixture 'ChatGPT';$fixture.chat=$false
Assert ((Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.chat) 'Stage failed to select Chat instead of Work'
Reset-Fixture 'ChatGPT';$fixture.chat=$false
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'send').ready -and -not $fixture.chat) 'Work accepted or changed during send'
Reset-Fixture 'ChatGPT';$fixture.conversationControls=$false
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready) 'Missing Chat/Work state accepted'
'HOST_CLASSIFICATION_CASES_PASSED=5' 
Reset-Fixture 'ChatGPT';$fixture.conversationControls=$false
Assert ((Confirm-ChatGPTEntryMode $window 'unified' 'stage' $true).ready) 'Existing Work view blocked creation of a new chat'
Assert (-not(Confirm-ChatGPTMode $window 'unified' 'stage').ready) 'New chat without Chat/Work controls bypassed composer guard'
$fixture.conversationControls=$true;$fixture.chat=$false
Assert ((Confirm-ChatGPTMode $window 'unified' 'stage').ready -and $fixture.chat) 'New composer did not switch to Chat'
Reset-Fixture 'ChatGPT';$fixture.conversationControls=$false
Assert (-not(Confirm-ChatGPTEntryMode $window 'unified' 'send' $true).ready) 'Send deferred Chat/Work guard using newChat flag'
Assert (-not(Confirm-ChatGPTEntryMode $window 'unified' 'inspect' $true).ready) 'Inspect deferred Chat/Work guard'
Assert (-not(Confirm-ChatGPTEntryMode $window 'unified' 'stage' $false).ready) 'Stage without new chat deferred composer guard'
'MODE_CASES_PASSED=19'

# Proven Work-origin Stop may keep running only while preparing a NEW ChatGPT chat.
Assert (Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true '_ComposerLayoutBody_5est5_2' 1 0 0) 'Live Work composer signature did not permit new ChatGPT chat'
Assert (-not(Test-WorkStopCanRemainRunning 'Codex' 'stage' $true '_ComposerLayoutBody_5est5_2' 1 0 0)) 'Codex mode bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'unknown' 'stage' $true '_ComposerLayoutBody_5est5_2' 1 0 0)) 'Unknown mode bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'send' $true '_ComposerLayoutBody_5est5_2' 1 0 0)) 'Send bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'inspect' $true '_ComposerLayoutBody_5est5_2' 1 0 0)) 'Inspect bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $false '_ComposerLayoutBody_5est5_2' 1 0 0)) 'Existing-chat stage bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true 'unverified-parent' 1 0 0)) 'Unknown composer bypassed busy guard'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true '_ComposerLayoutBody_5est5_2' 0 0 0)) 'Chat Stop without Work permission proof was ignored'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true '_ComposerLayoutBody_5est5_2' 2 0 0)) 'Ambiguous Work signature was accepted'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true '_ComposerLayoutBody_5est5_2' 1 1 0)) 'Chat model control Stop was ignored'
Assert (-not(Test-WorkStopCanRemainRunning 'ChatGPT' 'stage' $true '_ComposerLayoutBody_5est5_2' 1 0 1)) 'Chat composer Stop was ignored'
'WORK_STOP_ORIGIN_CASES_PASSED=11'

# Foreground fixtures inject every OS action; no actual window is read or activated.
function Reset-ForegroundFixture {
 $script:foreground=[IntPtr]7;$script:activations=0;$script:polls=0
 $script:focusWindow=[pscustomobject]@{Current=[pscustomobject]@{NativeWindowHandle=42}}
 $script:readFocus={ $script:foreground }
 $script:activateFocus={param($Target) $script:activations++}
 $script:waitFocus={ $script:polls++ }
}
Reset-ForegroundFixture;$script:foreground=[IntPtr]42
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert ($r.ready -and $activations -eq 0 -and $polls -eq 0) 'Already active window was touched'
Reset-ForegroundFixture
$r=Confirm-ChatGPTForeground $focusWindow 'send' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $activations -eq 0 -and $polls -eq 0) 'Send activated a window'
Reset-ForegroundFixture
$r=Confirm-ChatGPTForeground $focusWindow 'inspect' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $activations -eq 0) 'Inspect activated a window'
Reset-ForegroundFixture
$waitFocus={ $script:polls++;if($script:polls -eq 3){$script:foreground=[IntPtr]42} }
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert ($r.ready -and $r.attempts -eq 1 -and $polls -eq 3) 'Asynchronous activation was rejected at 150ms'
Reset-ForegroundFixture
$activateFocus={param($Target) $script:activations++;if($script:activations -eq 2){$script:foreground=[IntPtr]42}}
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert ($r.ready -and $r.attempts -eq 2) 'Second activation could not recover'
Reset-ForegroundFixture
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $r.attempts -eq 3 -and $polls -eq 12) 'Foreground retry was unbounded or inferred success'
Reset-ForegroundFixture
$activateFocus={param($Target) $script:activations++;throw 'PRIVATE_EXCEPTION_MESSAGE'}
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $r.attempts -eq 3 -and ($r.errors -join ',') -notmatch 'PRIVATE') 'Activation exception escaped or leaked'
Reset-ForegroundFixture
$waitFocus={ $script:polls++;$script:focusWindow.Current.NativeWindowHandle=99;$script:foreground=[IntPtr]42 }
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $activations -eq 1) 'Stale target handle was trusted'
Reset-ForegroundFixture;$focusWindow.Current.NativeWindowHandle=0
$r=Confirm-ChatGPTForeground $focusWindow 'stage' $readFocus $activateFocus $waitFocus
Assert (-not $r.ready -and $activations -eq 0) 'Invalid window was activated'
'FOREGROUND_FIXTURE_CASES_PASSED=9'

function New-ThumbFixture([double]$center,[switch]$allWhite){
 $b=New-Object System.Drawing.Bitmap 582,76;$g=[Drawing.Graphics]::FromImage($b)
 try {$g.Clear([Drawing.Color]::FromArgb(45,45,45));if($allWhite){$g.Clear([Drawing.Color]::White)}else{$g.FillEllipse([Drawing.Brushes]::White,[single]($center-32),[single]6,[single]64,[single]64)}}finally{$g.Dispose()}
 return $b
}
foreach($case in @(@(530,$true),@(510,$false),@(291,$false),@(52,$false))){$b=New-ThumbFixture $case[0];try{Assert ((Test-ProThumbAtMaximum $b) -eq $case[1]) 'Slider position classified incorrectly'}finally{$b.Dispose()}}
$b=New-ThumbFixture 530 -allWhite;try{Assert (-not(Test-ProThumbAtMaximum $b)) 'White overlay accepted as slider thumb'}finally{$b.Dispose()}
'PRO_SLIDER_VISUAL_CASES_PASSED=5'

Assert (Test-WindowsMentionSnapshot 'GPTBridge Windows' 'ControlType.Group' 'editor1' 'editor1' @('rounded-2xs _Icon_x _SlotSizedIcon_x')) 'Structured Windows token rejected'
Assert (-not(Test-WindowsMentionSnapshot 'GPTBridge Windows' 'ControlType.Edit' 'editor1' 'editor1' @())) 'Plain text trusted as plugin token'
Assert (-not(Test-WindowsMentionSnapshot 'GPTBridge Mac' 'ControlType.Group' 'editor1' 'editor1' @('_SlotSizedIcon_x'))) 'Mac token accepted'
Assert (-not(Test-WindowsMentionSnapshot 'GPTBridge Windows' 'ControlType.Group' 'history' 'editor1' @('_SlotSizedIcon_x'))) 'Historical plugin text accepted'
Assert (-not(Test-WindowsMentionSnapshot 'GPTBridge Windows' 'ControlType.Group' 'editor1' 'editor1' @())) 'Missing token icon accepted'
'MENTION_CASES_PASSED=5'
$inputStart=$source.IndexOf('# BEGIN UNICODE TEXT INPUT');$inputEnd=$source.IndexOf('# END UNICODE TEXT INPUT')
if($inputStart -lt 0 -or $inputEnd -le $inputStart){throw 'Missing Unicode text helper'}
Invoke-Expression $source.Substring($inputStart,$inputEnd-$inputStart)
$events=[ChatGPTTextInput]::BuildUnicodeInputs("中文`nTask")
Assert ($events.Length -eq 14) 'Unicode text events lost characters'
for($i=0;$i -lt $events.Length;$i++){Assert ($events[$i].type -eq 1 -and $events[$i].data.key.vk -eq 0) 'Text input emitted a virtual Return or other key';Assert ($events[$i].data.key.flags -eq $(if($i%2){6}else{4})) 'Unexpected text input flags'}
Assert ([Runtime.InteropServices.Marshal]::SizeOf([type][ChatGPTTextInput+INPUT]) -eq 40) 'Windows x64 INPUT ABI mismatch'
'UNICODE_PACKET_TEST_PASSED=1; SENDINPUT_NOT_CALLED'

# Exercise the exact range endpoint namespace against the real Windows assembly.
# Empty text means no Unicode input is sent; window/selection are offline fixtures.
$script:diagnostics=@{}
$script:rangeMoves=0;$script:rangeSelections=0
$script:range=[pscustomobject]@{}
$script:range|Add-Member ScriptMethod Clone {return $script:range}
$script:range|Add-Member ScriptMethod MoveEndpointByRange {param([System.Windows.Automation.Text.TextPatternRangeEndpoint]$start,$other,[System.Windows.Automation.Text.TextPatternRangeEndpoint]$end)
 Assert ($start -eq [System.Windows.Automation.Text.TextPatternRangeEndpoint]::Start -and $end -eq [System.Windows.Automation.Text.TextPatternRangeEndpoint]::End) 'Wrong range endpoints'
 $script:rangeMoves++
}
$script:range|Add-Member ScriptMethod Select {$script:rangeSelections++}
$script:textPattern=[pscustomobject]@{DocumentRange=$script:range}
$script:composerFixture=[pscustomobject]@{Current=[pscustomobject]@{HasKeyboardFocus=$true}}
$script:composerFixture|Add-Member ScriptMethod SetFocus {throw 'Fixture focus unavailable'}
$script:composerFixture|Add-Member ScriptMethod TryGetCurrentPattern {param($pattern,$result);$result.Value=$script:textPattern;return $true}
Assert (Append-TaskAfterMention $null $composerFixture '') 'Correct TextPattern endpoint path failed'
Assert ($rangeMoves -eq 1 -and $rangeSelections -eq 1) 'Caret not selected at document end'
$composerFixture.Current.HasKeyboardFocus=$false
Assert (-not (Append-TaskAfterMention $null $composerFixture '')) 'Failed composer focus was accepted'
Write-Output 'MENTION_APPEND_RANGE_CASES_PASSED=3; SENDINPUT_NOT_CALLED'
