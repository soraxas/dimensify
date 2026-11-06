//! Locate a URDF and its referenced meshes on disk or over HTTP(S).
use eyre::{Result, bail, ensure};
use std::path::PathBuf;
use url::Url;

/// Where a URDF comes from; its relative and `package://` meshes resolve from here.
#[derive(Clone, Debug, PartialEq)]
pub enum RobotSource {
    /// A local file. Packages resolve against its ancestor directories.
    File(PathBuf),
    /// An HTTP(S) URL. Packages resolve from explicit bases, then by matching a
    /// path segment, then against each ancestor directory.
    Url {
        /// URDF location after any GitHub page-to-raw conversion.
        url: Url,
        /// Explicit `package name -> base directory` overrides.
        packages: Vec<(String, Url)>,
    },
}

impl RobotSource {
    /// Parse a URL or, on native targets, a local path.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.starts_with("http://") || input.starts_with("https://") {
            return Ok(Self::Url {
                url: robot_url(input)?,
                packages: vec![],
            });
        }
        ensure!(
            cfg!(not(target_arch = "wasm32")),
            "Enter a full HTTP or HTTPS URDF URL"
        );
        Ok(Self::File(PathBuf::from(input)))
    }

    /// Add a `name=base` package override; only URL sources accept one.
    pub fn with_package(mut self, spec: &str) -> Result<Self> {
        let (name, base) = spec
            .split_once('=')
            .ok_or_else(|| eyre::eyre!("Package overrides use name=https://base/"))?;
        ensure!(
            !name.is_empty() && !name.contains('/'),
            "Invalid package name {name:?}"
        );
        let mut base = robot_url(base)?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        match &mut self {
            Self::Url { packages, .. } => packages.push((name.to_owned(), base)),
            Self::File(_) => bail!("Package overrides apply only to URL sources"),
        }
        Ok(self)
    }
}

/// Validate an HTTP(S) URL, converting GitHub file pages to raw content.
pub(super) fn robot_url(input: &str) -> Result<Url> {
    let url =
        Url::parse(input.trim()).map_err(|_| eyre::eyre!("Enter a full HTTP or HTTPS URL"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Use an HTTP or HTTPS URL"
    );
    if url.host_str() == Some("github.com") {
        let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
        if segments.len() >= 5 && matches!(segments[2], "blob" | "raw") {
            let path = [&segments[..2], &segments[3..]].concat().join("/");
            return Ok(Url::parse(&format!(
                "https://raw.githubusercontent.com/{path}"
            ))?);
        }
    }
    Ok(url)
}

/// Candidate URLs for one mesh reference, most likely first.
pub(super) fn mesh_candidates(
    urdf: &Url,
    packages: &[(String, Url)],
    filename: &str,
) -> Result<Vec<Url>> {
    let checked = |url: Url| -> Result<Url> {
        ensure!(
            matches!(url.scheme(), "http" | "https"),
            "Mesh {filename} must resolve to an HTTP(S) URL"
        );
        Ok(url)
    };
    let Some(relative) = filename.strip_prefix("package://") else {
        ensure!(
            !filename.starts_with("file://"),
            "Local mesh {filename} cannot be loaded from a URL source"
        );
        return Ok(vec![checked(urdf.join(filename)?)?]);
    };
    let (package, rest) = relative
        .split_once('/')
        .ok_or_else(|| eyre::eyre!("Invalid package URI {filename}"))?;
    ensure!(
        !package.is_empty() && !rest.is_empty(),
        "Invalid package URI {filename}"
    );
    if let Some((_, base)) = packages.iter().find(|(name, _)| name == package) {
        return Ok(vec![checked(base.join(rest)?)?]);
    }
    // Directory URLs from the URDF's own directory up to the host root.
    let mut ancestors = vec![urdf.join(".")?];
    while let Some(parent) = ancestors
        .last()
        .filter(|url| url.path() != "/")
        .map(|url| url.join(".."))
    {
        ancestors.push(parent?);
    }
    let mut candidates: Vec<Url> = ancestors
        .iter()
        .map(|directory| directory.join(relative))
        .collect::<Result<_, _>>()?;
    // A ROS checkout usually names its package directory after the package.
    let package_directory = format!("/{package}/");
    if let Some(directory) = ancestors
        .iter()
        .find(|directory| directory.path().ends_with(&package_directory))
    {
        let preferred = directory.join(rest)?;
        candidates.retain(|candidate| *candidate != preferred);
        candidates.insert(0, preferred);
    }
    candidates.into_iter().map(checked).collect()
}

/// File name used to select a mesh decoder, without any query or fragment.
pub(super) fn url_file_name(url: &Url) -> PathBuf {
    PathBuf::from(
        url.path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRANKA: &str = "https://raw.githubusercontent.com/o/r/abc/oems/franka_emika/franka_description/robots/dual_panda/dual_panda.urdf";

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn github_file_pages_become_raw_content_urls() {
        let page = "https://github.com/o/r/blob/abc/oems/franka_emika/franka_description/robots/dual_panda/dual_panda.urdf";
        assert_eq!(robot_url(page).unwrap().as_str(), FRANKA);
        let raw = "https://github.com/o/r/raw/main/a%20b/robot.urdf";
        assert_eq!(
            robot_url(raw).unwrap().as_str(),
            "https://raw.githubusercontent.com/o/r/main/a%20b/robot.urdf"
        );
        // Repository pages and other hosts are not rewritten.
        let other = "https://example.test/o/r/blob/abc/robot.urdf?token=a%2Bb";
        assert_eq!(robot_url(other).unwrap().as_str(), other);
    }

    #[test]
    fn non_http_sources_are_rejected() {
        for input in ["ftp://host/robot.urdf", "file:///robot.urdf", "robot.urdf"] {
            assert!(robot_url(input).is_err(), "{input}");
        }
    }

    #[test]
    fn package_uris_prefer_the_matching_directory_then_ancestors() {
        let candidates = mesh_candidates(
            &url(FRANKA),
            &[],
            "package://franka_description/meshes/visual/link0.dae",
        )
        .unwrap();
        assert_eq!(
            candidates[0].as_str(),
            "https://raw.githubusercontent.com/o/r/abc/oems/franka_emika/franka_description/meshes/visual/link0.dae"
        );
        // Every ancestor remains a fallback, deepest first, ending at the host root.
        assert_eq!(
            candidates[1].as_str(),
            "https://raw.githubusercontent.com/o/r/abc/oems/franka_emika/franka_description/robots/dual_panda/franka_description/meshes/visual/link0.dae"
        );
        assert_eq!(
            candidates.last().unwrap().as_str(),
            "https://raw.githubusercontent.com/franka_description/meshes/visual/link0.dae"
        );
        assert_eq!(
            candidates.len(),
            candidates
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
    }

    #[test]
    fn package_overrides_and_relative_paths_resolve_against_their_bases() {
        let source = RobotSource::parse("https://cdn.test/robots/arm.urdf?sig=1")
            .unwrap()
            .with_package("arm_description=https://assets.test/pkg?sig=2")
            .unwrap();
        let RobotSource::Url {
            url: urdf,
            packages,
        } = source
        else {
            unreachable!()
        };
        assert_eq!(
            mesh_candidates(&urdf, &packages, "package://arm_description/m/base.stl").unwrap(),
            vec![url("https://assets.test/pkg/m/base.stl")]
        );
        assert_eq!(
            mesh_candidates(&urdf, &packages, "meshes/../m/base.stl").unwrap(),
            vec![url("https://cdn.test/robots/m/base.stl")]
        );
        assert_eq!(
            url_file_name(&url("https://cdn.test/m/base.STL?sig=1#x")),
            PathBuf::from("base.STL")
        );
    }

    #[test]
    fn local_and_malformed_mesh_references_are_rejected_for_urls() {
        let urdf = url(FRANKA);
        for filename in [
            "file:///home/meshes/link0.dae",
            "package://franka_description",
            "package:///meshes/link0.dae",
            "ftp://host/link0.dae",
        ] {
            assert!(mesh_candidates(&urdf, &[], filename).is_err(), "{filename}");
        }
        assert!(
            RobotSource::parse("/tmp/robot.urdf")
                .unwrap()
                .with_package("a=https://x.test/")
                .is_err()
        );
        assert!(
            RobotSource::parse("https://x.test/r.urdf")
                .unwrap()
                .with_package("no-separator")
                .is_err()
        );
    }
}
