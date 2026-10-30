pub fn validate_pixels(rgba: &[u8], width: u32, height: u32) -> Result<usize, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("capture size does not match physical render resolution".into());
    }
    if rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 255) {
        return Err("PPM evidence requires opaque RGBA pixels to preserve its hash".into());
    }
    let count = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3].iter().any(|channel| *channel > 2))
        .count();
    if count == 0 {
        return Err("capture contains only background pixels".into());
    }
    Ok(count)
}

pub fn hardware_adapter(device_type: &str, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(device_type, "IntegratedGpu" | "DiscreteGpu")
        && ![
            "swiftshader",
            "llvmpipe",
            "lavapipe",
            "software",
            "basic render",
        ]
        .iter()
        .any(|s| name.contains(s))
}

pub fn validate_adapter(device_type: &str, name: &str, smoke: bool) -> Result<(), String> {
    if hardware_adapter(device_type, name) || smoke {
        Ok(())
    } else {
        Err(format!(
            "adapter {device_type} {name} is not a verified hardware GPU; use --smoke for correctness evidence only"
        ))
    }
}

#[derive(Clone, Default)]
pub struct PublicationEvidence<'a> {
    pub smoke: bool,
    pub debug: bool,
    pub hardware: bool,
    pub build_commit: Option<&'a str>,
    pub build_dirty: Option<bool>,
    pub build_opt_level: Option<&'a str>,
    pub runtime_commit: Option<&'a str>,
    pub runtime_dirty: Option<bool>,
}

pub fn publication_eligible(evidence: &PublicationEvidence<'_>) -> bool {
    publication_exclusions(evidence).is_empty()
}

pub fn publication_exclusions(e: &PublicationEvidence<'_>) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if e.smoke {
        reasons.push("explicit_smoke_run");
    }
    if e.debug {
        reasons.push("debug_assertions_enabled");
    }
    if !e.hardware {
        reasons.push("unverified_hardware_adapter");
    }
    if !matches!(e.build_opt_level, Some("1" | "2" | "3" | "s" | "z")) {
        reasons.push("unverified_optimized_build");
    }
    let valid_commit = e
        .build_commit
        .is_some_and(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()));
    if !valid_commit || e.build_commit != e.runtime_commit {
        reasons.push("missing_or_mismatched_build_commit");
    }
    if e.build_dirty != Some(false) {
        reasons.push("dirty_or_unverified_build_source");
    }
    if e.runtime_dirty != Some(false) {
        reasons.push("dirty_or_unverified_runtime_source");
    }
    reasons
}

pub fn create_output(path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("create output parent: {e}"))?;
    }
    std::fs::create_dir(path)
        .map_err(|e| format!("output directory must be new ({}): {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_must_contain_real_non_background_pixels() {
        assert!(validate_pixels(&[0, 0, 0, 255].repeat(4), 2, 2).is_err());
        assert!(validate_pixels(&[255, 0, 0, 255], 2, 2).is_err());
        assert!(validate_pixels(&[255, 0, 0, 128], 1, 1).is_err());
        assert_eq!(
            validate_pixels(&[255, 0, 0, 255, 0, 0, 0, 255], 2, 1).unwrap(),
            1
        );
    }
    #[test]
    fn software_and_unknown_adapters_are_smoke_only() {
        assert!(validate_adapter("Cpu", "llvmpipe", false).is_err());
        assert!(validate_adapter("Other", "unknown", false).is_err());
        assert!(validate_adapter("IntegratedGpu", "SwiftShader", false).is_err());
        assert!(validate_adapter("Cpu", "llvmpipe", true).is_ok());
        assert!(validate_adapter("IntegratedGpu", "Apple M5", false).is_ok());
    }

    #[test]
    fn only_clean_matching_identified_optimized_hardware_runs_are_publishable() {
        let commit = "a".repeat(40);
        let clean = PublicationEvidence {
            smoke: false,
            debug: false,
            hardware: true,
            build_commit: Some(&commit),
            build_dirty: Some(false),
            build_opt_level: Some("3"),
            runtime_commit: Some(&commit),
            runtime_dirty: Some(false),
        };
        assert!(publication_eligible(&clean));
        for evidence in [
            PublicationEvidence {
                smoke: true,
                ..clean.clone()
            },
            PublicationEvidence {
                debug: true,
                ..clean.clone()
            },
            PublicationEvidence {
                hardware: false,
                ..clean.clone()
            },
            PublicationEvidence {
                build_commit: None,
                ..clean.clone()
            },
            PublicationEvidence {
                runtime_commit: Some("unknown"),
                ..clean.clone()
            },
            PublicationEvidence {
                build_commit: Some("b"),
                ..clean.clone()
            },
            PublicationEvidence {
                build_dirty: Some(true),
                ..clean.clone()
            },
            PublicationEvidence {
                build_dirty: None,
                ..clean.clone()
            },
            PublicationEvidence {
                runtime_dirty: Some(true),
                ..clean.clone()
            },
            PublicationEvidence {
                runtime_dirty: None,
                ..clean.clone()
            },
            PublicationEvidence {
                build_opt_level: Some("0"),
                ..clean.clone()
            },
            PublicationEvidence {
                build_opt_level: None,
                ..clean.clone()
            },
        ] {
            assert!(!publication_eligible(&evidence));
        }
    }

    #[test]
    fn output_creation_refuses_existing_artifacts() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dimensify-gpu-output-{}-{unique}/new",
            std::process::id()
        ));
        assert!(create_output(&path).is_ok());
        std::fs::write(path.join("marker"), b"keep").unwrap();
        assert!(create_output(&path).is_err());
        assert_eq!(std::fs::read(path.join("marker")).unwrap(), b"keep");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
