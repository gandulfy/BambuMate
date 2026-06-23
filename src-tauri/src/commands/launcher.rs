use serde::Serialize;
use tracing::info;

use crate::profile::generator;

/// Result from launching Bambu Studio.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchResult {
    pub launched: bool,
    pub app_path: String,
    pub was_already_running: bool,
}

/// Detect the Bambu Studio application binary path.
///
/// Priority:
/// 1. Platform-specific default location
/// 2. Platform-specific search (Spotlight on macOS, registry/PATH on Windows)
///
/// Note: The "bambu_studio_path" preference stores the CONFIG DIRECTORY,
/// not the application binary path.
#[tauri::command]
pub async fn detect_bambu_studio_path(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_store::StoreExt;

    // 0. If the user saved a "bambu_studio_path" preference (config dir), try to
    //    infer the binary location from it before other searches. This allows the
    //    Settings UI to accept the config directory and still let the app find
    //    the BambuStudio executable.
    if let Ok(store) = app.store("preferences.json") {
        if let Some(pref_val) = store
            .get("bambu_studio_path")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            let pref_path = std::path::Path::new(&pref_val);
            if let Some(found) = find_bs_from_config_dir(pref_path) {
                return Ok(found);
            }
        }
    }

    // 1. Platform-specific default path
    if let Some(path) = default_bs_path() {
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
    }

    // 2. Platform-specific search
    if let Some(path) = search_bs_path() {
        return Ok(path);
    }

    Err("Bambu Studio not found. Please install it or set the path in Settings.".to_string())
}

/// Launch Bambu Studio with optional STL and profile file arguments.
#[tauri::command]
pub async fn launch_bambu_studio(
    app: tauri::AppHandle,
    stl_path: Option<String>,
    profile_path: Option<String>,
) -> Result<LaunchResult, String> {
    // Resolve BS path
    let bs_path = resolve_bs_path(&app)?;
    info!("Launching Bambu Studio from: {}", bs_path);

    let was_running = generator::is_bambu_studio_running();

    // Platform-specific launch
    launch_platform(&bs_path, stl_path.as_deref(), profile_path.as_deref())?;

    info!(
        "Bambu Studio launch initiated (was_already_running: {})",
        was_running
    );

    Ok(LaunchResult {
        launched: true,
        app_path: bs_path,
        was_already_running: was_running,
    })
}

/// Resolve the Bambu Studio application binary path.
///
/// Note: The "bambu_studio_path" preference stores the CONFIG DIRECTORY
/// (e.g., %APPDATA%\BambuStudio), not the application binary. We must not
/// use it here. Instead, we search for the actual binary.
fn resolve_bs_path(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_store::StoreExt;

    // 0. If the user saved a "bambu_studio_path" preference (config dir), try to
    //    infer the binary location from it. Many users point to the config
    //    directory (%APPDATA%\BambuStudio) when selecting the app in Settings.
    //    Attempt to locate BambuStudio.exe relative to that directory before
    //    falling back to global searches.
    if let Ok(store) = app.store("preferences.json") {
        if let Some(pref_val) = store
            .get("bambu_studio_path")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            let pref_path = std::path::Path::new(&pref_val);
            if let Some(found) = find_bs_from_config_dir(pref_path) {
                return Ok(found);
            }
        }
    }

    // 1. Platform-specific default path
    if let Some(path) = default_bs_path() {
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
    }

    // 2. Platform-specific search (Spotlight, registry, PATH, etc.)
    if let Some(path) = search_bs_path() {
        return Ok(path);
    }

    Err("Bambu Studio application not found. Please install Bambu Studio.".to_string())
}

/// Attempt to locate the BambuStudio binary by inspecting the provided
/// configuration directory and nearby common locations. Returns Some(path)
/// when found.
fn find_bs_from_config_dir(config_dir: &std::path::Path) -> Option<String> {
    use std::path::Path;

    if !config_dir.exists() {
        return None;
    }

    // Search upward through a few ancestor directories for a BambuStudio.exe
    for ancestor in config_dir.ancestors().take(6) {
        let cand = ancestor.join("BambuStudio.exe");
        if cand.exists() {
            return Some(cand.to_string_lossy().to_string());
        }
        let cand2 = ancestor.join("BambuStudio").join("BambuStudio.exe");
        if cand2.exists() {
            return Some(cand2.to_string_lossy().to_string());
        }
    }

    // Check common user-level program locations relative to local app data
    if let Some(local_data) = dirs::data_local_dir() {
        let cand = local_data.join("Programs").join("BambuStudio").join("BambuStudio.exe");
        if cand.exists() {
            return Some(cand.to_string_lossy().to_string());
        }
        let cand2 = local_data.join("BambuStudio").join("BambuStudio.exe");
        if cand2.exists() {
            return Some(cand2.to_string_lossy().to_string());
        }
    }

    // As a last resort, try looking at Program Files/Program Files (x86)
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        let cand = std::path::Path::new(&program_files).join("BambuStudio").join("BambuStudio.exe");
        if cand.exists() {
            return Some(cand.to_string_lossy().to_string());
        }
    }
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        let cand = std::path::Path::new(&program_files_x86).join("BambuStudio").join("BambuStudio.exe");
        if cand.exists() {
            return Some(cand.to_string_lossy().to_string());
        }
    }

    None
}


/// Get the platform-specific default Bambu Studio path.
#[cfg(target_os = "macos")]
fn default_bs_path() -> Option<String> {
    Some("/Applications/BambuStudio.app".to_string())
}

#[cfg(target_os = "windows")]
fn default_bs_path() -> Option<String> {
    // Common install locations on Windows
    let candidates = [
        r"C:\Program Files\BambuStudio\BambuStudio.exe",
        r"C:\Program Files (x86)\BambuStudio\BambuStudio.exe",
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Also check %PROGRAMFILES% environment variable
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        let path = std::path::PathBuf::from(&program_files)
            .join("BambuStudio")
            .join("BambuStudio.exe");
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn default_bs_path() -> Option<String> {
    let candidates = [
        "/usr/bin/BambuStudio",
        "/opt/BambuStudio/BambuStudio",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn default_bs_path() -> Option<String> {
    None
}

/// Platform-specific search for Bambu Studio.
#[cfg(target_os = "macos")]
fn search_bs_path() -> Option<String> {
    // Spotlight search
    if let Ok(output) = std::process::Command::new("mdfind")
        .arg("kMDItemCFBundleIdentifier == 'com.bambulab.bambu-studio'")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && std::path::Path::new(trimmed).exists() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn search_bs_path() -> Option<String> {
    // Search in PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("BambuStudio.exe")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && std::path::Path::new(trimmed).exists() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    // Search in common user install locations
    if let Some(local_data) = dirs::data_local_dir() {
        let path = local_data
            .join("BambuStudio")
            .join("BambuStudio.exe");
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn search_bs_path() -> Option<String> {
    if let Ok(output) = std::process::Command::new("which")
        .arg("BambuStudio")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let trimmed = stdout.trim();
            if !trimmed.is_empty() && std::path::Path::new(trimmed).exists() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn search_bs_path() -> Option<String> {
    None
}

/// Platform-specific application launch.
#[cfg(target_os = "macos")]
fn launch_platform(
    bs_path: &str,
    stl_path: Option<&str>,
    profile_path: Option<&str>,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new("open");
    cmd.arg("-a").arg(bs_path);

    let mut has_args = false;

    if let Some(stl) = stl_path {
        if std::path::Path::new(stl).exists() {
            if !has_args {
                cmd.arg("--args");
                has_args = true;
            }
            cmd.arg(stl);
            info!("  with STL: {}", stl);
        }
    }

    if let Some(profile) = profile_path {
        if std::path::Path::new(profile).exists() {
            if !has_args {
                cmd.arg("--args");
            }
            cmd.arg("--load-filaments").arg(profile);
            info!("  with profile: {}", profile);
        }
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to launch Bambu Studio: {}", e))?;
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn launch_platform(
    bs_path: &str,
    stl_path: Option<&str>,
    profile_path: Option<&str>,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new(bs_path);

    if let Some(stl) = stl_path {
        if std::path::Path::new(stl).exists() {
            cmd.arg(stl);
            info!("  with STL: {}", stl);
        }
    }

    if let Some(profile) = profile_path {
        if std::path::Path::new(profile).exists() {
            cmd.arg("--load-filaments").arg(profile);
            info!("  with profile: {}", profile);
        }
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to launch Bambu Studio: {}", e))?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn launch_platform(
    _bs_path: &str,
    _stl_path: Option<&str>,
    _profile_path: Option<&str>,
) -> Result<(), String> {
    Err("Unsupported platform for launching Bambu Studio".to_string())
}

/// Open an external URL in the system's default browser.
/// Uses the tauri-plugin-opener to handle cross-platform URL opening.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    info!("Opening external URL: {}", url);
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

/// Build the Tauri application (cargo build) and launch the resulting binary.
/// If `release` is true, builds with --release and launches the release binary.
#[tauri::command]
pub async fn build_and_launch_app(release: bool) -> Result<String, String> {
    use std::process::Command;
    use std::path::PathBuf;

    // Run the build in a blocking thread to avoid blocking the async runtime
    let res = std::thread::spawn(move || {
        // Determine build directory: prefer current dir if it contains Cargo.toml, else try ./src-tauri
        let mut cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if !cwd.join("Cargo.toml").exists() {
            let candidate = cwd.join("src-tauri");
            if candidate.join("Cargo.toml").exists() {
                cwd = candidate;
            }
        }

        // Build command
        let mut build_cmd = Command::new("cargo");
        build_cmd.arg("build");
        if release {
            build_cmd.arg("--release");
        }
        build_cmd.current_dir(&cwd);

        let status = build_cmd
            .status()
            .map_err(|e| format!("Failed to spawn cargo build: {}", e))?;

        if !status.success() {
            return Err(format!("cargo build failed with status: {}", status));
        }

        // Determine binary path
        let target_dir = cwd.join("target").join(if release { "release" } else { "debug" });
        // Binary name (platform-specific)
        let bin_name = if cfg!(windows) { "bambumate-tauri.exe" } else { "bambumate-tauri" };
        let bin_path = target_dir.join(bin_name);
        if !bin_path.exists() {
            return Err(format!("Built binary not found at {}", bin_path.display()));
        }

        // Launch the binary
        Command::new(bin_path.clone())
            .spawn()
            .map_err(|e| format!("Failed to launch built binary: {}", e))?;

        Ok(bin_path.to_string_lossy().to_string())
    })
    .join();

    match res {
        Ok(Ok(path)) => Ok(path),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Build thread panicked: {:?}", e)),
    }
}
