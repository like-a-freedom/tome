//! Application Service Layer for tome.
//!
//! Services orchestrate core modules to implement mutation business logic.
//! Add, remove, and update commands are thin adapters; list/export remain a
//! deliberate deferred read-side extraction.

use std::path::PathBuf;

pub mod skill_service;
pub mod update_service;

pub use skill_service::SkillService;
pub use update_service::UpdateService;

/// Shared configuration for the application services.
pub(crate) struct ServiceConfig {
    pub skills_dir: PathBuf,
    pub manifest_path: PathBuf,
}

impl ServiceConfig {
    pub fn new(skills_dir: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            skills_dir,
            manifest_path,
        }
    }
}
