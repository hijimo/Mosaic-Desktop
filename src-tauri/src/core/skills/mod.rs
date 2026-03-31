pub mod env_var_dependencies;
pub mod injection;
pub mod invocation_utils;
pub mod loader;
pub mod manager;
pub mod model;
pub mod permissions;
pub mod remote;
pub mod render;
pub mod system;

pub use env_var_dependencies::{
    collect_env_var_dependencies, resolve_dependencies, ResolvedDependencies, SkillDependencyInfo,
};
pub use injection::{
    build_skill_injections, build_skill_name_counts, collect_explicit_skill_mentions,
    collect_explicit_skill_mentions_from_text, extract_tool_mentions, normalize_skill_path,
    tool_kind_for_path, ToolMentionKind, ToolMentions,
};
pub use invocation_utils::{build_implicit_skill_path_indexes, detect_implicit_skill_invocation};
pub use loader::{load_skills_from_roots, skill_roots_for_cwd, SkillRoot};
pub use manager::{disabled_paths_from_entries, SkillsManager};
pub use model::{
    SkillDependencies, SkillError, SkillInterface, SkillLoadOutcome, SkillMetadata, SkillPolicy,
    SkillScope, SkillToolDependency,
};
pub use permissions::{
    compile_skill_permissions, normalize_permission_paths, MacOsSkillPermissions, SkillPermissions,
};
pub use render::render_skills_section;
pub use system::{install_system_skills, system_cache_root_dir};
