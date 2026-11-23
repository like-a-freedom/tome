use anyhow::Result;

/// Print the embedded agent manual: the core SKILL.md or a topic reference.
pub fn execute(topic: Option<&str>) -> Result<()> {
    println!("{}", crate::skill::document(topic)?);
    Ok(())
}
