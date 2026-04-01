use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn tool_contract_names_match_codex_main_default_surface() {
    let config = tauri_app_lib::core::tools::spec::ToolsConfig::default();
    let assembled = tauri_app_lib::core::tools::spec::build_specs(&config, false);
    let mut actual: Vec<String> = assembled
        .configured_specs
        .iter()
        .map(|spec| spec.spec.name().to_string())
        .collect();
    actual.sort();

    let expected = vec![
        "apply_patch",
        "grep_files",
        "list_dir",
        "read_file",
        "shell",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

fn codex_main_tools_root() -> PathBuf {
    PathBuf::from("/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools")
}

fn codex_main_spec_path() -> PathBuf {
    codex_main_tools_root().join("spec.rs")
}

#[test]
fn codex_main_reference_checkout_exists() {
    assert!(codex_main_spec_path().is_file());
}

#[test]
fn mosaic_is_missing_contract_groups_we_intend_to_add() {
    let expected_groups = [
        (
            "shell",
            vec!["shell", "shell_command", "exec_command", "write_stdin"],
        ),
        (
            "collab",
            vec![
                "spawn_agent",
                "send_input",
                "resume_agent",
                "wait",
                "close_agent",
            ],
        ),
        (
            "mcp",
            vec![
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
            ],
        ),
    ];

    let names = tauri_app_lib::core::tools::spec::build_specs(
        &tauri_app_lib::core::tools::spec::ToolsConfig::default(),
        false,
    )
    .configured_specs
    .into_iter()
    .map(|configured| configured.spec.name().to_string())
    .collect::<BTreeSet<_>>();

    for (_group, group_names) in expected_groups {
        for tool_name in group_names {
            assert!(names.contains(tool_name), "missing tool: {tool_name}");
        }
    }
}
