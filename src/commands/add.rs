use anyhow::Result;
use std::path::Path;

use crate::core::parse_source::Source;
use crate::style::{self, CYAN};

/// Thin adapter: CLI args → SkillService call → output formatting.
pub fn execute(
    source: &str,
    path_override: Option<String>,
    force: bool,
    sparse: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    // Create service and run install (business logic encapsulated in service)
    let parsed = Source::parse(source)?;
    eprintln!(
        "\n{} {}{}{} {}",
        style::styled(CYAN, "\u{25b8}"),
        style::BOLD,
        if parsed.is_file_url() {
            "Downloading"
        } else {
            "Cloning"
        },
        style::RESET,
        parsed.label(),
    );

    let service = crate::services::SkillService::with_manifest_path(
        skills_dir.to_path_buf(),
        manifest_path.to_path_buf(),
    );

    let result = service.install(source, path_override, force, sparse, None)?;

    // Output formatting (only output, no business logic)
    let mut report = crate::output::install_report(&result);
    for row in &mut report {
        row.source = parsed.label();
    }
    if !crate::output::browse_report("Install results", &mut report)? {
        print_install_result(&result);
    }

    Ok(())
}

fn print_install_result(result: &crate::core::install::InstallResult) {
    if !result.installed.is_empty() {
        eprintln!();
        if result.installed.len() == 1 {
            crate::output::print_installed(&result.installed[0]);
        } else {
            crate::output::print_installed_multi(&result.installed);
        }
    }

    for name in &result.skipped_tracked {
        crate::output::print_skipped_tracked(name);
    }

    for name in &result.skipped_existing {
        crate::output::print_skipped_existing(name);
    }

    if result.installed.is_empty()
        && result.skipped_tracked.is_empty()
        && result.skipped_existing.is_empty()
    {
        eprintln!("  No new skills installed.");
    }
}
