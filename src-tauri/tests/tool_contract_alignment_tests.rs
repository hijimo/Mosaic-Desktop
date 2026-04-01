use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn mosaic_default_tool_names() -> BTreeSet<String> {
    tauri_app_lib::core::tools::spec::build_specs(
        &tauri_app_lib::core::tools::spec::ToolsConfig::default(),
        false,
    )
    .configured_specs
    .into_iter()
    .map(|configured| configured.spec.name().to_string())
    .collect()
}

#[test]
fn tool_contract_names_match_frozen_current_default_surface() {
    let mut actual = mosaic_default_tool_names().into_iter().collect::<Vec<_>>();
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

fn codex_main_spec_source() -> String {
    fs::read_to_string(codex_main_spec_path()).expect("failed to read codex-main tools/spec.rs")
}

fn codex_main_spec_declares_tool(source: &str, tool_name: &str) -> bool {
    source.contains(&format!("name: \"{tool_name}\".to_string()"))
}

#[test]
fn codex_main_reference_spec_mentions_alignment_target_tools() {
    let source = codex_main_spec_source();

    for tool_name in [
        "shell",
        "shell_command",
        "exec_command",
        "write_stdin",
        "apply_patch",
        "list_dir",
        "read_file",
        "grep_files",
        "spawn_agent",
        "send_input",
        "resume_agent",
        "wait",
        "close_agent",
        "list_mcp_resources",
        "list_mcp_resource_templates",
        "read_mcp_resource",
    ] {
        assert!(
            codex_main_spec_declares_tool(&source, tool_name) || source.contains(tool_name),
            "codex-main reference spec does not mention tool {tool_name}"
        );
    }
}

#[test]
fn mosaic_default_surface_omits_future_contract_expansion_tools() {
    let missing_groups = [
        (
            "shell_expansion",
            vec!["shell_command", "exec_command", "write_stdin"],
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

    let names = mosaic_default_tool_names();

    for (group, group_names) in missing_groups {
        for tool_name in group_names {
            assert!(
                !names.contains(tool_name),
                "tool {tool_name} from {group} unexpectedly present in current default surface"
            );
        }
    }
}
