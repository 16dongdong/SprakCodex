use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codexmanager-marketplace-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct StubRunner {
    responses: Mutex<VecDeque<Result<String, CliRunError>>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl StubRunner {
    fn with_json(values: Vec<Value>) -> Self {
        Self {
            responses: Mutex::new(
                values
                    .into_iter()
                    .map(|value| Ok(serde_json::to_string(&value).expect("serialize stub JSON")))
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl CliRunner for StubRunner {
    fn run(&self, args: &[String], _codex_home: &Path) -> Result<CliOutput, CliRunError> {
        self.calls.lock().expect("calls lock").push(args.to_vec());
        let response = self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("stub response");
        response.map(|stdout| CliOutput { stdout })
    }
}

struct ResolvedCliRunner {
    command: ResolvedCliCommand,
}

impl CliRunner for ResolvedCliRunner {
    fn run(&self, args: &[String], codex_home: &Path) -> Result<CliOutput, CliRunError> {
        run_resolved_cli(&self.command, args, codex_home)
    }
}

#[cfg(target_os = "windows")]
fn write_fake_codex_cli(bin: &Path) -> PathBuf {
    fs::create_dir_all(bin).expect("create fake CLI directory");
    let cli = bin.join("codex.cmd");
    fs::write(
        &cli,
        concat!(
            "@echo off\r\n",
            "if \"%~1 %~2 %~3\"==\"plugin marketplace list\" (\r\n",
            "  echo {\"marketplaces\":[]}\r\n",
            "  exit /b 0\r\n",
            ")\r\n",
            "if \"%~1 %~2 %~3\"==\"plugin list --available\" (\r\n",
            "  echo {\"installed\":[],\"available\":[]}\r\n",
            "  exit /b 0\r\n",
            ")\r\n",
            "echo unexpected fake Codex arguments 1>&2\r\n",
            "exit /b 9\r\n",
        ),
    )
    .expect("write fake Windows Codex CLI");
    cli
}

#[cfg(unix)]
fn write_fake_codex_cli(bin: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(bin).expect("create fake CLI directory");
    let cli = bin.join("codex");
    fs::write(
        &cli,
        concat!(
            "#!/bin/sh\n",
            "case \"$1 $2 $3\" in\n",
            "  \"plugin marketplace list\") printf '%s\\n' '{\"marketplaces\":[]}' ;;\n",
            "  \"plugin list --available\") printf '%s\\n' '{\"installed\":[],\"available\":[]}' ;;\n",
            "  *) printf '%s\\n' 'unexpected fake Codex arguments' >&2; exit 9 ;;\n",
            "esac\n",
        ),
    )
    .expect("write fake Unix Codex CLI");
    let mut permissions = fs::metadata(&cli)
        .expect("read fake Unix Codex CLI metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&cli, permissions).expect("make fake Unix Codex CLI executable");
    cli
}

fn write_plugin(
    root: &Path,
    directory: &str,
    manifest_name: &str,
    version: &str,
    skill_name: Option<&str>,
    skill_description: Option<&str>,
) -> PathBuf {
    let plugin = root.join("plugins").join(directory);
    fs::create_dir_all(plugin.join(".codex-plugin")).expect("create manifest directory");
    fs::create_dir_all(plugin.join(".claude-plugin")).expect("create Claude manifest directory");
    fs::write(
        plugin.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"claude-copy"}"#,
    )
    .expect("write Claude manifest");
    fs::write(
        plugin.join(".codex-plugin").join("plugin.json"),
        serde_json::to_vec(&serde_json::json!({
            "name": manifest_name,
            "version": version,
            "description": format!("{manifest_name} description"),
            "author": { "name": "CodexManager tests" },
            "skills": "./skills/",
            "interface": { "category": "Productivity" }
        }))
        .expect("serialize manifest"),
    )
    .expect("write Codex manifest");
    if let Some(skill_name) = skill_name {
        let skill = plugin.join("skills").join(skill_name);
        fs::create_dir_all(&skill).expect("create skill directory");
        let description_line = skill_description
            .map(|description| format!("description: {description}\n"))
            .unwrap_or_default();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {skill_name}\n{description_line}---\n\n# Test\n"),
        )
        .expect("write SKILL.md");
    }
    plugin
}

fn marketplace_json(name: &str, root: &Path) -> Value {
    serde_json::json!({
        "marketplaces": [{
            "name": name,
            "root": root,
            "marketplaceSource": {
                "sourceType": "git",
                "source": "https://github.com/example/skills.git"
            }
        }]
    })
}

fn plugin_entry(
    name: &str,
    marketplace: &str,
    version: &str,
    path: &Path,
    installed: bool,
) -> Value {
    serde_json::json!({
        "pluginId": format!("{name}@{marketplace}"),
        "name": name,
        "marketplaceName": marketplace,
        "version": version,
        "installed": installed,
        "enabled": installed,
        "source": { "source": "local", "path": path },
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_USE"
    })
}

#[test]
fn github_source_and_ref_validation_is_strict() {
    assert_eq!(
        normalize_github_source("openai/role-specific-plugins").unwrap(),
        "https://github.com/openai/role-specific-plugins.git"
    );
    assert_eq!(
        normalize_github_source("https://github.com/openai/role-specific-plugins.git").unwrap(),
        "https://github.com/openai/role-specific-plugins.git"
    );
    for rejected in [
        "git@github.com:openai/repo.git",
        "http://github.com/openai/repo",
        "https://github.example/openai/repo",
        "https://github.com/openai/repo/tree/main",
        "https://token@github.com/openai/repo",
        "../repo",
        "openai/repo@main",
    ] {
        assert!(
            normalize_github_source(rejected).is_err(),
            "accepted {rejected}"
        );
    }

    assert_eq!(
        normalize_ref_name(Some("release/2026.07")).unwrap(),
        Some("release/2026.07".to_string())
    );
    for rejected in ["--help", "../main", "main..next", "refs/@{upstream}", "a b"] {
        assert!(
            normalize_ref_name(Some(rejected)).is_err(),
            "accepted {rejected}"
        );
    }
}

#[test]
fn inventory_keeps_only_local_standard_codex_skill_plugins() {
    let temp = TestDirectory::new("filter");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let good = write_plugin(
        &marketplace_root,
        "good-plugin",
        "good-plugin",
        "1.2.3",
        Some("good-skill"),
        Some("Use this standard Codex skill for tests."),
    );
    let missing_description = write_plugin(
        &marketplace_root,
        "missing-description",
        "missing-description",
        "1.0.0",
        Some("missing-description"),
        None,
    );
    let claude_only = marketplace_root.join("plugins").join("claude-only");
    fs::create_dir_all(claude_only.join(".claude-plugin")).expect("create Claude plugin");
    fs::write(
        claude_only.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"claude-only"}"#,
    )
    .expect("write Claude manifest");
    let outside_root = temp.path().join("outside");
    let outside = write_plugin(
        &outside_root,
        "outside-plugin",
        "outside-plugin",
        "1.0.0",
        Some("outside-skill"),
        Some("This source is outside the marketplace root."),
    );
    let plugins = serde_json::json!({
        "installed": [],
        "available": [
            plugin_entry("good-plugin", "test-market", "1.2.3", &good, false),
            plugin_entry(
                "missing-description",
                "test-market",
                "1.0.0",
                &missing_description,
                false
            ),
            plugin_entry("claude-only", "test-market", "1.0.0", &claude_only, false),
            plugin_entry("outside-plugin", "test-market", "1.0.0", &outside, false),
            {
                "pluginId": "remote@test-market",
                "name": "remote",
                "marketplaceName": "test-market",
                "version": "1.0.0",
                "installed": false,
                "enabled": false,
                "source": { "source": "url", "url": "https://example.com/plugin.zip" },
                "installPolicy": "AVAILABLE"
            }
        ]
    });
    let runner = StubRunner::with_json(vec![
        marketplace_json("test-market", &marketplace_root),
        plugins,
    ]);

    let inventory = list_with_runner(temp.path(), &runner).expect("list inventory");

    assert!(inventory.cli_available);
    assert_eq!(inventory.marketplaces.len(), 1);
    assert_eq!(inventory.plugins.len(), 1);
    let plugin = &inventory.plugins[0];
    assert_eq!(plugin.plugin_id, "good-plugin@test-market");
    assert_eq!(plugin.author, "CodexManager tests");
    assert_eq!(plugin.category, "Productivity");
    assert_eq!(plugin.skills.len(), 1);
    assert_eq!(plugin.skills[0].name, "good-skill");
}

#[test]
fn available_plugin_rejects_a_cli_revision_that_differs_from_the_manifest_version() {
    let temp = TestDirectory::new("available-version-mismatch");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let plugin_path = write_plugin(
        &marketplace_root,
        "version-mismatch",
        "version-mismatch",
        "1.2.3",
        Some("version-mismatch-skill"),
        Some("Use this standard Codex skill for a version mismatch test."),
    );
    let plugins = serde_json::json!({
        "installed": [],
        "available": [plugin_entry(
            "version-mismatch",
            "test-market",
            "marketplace-snapshot-revision",
            &plugin_path,
            false
        )]
    });
    let runner = StubRunner::with_json(vec![
        marketplace_json("test-market", &marketplace_root),
        plugins,
    ]);

    let inventory = list_with_runner(temp.path(), &runner).expect("list inventory");

    assert!(inventory.plugins.is_empty());
}

#[test]
fn unavailable_or_old_cli_returns_an_explicit_inventory_warning() {
    let runner = StubRunner {
        responses: Mutex::new(VecDeque::from([Err(CliRunError::Unavailable(
            "Codex CLI was not found on PATH".to_string(),
        ))])),
        calls: Mutex::new(Vec::new()),
    };

    let inventory = list_with_runner(Path::new("/tmp/test-codex-home"), &runner)
        .expect("unavailable inventory");

    assert!(!inventory.cli_available);
    assert!(inventory.plugins.is_empty());
    assert_eq!(inventory.warnings, ["Codex CLI was not found on PATH"]);
}

#[test]
fn windows_resolution_ignores_extensionless_shims_and_preserves_directory_priority() {
    let temp = TestDirectory::new("windows-cli-path-priority");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).expect("create first CLI directory");
    fs::create_dir_all(&second).expect("create second CLI directory");
    fs::write(first.join("codex"), b"#!/bin/sh\nexit 0\n").expect("write extensionless npm shim");
    fs::write(first.join("codex.cmd"), b"@exit /b 0\r\n").expect("write first codex.cmd");
    fs::write(second.join("codex.exe"), b"not a real executable").expect("write second codex.exe");

    let directories = canonical_search_directories([first.clone(), second]);
    let executable = find_windows_codex(&directories).expect("resolve Windows Codex CLI");

    assert_eq!(
        executable,
        first
            .join("codex.cmd")
            .canonicalize()
            .expect("canonical codex.cmd")
    );
    assert_ne!(
        executable,
        first
            .join("codex")
            .canonicalize()
            .expect("canonical extensionless shim")
    );
}

#[test]
fn windows_resolution_uses_com_exe_bat_cmd_extension_priority() {
    let temp = TestDirectory::new("windows-cli-extension-priority");
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).expect("create CLI directory");
    for extension in ["cmd", "bat", "exe", "com"] {
        fs::write(
            bin.join(format!("codex.{extension}")),
            format!("fake {extension}"),
        )
        .expect("write extension candidate");
    }

    let directories = canonical_search_directories([bin.clone()]);
    let executable = find_windows_codex(&directories).expect("resolve Windows Codex CLI");

    assert_eq!(
        executable,
        bin.join("codex.com")
            .canonicalize()
            .expect("canonical codex.com")
    );
}

#[test]
fn windows_known_locations_cover_nvm_npm_and_codex_desktop_in_priority_order() {
    let temp = TestDirectory::new("windows-cli-known-locations");
    let nvm = temp.path().join("nvm-symlink");
    let app_data = temp.path().join("roaming");
    let npm = app_data.join("npm");
    let local_app_data = temp.path().join("local");
    let desktop_bin = local_app_data
        .join("OpenAI")
        .join("Codex")
        .join("bin")
        .join("desktop-version");
    for directory in [&nvm, &npm, &desktop_bin] {
        fs::create_dir_all(directory).expect("create known CLI directory");
    }
    fs::write(nvm.join("codex.cmd"), b"@exit /b 0\r\n").expect("write NVM CLI");
    fs::write(npm.join("codex.cmd"), b"@exit /b 0\r\n").expect("write npm CLI");
    fs::write(desktop_bin.join("codex.exe"), b"fake desktop CLI").expect("write Codex Desktop CLI");

    let candidates =
        windows_known_search_directories(Some(nvm.clone()), Some(app_data), Some(local_app_data));
    let directories = canonical_search_directories(candidates);

    assert_eq!(
        find_windows_codex(&directories).expect("resolve NVM CLI"),
        nvm.join("codex.cmd")
            .canonicalize()
            .expect("canonical NVM CLI")
    );

    let without_nvm = directories
        .iter()
        .filter(|directory| **directory != nvm.canonicalize().expect("canonical NVM directory"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        find_windows_codex(&without_nvm).expect("resolve npm CLI"),
        npm.join("codex.cmd")
            .canonicalize()
            .expect("canonical npm CLI")
    );

    let canonical_npm = npm.canonicalize().expect("canonical npm directory");
    let without_nvm_or_npm = without_nvm
        .iter()
        .filter(|directory| **directory != canonical_npm)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        find_windows_codex(&without_nvm_or_npm).expect("resolve Codex Desktop CLI"),
        desktop_bin
            .join("codex.exe")
            .canonicalize()
            .expect("canonical Codex Desktop CLI")
    );
}

#[cfg(any(target_os = "windows", unix))]
#[test]
fn resolved_fake_cli_produces_a_marketplace_inventory() {
    let temp = TestDirectory::new("resolved-fake-cli");
    let bin = temp.path().join("bin");
    let expected_cli = write_fake_codex_cli(&bin)
        .canonicalize()
        .expect("canonical fake Codex CLI");
    let directories = canonical_search_directories([bin]);
    #[cfg(target_os = "windows")]
    let executable = find_windows_codex(&directories).expect("resolve fake Windows Codex CLI");
    #[cfg(unix)]
    let executable = find_unix_codex(&directories).expect("resolve fake Unix Codex CLI");
    let command = resolved_cli_command(executable.clone(), &directories)
        .expect("build resolved fake Codex command");
    let runner = ResolvedCliRunner { command };

    let inventory = list_with_runner(temp.path(), &runner).expect("list fake CLI inventory");

    assert!(executable.is_absolute());
    assert_eq!(executable, expected_cli);
    assert!(inventory.cli_available);
    assert!(inventory.marketplaces.is_empty());
    assert!(inventory.plugins.is_empty());
    assert!(inventory.warnings.is_empty());
}

#[cfg(unix)]
#[test]
fn unix_resolution_only_accepts_absolute_executable_path_entries() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TestDirectory::new("unix-cli-safe-path");
    let non_executable_bin = temp.path().join("non-executable");
    let executable_bin = temp.path().join("executable");
    fs::create_dir_all(&non_executable_bin).expect("create non-executable directory");
    fs::create_dir_all(&executable_bin).expect("create executable directory");
    fs::write(non_executable_bin.join("codex"), b"#!/bin/sh\nexit 0\n")
        .expect("write non-executable CLI");
    let executable = executable_bin.join("codex");
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").expect("write executable CLI");
    let mut permissions = fs::metadata(&executable)
        .expect("read executable CLI metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make CLI executable");

    let directories = canonical_search_directories([
        PathBuf::new(),
        PathBuf::from("relative"),
        non_executable_bin,
        executable_bin,
    ]);
    let resolved = find_unix_codex(&directories).expect("resolve safe Unix Codex CLI");

    assert_eq!(
        resolved,
        executable.canonicalize().expect("canonical executable CLI")
    );
}

#[test]
fn install_accepts_an_installed_cache_revision_and_keeps_the_manifest_version() {
    let temp = TestDirectory::new("install");
    let marketplace_root = temp.path().join("marketplace");
    fs::create_dir_all(&marketplace_root).expect("create marketplace");
    let plugin_path = write_plugin(
        &marketplace_root,
        "installable-plugin",
        "installable-plugin",
        "2.0.0",
        Some("installable-skill"),
        Some("Install this complete Codex plugin package."),
    );
    let before = serde_json::json!({
        "installed": [],
        "available": [plugin_entry(
            "installable-plugin",
            "test-market",
            "2.0.0",
            &plugin_path,
            false
        )]
    });
    let after = serde_json::json!({
        "installed": [plugin_entry(
            "installable-plugin",
            "test-market",
            "marketplace-snapshot-revision",
            &plugin_path,
            true
        )],
        "available": []
    });
    let marketplace = marketplace_json("test-market", &marketplace_root);
    let runner = StubRunner::with_json(vec![
        marketplace.clone(),
        before,
        serde_json::json!({ "pluginId": "installable-plugin@test-market" }),
        marketplace,
        after,
    ]);

    let inventory = install_with_runner("installable-plugin@test-market", temp.path(), &runner)
        .expect("install plugin");

    assert!(inventory.plugins[0].installed);
    assert_eq!(inventory.plugins[0].version, "2.0.0");
    assert_eq!(
        runner.calls()[2],
        strings(&[
            "plugin",
            "add",
            "--json",
            "--",
            "installable-plugin@test-market"
        ])
    );
}

#[test]
fn bounded_reader_drains_input_and_marks_truncation() {
    let captured = read_stream_bounded(std::io::Cursor::new(vec![b'x'; 128]), 16)
        .expect("read captured stream");
    assert_eq!(captured.bytes, vec![b'x'; 16]);
    assert!(captured.truncated);
}

#[test]
fn standard_skill_parser_requires_name_and_description() {
    assert_eq!(
        parse_standard_skill_metadata(
            "---\nname: useful-skill\ndescription: Use this when useful.\n---\n# Body\n"
        ),
        Some(CodexMarketplaceSkillSummary {
            name: "useful-skill".to_string(),
            description: "Use this when useful.".to_string(),
        })
    );
    assert!(parse_standard_skill_metadata("---\nname: useful-skill\n---\n").is_none());
    assert!(parse_standard_skill_metadata(
        "---\ndescription: Missing the required Codex skill name.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nmetadata:\n  name: nested-name\n  description: Nested values are not top-level fields.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nname: useful-skill\nname: duplicate-name\ndescription: Duplicate keys are ambiguous.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(
        "---\nname: useful-skill\ndescription: First description.\ndescription: Second description.\n---\n"
    )
    .is_none());
    assert!(parse_standard_skill_metadata(&format!(
        "---\nname: useful-skill\ndescription: {}\n---\n",
        "x".repeat(1025)
    ))
    .is_none());
}

#[test]
fn codex_home_must_be_absolute() {
    assert!(resolve_absolute_codex_home(Some("relative/codex-home")).is_err());
}
