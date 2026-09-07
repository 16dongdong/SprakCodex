use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const CLI_TIMEOUT: Duration = Duration::from_secs(90);
const CLI_POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BYTES: u64 = 512 * 1024;
const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;
const MAX_SKILLS_PER_PLUGIN: usize = 256;
const STALE_STAGING_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSkillsMarketplaceInventory {
    pub cli_available: bool,
    pub codex_home: String,
    pub marketplaces: Vec<CodexMarketplaceSummary>,
    pub plugins: Vec<CodexMarketplacePluginSummary>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexMarketplaceSummary {
    pub name: String,
    pub source_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexMarketplacePluginSummary {
    pub plugin_id: String,
    pub name: String,
    pub marketplace_name: String,
    pub version: String,
    pub installed: bool,
    pub enabled: bool,
    pub description: String,
    pub author: String,
    pub category: String,
    pub skills: Vec<CodexMarketplaceSkillSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexMarketplaceSkillSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
struct MarketplaceRecord {
    summary: CodexMarketplaceSummary,
    canonical_root: Option<PathBuf>,
}

#[derive(Debug)]
struct CliOutput {
    stdout: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedCliCommand {
    executable: PathBuf,
    search_path: OsString,
}

#[derive(Debug)]
enum CliRunError {
    Unavailable(String),
    Unsupported(String),
    Failed(String),
}

impl CliRunError {
    fn into_message(self) -> String {
        match self {
            Self::Unavailable(message) | Self::Unsupported(message) | Self::Failed(message) => {
                message
            }
        }
    }
}

trait CliRunner {
    fn run(&self, args: &[String], codex_home: &Path) -> Result<CliOutput, CliRunError>;
}

struct SystemCliRunner;

impl CliRunner for SystemCliRunner {
    fn run(&self, args: &[String], codex_home: &Path) -> Result<CliOutput, CliRunError> {
        run_system_cli(args, codex_home)
    }
}

#[derive(Debug)]
struct CapturedStream {
    bytes: Vec<u8>,
    truncated: bool,
}

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn list(codex_home: Option<&str>) -> Result<CodexSkillsMarketplaceInventory, String> {
    let codex_home = resolve_absolute_codex_home(codex_home)?;
    list_with_runner(&codex_home, &SystemCliRunner)
}

pub(crate) fn add(
    source: Option<&str>,
    ref_name: Option<&str>,
    codex_home: Option<&str>,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    let source = source
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing source".to_string())?;
    let source = normalize_github_source(source)?;
    let ref_name = normalize_ref_name(ref_name)?;
    let codex_home = resolve_absolute_codex_home(codex_home)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Codex marketplace mutation lock poisoned".to_string())?;
    let staging_warnings = stale_staging_warnings(&codex_home);

    let mut args = strings(&["plugin", "marketplace", "add", "--json"]);
    if let Some(ref_name) = ref_name {
        args.push("--ref".to_string());
        args.push(ref_name);
    }
    args.push("--".to_string());
    args.push(source);
    run_cli_json(&SystemCliRunner, &args, &codex_home).map_err(CliRunError::into_message)?;
    let mut inventory = list_with_runner(&codex_home, &SystemCliRunner)?;
    inventory.warnings.extend(staging_warnings);
    Ok(inventory)
}

pub(crate) fn refresh(
    marketplace_name: Option<&str>,
    codex_home: Option<&str>,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    let marketplace_name = marketplace_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if valid_marketplace_name(value) {
                Ok(value.to_string())
            } else {
                Err("invalid marketplaceName".to_string())
            }
        })
        .transpose()?;
    let codex_home = resolve_absolute_codex_home(codex_home)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Codex marketplace mutation lock poisoned".to_string())?;
    let staging_warnings = stale_staging_warnings(&codex_home);

    if let Some(expected) = marketplace_name.as_deref() {
        let inventory = list_with_runner(&codex_home, &SystemCliRunner)?;
        if !inventory.cli_available {
            return Err(inventory
                .warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "Codex plugin CLI is unavailable".to_string()));
        }
        if !inventory
            .marketplaces
            .iter()
            .any(|marketplace| marketplace.name == expected)
        {
            return Err("marketplace is not configured".to_string());
        }
    }

    let mut args = strings(&["plugin", "marketplace", "upgrade", "--json"]);
    if let Some(marketplace_name) = marketplace_name {
        args.push("--".to_string());
        args.push(marketplace_name);
    }
    run_cli_json(&SystemCliRunner, &args, &codex_home).map_err(CliRunError::into_message)?;
    let mut inventory = list_with_runner(&codex_home, &SystemCliRunner)?;
    inventory.warnings.extend(staging_warnings);
    Ok(inventory)
}

pub(crate) fn install(
    plugin_id: Option<&str>,
    codex_home: Option<&str>,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    let plugin_id = plugin_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing pluginId".to_string())?;
    if !valid_plugin_id(plugin_id) {
        return Err("invalid pluginId".to_string());
    }
    let codex_home = resolve_absolute_codex_home(codex_home)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Codex marketplace mutation lock poisoned".to_string())?;
    let staging_warnings = stale_staging_warnings(&codex_home);

    let mut inventory = install_with_runner(plugin_id, &codex_home, &SystemCliRunner)?;
    inventory.warnings.extend(staging_warnings);
    Ok(inventory)
}

fn install_with_runner(
    plugin_id: &str,
    codex_home: &Path,
    runner: &dyn CliRunner,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    // The inventory is deliberately rebuilt immediately before installation. The client never
    // gets to supply a filesystem path or substitute a plugin that was filtered out.
    let inventory = list_with_runner(codex_home, runner)?;
    if !inventory.cli_available {
        return Err(inventory
            .warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "Codex plugin CLI is unavailable".to_string()));
    }
    let plugin = inventory
        .plugins
        .iter()
        .find(|plugin| plugin.plugin_id == plugin_id)
        .ok_or_else(|| "plugin is not a compatible Codex Skills plugin".to_string())?;
    if plugin.installed {
        return Ok(inventory);
    }

    let args = vec![
        "plugin".to_string(),
        "add".to_string(),
        "--json".to_string(),
        "--".to_string(),
        plugin_id.to_string(),
    ];
    run_cli_json(runner, &args, codex_home).map_err(CliRunError::into_message)?;
    let inventory = list_with_runner(codex_home, runner)?;
    if !inventory
        .plugins
        .iter()
        .any(|plugin| plugin.plugin_id == plugin_id && plugin.installed)
    {
        return Err("Codex CLI did not confirm that the plugin was installed".to_string());
    }
    Ok(inventory)
}

fn list_with_runner(
    codex_home: &Path,
    runner: &dyn CliRunner,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    let codex_home_display = codex_home.to_string_lossy().to_string();
    let marketplace_json = match run_cli_json(
        runner,
        &strings(&["plugin", "marketplace", "list", "--json"]),
        codex_home,
    ) {
        Ok(value) => value,
        Err(CliRunError::Unavailable(message) | CliRunError::Unsupported(message)) => {
            return Ok(CodexSkillsMarketplaceInventory {
                cli_available: false,
                codex_home: codex_home_display,
                marketplaces: Vec::new(),
                plugins: Vec::new(),
                warnings: vec![message],
            });
        }
        Err(err) => return Err(err.into_message()),
    };
    let plugins_json = match run_cli_json(
        runner,
        &strings(&["plugin", "list", "--available", "--json"]),
        codex_home,
    ) {
        Ok(value) => value,
        Err(CliRunError::Unavailable(message) | CliRunError::Unsupported(message)) => {
            return Ok(CodexSkillsMarketplaceInventory {
                cli_available: false,
                codex_home: codex_home_display,
                marketplaces: Vec::new(),
                plugins: Vec::new(),
                warnings: vec![message],
            });
        }
        Err(err) => return Err(err.into_message()),
    };

    build_inventory(codex_home, &marketplace_json, &plugins_json)
}

fn run_cli_json(
    runner: &dyn CliRunner,
    args: &[String],
    codex_home: &Path,
) -> Result<Value, CliRunError> {
    let output = runner.run(args, codex_home)?;
    serde_json::from_str(&output.stdout)
        .map_err(|_| CliRunError::Failed("Codex CLI returned invalid JSON".to_string()))
}

fn run_system_cli(args: &[String], codex_home: &Path) -> Result<CliOutput, CliRunError> {
    let resolved = resolve_system_cli()?;
    run_resolved_cli(&resolved, args, codex_home)
}

fn run_resolved_cli(
    resolved: &ResolvedCliCommand,
    args: &[String],
    codex_home: &Path,
) -> Result<CliOutput, CliRunError> {
    let mut command = Command::new(&resolved.executable);
    command
        .args(args)
        .env("PATH", &resolved.search_path)
        .env("CODEX_HOME", codex_home)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            CliRunError::Unavailable("Codex CLI was not found on PATH".to_string())
        } else {
            CliRunError::Failed(format!("failed to start Codex CLI: {err}"))
        }
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliRunError::Failed("failed to capture Codex CLI stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CliRunError::Failed("failed to capture Codex CLI stderr".to_string()))?;
    let stdout_rx = capture_stream(stdout, MAX_STDOUT_BYTES);
    let stderr_rx = capture_stream(stderr, MAX_STDERR_BYTES);

    let deadline = Instant::now() + CLI_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(CLI_POLL_INTERVAL),
            Ok(None) => {
                terminate_child(&mut child);
                let _ = child.wait();
                return Err(CliRunError::Failed(format!(
                    "Codex CLI timed out after {} seconds",
                    CLI_TIMEOUT.as_secs()
                )));
            }
            Err(err) => {
                terminate_child(&mut child);
                let _ = child.wait();
                return Err(CliRunError::Failed(format!(
                    "failed while waiting for Codex CLI: {err}"
                )));
            }
        }
    };
    let stdout = receive_captured_stream(stdout_rx, deadline, "stdout").map_err(|err| {
        terminate_child(&mut child);
        err
    })?;
    let stderr = receive_captured_stream(stderr_rx, deadline, "stderr").map_err(|err| {
        terminate_child(&mut child);
        err
    })?;
    let stdout_text = captured_text(stdout);
    let stderr_text = captured_text(stderr);

    if !status.success() {
        let detail = if stderr_text.trim().is_empty() {
            stdout_text.trim()
        } else {
            stderr_text.trim()
        };
        let message = if detail.is_empty() {
            format!("Codex CLI exited with {status}")
        } else {
            format!("Codex CLI exited with {status}: {detail}")
        };
        if cli_plugin_commands_unsupported(detail) {
            return Err(CliRunError::Unsupported(
                "Installed Codex CLI does not support plugin marketplaces".to_string(),
            ));
        }
        return Err(CliRunError::Failed(message));
    }

    Ok(CliOutput {
        stdout: stdout_text,
    })
}

fn canonical_search_directories<I>(directories: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut resolved = Vec::new();
    for directory in directories {
        // Empty and relative PATH entries can resolve against an attacker-controlled cwd.
        if directory.as_os_str().is_empty() || !directory.is_absolute() {
            continue;
        }
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if !directory.is_dir() || resolved.contains(&directory) {
            continue;
        }
        resolved.push(directory);
    }
    resolved
}

fn resolved_cli_command(
    executable: PathBuf,
    search_directories: &[PathBuf],
) -> Result<ResolvedCliCommand, CliRunError> {
    let search_path = std::env::join_paths(search_directories).map_err(|err| {
        CliRunError::Failed(format!("failed to construct a safe Codex CLI PATH: {err}"))
    })?;
    Ok(ResolvedCliCommand {
        executable,
        search_path,
    })
}

#[cfg(any(target_os = "windows", test))]
const WINDOWS_CODEX_EXTENSIONS: [&str; 4] = ["com", "exe", "bat", "cmd"];

#[cfg(any(target_os = "windows", test))]
fn find_windows_codex(search_directories: &[PathBuf]) -> Option<PathBuf> {
    for directory in search_directories {
        // Do not consider the extensionless npm POSIX shim on Windows. Rust only infers `.exe`
        // for a bare command, while npm's runnable Windows entry point is normally `codex.cmd`.
        for extension in WINDOWS_CODEX_EXTENSIONS {
            let candidate = directory.join(format!("codex.{extension}"));
            if !candidate.is_file() {
                continue;
            }
            if let Ok(candidate) = candidate.canonicalize() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(any(target_os = "windows", test))]
fn codex_desktop_search_directories(local_app_data: &Path) -> Vec<PathBuf> {
    let bin_root = local_app_data.join("OpenAI").join("Codex").join("bin");
    let mut result = vec![bin_root.clone()];
    let mut versioned_directories = fs::read_dir(&bin_root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            metadata.is_dir().then(|| (path, metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    // Codex Desktop keeps CLI payloads under opaque version/hash directories. Prefer the newest
    // payload while keeping a deterministic path tie-breaker.
    versioned_directories.sort_by(|(left_path, left_modified), (right_path, right_modified)| {
        right_modified
            .cmp(left_modified)
            .then_with(|| left_path.cmp(right_path))
    });
    result.extend(versioned_directories.into_iter().map(|(path, _)| path));
    result
}

#[cfg(any(target_os = "windows", test))]
fn windows_known_search_directories(
    nvm_symlink: Option<PathBuf>,
    app_data: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(nvm_symlink) = nvm_symlink {
        result.push(nvm_symlink);
    }
    if let Some(app_data) = app_data {
        result.push(app_data.join("npm"));
    }
    if let Some(local_app_data) = local_app_data {
        result.extend(codex_desktop_search_directories(&local_app_data));
        result.push(
            local_app_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("resources"),
        );
        result.push(
            local_app_data
                .join("Programs")
                .join("Codex")
                .join("resources"),
        );
    }
    result
}

#[cfg(target_os = "windows")]
fn resolve_system_cli() -> Result<ResolvedCliCommand, CliRunError> {
    let mut candidates = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    candidates.extend(windows_known_search_directories(
        std::env::var_os("NVM_SYMLINK").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
    ));
    let search_directories = canonical_search_directories(candidates);
    let executable = find_windows_codex(&search_directories)
        .ok_or_else(|| CliRunError::Unavailable("Codex CLI was not found on PATH".to_string()))?;
    resolved_cli_command(executable, &search_directories)
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(unix)]
fn find_unix_codex(search_directories: &[PathBuf]) -> Option<PathBuf> {
    search_directories.iter().find_map(|directory| {
        let candidate = directory.join("codex");
        if !is_executable_file(&candidate) {
            return None;
        }
        candidate.canonicalize().ok()
    })
}

#[cfg(unix)]
fn resolve_system_cli() -> Result<ResolvedCliCommand, CliRunError> {
    let candidates = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let search_directories = canonical_search_directories(candidates);
    let executable = find_unix_codex(&search_directories)
        .ok_or_else(|| CliRunError::Unavailable("Codex CLI was not found on PATH".to_string()))?;
    resolved_cli_command(executable, &search_directories)
}

fn capture_stream(
    stream: impl Read + Send + 'static,
    max_bytes: usize,
) -> mpsc::Receiver<Result<CapturedStream, std::io::Error>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_stream_bounded(stream, max_bytes));
    });
    receiver
}

fn receive_captured_stream(
    receiver: mpsc::Receiver<Result<CapturedStream, std::io::Error>>,
    deadline: Instant,
    stream_name: &str,
) -> Result<CapturedStream, CliRunError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match receiver.recv_timeout(remaining) {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(err)) => Err(CliRunError::Failed(format!(
            "failed to read Codex CLI {stream_name}: {err}"
        ))),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(CliRunError::Failed(format!(
            "Codex CLI timed out after {} seconds",
            CLI_TIMEOUT.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CliRunError::Failed(format!(
            "Codex CLI {stream_name} reader stopped unexpectedly"
        ))),
    }
}

fn read_stream_bounded(
    mut stream: impl Read,
    max_bytes: usize,
) -> Result<CapturedStream, std::io::Error> {
    let mut stored = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(stored.len());
        if remaining > 0 {
            stored.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        if read > remaining {
            truncated = true;
        }
    }
    Ok(CapturedStream {
        bytes: stored,
        truncated,
    })
}

fn captured_text(stream: CapturedStream) -> String {
    let mut text = String::from_utf8_lossy(&stream.bytes).into_owned();
    if stream.truncated {
        text.push_str("\n[output truncated]");
    }
    text
}

fn terminate_child(child: &mut std::process::Child) {
    let root_pid = sysinfo::Pid::from_u32(child.id());
    let system = sysinfo::System::new_all();
    let mut descendants = HashSet::from([root_pid]);
    loop {
        let mut changed = false;
        for (pid, process) in system.processes() {
            if process
                .parent()
                .is_some_and(|parent| descendants.contains(&parent))
                && descendants.insert(*pid)
            {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for pid in descendants.iter().filter(|pid| **pid != root_pid) {
        if let Some(process) = system.process(*pid) {
            let _ = process
                .kill_with(sysinfo::Signal::Kill)
                .unwrap_or_else(|| process.kill());
        }
    }
    #[cfg(unix)]
    unsafe {
        let process_group = -(child.id() as i32);
        libc::kill(process_group, libc::SIGKILL);
    }
    let _ = child.kill();
}

fn cli_plugin_commands_unsupported(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("unrecognized subcommand 'plugin'")
        || message.contains("unrecognized subcommand ‘plugin’")
        || message.contains("unrecognized subcommand 'marketplace'")
        || message.contains("unrecognized subcommand ‘marketplace’")
}

fn stale_staging_warnings(codex_home: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let remote_staging = codex_home
        .join("plugins")
        .join(".remote-plugin-install-staging");
    if stale_directory(&remote_staging)
        && fs::read_dir(&remote_staging)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some()
    {
        warnings.push(
            "stale remote plugin staging data was left untouched; remove it after confirming no other Codex process is installing plugins"
                .to_string(),
        );
    }

    let marketplace_staging = codex_home
        .join(".tmp")
        .join("marketplaces")
        .join(".staging");
    if !unsafe_link_or_reparse(&marketplace_staging) {
        if let Ok(entries) = fs::read_dir(&marketplace_staging) {
            let stale_add_directory = entries.take(256).flatten().any(|entry| {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    return false;
                };
                let Some(suffix) = name.strip_prefix("marketplace-add-") else {
                    return false;
                };
                !suffix.is_empty()
                    && suffix.len() <= 128
                    && suffix.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                    && stale_directory(&entry.path())
            });
            if stale_add_directory {
                warnings.push(
                    "stale marketplace-add staging data was left untouched; remove it after confirming no other Codex process is adding a marketplace"
                        .to_string(),
                );
            }
        }
    }
    warnings
}

fn stale_directory(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&metadata) {
        return false;
    }
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= STALE_STAGING_AGE)
}

fn build_inventory(
    codex_home: &Path,
    marketplace_json: &Value,
    plugins_json: &Value,
) -> Result<CodexSkillsMarketplaceInventory, String> {
    let marketplace_values = marketplace_json
        .get("marketplaces")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex CLI marketplace JSON is missing marketplaces".to_string())?;
    let mut records = HashMap::new();
    for value in marketplace_values {
        let Some(name) = bounded_json_string(value.get("name"), 128) else {
            continue;
        };
        if !valid_marketplace_name(&name) || records.contains_key(&name) {
            continue;
        }
        let source_value = value.get("marketplaceSource");
        let source_type = source_value
            .and_then(|value| bounded_json_string(value.get("sourceType"), 64))
            .unwrap_or_default();
        let source = source_value
            .and_then(|value| bounded_json_string(value.get("source"), 2048))
            .unwrap_or_default();
        let canonical_root = value
            .get("root")
            .and_then(Value::as_str)
            .filter(|root| !root.is_empty())
            .map(Path::new)
            .filter(|root| root.is_absolute())
            .and_then(canonical_directory);
        records.insert(
            name.clone(),
            MarketplaceRecord {
                summary: CodexMarketplaceSummary {
                    name,
                    source_type,
                    source,
                },
                canonical_root,
            },
        );
    }

    let installed_values = plugins_json
        .get("installed")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex CLI plugin JSON is missing installed".to_string())?;
    let available_values = plugins_json
        .get("available")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex CLI plugin JSON is missing available".to_string())?;
    let mut plugins = BTreeMap::new();
    for (values, installed_bucket) in [
        (available_values.as_slice(), false),
        (installed_values.as_slice(), true),
    ] {
        for value in values {
            let Some(plugin) = compatible_plugin(value, installed_bucket, &records) else {
                continue;
            };
            plugins.insert(plugin.plugin_id.clone(), plugin);
        }
    }

    let mut marketplaces = records
        .into_values()
        .map(|record| record.summary)
        .collect::<Vec<_>>();
    marketplaces.sort_by(|left, right| left.name.cmp(&right.name));
    let mut plugins = plugins.into_values().collect::<Vec<_>>();
    plugins.sort_by(|left, right| {
        left.marketplace_name
            .cmp(&right.marketplace_name)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.version.cmp(&right.version))
    });

    Ok(CodexSkillsMarketplaceInventory {
        cli_available: true,
        codex_home: codex_home.to_string_lossy().to_string(),
        marketplaces,
        plugins,
        warnings: Vec::new(),
    })
}

fn compatible_plugin(
    value: &Value,
    installed_bucket: bool,
    marketplaces: &HashMap<String, MarketplaceRecord>,
) -> Option<CodexMarketplacePluginSummary> {
    let plugin_id = bounded_json_string(value.get("pluginId"), 260)?;
    let name = bounded_json_string(value.get("name"), 128)?;
    let marketplace_name = bounded_json_string(value.get("marketplaceName"), 128)?;
    let cli_version = bounded_json_string(value.get("version"), 128)?;
    if !valid_plugin_id(&plugin_id)
        || !valid_plugin_name(&name)
        || !valid_marketplace_name(&marketplace_name)
        || plugin_id != format!("{name}@{marketplace_name}")
    {
        return None;
    }
    let install_policy = value.get("installPolicy")?.as_str()?;
    if !matches!(install_policy, "AVAILABLE" | "INSTALLED_BY_DEFAULT") {
        return None;
    }
    let marketplace_root = marketplaces
        .get(&marketplace_name)?
        .canonical_root
        .as_ref()?;
    let source = value.get("source")?;
    if source.get("source")?.as_str()? != "local" {
        return None;
    }
    let source_path = Path::new(source.get("path")?.as_str()?);
    if !source_path.is_absolute() || unsafe_link_or_reparse(source_path) {
        return None;
    }
    let plugin_root = canonical_directory(source_path)?;
    if plugin_root == *marketplace_root || !plugin_root.starts_with(marketplace_root) {
        return None;
    }

    let manifest_directory_path = plugin_root.join(".codex-plugin");
    if unsafe_link_or_reparse(&manifest_directory_path) {
        return None;
    }
    let manifest_directory = canonical_directory(&manifest_directory_path)?;
    if manifest_directory == plugin_root || !manifest_directory.starts_with(&plugin_root) {
        return None;
    }
    let manifest_path = manifest_directory.join("plugin.json");
    let manifest_text = read_regular_text_bounded(&manifest_path, MAX_MANIFEST_BYTES).ok()?;
    let manifest: Value = serde_json::from_str(&manifest_text).ok()?;
    if manifest.get("name")?.as_str()? != name {
        return None;
    }
    let version = bounded_json_string(manifest.get("version"), 128)?;
    // For available plugins, the CLI version must describe the source we just inspected. Once a
    // plugin is installed, Codex may report its immutable cache revision (for example a commit
    // hash) instead of the manifest's semantic version. Keep accepting only the inspected
    // manifest version for display while treating the installed bucket as authoritative state.
    if !installed_bucket && cli_version != version {
        return None;
    }
    let skills_relative = manifest.get("skills")?.as_str()?.trim();
    if !skills_relative.starts_with("./") || !safe_relative_path(skills_relative) {
        return None;
    }
    let skills_path = plugin_root.join(skills_relative);
    if unsafe_link_or_reparse(&skills_path) {
        return None;
    }
    let skills_root = canonical_directory(&skills_path)?;
    if skills_root == plugin_root || !skills_root.starts_with(&plugin_root) {
        return None;
    }
    let skills = read_standard_skills(&skills_root, &plugin_root);
    if skills.is_empty() {
        return None;
    }

    let description = bounded_json_string(manifest.get("description"), 8192).unwrap_or_default();
    let author = manifest
        .get("author")
        .and_then(|author| {
            bounded_json_string(Some(author), 512)
                .or_else(|| bounded_json_string(author.get("name"), 512))
        })
        .unwrap_or_default();
    let category = manifest
        .get("interface")
        .and_then(|interface| bounded_json_string(interface.get("category"), 256))
        .unwrap_or_default();
    let installed = value
        .get("installed")
        .and_then(Value::as_bool)
        .unwrap_or(installed_bucket);
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(CodexMarketplacePluginSummary {
        plugin_id,
        name,
        marketplace_name,
        version,
        installed,
        enabled,
        description,
        author,
        category,
        skills,
    })
}

fn read_standard_skills(
    skills_root: &Path,
    plugin_root: &Path,
) -> Vec<CodexMarketplaceSkillSummary> {
    let Ok(entries) = fs::read_dir(skills_root) else {
        return Vec::new();
    };
    let mut skills = Vec::new();
    let mut seen_names = HashSet::new();
    for entry in entries.take(MAX_SKILLS_PER_PLUGIN + 1) {
        if skills.len() >= MAX_SKILLS_PER_PLUGIN {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let directory_name = entry.file_name();
        let Some(directory_name) = directory_name.to_str() else {
            continue;
        };
        let path = entry.path();
        let Some(canonical_path) = canonical_directory(&path) else {
            continue;
        };
        if canonical_path == *plugin_root || !canonical_path.starts_with(plugin_root) {
            continue;
        }
        let Ok(text) =
            read_regular_text_bounded(&canonical_path.join("SKILL.md"), MAX_SKILL_MD_BYTES)
        else {
            continue;
        };
        let Some(skill) = parse_standard_skill_metadata(&text) else {
            continue;
        };
        if skill.name != directory_name || !seen_names.insert(skill.name.clone()) {
            continue;
        }
        skills.push(skill);
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

fn parse_standard_skill_metadata(content: &str) -> Option<CodexMarketplaceSkillSummary> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let lines = content.lines().collect::<Vec<_>>();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return None;
    }
    let closing = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| (line.trim() == "---").then_some(index))?;
    let frontmatter = &lines[1..closing];
    let name = frontmatter_scalar(frontmatter, "name")?;
    let description = frontmatter_scalar(frontmatter, "description")?;
    if !valid_skill_name(&name)
        || description.is_empty()
        || description.chars().count() > 1024
        || description
            .chars()
            .any(|character| character == '\0' || (character.is_control() && character != '\n'))
    {
        return None;
    }
    Some(CodexMarketplaceSkillSummary { name, description })
}

fn frontmatter_scalar(lines: &[&str], expected_key: &str) -> Option<String> {
    let mut found = None;
    let mut seen = false;
    for (index, line) in lines.iter().enumerate() {
        if line.chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.trim() != expected_key {
            continue;
        }
        if seen {
            return None;
        }
        seen = true;
        let value = value.trim();
        if matches!(value, "|" | "|-" | ">" | ">-") {
            let folded = value.starts_with('>');
            let mut parts = Vec::new();
            for continuation in &lines[index + 1..] {
                if continuation.trim().is_empty() {
                    parts.push(String::new());
                    continue;
                }
                if !continuation.starts_with(' ') && !continuation.starts_with('\t') {
                    break;
                }
                parts.push(continuation.trim().to_string());
            }
            let joined = if folded {
                parts.join(" ")
            } else {
                parts.join("\n")
            };
            let joined = joined.trim().to_string();
            found = (!joined.is_empty()).then_some(joined);
            continue;
        }
        found = parse_yaml_scalar(value);
    }
    found
}

fn parse_yaml_scalar(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let parsed = if value.starts_with('"') {
        serde_json::from_str::<String>(value).ok()
    } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        Some(value[1..value.len() - 1].replace("''", "'"))
    } else {
        Some(value.split(" #").next().unwrap_or(value).trim().to_string())
    }?;
    let parsed = parsed.trim().to_string();
    (!parsed.is_empty()).then_some(parsed)
}

fn read_regular_text_bounded(path: &Path, max_bytes: u64) -> Result<String, String> {
    if unsafe_link_or_reparse(path) {
        return Err("file must not be a symbolic link".to_string());
    }
    let file = open_read_only_no_follow(path).map_err(|_| "unable to open file".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "unable to inspect file".to_string())?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err("file is not a bounded regular file".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unable to read file".to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err("file is too large".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "file must be UTF-8".to_string())
}

fn open_read_only_no_follow(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn canonical_directory(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&metadata) {
        return None;
    }
    fs::canonicalize(path).ok()
}

fn unsafe_link_or_reparse(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata_is_unsafe_link_or_reparse(&metadata))
        .unwrap_or(false)
}

fn metadata_is_unsafe_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::CurDir | Component::Normal(_)))
}

fn bounded_json_string(value: Option<&Value>, max_bytes: usize) -> Option<String> {
    let value = value?.as_str()?.trim();
    (!value.is_empty() && value.len() <= max_bytes).then(|| value.to_string())
}

fn resolve_absolute_codex_home(codex_home: Option<&str>) -> Result<PathBuf, String> {
    let codex_home = crate::codex_profile::resolve_profile_dir(codex_home)?;
    if !codex_home.is_absolute() {
        return Err("codexHome must be an absolute path on the service host".to_string());
    }
    Ok(codex_home)
}

fn normalize_github_source(source: &str) -> Result<String, String> {
    let (owner, repository) = if source.contains("://") {
        let parsed = url::Url::parse(source)
            .map_err(|_| "source must be a GitHub owner/repo or HTTPS URL".to_string())?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("github.com")
            || parsed.port().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err("source must be a GitHub owner/repo or HTTPS URL".to_string());
        }
        let path = parsed.path();
        if path.ends_with('/') {
            return Err("GitHub source must identify exactly one owner and repository".to_string());
        }
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err("GitHub source must identify exactly one owner and repository".to_string());
        }
        (parts[0].to_string(), parts[1].to_string())
    } else {
        let parts = source.split('/').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err("GitHub source must use owner/repo".to_string());
        }
        (parts[0].to_string(), parts[1].to_string())
    };
    let repository = repository
        .strip_suffix(".git")
        .unwrap_or(&repository)
        .to_string();
    if !valid_github_owner(&owner) || !valid_github_repository(&repository) {
        return Err("GitHub owner or repository name is invalid".to_string());
    }
    Ok(format!("https://github.com/{owner}/{repository}.git"))
}

fn valid_github_owner(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_github_repository(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('.')
        && !value.starts_with('-')
        && !value.ends_with('.')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn normalize_ref_name(ref_name: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = ref_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 255
        || value.starts_with('.')
        || value.starts_with('/')
        || value.starts_with('-')
        || value.ends_with('.')
        || value.ends_with('/')
        || value.contains("..")
        || value.contains("//")
        || value.contains("@{")
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_' | b'.' | b'/'))
        || value.split('/').any(|component| {
            component.is_empty()
                || component.starts_with('.')
                || component.ends_with(".lock")
                || component.ends_with('.')
        })
    {
        return Err("invalid refName".to_string());
    }
    Ok(Some(value.to_string()))
}

fn valid_plugin_id(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(plugin_name) = parts.next() else {
        return false;
    };
    let Some(marketplace_name) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && valid_plugin_name(plugin_name)
        && valid_marketplace_name(marketplace_name)
}

fn valid_plugin_name(value: &str) -> bool {
    valid_kebab_name(value, 128)
}

fn valid_skill_name(value: &str) -> bool {
    valid_kebab_name(value, 64)
}

fn valid_kebab_name(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_marketplace_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
#[path = "codex_skills_marketplace_tests.rs"]
mod tests;
