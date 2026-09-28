<#
.SYNOPSIS
  hotbar - a floating, always-on-top Windows bar built with PowerShell and WPF.

.DESCRIPTION
  A half-moon shaped widget that lives above every window (including the terminal),
  pinned to the right edge of a monitor and centred vertically. macOS-dock-like:
  a vertical column of round glyph buttons, dark glass, gold on hover, no frame, no
  taskbar entry. Drag it with the mouse to move it to another monitor: on release
  it snaps to the right edge of the monitor under its centre and remembers it in
  config.json for the next launch.

  Everything visible comes from hotbar/config.json. Items are generated at startup
  from a loop, so adding or reordering an entry is a JSON edit, never a code change.
  Each item carries an `action`:

    none              do nothing (the tooltip explains how to wire it up)
    omniroute-status  toggle the inline OmniRoute panel (UP/DOWN + combos)
    agent-usage       toggle the inline panel with the live claude/codex/opencode
                      session usage, refreshed every few seconds while it is open
    run:<command>     run a command windowlessly
    edit-config       open config.json in the editor

  Design notes that are not obvious from the code:

  * STA is required before any window exists. WPF refuses to create a window on an
    MTA thread, and the apartment state of the current thread cannot be changed from
    inside it. If this script is not already on an STA thread it re-launches itself
    with -STA once, and forwards the child's exit code.
  * The XAML is a single-quoted here-string. A double-quoted one would let
    PowerShell expand `$` inside the markup and corrupt it.
  * No glyph is ever typed literally into this file. The .ps1 files are pure ASCII
    so the bytes survive any editor, diff and code page; glyphs come from config as
    0xNNNN strings converted with [char], and the two chevrons baked into the XAML
    use XML numeric entities (&#x2039; / &#x203A;).
  * The chevrons point where the motion goes, not at the panel: collapse shrinks
    the bar toward the screen edge (handle shows &#x203A;), expand grows it back
    into the desktop (tab shows &#x2039;). Flipping them "to match the panel" is a
    UX regression, not a fix.
  * Dragging is a window-level MouseLeftButtonDown handler. Buttons mark that
    routed event as handled, so only empty bar space drags; a click on a button
    stays a click. On release, Snap-HotbarToActiveScreen latches the right edge to
    the monitor under the window centre and persists the device name to config.
  * The two panels have different refresh contracts. The OmniRoute panel is a
    snapshot: it is read when it opens and then left alone. The usage panel is
    live, because a session balance that only updates on click is not a balance.
    Its DispatcherTimer is owned by Set-HotbarPanelLines, the single choke point
    every panel write goes through, so a timer can never outlive its panel.
  * Screen geometry is converted from physical pixels to device-independent units
    before it reaches Window.Left/Top. WinForms reports pixels and WPF positions in
    DIUs, so on a scaled display the two disagree by exactly the scale factor. The
    drag-snap round-trips the centre through the scale the same way.
  * The data readers live in hotbar/lib as dot-sourced copies, so the widget is
    standalone: it never reaches into the Herdr plugin's script tree.

.PARAMETER SelfTest
  Validate the widget without a human: parses the XAML, loads the config, computes
  both positions, exercises the inline panel data read, opens the window for
  SelfTestMs (400 ms by default) and closes it again. Prints HOTBAR_SELFTEST PASS
  and exits 0, or prints the failures and exits 1. It always terminates: the
  window is closed by a DispatcherTimer, with a background watchdog behind it in
  case the timer never fires. SelfTest deliberately skips the single-instance
  check, so it can always be run while a live bar is on screen.

.EXAMPLE
  .\hotbar.ps1
  Runs the bar. Right-click it for the menu, Escape also quits.

.EXAMPLE
  .\hotbar.ps1 -SelfTest
  Headless-ish verification. Flashes a window for under a second.
#>
[CmdletBinding()]
param(
  [switch]$SelfTest,
  # 400 ms, not the 700 ms this used to default to. The whole check is measured to
  # finish in ~1.8 s and the window is the largest single item in it: the other
  # phases together (process start and WPF 318 ms, the omniroute netstat+sqlite
  # probe 254 ms, the usage read 468 ms, rendering 162 ms) come to ~1.2 s, so a
  # 700 ms wait cannot fit in a 2 s ceiling on this machine - it lands at 2.1 s.
  # The window is pure wait; the geometry, glyph and panel-width assertions are
  # made from data, not from a human watching it. Pass -SelfTestMs to hold it
  # open longer and watch it yourself.
  [int]$SelfTestMs = 400
)

$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Constants. Sizes are device-independent units (what WPF calls a DIP); only the
# screen rectangle is converted from pixels (see Get-HotbarDpiScale).
# ---------------------------------------------------------------------------
$script:HotbarMutexName = "Local\herdr.hotbar.widget.v1"
$script:BarWidth = 72
$script:BarHeight = 400
$script:PanelWidth = 320
$script:CollapsedSize = 46
$script:DefaultMargin = 8
$script:MaxPanelChars = 38
$script:UsageRefreshSeconds = 5
# Per-agent history panel: how many of the agent's own top-spending projects
# this month are rendered as rows. Local-only replacement for the old
# Herdr-open-projects filter (O2): ranks by output tokens instead of asking an
# external program which projects are "open".
$script:AgentHistoryMaxProjectsShown = 5
$script:GlyphFont = "Segoe UI Symbol, Segoe MDL2 Assets, Segoe UI"
$script:PanelFont = "Consolas, Courier New"
$script:PanelFontSize = 10.0

# Colours. Dark glass body, gold accent on hover, honest red/green for the gateway.
$script:ColorText = "#C8C8D4"
$script:ColorDim = "#8A8A96"
$script:ColorGold = "#E8C46A"
$script:ColorUp = "#6FCF6F"
$script:ColorDown = "#E06C6C"
$script:ColorWarn = "#E8B04A"

# The hotbar folder's parent is the repository root, which is the default working
# directory for a `run:` action. Paths are combined with [System.IO.Path]::Combine
# on purpose: the first Join-Path in a process autoloads
# Microsoft.PowerShell.Management, which costs about 95 ms before the window even
# appears. Do not "tidy" these back into Join-Path.
$script:HotbarRoot = $PSScriptRoot
$script:RepoRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, ".."))
$script:ConfigPath = [System.IO.Path]::Combine($PSScriptRoot, "config.json")

# Actions the widget understands. Anything else is treated as a no-op: an
# unrecognised action must never look like a successful one.
$script:KnownActions = @("none", "omniroute-status", "agent-usage", "edit-config")

# Live state, filled in by Read-HotbarConfig and New-HotbarWindow.
$script:Config = $null
$script:Margin = $script:DefaultMargin
$script:Collapsed = $false
$script:PanelOpen = $false
# UsagePanelActive is deliberately separate from PanelOpen: the panel is open
# either way, but only the usage panel owns a refresh timer. Set-HotbarPanelLines
# is the only writer of both, so the timer cannot outlive the panel it refreshes.
$script:UsagePanelActive = $false
$script:UsageTimer = $null
# Agent panels (agent-usage:<agent>) are snapshot-only: they never own a refresh
# timer. This flag just lets their toggle close the panel that is already open.
$script:AgentPanelActive = $false
$script:ActiveScreen = $null
$script:Window = $null

# ---------------------------------------------------------------------------
# Apartment state. Must happen before Add-Type PresentationFramework or any
# Window is created: WPF cannot create a window on an MTA thread, and a thread
# cannot change its own apartment state.
# ---------------------------------------------------------------------------
function Invoke-HotbarSelfRelaunchOnSta {
  $host32 = [System.IO.Path]::Combine($env:SystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe")
  if (-not [System.IO.File]::Exists($host32)) { $host32 = "powershell.exe" }

  $self = ('"{0}"' -f $PSCommandPath)
  $args = @("-NoProfile", "-STA", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File", $self)

  $child = Start-Process -FilePath $host32 -ArgumentList $args -WindowStyle Hidden -PassThru -Wait
  if ($null -eq $child) { return 1 }
  return $child.ExitCode
}

if ([System.Threading.Thread]::CurrentThread.ApartmentState -ne [System.Threading.ApartmentState]::STA) {
  if ($SelfTest) {
    Write-Output ("HOTBAR_SELFTEST FAIL: current thread is {0}; WPF needs STA. Run with: powershell -STA -File hotbar.ps1 -SelfTest" -f [System.Threading.Thread]::CurrentThread.ApartmentState)
    exit 1
  }
  # Re-launch once under -STA and behave exactly like that process.
  exit (Invoke-HotbarSelfRelaunchOnSta)
}

# ---------------------------------------------------------------------------
# Assemblies. PresentationFramework/PresentationCore/WindowsBase are WPF proper;
# System.Windows.Forms is used for one thing only - Screen.WorkingArea, which is
# the reliable way to ask for the usable area of a monitor - and System.Drawing for
# the DPI probe. No NuGet, no packages, nothing to install.
# ---------------------------------------------------------------------------
foreach ($assembly in @("PresentationFramework", "PresentationCore", "WindowsBase", "System.Windows.Forms", "System.Drawing")) {
  Add-Type -AssemblyName $assembly
}

# The data readers are dot-sourced at SCRIPT scope on purpose. Read-SqliteQuery is
# only checked for existence here: Get-AgentUsage dot-sources it itself, and
# loading it twice would re-parse 13 KB for nothing. These four are the core: the
# bar must run without them, so a missing one is fatal.
$script:LibRoot = [System.IO.Path]::Combine($PSScriptRoot, "lib")
foreach ($needed in @("Invoke-Native.ps1", "Read-SqliteQuery.ps1", "Get-AgentUsage.ps1", "Get-AgentPricing.ps1")) {
  $path = [System.IO.Path]::Combine($script:LibRoot, $needed)
  if (-not [System.IO.File]::Exists($path)) {
    Write-Output ("hotbar: missing data helper " + $path)
    exit 1
  }
}
. ([System.IO.Path]::Combine($script:LibRoot, "Invoke-Native.ps1"))
. ([System.IO.Path]::Combine($script:LibRoot, "Get-AgentUsage.ps1"))
# Pricing is dot-sourced explicitly (Get-AgentUsage uses it). It only defines
# constants and functions, so loading it twice would only re-parse for nothing.
. ([System.IO.Path]::Combine($script:LibRoot, "Get-AgentPricing.ps1"))

# OmniRoute is an optional extension (odd/orbitbar-rebrand O3): its readers live
# under extensions/omniroute/hotbar/, not in hotbar/lib. Load them only when
# present, so a clone without the extension still runs the bar; the
# omniroute-status item then renders "extension not installed" (see
# Get-HotbarOmniRouteSnapshot) instead of failing on an undefined function.
$script:RepoRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, ".."))
$script:OmniRouteExtensionLib = [System.IO.Path]::Combine($script:RepoRoot, "extensions", "omniroute", "hotbar")
$omniRouteStatusPath = [System.IO.Path]::Combine($script:OmniRouteExtensionLib, "Get-OmniRouteStatus.ps1")
$omniRouteCombosPath = [System.IO.Path]::Combine($script:OmniRouteExtensionLib, "Get-OmniRouteCombos.ps1")
$script:OmniRouteExtensionAvailable = ([System.IO.File]::Exists($omniRouteStatusPath) -and [System.IO.File]::Exists($omniRouteCombosPath))
if ($script:OmniRouteExtensionAvailable) {
  . $omniRouteStatusPath
  . $omniRouteCombosPath
}

# ---------------------------------------------------------------------------
# The window markup. Single-quoted here-string: no PowerShell expansion touches it.
# x:Class is deliberately absent - this is loose XAML parsed by XamlReader, which has
# no code-behind to resolve a class against.
# ---------------------------------------------------------------------------
$script:HotbarXaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="hotbar"
        Width="72"
        Height="400"
        WindowStartupLocation="Manual"
        WindowStyle="None"
        AllowsTransparency="True"
        Background="Transparent"
        ResizeMode="NoResize"
        ShowInTaskbar="False"
        ShowActivated="False"
        Topmost="True">
  <Window.Resources>
    <LinearGradientBrush x:Key="BarFill" StartPoint="0,0" EndPoint="0,1">
      <GradientStop Offset="0.0" Color="#232329" />
      <GradientStop Offset="1.0" Color="#101014" />
    </LinearGradientBrush>
    <SolidColorBrush x:Key="BarStroke" Color="#3A3A44" />

    <Style x:Key="ItemButton" TargetType="Button">
      <Setter Property="Background" Value="Transparent" />
      <Setter Property="Foreground" Value="#C8C8D4" />
      <!-- Sized against the crescent, not by eye. The bar is a half-ellipse
           72x400, so the usable width at row y is
           72 * sqrt(1 - ((y-200)/200)^2). The item column is right-aligned to
           the flat edge, so its left edge must stay right of that curve: the
           content spans dy 68..332, where the curve leaves 54 px, and the
           column is 44 px. That is the 10 px of headroom that keeps the outer
           items from being sliced by the bulge. -->
      <Setter Property="Width" Value="44" />
      <Setter Property="Height" Value="44" />
      <Setter Property="Margin" Value="0,2,0,2" />
      <Setter Property="Cursor" Value="Hand" />
      <Setter Property="Focusable" Value="False" />
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Grid>
              <Border x:Name="Glow" Background="{TemplateBinding Background}" CornerRadius="22" />
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center" />
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
      <Style.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Setter Property="Background" Value="#3A3322" />
          <Setter Property="Foreground" Value="#E8C46A" />
        </Trigger>
        <Trigger Property="IsPressed" Value="True">
          <Setter Property="Background" Value="#4A3F22" />
        </Trigger>
      </Style.Triggers>
    </Style>

    <Style x:Key="MiniButton" TargetType="Button">
      <Setter Property="Background" Value="Transparent" />
      <Setter Property="Foreground" Value="#8A8A96" />
      <Setter Property="Cursor" Value="Hand" />
      <Setter Property="Focusable" Value="False" />
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Grid>
              <Border x:Name="Glow" Background="{TemplateBinding Background}" CornerRadius="8" />
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center" />
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
      <Style.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Setter Property="Background" Value="#3A3322" />
          <Setter Property="Foreground" Value="#E8C46A" />
        </Trigger>
      </Style.Triggers>
    </Style>

    <Style x:Key="TabButton" TargetType="Button">
      <Setter Property="Background" Value="Transparent" />
      <Setter Property="Foreground" Value="#C8C8D4" />
      <Setter Property="Cursor" Value="Hand" />
      <Setter Property="Focusable" Value="False" />
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Grid>
              <Border x:Name="Glow" Background="{TemplateBinding Background}" CornerRadius="22" />
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center" />
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
      <Style.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Setter Property="Background" Value="#3A3322" />
          <Setter Property="Foreground" Value="#E8C46A" />
        </Trigger>
      </Style.Triggers>
    </Style>
  </Window.Resources>

  <Grid>
    <!-- EXPANDED: the crescent bar, plus the inline panel that opens to its left.
         No Width here on purpose: this Border wraps BOTH the panel and the bar,
         so pinning it to 72 would constrain the panel to nothing and centre the
         whole thing inside the wider panel-open window. The bar's width comes
         from the last Grid column instead, and the window width comes from
         Get-HotbarGeometry. -->
    <Border x:Name="BarBorder"
            Background="{StaticResource BarFill}"
            BorderBrush="{StaticResource BarStroke}"
            BorderThickness="1"
            CornerRadius="200,0,0,200">
      <Border.Effect>
        <DropShadowEffect BlurRadius="6" ShadowDepth="2" Direction="270" Opacity="0.65" Color="#FF000000" />
      </Border.Effect>
      <Grid>
        <Grid.ColumnDefinitions>
          <ColumnDefinition Width="Auto" />
          <ColumnDefinition Width="72" />
        </Grid.ColumnDefinitions>

        <!-- The bar is the RIGHT column on purpose. The window keeps its right
             edge pinned to the screen and shifts left by the panel width when the
             panel opens, so the bar must sit in the last column or the crescent
             would slide off the screen edge with the panel. -->
        <StackPanel Grid.Column="1"
                    VerticalAlignment="Center"
                    HorizontalAlignment="Right"
                    Margin="0,0,3,0">
          <Button x:Name="CollapseHandle"
                  Style="{StaticResource MiniButton}"
                  Width="56"
                  Height="36"
                  Margin="0,0,0,8"
                  ToolTip="Collapse the bar">
            <!-- The chevron points where the motion goes: collapsing shrinks the
                 bar toward the screen edge, so the handle points RIGHT. -->
            <TextBlock Text="&#x203A;" FontSize="24" FontFamily="Segoe UI, Arial" />
          </Button>
          <StackPanel x:Name="ItemsPanel" />
        </StackPanel>

        <Border x:Name="PanelBorder"
                Grid.Column="0"
                Width="320"
                Visibility="Collapsed"
                Background="{StaticResource BarFill}"
                BorderBrush="{StaticResource BarStroke}"
                BorderThickness="0,1,0,1">
          <ScrollViewer VerticalScrollBarVisibility="Auto" HorizontalScrollBarVisibility="Disabled" Padding="12,10">
            <StackPanel x:Name="PanelStack" />
          </ScrollViewer>
        </Border>
      </Grid>
    </Border>

    <!-- COLLAPSED: a single semicircular tab with a chevron. -->
    <Border x:Name="TabBorder"
            Width="46"
            Height="46"
            Visibility="Collapsed"
            Background="{StaticResource BarFill}"
            BorderBrush="{StaticResource BarStroke}"
            BorderThickness="1"
            CornerRadius="23,0,0,23">
      <Border.Effect>
        <DropShadowEffect BlurRadius="6" ShadowDepth="2" Direction="270" Opacity="0.65" Color="#FF000000" />
      </Border.Effect>
      <Button x:Name="ExpandButton"
              Style="{StaticResource TabButton}"
              ToolTip="Expand the bar">
        <!-- Inverse of the collapse handle: expanding grows the bar LEFT into
             the desktop, so the chevron points LEFT. -->
        <TextBlock Text="&#x2039;" FontSize="16" FontFamily="Segoe UI, Arial" />
      </Button>
    </Border>
  </Grid>
</Window>
'@

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
function Read-HotbarConfig {
  [CmdletBinding()]
  param([string]$Path = $script:ConfigPath)

  if (-not [System.IO.File]::Exists($Path)) {
    # config.json is gitignored (it holds the local monitor pick); a fresh
    # checkout only has the example. Seed the local file from it so the
    # first launch works without any manual setup.
    $examplePath = [System.IO.Path]::Combine([System.IO.Path]::GetDirectoryName($Path), "config.example.json")
    if ([System.IO.File]::Exists($examplePath)) {
      [System.IO.File]::Copy($examplePath, $Path)
    } else {
      throw ("config not found: " + $Path)
    }
  }
  $raw = [System.IO.File]::ReadAllText($Path)
  $config = $raw | ConvertFrom-Json
  if ($null -eq $config) { throw ("config is not valid JSON: " + $Path) }
  return $config
}

# "0x2733" -> the character U+2733. The 0x prefix is optional and a code point
# above the BMP is rejected rather than silently truncated, because [char] would
# wrap it into an unrelated glyph and the bar would show nonsense.
function ConvertFrom-HotbarGlyph {
  [CmdletBinding()]
  param([string]$Code)

  $text = ([string]$Code).Trim()
  if (-not $text) { return "" }
  if ($text.StartsWith("0x") -or $text.StartsWith("0X")) { $text = $text.Substring(2) }
  if ($text.StartsWith("#")) { $text = $text.Substring(1) }

  $value = 0
  if (-not [int]::TryParse($text, [System.Globalization.NumberStyles]::HexNumber, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$value)) {
    throw ("glyph is not a hex code point: " + $Code)
  }
  if ($value -lt 0 -or $value -gt 0xFFFF) {
    throw ("glyph outside the BMP (surrogate pairs are not supported): " + $Code)
  }
  return [string][char]$value
}

<#
.SYNOPSIS
  The monitor the bar lives on. "primary" (or an absent field) means the primary
  display; any other value is matched against the WinForms display device name
  (for example "\\.\DISPLAY2"), which is what Set-HotbarPersistedMonitor writes
  back after the bar is dragged to another monitor.
#>
function Get-HotbarScreenFromConfig {
  param($Config)

  $screen = [System.Windows.Forms.Screen]::PrimaryScreen
  $configured = ""
  if ($null -ne $Config -and $null -ne $Config.monitor) { $configured = ([string]$Config.monitor).Trim() }
  if ($configured -and $configured -ne "primary") {
    foreach ($candidate in [System.Windows.Forms.Screen]::AllScreens) {
      if ([string]$candidate.DeviceName -eq $configured) { $screen = $candidate; break }
    }
  }
  return $screen
}

# The widget starts collapsed when the config says so, so the first frame after a
# restart is the same every time instead of depending on what was on screen.
function Initialize-HotbarStateFromConfig {
  $config = Read-HotbarConfig
  $script:Config = $config

  $margin = $script:DefaultMargin
  if ($null -ne $config.margin) {
    $parsed = 0
    if ([int]::TryParse(([string]$config.margin), [ref]$parsed) -and $parsed -ge 0) { $margin = $parsed }
  }
  $script:Margin = $margin

  $script:ActiveScreen = Get-HotbarScreenFromConfig -Config $config

  $collapsed = $false
  if ($null -ne $config.collapsed) { $collapsed = [bool]$config.collapsed }
  $script:Collapsed = $collapsed
  $script:PanelOpen = $false

  return $config
}

# ---------------------------------------------------------------------------
# Geometry
# ---------------------------------------------------------------------------

# Physical pixels per device-independent unit. WinForms reports Screen geometry in
# physical pixels; WPF positions and sizes in DIUs. powershell.exe is system-DPI
# aware, so the process-wide GDI DPI is the scale both views must agree on. The
# screen DC is read directly with System.Drawing because GetDpiForMonitor would need
# an Add-Type compile, and this is on the path between the click and the window.
function Get-HotbarDpiScale {
  try {
    $device = [System.Drawing.Graphics]::FromHwnd([IntPtr]::Zero)
    $dpiX = $device.DpiX
    $device.Dispose()
    if ($dpiX -gt 0) { return [Math]::Round(($dpiX / 96.0), 4) }
  } catch {
    # No GDI+ surface: assume 100% and place the bar with raw pixels.
  }
  return 1.0
}

<#
.SYNOPSIS
  Where the window goes, in device-independent units.

.DESCRIPTION
  Expanded: x = wa.Right - 72 - margin, y = wa.Top + (wa.Height - 400) / 2.
  Collapsed: x = wa.Right - 46 - margin, same vertical centre.

  With the inline panel open the window grows by the panel width and the LEFT edge
  moves instead, so the bar itself never shifts: the right edge stays pinned to the
  margin. The screen is the active monitor by default: "primary" (the default in
  config.json), or the monitor the bar was dragged to last time (persisted as its
  display device name), or the monitor it was dragged to in this session.
#>
function Get-HotbarGeometry {
  [CmdletBinding()]
  param(
    [bool]$Collapsed,
    [bool]$PanelOpen,
    $Screen = $null
  )

  if ($null -eq $Screen) { $Screen = $script:ActiveScreen }
  if ($null -eq $Screen) { $Screen = [System.Windows.Forms.Screen]::PrimaryScreen }
  $working = $Screen.WorkingArea
  $scale = Get-HotbarDpiScale

  $right = $working.Right / $scale
  $top = $working.Top / $scale
  $height = $working.Height / $scale

  if ($Collapsed) {
    $width = $script:CollapsedSize
    $boxHeight = $script:CollapsedSize
  } else {
    $width = $script:BarWidth
    if ($PanelOpen) { $width += $script:PanelWidth }
    $boxHeight = $script:BarHeight
  }

  return [pscustomobject]@{
    X           = [Math]::Round(($right - $width - $script:Margin), 0)
    Y           = [Math]::Round(($top + (($height - $boxHeight) / 2)), 0)
    Width       = $width
    Height      = $boxHeight
    Scale       = $scale
    WorkingArea = ("{0}x{1}+{2}+{3}" -f $working.Width, $working.Height, $working.Left, $working.Top)
  }
}

function Update-HotbarGeometry {
  if ($null -eq $script:Window) { return }
  $geometry = Get-HotbarGeometry -Collapsed $script:Collapsed -PanelOpen $script:PanelOpen
  $script:Window.Left = $geometry.X
  $script:Window.Top = $geometry.Y
  $script:Window.Width = $geometry.Width
  $script:Window.Height = $geometry.Height
}

<#
.SYNOPSIS
  After a drag, re-latch the bar to the right edge of the monitor it now sits on.

.DESCRIPTION
  The window centre is turned back into a physical-pixel point (Left/Top are DIUs
  and Screen.FromPoint works in pixels, so the scale round-trip matters) and the
  monitor that owns that point becomes the active one. Changed monitors are
  persisted to config.json so a restart comes back to the same monitor; a failed
  write keeps the session placement and only forgets it on the next launch.
#>
function Snap-HotbarToActiveScreen {
  if ($null -eq $script:Window) { return }

  $scale = Get-HotbarDpiScale
  $centerX = $script:Window.Left + ($script:Window.Width / 2)
  $centerY = $script:Window.Top + ($script:Window.Height / 2)
  $point = New-Object System.Drawing.Point(
    [int][Math]::Round($centerX * $scale),
    [int][Math]::Round($centerY * $scale))

  $screen = [System.Windows.Forms.Screen]::FromPoint($point)
  if ($null -ne $screen -and $null -ne $script:ActiveScreen -and
      $screen.DeviceName -ne $script:ActiveScreen.DeviceName) {
    $script:ActiveScreen = $screen
    Set-HotbarPersistedMonitor -Screen $screen
  }
  Update-HotbarGeometry
}

# Writes the `monitor` field of config.json. The checked-in default is "primary";
# after a drag it becomes the display device name ("\\.\DISPLAY2" and friends),
# which is exactly what Get-HotbarScreenFromConfig matches on the next launch.
function Set-HotbarPersistedMonitor {
  param($Screen)

  try {
    $config = Read-HotbarConfig
    if ([string]$config.monitor -eq [string]$Screen.DeviceName) { return }
    $config.monitor = [string]$Screen.DeviceName
    $json = $config | ConvertTo-Json -Depth 6
    [System.IO.File]::WriteAllText($script:ConfigPath, $json, (New-Object System.Text.UTF8Encoding($false)))
  } catch {
    # Best effort only: the in-session placement above already stands.
  }
}

# ---------------------------------------------------------------------------
# Window construction
# ---------------------------------------------------------------------------
function Get-HotbarElement {
  param($Window, [string]$Name)
  $element = $Window.FindName($Name)
  if ($null -eq $element) { throw ("XAML element not found: " + $Name) }
  return $element
}

<#
.SYNOPSIS
  Clips the item column to the bar's crescent outline.

.DESCRIPTION
  WPF Border does not clip its children to its CornerRadius: the rounded corners
  are a paint, not a boundary, so anything the bar owns can spill past the bulge
  into the transparent window area. The hover fill on the outer buttons did, and
  a glow floating outside the crescent reads as a broken paint job.

  The clip geometry is the actual silhouette, not a decoration. The original
  sizing comment says the usable width at row y is
  72 * sqrt(1 - ((y-200)/200)^2): that is the RIGHT half of an ellipse with
  horizontal semi-axis 72 and vertical semi-axis 200, whose flat side is the
  bar's right edge at x=72. The clip path traces that exactly: the rectangle
  minus the arc from (0, h) back to (0, 0) through the bulge, swept clockwise
  (bottom -> right -> top on a clock face). Running on ItemStackPanel -- the bar
  column, always 72 wide regardless of the panel state -- keeps the inline panel
  untouched, because the panel lives in the Auto column inside the same Border.

  The call runs eagerly at construction AND on SizeChanged: the first paint happy
  path reads a zero-size element if the window measures later, and the handler
  catches the resize that layout produces.
#>
function Update-HotbarBarClip {
  [CmdletBinding()]
  param()

  if ($null -eq $script:Window) { return }
  $items = Get-HotbarElement -Window $script:Window -Name "ItemsPanel"
  $w = [double]$items.ActualWidth
  $h = [double]$items.ActualHeight
  if ($w -le 0 -or $h -le 0) { return }

  $geometry = [System.Windows.Media.PathGeometry]::new()
  $figure = [System.Windows.Media.PathFigure]::new()
  $figure.StartPoint = [System.Windows.Point]::new(0, 0)
  $figure.IsClosed = $true

  # ::new() everywhere, on purpose: New-Object nested inside another constructor's
  # parentheses is parsed as a command argument list and its output becomes an
  # Object[] that no Point/Size/LineSegment constructor can bind.
  $edgeNw = [System.Windows.Point]::new(0, 0)
  $edgeNe = [System.Windows.Point]::new($w, 0)
  $edgeSe = [System.Windows.Point]::new($w, $h)
  $edgeSw = [System.Windows.Point]::new(0, $h)
  $size = [System.Windows.Size]::new($w, ($h / 2))
  $top = [System.Windows.Media.LineSegment]::new($edgeNe, $false)
  $right = [System.Windows.Media.LineSegment]::new($edgeSe, $false)
  $bottom = [System.Windows.Media.LineSegment]::new($edgeSw, $false)
  $arc = [System.Windows.Media.ArcSegment]::new($edgeNw, $size, 0, $false,
    [System.Windows.Media.SweepDirection]::Clockwise, $false)

  $null = $figure.Segments.Add($top)
  $null = $figure.Segments.Add($right)
  $null = $figure.Segments.Add($bottom)
  $null = $figure.Segments.Add($arc)
  $null = $geometry.Figures.Add($figure)
  $geometry.Freeze()
  $items.Clip = $geometry
}

function New-HotbarTextBlock {
  param(
    [string]$Text,
    [string]$Color,
    [double]$Size = 0,
    [bool]$Bold = $false,
    [string]$FontFamily = ""
  )

  $block = New-Object System.Windows.Controls.TextBlock
  $block.Text = $Text
  $block.Foreground = New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.ColorConverter]::ConvertFromString($Color))
  $block.FontFamily = New-Object System.Windows.Media.FontFamily ($(if ($FontFamily) { $FontFamily } else { $script:PanelFont }))
  $block.FontSize = $(if ($Size -gt 0) { $Size } else { $script:PanelFontSize })
  $block.Margin = New-Object System.Windows.Thickness(0, 1, 0, 1)
  if ($Bold) { $block.FontWeight = [System.Windows.FontWeights]::Bold }
  return $block
}

<#
.SYNOPSIS
  Builds one round glyph button and wires its click to the item's action.

.DESCRIPTION
  The item is attached as Tag rather than captured in the handler's closure. A
  PowerShell loop variable captured by an event handler is the classic late-binding
  bug: every button would end up dispatching the last item in the list.
#>
function Add-HotbarItemButton {
  param(
    $Panel,
    $Item,
    [string]$StyleKey
  )

  $button = New-Object System.Windows.Controls.Button
  $button.Style = $script:Window.Resources[$StyleKey]
  $button.ToolTip = ([string]$Item.tooltip)
  if (-not $button.ToolTip) { $button.ToolTip = [string]$Item.label }
  $button.Tag = $Item

  $label = New-Object System.Windows.Controls.TextBlock
  $label.Text = ConvertFrom-HotbarGlyph $Item.glyph
  $label.FontFamily = New-Object System.Windows.Media.FontFamily($script:GlyphFont)
  $label.FontSize = 20
  $label.HorizontalAlignment = [System.Windows.HorizontalAlignment]::Center
  $label.VerticalAlignment = [System.Windows.VerticalAlignment]::Center
  $button.Content = $label

  $button.Add_Click({
      param($sender, $eventArgs)
      if ($null -eq $sender) { return }
      Invoke-HotbarItemAction -Item $sender.Tag
    })

  $null = $Panel.Children.Add($button)
  return $button
}

function Sync-HotbarItems {
  $panel = Get-HotbarElement -Window $script:Window -Name "ItemsPanel"
  $panel.Children.Clear()
  if ($null -eq $script:Config.items) { return 0 }
  foreach ($item in $script:Config.items) {
    $null = Add-HotbarItemButton -Panel $panel -Item $item -StyleKey "ItemButton"
  }
  return $panel.Children.Count
}

<#
.SYNOPSIS
  Parses the XAML and returns the wired-up window.

.NOTES
  [xml] first, so a malformed here-string fails as a structural XML error instead
  of an opaque XamlReader exception. This is also the SelfTest's XAML check.
#>
function New-HotbarWindow {
  [CmdletBinding()]
  param()

  $document = [xml]$script:HotbarXaml
  $window = [System.Windows.Markup.XamlReader]::Parse($document.OuterXml)

  $script:Window = $window

  # Touch every named element once, so a typo in an x:Name is a load-time error and
  # not a null reference on the first click.
  foreach ($name in @("BarBorder", "TabBorder", "PanelBorder", "PanelStack", "ItemsPanel", "CollapseHandle", "ExpandButton")) {
    $null = Get-HotbarElement -Window $window -Name $name
  }

  $null = Sync-HotbarItems

  $collapse = Get-HotbarElement -Window $window -Name "CollapseHandle"
  $collapse.Add_Click({ Set-HotbarCollapsed -Collapsed $true })

  $expand = Get-HotbarElement -Window $window -Name "ExpandButton"
  $expand.Add_Click({ Set-HotbarCollapsed -Collapsed $false })

  # A widget with no way out is a defect, not a design choice. Right-click is the
  # discoverable path; Escape is the fast one, and it works after the first click
  # has given the window focus.
  $window.ContextMenu = New-HotbarContextMenu
  $window.Add_KeyDown({
      param($sender, $eventArgs)
      if ($null -ne $eventArgs -and $eventArgs.Key -eq [System.Windows.Input.Key]::Escape) { $window.Close() }
    })

  # Every exit path goes through Closed, including the self test's, so the usage
  # refresh is never left pointing at a window that no longer exists.
  $window.Add_Closed({
      $script:UsagePanelActive = $false
      $script:AgentPanelActive = $false
      Stop-HotbarUsagePanelTimer
    })

  # Drag the bar between monitors. Buttons mark MouseLeftButtonDown as handled
  # (that is what makes a click a click), so an unhandled press here means empty
  # bar space and the whole window moves. On release the bar snaps back to the
  # right edge of the monitor its centre ended up on.
  $window.Add_MouseLeftButtonDown({
      param($sender, $eventArgs)
      if ($null -ne $eventArgs -or -not $eventArgs.Handled) {
        try { $window.DragMove() } catch { }
        Snap-HotbarToActiveScreen
      }
    })

  $bar = Get-HotbarElement -Window $window -Name "BarBorder"
  $tab = Get-HotbarElement -Window $window -Name "TabBorder"
  if ($script:Collapsed) {
    $bar.Visibility = [System.Windows.Visibility]::Collapsed
    $tab.Visibility = [System.Windows.Visibility]::Visible
  } else {
    $bar.Visibility = [System.Windows.Visibility]::Visible
    $tab.Visibility = [System.Windows.Visibility]::Collapsed
  }

  # Clip the item column to the crescent. Eager first, then on every resize: the
  # happy-path first paint can arrive before layout, and the SizeChanged handler
  # catches the resize that follows. Guarded on the element itself, so the
  # Collapsed tab state (no bar at all) still wires a no-op.
  $items = Get-HotbarElement -Window $window -Name "ItemsPanel"
  $items.Add_SizeChanged({
      param($sender, $eventArgs)
      Update-HotbarBarClip
    })
  Update-HotbarBarClip

  Update-HotbarGeometry
  return $window
}

function New-HotbarContextMenu {
  $menu = New-Object System.Windows.Controls.ContextMenu

  $edit = New-Object System.Windows.Controls.MenuItem
  $edit.Header = "Edit config.json"
  $edit.Add_Click({ Open-HotbarConfig })
  $null = $menu.Items.Add($edit)

  $reload = New-Object System.Windows.Controls.MenuItem
  $reload.Header = "Reload config"
  $reload.Add_Click({
      try {
        $null = Initialize-HotbarStateFromConfig
        $count = Sync-HotbarItems
        Set-HotbarPanelLines -Open $true -Lines @(
          (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
          (New-HotbarLine ("  reloaded " + $count + " items") $script:ColorDim 9)
        )
      } catch {
        Set-HotbarPanelLines -Open $true -Lines @(
          (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
          (New-HotbarErrorLine $_.Exception.Message "  reload failed")
        )
      }
    })
  $null = $menu.Items.Add($reload)

  $collapse = New-Object System.Windows.Controls.MenuItem
  $collapse.Header = "Collapse bar"
  $collapse.Add_Click({ Set-HotbarCollapsed -Collapsed $true })
  $null = $menu.Items.Add($collapse)

  $quit = New-Object System.Windows.Controls.MenuItem
  $quit.Header = "Quit hotbar"
  $quit.Add_Click({ $script:Window.Close() })
  $null = $menu.Items.Add($quit)

  return $menu
}

# ---------------------------------------------------------------------------
# Collapse / expand
# ---------------------------------------------------------------------------
function Set-HotbarCollapsed {
  [CmdletBinding()]
  param([bool]$Collapsed)

  $script:Collapsed = $Collapsed
  # The panel is a property of the expanded bar, so collapsing closes it. Session
  # only: nothing about the collapsed state is written back to config.json. The
  # usage refresh goes with it, for the same reason: there is nothing left to
  # refresh into.
  if ($Collapsed) {
    $script:PanelOpen = $false
    $script:UsagePanelActive = $false
    Stop-HotbarUsagePanelTimer
  }

  $bar = Get-HotbarElement -Window $script:Window -Name "BarBorder"
  $tab = Get-HotbarElement -Window $script:Window -Name "TabBorder"
  $panelBorder = Get-HotbarElement -Window $script:Window -Name "PanelBorder"

  if ($Collapsed) {
    $bar.Visibility = [System.Windows.Visibility]::Collapsed
    $tab.Visibility = [System.Windows.Visibility]::Visible
    $panelBorder.Visibility = [System.Windows.Visibility]::Collapsed
  } else {
    $bar.Visibility = [System.Windows.Visibility]::Visible
    $tab.Visibility = [System.Windows.Visibility]::Collapsed
  }

  Update-HotbarGeometry
}

# ---------------------------------------------------------------------------
# Inline OmniRoute panel
# ---------------------------------------------------------------------------
function New-HotbarLine {
  param([string]$Text, [string]$Color, [double]$Size = 0, [bool]$Bold = $false)
  return [pscustomobject]@{ Text = $Text; Color = $Color; Size = $Size; Bold = $Bold }
}

# One honest line when something could not be read. The reason is kept, because
# "sin datos" with no cause is the same failure mode as an empty list: the reader
# cannot tell the user whether the gateway is down or the database is missing.
function New-HotbarErrorLine {
  param([string]$Reason, [string]$Prefix = "  sin datos")
  $reason = ([string]$Reason).Trim()
  if (-not $reason) { $reason = "no reason reported" }
  return (New-HotbarLine (Format-HotbarLine ($Prefix + " (" + $reason + ")")) $script:ColorWarn 9)
}

function Format-HotbarLine {
  param([string]$Text, [int]$MaxChars = 0)
  if ($MaxChars -le 0) { $MaxChars = $script:MaxPanelChars }
  $text = [string]$Text
  if ($text.Length -le $MaxChars) { return $text }
  return ($text.Substring(0, $MaxChars - 3) + "...")
}

function Get-HotbarFirstLine {
  param([string]$Text)
  if (-not $Text) { return "" }
  foreach ($line in ($Text -split "`r?`n")) {
    $trimmed = $line.Trim()
    if ($trimmed) { return $trimmed }
  }
  return ""
}

<#
.SYNOPSIS
  One sample of gateway state: the listening socket and the configured combos.

.DESCRIPTION
  Both checks are independent, so they are started together and collected after:
  the panel pays for the slower one instead of their sum. Every read is bounded and
  windowless (CreateNoWindow), because a console flashing on top of an always-on-top
  widget is the one bug that would be immediately visible.

  This runs on the UI thread. It is bounded on purpose - roughly 60 ms for the
  database and 160 ms for the netstat spawn on this machine, both overlapping - so
  the bar freezes for a fraction of a second instead of gaining a runspace and a
  Dispatcher hop to avoid it.

  Nothing here can fabricate a value. No gateway API call is made (they all need a
  key, and this widget never touches one), and a failed read is reported as a
  failure rather than as an empty configuration.
#>
function Get-HotbarOmniRouteSnapshot {
  [CmdletBinding()]
  param(
    [int]$NetstatTimeoutMs = 3000,
    [int]$BusyTimeoutMs = 1200
  )

  if (-not $script:OmniRouteExtensionAvailable) {
    return [pscustomobject]@{
      Gateway          = [pscustomobject]@{ Up = $false; Port = $null; Detail = "extension not installed" }
      Combos           = [pscustomobject]@{ Ok = $false; Combos = @(); Provider = ""; Error = "extension not installed"; ActiveComboName = "" }
      TakenAt          = (Get-Date)
      ExtensionMissing = $true
    }
  }

  $portJob = $null
  try { $portJob = Start-HotbarGatewayProbe } catch { }

  $comboJob = $null
  try { $comboJob = Start-OmniRouteComboRead -BusyTimeoutMs $BusyTimeoutMs } catch { }

  $gateway = $null
  if ($null -ne $portJob) {
    try { $gateway = Complete-HotbarGatewayProbe -Job $portJob -TimeoutMs $NetstatTimeoutMs } catch { }
  }
  if ($null -eq $gateway) {
    $gateway = [pscustomobject]@{ Up = $false; Port = $script:GatewayPort; Detail = "probe failed" }
  }

  $read = $null
  if ($null -ne $comboJob) {
    try { $read = Complete-OmniRouteComboRead -Job $comboJob } catch { }
  }
  if ($null -eq $read) {
    $read = [pscustomobject]@{ Ok = $false; Combos = @(); Provider = ""; Error = "reader unavailable"; ActiveComboName = "" }
  }

  return [pscustomobject]@{
    Gateway = $gateway
    Combos  = $read
    TakenAt = (Get-Date)
  }
}

function Get-HotbarOmniRouteLines {
  [CmdletBinding()]
  param([Parameter(Mandatory = $true)]$Snapshot)

  $lines = @()
  $lines += New-HotbarLine "OmniRoute gateway" $script:ColorGold 11 $true

  if ($Snapshot.ExtensionMissing) {
    $lines += New-HotbarLine "  extension not installed" $script:ColorDim 9
    $lines += New-HotbarLine (Format-HotbarLine "  see extensions/README.md") $script:ColorDim 9
    $lines += New-HotbarLine (Format-HotbarLine "  click the icon again to close") $script:ColorDim 9
    return $lines
  }

  if ($Snapshot.Gateway.Up) {
    $lines += New-HotbarLine ("  UP     localhost:" + $Snapshot.Gateway.Port) $script:ColorUp
  } else {
    $lines += New-HotbarLine ("  DOWN   localhost:" + $Snapshot.Gateway.Port) $script:ColorDown
    $lines += New-HotbarLine (Format-HotbarLine ("  " + $Snapshot.Gateway.Detail)) $script:ColorDim 9
  }
  $lines += New-HotbarLine (Format-HotbarLine ("  http://localhost:" + $Snapshot.Gateway.Port + "  -  " + $Snapshot.Combos.Provider)) $script:ColorDim 9
  $lines += New-HotbarLine "" $script:ColorDim 5
  $lines += New-HotbarLine "Combos" $script:ColorGold 10 $true

  $read = $Snapshot.Combos
  if (-not $read.Ok) {
    $lines += New-HotbarErrorLine (Get-HotbarFirstLine $read.Error) "  combos: sin datos"
  } elseif (@($read.Combos).Count -eq 0) {
    $lines += New-HotbarLine "  sin combos" $script:ColorDim 9
  } else {
    foreach ($combo in $read.Combos) {
      $state = if ($combo.Enabled) { "enabled" } else { "disabled" }
      $text = "  " + ([string]$combo.Name).PadRight(16) + " [" + ([string]$combo.Strategy).PadRight(9) + "] " + $state
      $color = if ($combo.Enabled) { $script:ColorText } else { $script:ColorDim }
      $lines += New-HotbarLine (Format-HotbarLine $text) $color
    }
  }

  # The active combo is only drawn when the reader actually found the setting. The
  # gateway keeps it in runtime memory and only exposes it through an authenticated
  # route, so "sin datos" is the truthful answer and a marker would be a guess.
  if ($read.ActiveComboName) {
    $lines += New-HotbarLine (Format-HotbarLine ("  activo: " + $read.ActiveComboName)) $script:ColorGold 9
  } else {
    $lines += New-HotbarLine "  combo activo: sin datos" $script:ColorDim 9
  }

  $lines += New-HotbarLine "" $script:ColorDim 5
  $lines += New-HotbarLine (Format-HotbarLine ("  sample " + $Snapshot.TakenAt.ToString("HH:mm:ss") + " - click the icon again to close")) $script:ColorDim 9
  return $lines
}

<#
.SYNOPSIS
  Writes the inline panel and owns the refresh timer that belongs to it.

.PARAMETER Panel
  Which panel these lines are. "usage" is the only live one and is the only one
  that arms a DispatcherTimer; anything else (or nothing) parks the timer.

.NOTES
  This is the single choke point every panel write goes through, on purpose. A
  timer managed at each call site is a timer that one forgotten call site leaves
  running, and a usage panel that keeps re-sampling after it was closed is a
  widget that burns a core behind a bar nobody is looking at. Deciding here means
  "the panel changed, so the timer follows" cannot be got wrong.

  The XAML element is deliberately NOT called $panel: PowerShell variables are
  case-insensitive, so a [string]$Panel parameter and a $panel local are the same
  variable, and assigning the Border to the [string] parameter coerces the element
  into the text "System.Windows.Controls.Border" - after which $panel.Visibility
  dies with "the property 'Visibility' cannot be found on this object". Same class
  of bug as the $Port/$GatewayPort collision in Get-OmniRouteStatus.ps1.
#>
function Set-HotbarPanelLines {
  [CmdletBinding()]
  param(
    [object[]]$Lines,
    [bool]$Open = $true,
    [string]$Panel = ""
  )

  if ($null -eq $script:Window) { return }
  $stack = Get-HotbarElement -Window $script:Window -Name "PanelStack"
  $panelBorder = Get-HotbarElement -Window $script:Window -Name "PanelBorder"

  $stack.Children.Clear()
  foreach ($line in $Lines) {
    $null = $stack.Children.Add((New-HotbarTextBlock -Text $line.Text -Color $line.Color -Size $line.Size -Bold $line.Bold))
  }

  $script:PanelOpen = $Open
  $script:UsagePanelActive = ($Open -and $Panel -eq "usage")
  if (-not $script:UsagePanelActive) { Stop-HotbarUsagePanelTimer }
  $script:AgentPanelActive = ($Open -and $Panel.StartsWith("agent:"))

  if ($Open) {
    $panelBorder.Visibility = [System.Windows.Visibility]::Visible
  } else {
    $panelBorder.Visibility = [System.Windows.Visibility]::Collapsed
  }
  Update-HotbarGeometry
}

<#
.SYNOPSIS
  Toggles the OmniRoute panel, re-reading on every open.

.NOTES
  Snapshot semantics, the same contract as the status popup: the data is sampled
  when the panel opens and never refreshes on its own, because a repaint loop on top
  of the desktop is worse than a click. The usage panel below is the deliberate
  exception, and the reason it is safe is that its timer dies with the panel.
#>
function Toggle-HotbarPanel {
  if ($script:PanelOpen) {
    Set-HotbarPanelLines -Lines @() -Open $false
    return
  }

  # Show the frame first with a placeholder, so a slow read reads as "busy" instead
  # of as a button that did nothing.
  Set-HotbarPanelLines -Open $true -Lines @(
    (New-HotbarLine "OmniRoute gateway" $script:ColorGold 11 $true),
    (New-HotbarLine "  reading..." $script:ColorDim 9)
  )

  $snapshot = Get-HotbarOmniRouteSnapshot
  Set-HotbarPanelLines -Open $true -Lines (Get-HotbarOmniRouteLines -Snapshot $snapshot)
}

# ---------------------------------------------------------------------------
# Inline session-usage panel
# ---------------------------------------------------------------------------

# Thousands separated with dots, not commas: the panel is Spanish and the
# numbers are the part everyone reads. Invariant culture under the hood, so the
# separator cannot follow some other machine's locale.
function Format-HotbarCount {
  param([double]$Value)

  $text = $Value.ToString("#,##0", [System.Globalization.CultureInfo]::InvariantCulture)
  return $text.Replace(",", ".")
}

# Two decimals, invariant, so a money value never renders as "12,3" or "12.345"
# depending on the thread's culture.
function Format-HotbarMoney {
  param($Cost)

  try { $value = [double]$Cost } catch { return "" }
  return "$" + $value.ToString("#,##0.00", [System.Globalization.CultureInfo]::InvariantCulture)
}

# Compact token count for the agent-history panel: "158.3k" shares and "2.4M"
# millions, with the dot thousands separator. Raw numbers stay under 5 chars in
# the ranges these stores actually produce, which is what keeps a repo line from
# overflowing the panel width.
function Format-HotbarCompactCount {
  param([double]$Value)

  if ($Value -lt 1000) { return ([long]$Value).ToString([System.Globalization.CultureInfo]::InvariantCulture) }
  if ($Value -lt 1000000) { return ($Value / 1000.0).ToString("0.0", [System.Globalization.CultureInfo]::InvariantCulture).Replace(".", ",") + "k" }
  return ($Value / 1000000.0).ToString("0.0", [System.Globalization.CultureInfo]::InvariantCulture).Replace(".", ",") + "M"
}

<#
.SYNOPSIS
  Draws one snapshot of the three agents.

.DESCRIPTION
  Four lines per agent at most, and every number on screen came out of a store:

    claude                 name
      in 322 out 108.595 cache 27.650.764
      reas 18.739  claude-opus-5-5
      $12.34   or   costo: sin datos

  The money line is the honest one. Neither jsonl store writes a cost field on
  this machine, so those two agents render "costo: sin datos" - not a zero, which
  would be a number nobody measured. opencode does store cost, and its local
  model really costs 0.0, so it renders $0.00 and says so on the model line.

  A "~" on the token line means the read was bounded: some lines were left
  outside the read budget, so the totals are a sample of the session, not all of
  it. Approximate is never silently presented as exact. The marker sits right
  after the agent name rather than at the end of the line because a line over
  MaxPanelChars is trimmed from the right, and a marker that gets trimmed off is
  a lie told by omission.

  Spanish without accents on purpose: every .ps1 in this widget is pure ASCII so
  the bytes survive any editor and code page.
#>
function Get-HotbarUsageLines {
  [CmdletBinding()]
  param([Parameter(Mandatory = $true)]$Snapshot)

  $lines = @()
  $lines += New-HotbarLine "Uso de sesion" $script:ColorGold 11 $true

  $takenAt = $null
  $anyApprox = $false
  $anyEst = $false
  foreach ($usage in $Snapshot) {
    if ($null -ne $usage.TakenAt -and $null -eq $takenAt) { $takenAt = $usage.TakenAt }
    if ($usage.Approximate) { $anyApprox = $true }
    if ($usage.Estimated) { $anyEst = $true }

    if (-not $usage.Ok) {
      $lines += New-HotbarLine ("  " + $usage.Agent) $script:ColorGold 10 $true
      $lines += New-HotbarErrorLine (Get-HotbarFirstLine $usage.Error) ("  " + $usage.Agent + ": sin datos")
      continue
    }

    $approx = ""
    if ($usage.Approximate) { $approx = "~ " }
    $lines += New-HotbarLine ("  " + $usage.Agent + $approx) $script:ColorGold 10 $true

    $tokens = "  in " + (Format-HotbarCount $usage.InputTokens) +
      " out " + (Format-HotbarCount $usage.OutputTokens) +
      " cache " + (Format-HotbarCount $usage.CacheTokens)
    $lines += New-HotbarLine (Format-HotbarLine $tokens) $script:ColorText

    # Reasoning tokens and the model share a line: they are both context, and a
    # panel that wrapped them would push the money out of view.
    $context = ""
    if ($usage.ReasoningTokens -gt 0) { $context = "reas " + (Format-HotbarCount $usage.ReasoningTokens) + "  " }
    if ($usage.Model) {
      $context = $context + $usage.Model
      if ($usage.Local -and $null -ne $usage.Cost -and [double]$usage.Cost -eq 0) { $context = $context + " (modelo local)" }
    }
    if ($context) { $lines += New-HotbarLine (Format-HotbarLine ("  " + $context)) $script:ColorDim 9 }

    if ($null -eq $usage.Cost) {
      $lines += New-HotbarLine "  costo: sin datos" $script:ColorDim 9
    } else {
      $money = Format-HotbarMoney $usage.Cost
      if ($usage.Estimated) { $money = $money + " (est)" }
      $lines += New-HotbarLine ("  " + $money) $script:ColorUp 10 $true
    }
  }

  # Both markers need a legend or they read as typos. The ~ sits right after the
  # agent name (a trim from the right would eat it); (est) hangs off the money.
  if ($anyApprox) {
    $lines += New-HotbarLine (Format-HotbarLine "  ~ = lectura parcial") $script:ColorDim 9
  }
  if ($anyEst) {
    $lines += New-HotbarLine (Format-HotbarLine "  (est) = costo estimado") $script:ColorDim 9
  }

  $lines += New-HotbarLine "" $script:ColorDim 5
  $stamp = ""
  if ($null -ne $takenAt) { $stamp = $takenAt.ToString("HH:mm:ss") }
  $lines += New-HotbarLine (Format-HotbarLine ("  muestra " + $stamp + " - refresco " + $script:UsageRefreshSeconds + "s")) $script:ColorDim 9
  $lines += New-HotbarLine (Format-HotbarLine "  click de nuevo para cerrar") $script:ColorDim 9
  return $lines
}

# Parks the refresh. Safe to call when no timer was ever built: the null check is
# the difference between "no-op" and a null-reference crash on window close.
function Stop-HotbarUsagePanelTimer {
  if ($null -eq $script:UsageTimer) { return }
  try { $script:UsageTimer.Stop() } catch { }
}

<#
.SYNOPSIS
  Arms the live refresh for the usage panel.

.NOTES
  One DispatcherTimer, built once and restarted, because a new timer per tick
  would queue ticks on a dead object. It lives on the UI thread, like everything
  else here: the readers are bounded (measured 452 ms cold for the slowest one,
  ~0.5 s for all three), so the bar pauses for a fraction of the 5 s period
  instead of gaining a runspace and a dispatcher hop to avoid it.
#>
function Start-HotbarUsagePanelTimer {
  if ($null -eq $script:Window) { return }

  if ($null -eq $script:UsageTimer) {
    $script:UsageTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:UsageTimer.Interval = [TimeSpan]::FromSeconds($script:UsageRefreshSeconds)
    $script:UsageTimer.Add_Tick({ Update-HotbarUsagePanel })
  }
  $script:UsageTimer.Start()
}

# The refresh itself. The UsagePanelActive guard is the load-bearing line: a tick
# that fires after the panel closed must not resurrect it, and Set-HotbarPanelLines
# is what closes it.
function Update-HotbarUsagePanel {
  [CmdletBinding()]
  param()

  if (-not $script:UsagePanelActive) {
    Stop-HotbarUsagePanelTimer
    return
  }

  try {
    $snapshot = Get-AgentUsageSnapshot
    if (-not $script:UsagePanelActive) { return }
    Set-HotbarPanelLines -Open $true -Panel "usage" -Lines (Get-HotbarUsageLines -Snapshot $snapshot)
  } catch {
    Set-HotbarPanelLines -Open $true -Panel "usage" -Lines @(
      (New-HotbarLine "Uso de sesion" $script:ColorGold 11 $true),
      (New-HotbarErrorLine $_.Exception.Message "  lectura fallida")
    )
  }
}

<#
.SYNOPSIS
  Toggles the live usage panel.

.NOTES
  Opens with a placeholder so the frame appears immediately, then fills it and
  arms the refresh. Closing parks the timer through Set-HotbarPanelLines, which
  every panel write goes through.
#>
function Toggle-HotbarUsagePanel {
  if ($script:UsagePanelActive) {
    Set-HotbarPanelLines -Lines @() -Open $false
    return
  }

  Set-HotbarPanelLines -Open $true -Panel "usage" -Lines @(
    (New-HotbarLine "Uso de sesion" $script:ColorGold 11 $true),
    (New-HotbarLine "  reading..." $script:ColorDim 9)
  )

  $snapshot = Get-AgentUsageSnapshot
  Set-HotbarPanelLines -Open $true -Panel "usage" -Lines (Get-HotbarUsageLines -Snapshot $snapshot)
  Start-HotbarUsagePanelTimer
}

<#
.SYNOPSIS
  Draws one month-to-date history panel for a single agent.

.DESCRIPTION
  One read of the stores, rendered over the agent's own top projects this
  month (ranked by output tokens, no external program consulted):

    claude sep
      mes: out 158.3k  $28.17 (est)
      incoders-commerce 149.9k  $27.37 (est)
      (est) = costo estimado
      click de nuevo para cerrar

  Money is honest about what it is: claude/codex stores keep no cost field, so
  their cost here is the official-price estimate and is marked (est); opencode
  stores a cost and its local model really is $0.00. A project shows
  "costo: sin datos" when it had tokens but no price could be applied (unknown
  model). A "-" after a session count marks an approximate read: the Detail
  carried by the snapshot says how much of the budget was spent, and the panel
  never presents the sample as the total.

  The read is snapshot-only: unlike the live usage panel there is no timer, so
  the numbers are what the store said at click time and stay until the next
  click. This is the same contract as the OmniRoute panel.
#>
function Get-HotbarAgentLines {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$Agent,
    [long]$MaxBytes = 0
  )

  $snapshot = Get-AgentHistorySnapshot -MaxBytes $MaxBytes
  # The local is deliberately NOT named $agent: PowerShell variables are
  # case-insensitive, so "$agent = $null" would overwrite the [string] $Agent
  # parameter (coercing it to "" because of the type) and the comparison below
  # would never match. This is the same trap that made the first revision
  # render every agent as "sin datos".
  $matched = $null
  foreach ($candidate in @($snapshot.Agents)) {
    if ($candidate.Agent -eq $Agent) { $matched = $candidate; break }
  }

  $lines = @()
  if ($null -eq $matched) {
    $lines += New-HotbarLine ("  " + $Agent) $script:ColorGold 10 $true
    $lines += New-HotbarErrorLine ("unknown agent: " + $Agent)
    return $lines
  }

  # The ~ marker sits right after the name for the same reason as in the live
  # panel: a trim from the right must never eat it.
  $approx = ""
  if ($matched.Approximate) { $approx = "~ " }
  $lines += New-HotbarLine ("  " + $matched.Agent + $approx + $snapshot.MonthLabel) $script:ColorGold 10 $true

  if ($matched.MonthEntries -le 0) {
    $lines += New-HotbarLine "  mes: sin datos" $script:ColorDim 9
  } else {
    $month = "  mes: out " + (Format-HotbarCompactCount $matched.MonthTokens)
    if ($null -ne $matched.MonthCost) {
      $money = Format-HotbarMoney $matched.MonthCost
      if ($matched.MonthEstimated) { $money = $money + " (est)" }
      $month = $month + "  " + $money
    } else {
      $month = $month + "  costo: sin datos"
    }
    $lines += New-HotbarLine (Format-HotbarLine $month) $script:ColorText
  }

  $anyEst = $false
  if ($matched.MonthEstimated) { $anyEst = $true }
  $topProjects = @($matched.Projects | Where-Object { $_.Entries -gt 0 } | Select-Object -First $script:AgentHistoryMaxProjectsShown)
  foreach ($project in $topProjects) {
    $text = "  " + $project.Name + " " + (Format-HotbarCompactCount $project.Output)
    $lines += New-HotbarLine (Format-HotbarLine $text) $script:ColorText

    if ($null -ne $project.Cost) {
      $money = Format-HotbarMoney $project.Cost
      if ($project.Estimated) { $money = $money + " (est)"; $anyEst = $true }
      $lines += New-HotbarLine (Format-HotbarLine ("  " + $money)) $script:ColorUp 10 $true
    } else {
      $lines += New-HotbarLine "  costo: sin datos" $script:ColorDim 9
    }
  }

  if ($anyEst) {
    $lines += New-HotbarLine (Format-HotbarLine "  (est) = costo estimado") $script:ColorDim 9
  }
  $lines += New-HotbarLine (Format-HotbarLine "  click de nuevo para cerrar") $script:ColorDim 9
  return $lines
}

# Toggles one agent history panel. "AgentPanelActive" is per agent panel (the
# same shape as "UsagePanelActive"): a second click closes what the first opened,
# while another panel (usage or omniroute) is replaced, exactly like usage does.
function Toggle-HotbarAgentPanel {
  [CmdletBinding()]
  param([Parameter(Mandatory = $true)][string]$Agent)

  if ($script:AgentPanelActive) {
    Set-HotbarPanelLines -Lines @() -Open $false
    return
  }

  Set-HotbarPanelLines -Open $true -Panel ("agent:" + $Agent) -Lines @(
    (New-HotbarLine ("  " + $Agent) $script:ColorGold 10 $true),
    (New-HotbarLine "  reading..." $script:ColorDim 9)
  )

  try {
    $lines = Get-HotbarAgentLines -Agent $Agent
    Set-HotbarPanelLines -Open $true -Panel ("agent:" + $Agent) -Lines $lines
  } catch {
    Set-HotbarPanelLines -Open $true -Panel ("agent:" + $Agent) -Lines @(
      (New-HotbarLine ("  " + $Agent) $script:ColorGold 10 $true),
      (New-HotbarErrorLine $_.Exception.Message "  lectura fallida")
    )
  }
}

# ---------------------------------------------------------------------------
# Item actions
# ---------------------------------------------------------------------------
function Invoke-HotbarItemAction {
  [CmdletBinding()]
  param($Item)

  if ($null -eq $Item) { return }
  $action = [string]$Item.action
  if (-not $action) { $action = "none" }

  try {
    switch -Regex ($action) {
      "^none$" { return }
      "^omniroute-status$" { Toggle-HotbarPanel; return }
      "^agent-usage:(claude|codex|opencode)$" { Toggle-HotbarAgentPanel -Agent $Matches[1]; return }
      "^agent-usage$" { Toggle-HotbarUsagePanel; return }
      "^edit-config$" { Open-HotbarConfig; return }
      "^run:(.+)$" { Start-HotbarCommand -Command $Matches[1] -Item $Item; return }
      default {
        # Unknown action: say so in the panel instead of doing nothing silently.
        # A typo in config.json must be visible, not mysterious.
        Set-HotbarPanelLines -Open $true -Lines @(
          (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
          (New-HotbarErrorLine ("unknown action: " + $action))
        )
        return
      }
    }
  } catch {
    Set-HotbarPanelLines -Open $true -Lines @(
      (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
      (New-HotbarErrorLine $_.Exception.Message "  action failed")
    )
  }
}

function Open-HotbarConfig {
  [CmdletBinding()]
  param()

  if (-not [System.IO.File]::Exists($script:ConfigPath)) {
    Set-HotbarPanelLines -Open $true -Lines @(
      (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
      (New-HotbarErrorLine ("config not found: " + $script:ConfigPath) "  cannot open")
    )
    return
  }
  # notepad.exe is a GUI subsystem binary, so Start-Process shows the editor and no
  # console. Swap this for `code` or another editor by changing this one line.
  Start-Process -FilePath "notepad.exe" -ArgumentList $script:ConfigPath
}

function Test-HotbarExecutableOnPath {
  [CmdletBinding()]
  param([string]$FileName)

  if (-not $FileName) { return $false }
  if ([System.IO.Path]::GetExtension($FileName)) { return [System.IO.File]::Exists($FileName) }
  foreach ($dir in $env:PATH.Split(";")) {
    if (-not $dir) { continue }
    try {
      $candidate = [System.IO.Path]::Combine($dir, $FileName)
      if ([System.IO.File]::Exists($candidate)) { return $true }
    } catch {
      # Unreadable PATH entry: keep scanning, the next one may hold the binary.
    }
  }
  return $false
}

<#
.SYNOPSIS
  Runs an item's `run:<command>` without allocating a console window.

.DESCRIPTION
  CreateNoWindow = $true and UseShellExecute = $false, the same rule the status
  popup follows. Output is deliberately NOT redirected: a redirected pipe that
  nobody drains fills its buffer and deadlocks the child, and inheriting the
  parent's (hidden) handles keeps a chatty command invisible but alive.

  `cmd.exe /d /c` is used when the target cannot be launched directly. That is the
  common case on Windows for the tools this bar is meant to launch: npm and
  friends install claude/codex/opencode as .cmd shims, and a .cmd is not a valid
  Win32 application, so Process.Start would fail with "not a valid Win32
  application". /d skips AutoRun commands, which keeps the start fast and free of
  surprises.
#>
function Start-HotbarCommand {
  [CmdletBinding()]
  param(
    [string]$Command,
    $Item
  )

  $commandText = ([string]$Command).Trim()
  if (-not $commandText) { return }

  $workingDirectory = $script:RepoRoot
  if ($null -ne $Item -and $Item.cwd) {
    $candidate = [string]$Item.cwd
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
      $candidate = [System.IO.Path]::Combine($script:RepoRoot, $candidate)
    }
    if ([System.IO.Directory]::Exists($candidate)) { $workingDirectory = $candidate }
  }

  $parts = $commandText -split '\s+', 2
  $executable = $parts[0]
  $arguments = ""
  if ($parts.Count -gt 1) { $arguments = $parts[1] }

  $extension = [System.IO.Path]::GetExtension($executable)
  $useShell = $false
  if ($extension) {
    $useShell = ($extension -match '^\.(cmd|bat|ps1|psm1|vbs|js|wsf)$')
  } else {
    $useShell = -not (Test-HotbarExecutableOnPath ($executable + ".exe"))
  }

  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
  $startInfo.WorkingDirectory = $workingDirectory

  if ($useShell) {
    $startInfo.FileName = $env:ComSpec
    if (-not $startInfo.FileName) { $startInfo.FileName = "cmd.exe" }
    $startInfo.Arguments = "/d /c " + $commandText
  } else {
    $startInfo.FileName = $executable
    $startInfo.Arguments = $arguments
  }

  $process = [System.Diagnostics.Process]::Start($startInfo)
  if ($null -eq $process) {
    Set-HotbarPanelLines -Open $true -Lines @(
      (New-HotbarLine "hotbar" $script:ColorGold 11 $true),
      (New-HotbarErrorLine ("could not start: " + $commandText))
    )
  }
}

# ---------------------------------------------------------------------------
# Self test
# ---------------------------------------------------------------------------
<#
.SYNOPSIS
  Validates everything that can be validated without a human, and always returns.

.NOTES
  Closes the window from a DispatcherTimer at SelfTestMs, with a background watchdog
  at SelfTestMs + 2000 ms behind it. The window cannot outlive the check, so the
  command can never hang the caller.

  The visible flash defaults to SelfTestMs, not a fixed 700 ms, because the usage
  panel is on this path and a real read of a live session is 400-500 ms of it.
  The check has a 2 s ceiling end to end, and a longer flash buys no extra
  coverage: the geometry, the glyph raster and the panel width are all asserted
  from data, not from a human looking at it. Pass -SelfTestMs to see it longer.
#>
function Invoke-HotbarSelfTest {
  [CmdletBinding()]
  param([int]$DisplayMs = 400)

  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  $failures = @()
  $report = @()
  $window = $null

  try {
    $config = Initialize-HotbarStateFromConfig
    $report += ("config=" + [System.IO.Path]::GetFileName($script:ConfigPath))

    $itemCount = 0
    if ($null -ne $config.items) { $itemCount = @($config.items).Count }
    $report += ("items=" + $itemCount)
    if ($itemCount -lt 1) { $failures += "config has no items" }

    foreach ($item in @($config.items)) {
      $action = [string]$item.action
      if (-not $action) { $action = "none" }
      $known = ($script:KnownActions -contains $action) -or
        $action.StartsWith("run:") -or
        ($action -match "^agent-usage:(claude|codex|opencode)$")
      if (-not $known) { $failures += ("item " + [string]$item.id + " has an unsupported action: " + $action) }
      $null = ConvertFrom-HotbarGlyph $item.glyph
    }

    $window = New-HotbarWindow
    $report += ("xaml=ok buttons=" + (Get-HotbarElement -Window $window -Name "ItemsPanel").Children.Count)

    $primary = [System.Windows.Forms.Screen]::PrimaryScreen
    $active = Get-HotbarScreenFromConfig -Config $config
    if ($null -eq $active) { $failures += "screen resolution returned no monitor" }
    $report += ("monitor=" + $active.DeviceName)

    $expanded = Get-HotbarGeometry -Collapsed $false -PanelOpen $false -Screen $primary
    $collapsed = Get-HotbarGeometry -Collapsed $true -PanelOpen $false -Screen $primary
    $withPanel = Get-HotbarGeometry -Collapsed $false -PanelOpen $true -Screen $primary
    $working = $primary.WorkingArea
    $scale = $expanded.Scale

    $expectedX = [Math]::Round((($working.Right / $scale) - $script:BarWidth - $script:Margin), 0)
    $expectedY = [Math]::Round((($working.Top / $scale) + ((($working.Height / $scale) - $script:BarHeight) / 2)), 0)
    if ($expanded.X -ne $expectedX -or $expanded.Y -ne $expectedY) {
      $failures += ("expanded geometry " + $expanded.X + "," + $expanded.Y + " != " + $expectedX + "," + $expectedY)
    }
    if ($expanded.Width -ne $script:BarWidth -or $expanded.Height -ne $script:BarHeight) {
      $failures += ("expanded size " + $expanded.Width + "x" + $expanded.Height + " is not " + $script:BarWidth + "x" + $script:BarHeight)
    }

    $collapsedX = [Math]::Round((($working.Right / $scale) - $script:CollapsedSize - $script:Margin), 0)
    $collapsedY = [Math]::Round((($working.Top / $scale) + ((($working.Height / $scale) - $script:CollapsedSize) / 2)), 0)
    if ($collapsed.X -ne $collapsedX -or $collapsed.Y -ne $collapsedY) {
      $failures += ("collapsed geometry " + $collapsed.X + "," + $collapsed.Y + " != " + $collapsedX + "," + $collapsedY)
    }
    if ($withPanel.X -ne ($expanded.X - $script:PanelWidth)) {
      $failures += "panel geometry does not keep the right edge pinned"
    }
    $report += ("geometry expanded=" + $expanded.X + "," + $expanded.Y + " collapsed=" + $collapsed.X + "," + $collapsed.Y + " panel=" + $withPanel.X + "," + $withPanel.Y)
    $report += ("screen=" + $expanded.WorkingArea + " dpi_scale=" + $scale)

    $snapshot = Get-HotbarOmniRouteSnapshot -NetstatTimeoutMs 2000 -BusyTimeoutMs 600
    $lines = Get-HotbarOmniRouteLines -Snapshot $snapshot
    if ($lines.Count -lt 4) { $failures += "panel produced too few lines" }
    foreach ($line in $lines) {
      if ($line.Text.Length -gt $script:MaxPanelChars) {
        $failures += ("panel line overflows the width: " + $line.Text.Length + " chars")
      }
    }
    $report += ("data gateway=" + $(if ($snapshot.Gateway.Up) { "UP" } else { "DOWN" }) + " combos=" + @($snapshot.Combos.Combos).Count + " provider=" + $snapshot.Combos.Provider)

    Set-HotbarPanelLines -Open $true -Lines $lines
    $report += ("panel_lines=" + $lines.Count)

    # The usage panel, exercised for real and drawn for real. Each agent must be
    # either Ok or carry a reason: a failed read is a valid answer, a failed read
    # with no reason is a bug. The lines are rendered and measured against the
    # panel width, because a line that overflows is clipped instead of wrapped.
    #
    # The read is bounded to 512 KB rather than the production 8 MB, on purpose,
    # and the arithmetic is worth writing down. This check has a 2 s ceiling and
    # the committed baseline already spends 1409 ms of it (measured: process
    # start, WPF, the omniroute netstat+sqlite probe and a 700 ms window), which
    # leaves ~590 ms. A cold real read of all three stores costs 550-730 ms on
    # this machine, so reading the production budget here would push the check
    # over its own ceiling - and worse, it would make the result depend on how
    # long the current session has been running, since a session file grows
    # without limit. At 512 KB the read is ~300 ms and still reads a real recent
    # slice of a real session: dedup, the cumulative claude counters, the codex
    # last-record rule, truncation, Approximate, the marker and the render are
    # all exercised. The panel, not this check, reports the full session. The
    # budget in force is printed below so nobody reads these numbers as totals.
    $usageBudget = 512KB
    $usageWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $usageSnapshot = Get-AgentUsageSnapshot -BusyTimeoutMs 600 -MaxBytes $usageBudget
    $usageWatch.Stop()
    $usageLines = Get-HotbarUsageLines -Snapshot $usageSnapshot
    foreach ($line in $usageLines) {
      if ($line.Text.Length -gt $script:MaxPanelChars) {
        $failures += ("usage line overflows the width: " + $line.Text.Length + " chars: " + $line.Text)
      }
    }
    if ($usageLines.Count -lt 4) { $failures += "usage panel produced too few lines" }

    $usageReport = @()
    $approxSeen = $false
    $markerSeen = $false
    foreach ($usage in $usageSnapshot) {
      if ($usage.Ok -and $usage.Approximate) { $approxSeen = $true }
      if ($usage.Ok) {
        $cost = if ($null -eq $usage.Cost) { "sin datos" } else { Format-HotbarMoney $usage.Cost }
        $usageReport += ($usage.Agent + "=OK " + $cost + $(if ($usage.Approximate) { " aprox" } else { "" }))
      } else {
        if (-not ([string]$usage.Error).Trim()) { $failures += ($usage.Agent + " failed without a reason") }
        $usageReport += ($usage.Agent + "=sin datos")
      }
    }
    # If a read was bounded, the rendered panel has to say so. The marker is the
    # only thing standing between a sampled total and an exact-looking one.
    foreach ($line in $usageLines) { if ($line.Text -match '~') { $markerSeen = $true } }
    if ($approxSeen -and -not $markerSeen) { $failures += "a bounded read was rendered without the ~ marker" }
    $report += ("usage " + ($usageReport -join " "))
    $report += ("usage_lines=" + $usageLines.Count + " usage_ms=" + $usageWatch.ElapsedMilliseconds + " usage_budget=" + $usageBudget + "B marker=" + $markerSeen)

    # The agent history panel, exercised for real with the same bounded read
    # rationale as the live usage panel above (256 KB keeps this check inside its
    # ceiling). claude exercises the estimated-money path, which is the one most
    # likely to regress; opencode exercises the real-money path; both must render
    # within the panel width, and estimated money without its legend is a lie.
    $agentBudget = 256KB
    $agentWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $agentLines = Get-HotbarAgentLines -Agent "claude" -MaxBytes $agentBudget
    $agentWatch.Stop()
    foreach ($line in $agentLines) {
      if ($line.Text.Length -gt $script:MaxPanelChars) {
        $failures += ("agent panel line overflows the width: " + $line.Text.Length + " chars: " + $line.Text)
      }
    }
    if ($agentLines.Count -lt 4) { $failures += "agent panel produced too few lines" }
    $estSeen = $false
    $legendSeen = $false
    foreach ($line in $agentLines) {
      if ($line.Text -match '\(est\)') { $estSeen = $true }
      if ($line.Text -match 'costo estimado') { $legendSeen = $true }
    }
    if ($estSeen -and -not $legendSeen) { $failures += "estimated money was rendered without its legend" }
    $report += ("agent_panel_lines=" + $agentLines.Count + " agent_ms=" + $agentWatch.ElapsedMilliseconds + " agent_est=" + $estSeen)

    # Timer ownership, checked without waiting for a tick. The interval is 5 s and
    # the whole self test is under 2 s, so what is under test is the wiring: a live
    # panel starts a timer, rewriting the same live panel keeps it, and closing
    # the panel stops it. That last one is the bug worth catching - a usage panel
    # that keeps sampling after it was closed burns a core behind a hidden bar.
    #
    # The intermediate writes render two lines, not sixteen. Each TextBlock costs
    # about 2 ms to create and configure from PowerShell, and rendering the full
    # panel four times to check three booleans cost 70 ms of a 2 s budget. The
    # ownership rule depends on the Panel argument, not on how many lines came
    # with it, so two lines exercise the same branch. The full panel is rendered
    # once, at the end, as the state the window actually shows.
    $timerProbe = @($usageLines[0], $usageLines[1])

    Set-HotbarPanelLines -Open $true -Panel "usage" -Lines $timerProbe
    Start-HotbarUsagePanelTimer
    if (-not ($null -ne $script:UsageTimer -and $script:UsageTimer.IsEnabled)) {
      $failures += "usage refresh timer did not start"
    }

    Set-HotbarPanelLines -Open $true -Panel "usage" -Lines $timerProbe
    if (-not ($null -ne $script:UsageTimer -and $script:UsageTimer.IsEnabled)) {
      $failures += "rewriting the usage panel stopped its own refresh"
    }

    Set-HotbarPanelLines -Lines @() -Open $false
    if ($null -ne $script:UsageTimer -and $script:UsageTimer.IsEnabled) {
      $failures += "usage refresh survived the panel closing"
    }

    # Another live panel takes the bar over, so the usage refresh must park.
    Start-HotbarUsagePanelTimer
    Set-HotbarPanelLines -Open $true -Panel "omniroute" -Lines $timerProbe
    if ($null -ne $script:UsageTimer -and $script:UsageTimer.IsEnabled) {
      $failures += "usage refresh survived a panel change"
    }
    $report += "usage_timer=ok"

    # Leave the bar showing the usage panel, with its refresh armed, exactly as a
    # click on the item would.
    Set-HotbarPanelLines -Open $true -Panel "usage" -Lines $usageLines
    Start-HotbarUsagePanelTimer

    # Show it for real, then close it on a timer.
    Update-HotbarGeometry
    $timer = New-Object System.Windows.Threading.DispatcherTimer
    $timer.Interval = [TimeSpan]::FromMilliseconds($DisplayMs)
    $timer.Add_Tick({
        $timer.Stop()
        $window.Close()
      })
    # Explicit (priority, delegate) overloads on both Dispatcher and the timer.
    # The single-argument BeginInvoke([Action]) form depends on PowerShell
    # binding a params object[] through a non-UI thread, which is unreliable.
    $watchdog = New-Object System.Threading.Timer ([System.Threading.TimerCallback] {
        param($state)
        $null = $window.Dispatcher.BeginInvoke(
          [System.Windows.Threading.DispatcherPriority]::Background,
          [System.Action] { $window.Close() })
      }, $null, ($DisplayMs + 2000), 0)

    $timer.Start()
    # ShowDialog returns [bool]; a bare call would leak it into the report stream.
    $null = $window.ShowDialog()
    $timer.Stop()
    $watchdog.Dispose()

    if ($window.IsVisible) { $failures += "window did not close after the self test" }
    $report += ("shown_ms=" + $DisplayMs)
  } catch {
    $failures += $_.Exception.Message
  } finally {
    if ($null -ne $window) {
      try { $window.Close() } catch { }
    }
  }

  $watch.Stop()
  foreach ($line in $report) { Write-Output ("HOTBAR_SELFTEST " + $line) }
  Write-Output ("HOTBAR_SELFTEST elapsed_ms=" + $watch.ElapsedMilliseconds)

  # The exit code travels in a script variable, never as a return value: the
  # report above is written to the output stream, and a PowerShell function
  # returns its whole output stream, so `exit (Invoke-...)` would try to cast
  # an array of strings to an int.
  if ($failures.Count -gt 0) {
    foreach ($line in $failures) { Write-Output ("HOTBAR_SELFTEST check FAIL: " + $line) }
    Write-Output "HOTBAR_SELFTEST FAIL"
    $script:SelfTestExitCode = 1
  } else {
    Write-Output "HOTBAR_SELFTEST PASS"
    $script:SelfTestExitCode = 0
  }
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
$application = $null
if (-not [System.Windows.Application]::Current) {
  # One Application per AppDomain initialises the dispatcher properly. ShowDialog
  # is used to run the loop, so the STA thread is never blocked without pumping.
  $application = New-Object System.Windows.Application
}

if ($SelfTest) {
  Invoke-HotbarSelfTest -DisplayMs $SelfTestMs
  exit $script:SelfTestExitCode
}

$mutex = New-Object System.Threading.Mutex($false, $script:HotbarMutexName)
$owned = $false
try {
  $owned = $mutex.WaitOne(0)
} catch [System.Threading.AbandonedMutexException] {
  # A previous bar was killed without releasing: the handle is ours now.
  $owned = $true
}

if (-not $owned) {
  Write-Output "HOTBAR_ALREADY_RUNNING"
  exit 3
}

try {
  $null = Initialize-HotbarStateFromConfig
  $null = New-HotbarWindow
  # ShowDialog returns [bool]; keep it off the output stream.
  $null = $script:Window.ShowDialog()
  exit 0
} catch {
  Write-Output ("HOTBAR_ERROR " + $_.Exception.Message)
  exit 1
} finally {
  try { $mutex.ReleaseMutex() } catch { }
  $mutex.Dispose()
}
