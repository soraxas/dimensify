//! Prepare a fresh capture directory without replacing earlier evidence.
use std::{fs, path::Path};
pub fn create_output(path: &Path) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| format!("create output parents: {error}"))?;
    }
    fs::create_dir(path).map_err(|error| format!("output must be a new directory: {error}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_parents_are_created_but_existing_evidence_is_preserved() {
        let root = std::env::temp_dir().join(format!(
            "dimensify-visual-output-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let output = root.join("missing").join("captures");
        assert!(create_output(&output).is_ok());
        let evidence = output.join("evidence");
        fs::write(&evidence, b"previous capture").unwrap();
        assert!(create_output(&output).is_err());
        assert_eq!(fs::read(&evidence).unwrap(), b"previous capture");
        fs::remove_dir_all(root).unwrap();
    }
}
