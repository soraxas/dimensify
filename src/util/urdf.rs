use bevy_egui::egui::TextBuffer;

/// if there's package prefix, strip it, otherwise return the base_dir + original string
pub fn replace_package_with_base_dir<P>(filename: &str, base_dir: &Option<P>) -> String
where
    P: std::fmt::Display,
{
    let filename = match filename.strip_prefix("package://") {
        Some(path) => path,
        None => filename,
    };

    match base_dir {
        Some(base_dir) => {
            format!("{base_dir}/{filename}")
        }
        None => filename.to_string(),
    }
}

/// A struct that generates possible URLs by progressively trimming the path from the base URL.
pub struct UrlParentDirGenerator<'a> {
    base_url: &'a str,
    parts: Vec<&'a str>,
}

impl<'a> UrlParentDirGenerator<'a> {
    /// Creates a new `UrlParentDirGenerator` instance.
    ///
    /// # Arguments
    ///
    /// * `base_url` - A string slice representing the base URL to start with (e.g., "https://example.com/a/b/c").
    ///
    /// # Returns
    ///
    /// Returns a `UrlParentDirGenerator` instance that will generate URLs by trimming the path of `base_url`.
    pub fn new(base_url: &'a str) -> Self {
        let mut parts: Vec<&str> = base_url.split('/').collect();
        // if the base URL is not ends with a trailing slash, we
        // need to manually add the last part. empty string denotes that there
        // will be a slash there.
        if !base_url.ends_with('/') {
            parts.push(""); // Add the last part of the path again to simulate trailing slash
        }
        UrlParentDirGenerator { base_url, parts }
    }
}

impl Iterator for UrlParentDirGenerator<'_> {
    type Item = String;

    /// The `next` method generates the next possible URL by trimming the base URL and appending the filename.
    ///
    /// # Returns
    ///
    /// This method returns a `Some(String)` with the next URL in the sequence or `None` when the root URL is reached.
    fn next(&mut self) -> Option<Self::Item> {
        // Stop at the root URL (only scheme, empty string after //, and domain left)
        if self.parts.len() <= 3 {
            return None;
        }

        // Remove the last path segment and rebuild the URL
        self.parts.pop();
        let path = self.parts.join("/");

        // Return the new URL with the filename appended
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_urls_with_trailing_slash() {
        let base_dir = "https://example.com/a/b/c/"; // With trailing slash
        let generator = UrlParentDirGenerator::new(base_dir);

        let urls: Vec<String> = generator.collect();

        let expected = vec![
            "https://example.com/a/b/c".to_string(),
            "https://example.com/a/b".to_string(),
            "https://example.com/a".to_string(),
            "https://example.com".to_string(),
        ];

        assert_eq!(urls, expected);
    }

    #[test]
    fn test_generate_urls_without_trailing_slash() {
        let base_dir = "https://example.com/a/b/c"; // Without trailing slash
        let generator = UrlParentDirGenerator::new(base_dir);

        let urls: Vec<String> = generator.collect();

        let expected = vec![
            "https://example.com/a/b/c".to_string(),
            "https://example.com/a/b".to_string(),
            "https://example.com/a".to_string(),
            "https://example.com".to_string(),
        ];

        assert_eq!(urls, expected);
    }

    #[test]
    fn test_generate_urls_with_root_only() {
        let base_dir = "https://example.com"; // Just the root URL
        let generator = UrlParentDirGenerator::new(base_dir);

        let urls: Vec<String> = generator.collect();

        let expected: Vec<String> = vec!["https://example.com".to_string()];

        assert_eq!(urls, expected);
    }

    #[test]
    fn test_generate_urls_empty_path() {
        let base_dir = "https://example.com/"; // Just the root with a slash
        let generator = UrlParentDirGenerator::new(base_dir);

        let urls: Vec<String> = generator.collect();

        let expected = vec!["https://example.com".to_string()];

        assert_eq!(urls, expected);
    }
}
