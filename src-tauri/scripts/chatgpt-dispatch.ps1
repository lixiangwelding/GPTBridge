param([Parameter(Mandatory=$true)][string]$RequestPath)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)
function Result($status,$detail,$ref='') { @{status=$status;detail=$detail;windowRef=$ref;requestId=$req.requestId}|ConvertTo-Json -Compress;exit 0 }
try {
  $req=Get-Content -LiteralPath $RequestPath -Raw -Encoding UTF8|ConvertFrom-Json
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
  $root=[System.Windows.Automation.AutomationElement]::RootElement
  $windows=@($root.FindAll([System.Windows.Automation.TreeScope]::Children,[System.Windows.Automation.Condition]::TrueCondition))
  $targets=@()
  foreach($w in $windows) {
    try {
      $proc=Get-Process -Id $w.Current.ProcessId
      if($req.client -eq 'desktop' -and $proc.ProcessName -eq 'ChatGPT'){$targets+=,$w}
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
  $window=$targets[0];$ref="$($window.Current.ProcessId):$($window.Current.NativeWindowHandle)"
  if($req.windowRef -and $req.windowRef -ne $ref){Result 'blocked' '目标窗口已变化，请重新检查并填入任务。' $ref}
  if($req.action -eq 'stage') {
    $busyButtons=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button))|Where-Object {$_.Current.Name -in @('Stop generating','Stop streaming','停止生成','停止响应') -and -not $_.Current.IsOffscreen})
    if($busyButtons.Count -gt 0){Result 'busy' 'ChatGPT 仍在处理上一条消息；未创建新聊天、未填入或发送。' $ref}
  }
  if($req.action -eq 'stage' -and $req.newChat) {
    $currentButtons=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button)))
    $new=@($currentButtons|Where-Object {$_.Current.Name -in @('New chat','新聊天','新建聊天') -and -not $_.Current.IsOffscreen})
    if($new.Count -ne 1){Result 'blocked' '无法核对唯一的新聊天入口，未填入或发送；请适配当前客户端名称。' $ref}
    $invoke=$null
    if(-not $new[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){Result 'blocked' '新聊天入口不可访问，没有发送。' $ref}
    $invoke.Invoke()
    Start-Sleep -Milliseconds 700
  }
  function Select-ExactOption($triggerNames,$optionName) {
    $controls=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
    $already=@($controls|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in @($optionName,"ChatGPT $optionName") -and -not $_.Current.IsOffscreen})
    if($already.Count -eq 1){return}
    $triggers=@($controls|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -in $triggerNames -and -not $_.Current.IsOffscreen})
    if($triggers.Count -ne 1){return}
    $invoke=$null
    if(-not $triggers[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){return}
    $invoke.Invoke();Start-Sleep -Milliseconds 250
    $items=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.Name -eq $optionName -and -not $_.Current.IsOffscreen -and $_.Current.ControlType -in @([System.Windows.Automation.ControlType]::MenuItem,[System.Windows.Automation.ControlType]::Button,[System.Windows.Automation.ControlType]::CheckBox)})
    if($items.Count -ne 1){return}
    $selection=$null;$toggle=$null;$invoke=$null
    if($items[0].TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selection)){$selection.Select()}
    elseif($items[0].TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$toggle)){if($toggle.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On){$toggle.Toggle()}}
    elseif($items[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){$invoke.Invoke()}
    Start-Sleep -Milliseconds 250
  }
  if($req.action -eq 'stage') {
    Select-ExactOption @('Model selector','Choose model','选择模型','模型选择器') $req.model
    Select-ExactOption @('Thinking effort','思考强度','思考时长') $req.effort
    $attached=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {$_.Current.Name -in @('Remove GPTBridge Windows','移除 GPTBridge Windows','删除 GPTBridge Windows') -and -not $_.Current.IsOffscreen})
    if($attached.Count -ne 1){Select-ExactOption @('Tools','工具','Add files and more','添加文件和更多') 'GPTBridge Windows'}
  }
  $all=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition))
  $buttons=@($all|Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and -not $_.Current.IsOffscreen})
  $model=@($buttons|Where-Object {$_.Current.Name -in @($req.model,"ChatGPT $($req.model)","模型，$($req.model)","Model, $($req.model)")})
  $effort=@($buttons|Where-Object {$_.Current.Name -eq $req.effort})
  # Only an attached removable chip counts, never a mention in old chat text or a picker result.
  $bridge=@($buttons|Where-Object {$_.Current.Name -in @('Remove GPTBridge Windows','移除 GPTBridge Windows','删除 GPTBridge Windows')})
  if($model.Count -ne 1 -or $effort.Count -ne 1 -or $bridge.Count -ne 1){Result 'blocked' '尚未核对到唯一的模型、思考强度和已附加的 GPTBridge Windows。请在 ChatGPT 中手动选择；不同客户端可访问性名称需另行适配。' $ref}
  $composers=@($all|Where-Object { -not $_.Current.IsOffscreen -and $_.Current.IsEnabled -and $_.Current.ControlType -in @([System.Windows.Automation.ControlType]::Edit,[System.Windows.Automation.ControlType]::Document) -and $_.Current.Name -in @('Message ChatGPT','Ask anything','向 ChatGPT 发送消息','发送消息','询问任何问题','提示词','Prompt') })
  if($composers.Count -ne 1){Result 'blocked' '没有唯一可访问的输入框，请先打开一个新的空白聊天。' $ref}
  $composer=$composers[0];$value=$null
  if(-not $composer.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$value) -or $value.Current.IsReadOnly){Result 'blocked' '客户端输入框未提供可核对的 ValuePattern，已停止；不会用坐标或盲打发送。' $ref}
  if($req.action -eq 'inspect'){Result 'ready' '已核对窗口、模型、思考强度、GPTBridge Windows 和输入框；尚未填入或发送。' $ref}
  if($req.action -eq 'stage') {
    if($value.Current.Value.Trim()){Result 'blocked' '输入框已有内容，请先在 ChatGPT 中处理；不会覆盖未发送草稿。' $ref}
    $value.SetValue([string]$req.prompt)
    if($value.Current.Value -cne $req.prompt){Result 'blocked' '填入后内容回读不一致，没有发送。' $ref}
    Result 'prepared' '任务已填入并逐字回读，尚未发送。请检查客户端内容后点击发送。' $ref
  }
  if($req.action -ne 'send'){Result 'blocked' '不支持的操作。' $ref}
  if($value.Current.Value -cne $req.prompt){Result 'blocked' '输入内容已变化，请重新核对；没有发送。' $ref}
  $send=@($buttons|Where-Object {$_.Current.Name -in @('Send prompt','发送提示','发送消息','发送') -and $_.Current.IsEnabled})
  if($send.Count -ne 1){Result 'blocked' '没有唯一可用的发送按钮，没有发送。' $ref}
  $pattern=$null
  if(-not $send[0].TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)){Result 'blocked' '发送按钮不支持可访问性调用，没有发送。' $ref}
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
