use super::super::decode::MAX_BYTES;

pub(super) struct ResourceUrl {
    pub name: String,
    pub url: url::Url,
}
impl ResourceUrl {
    /// Validate a direct HTTP(S) Gaussian file URL, preserving query parameters.
    pub(super) fn parse(input: &str) -> Result<Self, String> {
        let url = url::Url::parse(input.trim())
            .map_err(|_| "Enter a full HTTP or HTTPS URL".to_owned())?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("Use an HTTP or HTTPS URL".into());
        }
        let name = url
            .path_segments()
            .and_then(|mut parts| parts.next_back())
            .unwrap_or_default()
            .to_owned();
        if !name.to_ascii_lowercase().ends_with(".ply")
            && !name.to_ascii_lowercase().ends_with(".sog")
            && !name.to_ascii_lowercase().ends_with(".json")
        {
            return Err("The URL path must end in .ply, .sog or a SOG manifest .json".into());
        }
        Ok(Self { name, url })
    }
}

fn append_bounded(bytes: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(bytes.len()) {
        return Err("Download exceeds the 256 MiB input limit".into());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
/// Download a bounded response on a worker thread, using normal TLS verification.
pub(super) fn fetch(resource: &ResourceUrl) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let client = reqwest::blocking::Client::builder()
        .referer(false)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "Could not start HTTP client".to_owned())?;
    let mut response = client
        .get(resource.url.clone())
        .send()
        .map_err(|_| "Download failed (connection, TLS or timeout)".to_owned())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err("Download exceeds the 256 MiB input limit".into());
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
        append_bounded(&mut bytes, &buffer[..count], MAX_BYTES)?;
    }
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
/// Fetch a bounded browser stream with the host's normal CORS policy.
pub(super) async fn fetch(resource: &ResourceUrl) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let response: web_sys::Response = JsFuture::from(
        web_sys::window()
            .ok_or("No browser window")?
            .fetch_with_str(resource.url.as_str()),
    )
    .await
    .map_err(|_| "Download failed; check the URL, HTTPS and the host's CORS settings".to_owned())?
    .dyn_into()
    .map_err(|_| "Invalid HTTP response".to_owned())?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err("Download exceeds the 256 MiB input limit".into());
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
        if chunk.length() as usize > MAX_BYTES.saturating_sub(bytes.len()) {
            let _ = reader.cancel();
            return Err("Download exceeds the 256 MiB input limit".into());
        }
        append_bounded(&mut bytes, &chunk.to_vec(), MAX_BYTES)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(fetch(&resource).unwrap(), b"ply");
        from.join().unwrap();
        assert!(!to.join().unwrap().contains("referer:"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn remote_reader_handles_success_http_errors_and_bounded_bodies() {
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
                Err("256 MiB"),
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
            let actual = fetch(&resource);
            match expected {
                Ok(bytes) => assert_eq!(actual.unwrap(), bytes),
                Err(message) => assert!(actual.unwrap_err().contains(message)),
            }
            worker.join().unwrap();
        }
        let mut body = vec![0; 9];
        assert!(append_bounded(&mut body, &[0, 0], 10).is_err());
        assert_eq!(body.len(), 9);
    }
}
