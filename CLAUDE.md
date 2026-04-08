# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

v2rayN is a GUI client for Windows, Linux and macOS that supports Xray, sing-box, and other proxy cores. It's a .NET 8 application using MVVM architecture with ReactiveUI.

## Project Structure

The solution is located in `/Users/dywang/Code/github/NetworkExtensions/v2rayN/v2rayN/` and consists of the following projects:

| Project | Description |
|---------|-------------|
| `ServiceLib` | Core library containing business logic, models, handlers, and services. Shared between all UI implementations. |
| `v2rayN` | WPF-based Windows application (net8.0-windows10.0.19041.0). Uses MaterialDesignThemes. |
| `v2rayN.Desktop` | Avalonia-based cross-platform application for Linux/macOS. Uses Semi.Avalonia theme. |
| `AmazTool` | Command-line utility tool for self-updates and reboot operations. |
| `GlobalHotKeys` | Global hotkey support for the Avalonia desktop application (Git submodule). |

### ServiceLib Architecture

The core library (`ServiceLib/`) is organized as:

- **`Handler/`** - Core business logic handlers (ConfigHandler, CoreConfigHandler, SubscriptionHandler, etc.)
  - `Fmt/` - Protocol format handlers (Vmess, Vless, Trojan, etc.)
  - `SysProxy/` - System proxy configuration handlers
  - `Builder/` - Core configuration builders (Xray, sing-box)
- **`Services/`** - Core services
  - `CoreConfig/` - Core configuration generation services
  - `Statistics/` - Traffic statistics services
- **`ViewModels/`** - Shared ViewModels used by both WPF and Avalonia UIs
- **`Models/`** - Data models (Config, ProfileItem, etc.)
- **`Common/`** - Utility classes and helpers
- **`Resx/`** - Localization resources (supports zh-Hans, zh-Hant, fa-Ir, fr, hu, ru)
- **`Sample/`** - Embedded resource templates for configurations

## Build Commands

All commands should be run from the `v2rayN/` directory containing the solution file.

### Prerequisites

- .NET 8.0 SDK
- For Windows builds on Linux: Enable Windows targeting (`-p:EnableWindowsTargeting=true`)

### Build for Development

```bash
cd v2rayN

# Build solution
dotnet build v2rayN.sln

# Build specific project
dotnet build ServiceLib/ServiceLib.csproj
dotnet build v2rayN/v2rayN.csproj
dotnet build v2rayN.Desktop/v2rayN.Desktop.csproj
```

### Publish for Release

**Windows (from Ubuntu):**
```bash
cd v2rayN
dotnet publish ./v2rayN/v2rayN.csproj -c Release -r win-x64 -p:SelfContained=true -p:EnableWindowsTargeting=true -o ./Release/windows-64
dotnet publish ./AmazTool/AmazTool.csproj -c Release -r win-x64 -p:SelfContained=true -p:EnableWindowsTargeting=true -p:PublishTrimmed=true -o ./Release/windows-64
```

**Linux:**
```bash
cd v2rayN
dotnet publish ./v2rayN.Desktop/v2rayN.Desktop.csproj -c Release -r linux-x64 -p:SelfContained=true -o ./Release/linux-64
dotnet publish ./AmazTool/AmazTool.csproj -c Release -r linux-x64 -p:SelfContained=true -p:PublishTrimmed=true -o ./Release/linux-64
```

**macOS:**
```bash
cd v2rayN
dotnet publish ./v2rayN.Desktop/v2rayN.Desktop.csproj -c Release -r osx-x64 -p:SelfContained=true -o ./Release/macos-64
dotnet publish ./AmazTool/AmazTool.csproj -c Release -r osx-x64 -p:SelfContained=true -p:PublishTrimmed=true -o ./Release/macos-64
```

### Packaging

Root-level scripts handle packaging:

```bash
# Create release zip
./package-release-zip.sh <arch> <output_path>

# Create Debian package (requires Debian-based system)
./package-debian.sh <release_tag> --arch all

# Create RPM package (requires RHEL-based system)
./package-rhel.sh <release_tag> --arch all

# Create macOS DMG (requires macOS with create-dmg)
./package-osx.sh <arch> <output_path> <release_tag>
```

## Code Style

The project uses `.editorconfig` with the following key conventions:

- **Indentation**: 4 spaces (2 for YAML)
- **Line endings**: CRLF for all files
- **Usings**: Outside namespace, System directives first
- **Namespace**: File-scoped namespaces preferred
- **Braces**: Required for all control blocks (`csharp_prefer_braces = true`)
- **Var**: Use var everywhere (`csharp_style_var_* = true`)
- **Expression-bodied members**: Only for accessors/indexers on single line
- **Collection initializers**: Object initializers preferred, collection expressions not preferred
- **Static members**: Prefer static local/anonymous functions

## Key Patterns

### Handler Pattern

Business logic is organized in static handler classes in `ServiceLib/Handler/`:

```csharp
public static class ConfigHandler
{
    public static Config? LoadConfig() { ... }
    public static int SaveConfig(Config config) { ... }
}
```

### ViewModel Pattern

ViewModels use ReactiveUI with Fody weaving for automatic property change notification:

```csharp
public class MainViewModel : MyReactiveObject
{
    [Reactive]
    public string SelectedServer { get; set; }
}
```

### Localization

Strings are localized via `ResUI.resx` files in `ServiceLib/Resx/`. Access via:

```csharp
var message = ResUI.MsgOperationSuccess;
```

### Configuration Storage

User configuration is stored in JSON format. The `Config` model defines the structure, with `ConfigHandler` handling persistence.

## Runtime Identifiers

| Platform | RID |
|----------|-----|
| Windows x64 | `win-x64` |
| Windows ARM64 | `win-arm64` |
| Linux x64 | `linux-x64` |
| Linux ARM64 | `linux-arm64` |
| macOS x64 | `osx-x64` |
| macOS ARM64 | `osx-arm64` |

## CI/CD Workflows

GitHub Actions workflows in `.github/workflows/`:

- `build-windows.yml` - Builds Windows releases (runs on Ubuntu)
- `build-linux.yml` - Builds Linux releases and creates DEB/RPM packages
- `build-osx.yml` - Builds macOS releases and creates DMG
- `build-all.yml` - Orchestrates all platform builds

## Important Notes

- The project uses central package management (`Directory.Packages.props`)
- Release builds are self-contained single-file executables
- `AmazTool` is published with trimming enabled
- The Windows build uses WPF with MaterialDesign themes
- The cross-platform build uses Avalonia with Semi.Avalonia theme
- Global hotkeys are implemented via a separate GlobalHotKeys project (submodule)
