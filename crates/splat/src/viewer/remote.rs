/// Explicit transport format; Auto inspects JSON content after download.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum DataFormat {
    #[default]
    Auto,
    Ply,
    Sog,
    SogUnbundled,
    SogStream,
}
impl std::str::FromStr for DataFormat {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "ply" => Ok(Self::Ply),
            "sog" => Ok(Self::Sog),
            "sog-unbundled" => Ok(Self::SogUnbundled),
            "sog-stream" => Ok(Self::SogStream),
            _ => Err("format must be auto, ply, sog, sog-unbundled or sog-stream".into()),
        }
    }
}
#[derive(serde::Deserialize)]
pub(super) struct MetadataShape {
    #[serde(default, deserialize_with = "present")]
    pub means: bool,
    #[serde(default, deserialize_with = "present")]
    pub tree: bool,
}
fn present<'de, D: serde::Deserializer<'de>>(input: D) -> Result<bool, D::Error> {
    <Option<serde::de::IgnoredAny> as serde::Deserialize>::deserialize(input)
        .map(|value| value.is_some())
}
pub(super) fn metadata_shape(bytes: &[u8]) -> Result<MetadataShape, String> {
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err("Expected a JSON metadata object".into());
    }
    serde_json::from_slice(bytes).map_err(|_| "Expected a JSON metadata object".into())
}
impl DataFormat {
    fn validate(self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::SogUnbundled | Self::SogStream => {
                let value = metadata_shape(bytes)?;
                let plain = value.means;
                let hierarchy = value.tree;
                if (self == Self::SogUnbundled && plain && !hierarchy)
                    || (self == Self::SogStream && hierarchy && !plain)
                {
                    Ok(())
                } else {
                    Err("JSON metadata does not match the requested format".into())
                }
            }
            _ => Ok(()),
        }
    }
}

pub(super) struct ResourceUrl {
    pub name: String,
    pub url: url::Url,
    pub format: DataFormat,
    /// Tried once when the primary URL is missing (SuperSplat v1 publishes).
    pub fallback: Option<url::Url>,
}

/// Published SuperSplat scenes are plain SOG metadata on this CDN, under
/// `v2/` for current publishes and `v1/` for older ones.
const SUPERSPLAT_CDN: &str = "https://d28zzqy0iyovbz.cloudfront.net";

fn supersplat_id(id: &str) -> Option<&str> {
    (id.len() == 8 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
}

/// Resolve a SuperSplat scene link, view link, bare id or CDN metadata URL to its id.
pub(super) fn supersplat_scene(input: &str) -> Option<String> {
    let input = input.trim();
    if let Some(id) = supersplat_id(input) {
        return Some(id.to_ascii_lowercase());
    }
    let url = url::Url::parse(input).ok()?;
    let host = url.host_str()?;
    let segments: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let id = match (host, segments.as_slice()) {
        ("superspl.at" | "www.superspl.at", ["scene", id, ..]) => id.to_string(),
        ("superspl.at" | "www.superspl.at", ["view" | "s"]) => url
            .query_pairs()
            .find(|(key, _)| key == "id")?
            .1
            .into_owned(),
        (_, [id, "v2", "meta.json"]) if SUPERSPLAT_CDN.ends_with(host) => id.to_string(),
        _ => return None,
    };
    supersplat_id(&id).map(str::to_ascii_lowercase)
}

fn supersplat_metadata(id: &str, version: &str) -> url::Url {
    url::Url::parse(&format!("{SUPERSPLAT_CDN}/{id}/{version}/meta.json"))
        .expect("static SuperSplat CDN URL")
}

#[derive(Debug)]
pub(super) struct Download {
    pub bytes: Vec<u8>,
    pub url: url::Url,
}

impl ResourceUrl {
    fn limit(&self) -> Option<usize> {
        match self.format {
            DataFormat::SogUnbundled => Some(1024 * 1024),
            DataFormat::Auto | DataFormat::SogStream => Some(256 * 1024 * 1024),
            DataFormat::Ply | DataFormat::Sog => None,
        }
    }

    /// Validate a direct HTTP(S) Gaussian file URL, preserving query parameters.
    pub(super) fn parse(input: &str) -> Result<Self, String> {
        Self::parse_with_format(input, DataFormat::Auto)
    }

    pub(super) fn parse_with_format(input: &str, format: DataFormat) -> Result<Self, String> {
        let (input, fallback) = match supersplat_scene(input) {
            Some(id) => (
                supersplat_metadata(&id, "v2").to_string(),
                Some(supersplat_metadata(&id, "v1")),
            ),
            None => (input.to_owned(), None),
        };
        let url = url::Url::parse(input.trim())
            .map_err(|_| "Enter a full HTTP or HTTPS URL".to_owned())?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("Use an HTTP or HTTPS URL".into());
        }
        let mut name = url
            .path_segments()
            .and_then(|mut parts| parts.next_back())
            .unwrap_or_default()
            .to_owned();
        if format == DataFormat::Auto
            && !name.to_ascii_lowercase().ends_with(".ply")
            && !name.to_ascii_lowercase().ends_with(".sog")
            && !name.to_ascii_lowercase().ends_with(".json")
        {
            return Err("The URL path must end in .ply, .sog or a SOG manifest .json".into());
        }
        let format = if format == DataFormat::Auto {
            match name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "ply" => DataFormat::Ply,
                "sog" => DataFormat::Sog,
                _ => DataFormat::Auto,
            }
        } else {
            format
        };
        // The existing file decoder selects its input by suffix. Explicit types
        // override endpoint naming; the original URL remains untouched.
        let suffix = match format {
            DataFormat::Ply => Some(".ply"),
            DataFormat::Sog => Some(".sog"),
            _ => None,
        };
        if let Some(suffix) = suffix
            && !name.to_ascii_lowercase().ends_with(suffix)
        {
            name.push_str(suffix);
        }
        Ok(Self {
            name,
            url,
            format,
            fallback,
        })
    }
}

/// The CDN answers 403 for a version that was never published.
fn try_fallback(resource: &ResourceUrl, result: &Result<Download, String>) -> Option<url::Url> {
    match result {
        Err(error) if matches!(error.as_str(), "HTTP 403" | "HTTP 404") => {
            resource.fallback.clone()
        }
        _ => None,
    }
}

fn fallback_error(resource: &ResourceUrl, error: String) -> String {
    match resource.fallback {
        Some(_) => format!("Could not find this SuperSplat scene's published SOG ({error})"),
        None => error,
    }
}

fn limit_error(limit: usize) -> String {
    format!(
        "Download exceeds the {} MiB metadata limit",
        limit / (1024 * 1024)
    )
}

fn append_bounded(bytes: &mut Vec<u8>, chunk: &[u8], limit: Option<usize>) -> Result<(), String> {
    if let Some(limit) = limit
        && chunk.len() > limit.saturating_sub(bytes.len())
    {
        return Err(limit_error(limit));
    }
    bytes
        .try_reserve(chunk.len())
        .map_err(|_| "Could not allocate memory for the HTTP response".to_owned())?;
    bytes.extend_from_slice(chunk);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
/// Download on a worker thread, bounding metadata and verifying TLS normally.
pub(super) fn fetch(resource: &ResourceUrl) -> Result<Download, String> {
    let result = fetch_from(resource, &resource.url);
    match try_fallback(resource, &result) {
        Some(url) => fetch_from(resource, &url).map_err(|error| fallback_error(resource, error)),
        None => result,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_from(resource: &ResourceUrl, url: &url::Url) -> Result<Download, String> {
    use std::io::Read;
    let limit = resource.limit();
    let client = reqwest::blocking::Client::builder()
        .referer(false)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "Could not start HTTP client".to_owned())?;
    let mut response = client
        .get(url.clone())
        .send()
        .map_err(|_| "Download failed (connection, TLS or timeout)".to_owned())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    if let Some(limit) = limit
        && response
            .content_length()
            .is_some_and(|size| size > limit as u64)
    {
        return Err(limit_error(limit));
    }
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|_| "Could not read the HTTP response".to_owned())?;
        if count == 0 {
            break;
        }
        append_bounded(&mut bytes, &buffer[..count], limit)?;
    }
    resource.format.validate(&bytes)?;
    Ok(Download {
        bytes,
        url: response.url().clone(),
    })
}

#[cfg(target_arch = "wasm32")]
/// Fetch a browser stream with metadata bounds and the host's normal CORS policy.
pub(super) async fn fetch(resource: &ResourceUrl) -> Result<Download, String> {
    let result = fetch_from(resource, &resource.url).await;
    match try_fallback(resource, &result) {
        Some(url) => fetch_from(resource, &url)
            .await
            .map_err(|error| fallback_error(resource, error)),
        None => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_from(resource: &ResourceUrl, url: &url::Url) -> Result<Download, String> {
    let limit = resource.limit();
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let response: web_sys::Response = JsFuture::from(
        web_sys::window()
            .ok_or("No browser window")?
            .fetch_with_str(url.as_str()),
    )
    .await
    .map_err(|_| "Download failed; check the URL, HTTPS and the host's CORS settings".to_owned())?
    .dyn_into()
    .map_err(|_| "Invalid HTTP response".to_owned())?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    if let Some(limit) = limit
        && response
            .headers()
            .get("Content-Length")
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|size| size > limit as u64)
    {
        return Err(limit_error(limit));
    }
    let reader: web_sys::ReadableStreamDefaultReader = response
        .body()
        .ok_or("HTTP response has no body")?
        .get_reader()
        .dyn_into()
        .map_err(|_| "Could not read HTTP stream")?;
    let mut bytes = Vec::new();
    loop {
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|_| "Could not read the HTTP response")?;
        let done = js_sys::Reflect::get(&result, &"done".into())
            .map_err(|_| "Invalid HTTP stream")?
            .as_bool()
            .unwrap_or(false);
        if done {
            break;
        }
        let value =
            js_sys::Reflect::get(&result, &"value".into()).map_err(|_| "Invalid HTTP chunk")?;
        let chunk = js_sys::Uint8Array::new(&value);
        if let Some(limit) = limit
            && chunk.length() as usize > limit.saturating_sub(bytes.len())
        {
            let _ = reader.cancel();
            return Err(limit_error(limit));
        }
        append_bounded(&mut bytes, &chunk.to_vec(), limit)?;
    }
    resource.format.validate(&bytes)?;
    let url = url::Url::parse(&response.url()).map_err(|_| "Invalid final response URL")?;
    Ok(Download { bytes, url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_metadata_fields_are_absent_for_format_discrimination() {
        let hierarchy = br#"{"means":null,"tree":{}}"#;
        assert!(!metadata_shape(hierarchy).unwrap().means);
        assert!(DataFormat::SogStream.validate(hierarchy).is_ok());
        let plain = br#"{"means":{},"tree":null}"#;
        assert!(!metadata_shape(plain).unwrap().tree);
        assert!(DataFormat::SogUnbundled.validate(plain).is_ok());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn unbundled_metadata_is_bounded_before_reading_the_body() {
        use std::{io::Write, net::TcpListener};
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let resource = ResourceUrl::parse_with_format(
            &format!("http://{}/meta.json", server.local_addr().unwrap()),
            DataFormat::SogUnbundled,
        )
        .unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            request_headers(&mut socket);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        assert!(fetch(&resource).err().unwrap().contains("1 MiB"));
        worker.join().unwrap();
    }

    #[test]
    fn auto_json_keeps_the_existing_large_hierarchy_transport_limit() {
        let resource = ResourceUrl::parse("https://example.test/lod-meta.json").unwrap();
        assert_eq!(resource.limit(), Some(256 * 1024 * 1024));
        assert_eq!(
            metadata_shape(b"[]").err().unwrap(),
            "Expected a JSON metadata object"
        );
    }

    #[test]
    fn explicit_formats_override_endpoint_suffix_without_changing_the_url() {
        for (format, suffix) in [(DataFormat::Ply, ".ply"), (DataFormat::Sog, ".sog")] {
            let input = "https://assets.test/download.json?key=a%2Bb&x=1";
            let resource = ResourceUrl::parse_with_format(input, format).unwrap();
            assert!(resource.name.ends_with(suffix));
            assert_eq!(resource.url.as_str(), input);
            assert_eq!(resource.format, format);
        }
        let plain = include_bytes!("../../tests/fixtures/hosted-stream/chunks/meta.json");
        let hierarchy = include_bytes!("../../tests/fixtures/hosted-stream/lod-meta.json");
        assert!(DataFormat::SogUnbundled.validate(plain).is_ok());
        assert!(DataFormat::SogStream.validate(hierarchy).is_ok());
        assert!(DataFormat::SogUnbundled.validate(hierarchy).is_err());
        assert!(DataFormat::SogStream.validate(plain).is_err());
        assert!(DataFormat::SogUnbundled.validate(b"invalid JSON").is_err());
    }

    #[test]
    fn supersplat_links_resolve_to_published_metadata_with_v1_fallback() {
        for input in [
            "https://superspl.at/scene/78e7a5cd",
            " https://superspl.at/scene/78E7A5CD/ ",
            "https://superspl.at/view?id=78e7a5cd",
            "78e7a5cd",
            "https://d28zzqy0iyovbz.cloudfront.net/78e7a5cd/v2/meta.json",
        ] {
            let resource = ResourceUrl::parse(input).expect(input);
            assert_eq!(
                resource.url.as_str(),
                "https://d28zzqy0iyovbz.cloudfront.net/78e7a5cd/v2/meta.json"
            );
            assert_eq!(
                resource.fallback.as_ref().map(url::Url::as_str),
                Some("https://d28zzqy0iyovbz.cloudfront.net/78e7a5cd/v1/meta.json")
            );
            assert_eq!(resource.name, "meta.json");
        }
        for input in [
            "https://superspl.at/scene/not-an-id",
            "https://example.test/scene/78e7a5cd",
            "https://d28zzqy0iyovbz.cloudfront.net/78e7a5cd/v1/meta.json",
        ] {
            assert_eq!(supersplat_scene(input), None, "{input}");
        }
        let missing = ResourceUrl::parse("https://superspl.at/scene/78e7a5cd").unwrap();
        assert!(try_fallback(&missing, &Err("HTTP 403".into())).is_some());
        assert!(try_fallback(&missing, &Err("HTTP 500".into())).is_none());
        let plain = ResourceUrl::parse("https://example.test/meta.json").unwrap();
        assert!(try_fallback(&plain, &Err("HTTP 403".into())).is_none());
    }

    #[test]
    fn hosted_manifest_urls_preserve_authorized_query() {
        let resource = ResourceUrl::parse("https://example.test/scene/lod-meta.json?token=private")
            .expect("hosted streaming manifest");
        assert_eq!(resource.name, "lod-meta.json");
        assert_eq!(resource.url.query(), Some("token=private"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn request_headers(socket: &mut std::net::TcpStream) -> String {
        use std::io::Read;
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0 && request.len() + count <= 8192);
            request.extend_from_slice(&chunk[..count]);
        }
        String::from_utf8(request).unwrap().to_ascii_lowercase()
    }

    #[test]
    fn resource_urls_keep_signed_queries_but_use_path_for_format() {
        let resource =
            ResourceUrl::parse(" https://example.com/capture.PLY?token=secret#view ").unwrap();
        assert_eq!(resource.name, "capture.PLY");
        assert!(resource.url.as_str().contains("?token=secret"));
        for invalid in [
            "file:///scene.ply",
            "ftp://host/scene.sog",
            "https://host/scene.txt",
            "https://host/",
            "not a URL",
        ] {
            assert!(ResourceUrl::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn redirects_do_not_forward_signed_queries_as_referer() {
        use std::{io::Write, net::TcpListener};
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        let destination_url = format!("http://{}/final.ply", destination.local_addr().unwrap());
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let resource = ResourceUrl::parse(&format!(
            "http://{}/scene.ply?token=private",
            origin.local_addr().unwrap()
        ))
        .unwrap();
        let from = std::thread::spawn(move || {
            let (mut socket, _) = origin.accept().unwrap();
            request_headers(&mut socket);
            write!(socket, "HTTP/1.1 302 Found\r\nLocation: {destination_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let to = std::thread::spawn(move || {
            let (mut socket, _) = destination.accept().unwrap();
            let request = request_headers(&mut socket);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nply")
                .unwrap();
            request
        });
        let downloaded = fetch(&resource).unwrap();
        assert_eq!(downloaded.bytes, b"ply");
        assert!(downloaded.url.as_str().ends_with("/final.ply"));
        from.join().unwrap();
        assert!(!to.join().unwrap().contains("referer:"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn remote_reader_handles_success_http_errors_and_large_truncated_bodies() {
        use std::{io::Write, net::TcpListener};
        for (response, expected) in [
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nply",
                Ok(b"ply".to_vec()),
            ),
            (
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                Err("HTTP"),
            ),
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 268435457\r\nConnection: close\r\n\r\n",
                // The reader must attempt the body, not reject its advertised
                // size at the former cutoff. The server then closes early.
                Err("Could not read the HTTP response"),
            ),
        ] {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let resource = ResourceUrl::parse(&format!(
                "http://{}/scene.ply",
                server.local_addr().unwrap()
            ))
            .unwrap();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = server.accept().unwrap();
                request_headers(&mut socket);
                socket.write_all(response.as_bytes()).unwrap();
            });
            let actual = fetch(&resource).map(|download| download.bytes);
            match expected {
                Ok(bytes) => assert_eq!(actual.unwrap(), bytes),
                Err(message) => assert!(actual.unwrap_err().contains(message)),
            }
            worker.join().unwrap();
        }
        let mut body = vec![0; 9];
        assert!(append_bounded(&mut body, &[0, 0], Some(10)).is_err());
        assert_eq!(body.len(), 9);
    }
}
