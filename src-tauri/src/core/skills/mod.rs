pub mod injection;
pub mod loader;
pub mod manager;
pub mod model;
pub mod permissions;
pub mod remote;
pub mod render;
pub mod system;

pub use loader::{load_skills_from_roots, SkillRoot};
pub use manager::SkillsManager;
pub use model::{
    SkillDependencies, SkillError, SkillInterface, SkillLoadOutcome, SkillMetadata, SkillPolicy,
    SkillScope, SkillToolDependency,
};
pub use render::render_skills_section;
pub use system::{install_system_skills, system_cache_root_dir};
