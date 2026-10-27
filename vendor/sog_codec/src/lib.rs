//! Byte-exact WebP data decoding shared by SOG and its isolated WASM worker.
//! No engine, DOM, colour management, premultiplication or spatial conversion.
use image::{DynamicImage, ImageDecoder, codecs::webp::WebPDecoder};
use std::io::Cursor;

#[derive(Debug)]
pub enum Error {
    Image(image::ImageError),
    Invalid(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image(e) => e.fmt(f),
            Self::Invalid(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: u64,
}

fn header(
    decoder: &WebPDecoder<Cursor<&[u8]>>,
    max_dimension: u32,
    max_bytes: u64,
) -> Result<Header, Error> {
    let (width, height) = decoder.dimensions();
    if decoder.has_animation() {
        return Err(Error::Invalid("animated WebP is not SOG data"));
    }
    if width == 0 || height == 0 || width > max_dimension || height > max_dimension {
        return Err(Error::Invalid("WebP dimensions exceed allowance"));
    }
    let rgba_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::Invalid("WebP byte length overflow"))?;
    if rgba_bytes > max_bytes || usize::try_from(rgba_bytes).is_err() {
        return Err(Error::Invalid("WebP output exceeds allowance"));
    }
    Ok(Header {
        width,
        height,
        rgba_bytes,
    })
}

pub fn inspect(bytes: &[u8], max_dimension: u32, max_bytes: u64) -> Result<Header, Error> {
    let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(Error::Image)?;
    header(&decoder, max_dimension, max_bytes)
}

pub fn decode(
    bytes: &[u8],
    max_dimension: u32,
    max_bytes: u64,
) -> Result<(Header, Vec<u8>), Error> {
    let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(Error::Image)?;
    let header = header(&decoder, max_dimension, max_bytes)?;
    let rgba = DynamicImage::from_decoder(decoder)
        .map_err(Error::Image)?
        .into_rgba8()
        .into_raw();
    Ok((header, rgba))
}

#[cfg(all(feature = "worker", target_arch = "wasm32"))]
mod bindings {
    use wasm_bindgen::prelude::*;

    /// Collect bounded encoded inputs and expand all attributes inside this worker.
    #[wasm_bindgen]
    pub struct ExpandedJob {
        meta: crate::expanded::SogMeta,
        files: std::collections::HashMap<String, Vec<u8>>,
        coefficients_per_channel: usize,
    }
    #[wasm_bindgen]
    impl ExpandedJob {
        #[wasm_bindgen(constructor)]
        pub fn new(meta: &str, coefficients_per_channel: usize) -> Result<ExpandedJob, JsValue> {
            Ok(Self {
                meta: crate::expanded::parse_meta(meta.as_bytes())
                    .map_err(|e| JsValue::from_str(&e.to_string()))?,
                files: Default::default(),
                coefficients_per_channel,
            })
        }
        pub fn add(&mut self, name: String, bytes: Vec<u8>) {
            self.files.insert(name, bytes);
        }
        pub fn finish(&mut self) -> Result<Vec<f32>, JsValue> {
            let cloud = crate::expanded::decode_expanded(
                &self.meta,
                &self.files,
                self.coefficients_per_channel,
            )
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
            self.files.clear();
            let mut planar = cloud.positions;
            // Exact final capacity avoids geometric growth crossing wasm32's
            // isize allocation limit for otherwise addressable large scenes.
            planar
                .reserve_exact(cloud.rotations.len() + cloud.scales.len() + cloud.harmonics.len());
            planar.extend(cloud.rotations);
            planar.extend(cloud.scales);
            planar.extend(cloud.harmonics);
            Ok(planar)
        }
    }

    #[wasm_bindgen]
    pub struct DecodedImage {
        pub width: u32,
        pub height: u32,
        pixels: Vec<u8>,
    }
    #[wasm_bindgen]
    impl DecodedImage {
        pub fn take_pixels(&mut self) -> Vec<u8> {
            std::mem::take(&mut self.pixels)
        }
    }
    #[wasm_bindgen]
    pub fn decode_image(
        bytes: &[u8],
        max_dimension: u32,
        max_bytes: u32,
    ) -> Result<DecodedImage, JsValue> {
        let (header, pixels) = super::decode(bytes, max_dimension, u64::from(max_bytes))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(DecodedImage {
            width: header.width,
            height: header.height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<u8>, Vec<u8>) {
        let rgba: Vec<u8> = (0..=255).flat_map(|a| [193, 17, 251, a]).collect();
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(&rgba, 16, 16, image::ExtendedColorType::Rgba8)
            .unwrap();
        (bytes, rgba)
    }
    #[test]
    fn preserves_rgb_under_every_alpha_and_inspects_before_decode() {
        let (bytes, expected) = fixture();
        assert_eq!(
            inspect(&bytes, 16, 1024).unwrap(),
            Header {
                width: 16,
                height: 16,
                rgba_bytes: 1024
            }
        );
        assert_eq!(decode(&bytes, 16, 1024).unwrap().1, expected);
    }
    #[test]
    fn rejects_malformed_and_over_budget_images() {
        let (bytes, _) = fixture();
        for bytes in [&b"bad"[..], &bytes[..bytes.len() / 2]] {
            assert!(decode(bytes, 16, 1024).is_err());
        }
        assert!(decode(&bytes, 15, 1024).is_err());
        assert!(decode(&bytes, 16, 1023).is_err());
    }
}

pub mod expanded;
