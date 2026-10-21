param([int64]$Handle, [string]$Name, [string]$Action = "invoke", [string]$Type = "", [string]$Text = "", [int]$Nth = 0)
# uia.ps1 -Handle <hwnd> -Name "Share" [-Action invoke|select|expand|set|list|scrollto|scroll] [-Type Button|TabItem|...] [-Text value] [-Nth n]
# drives a window through UI Automation patterns (no mouse, works on windows of other virtual desktops).
# `list` prints every named element whose name matches the regex -Name.
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Handle)
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$hits = @()
foreach ($e in $all) {
  $n = $e.Current.Name
  if (-not $n) { continue }
  $t = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  if ($Type -and $t -ne $Type) { continue }
  if ($Action -eq "list") { if ($n -match $Name) { $b = $e.Current.BoundingRectangle; "{0}`t{1}`t{2},{3} {4}x{5}" -f $t, $n, [int]$b.X, [int]$b.Y, [int]$b.Width, [int]$b.Height } }
  elseif ($n -eq $Name) { $hits += $e }
}
if ($Action -eq "list") { return }
if ($hits.Count -le $Nth) { Write-Error "not found: $Name"; exit 1 }
$e = $hits[$Nth]
switch ($Action) {
  "invoke" { try { $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() } catch { try { $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select() } catch { $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle() } } }
  "select" { $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select() }
  "expand" { $e.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand() }
  "set" { $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Text) }
  # scrollto: bring a virtualised list row into view (ScrollItemPattern) so it can be selected afterwards
  "scrollto" { $e.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView() }
  # scroll: -Text "down|up|pagedown|pageup" scrolls the nearest ancestor that supports ScrollPattern
  "scroll" {
    $p = $e
    $w = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    while ($p -and -not $p.GetSupportedPatterns().Contains([System.Windows.Automation.ScrollPattern]::Pattern)) { $p = $w.GetParent($p) }
    if (-not $p) { Write-Error "no scrollable ancestor of $Name"; exit 1 }
    $sp = $p.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
    $amt = switch ($Text) { "up" { [System.Windows.Automation.ScrollAmount]::SmallDecrement } "pageup" { [System.Windows.Automation.ScrollAmount]::LargeDecrement } "pagedown" { [System.Windows.Automation.ScrollAmount]::LargeIncrement } default { [System.Windows.Automation.ScrollAmount]::SmallIncrement } }
    $sp.Scroll([System.Windows.Automation.ScrollAmount]::NoAmount, $amt)
  }
}
"ok $Name"
