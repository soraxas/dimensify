//! Per-Gaussian anisotropic visibility field: CPU loading, packing and reference evaluation.
//!
//! A field stores, for every Gaussian, real spherical-harmonic coefficients (degree <= 4, gsplat
//! basis) of the evidence `v(d)` that the Gaussian was observed from direction `d` (camera to
//! Gaussian, in the cloud's local frame), plus the peak evidence `v_peak` over the sphere.
//! Visibility from `d` is `(1 - exp(-gain v)) * clamp(v / (rho v_peak), 0, 1)`.
//!
//! Files are safetensors with `vis_sh` (F16 `[N, K]`, K = (degree+1)^2), `v_peak` (F32 or F16
//! `[N]`) and optional metadata key `field` (JSON with `degree`, `rho`, `kappa`, `n_views`).

use half::f16;

/// Coefficients stored per Gaussian on the GPU: 25 SH halves plus `v_peak`, as 13 `u32` words.
pub const PACKED_WORDS: usize = 13;
pub const SH_COEFFS: usize = 25;

#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityField {
    pub count: usize,
    pub degree: u32,
    /// Relative-evidence threshold the field was built with (default for the query rule).
    pub rho: f32,
    pub kappa: Option<f32>,
    pub n_views: Option<u32>,
    /// `count * PACKED_WORDS` words: halves `[sh_0 .. sh_24, v_peak]`, two per word, low half first.
    pub packed: Vec<u32>,
}

#[derive(Debug)]
pub enum FieldError {
    Format(String),
    Missing(&'static str),
    Tensor(&'static str, String),
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(m) => write!(f, "not a safetensors visibility field: {m}"),
            Self::Missing(n) => write!(f, "missing tensor `{n}`"),
            Self::Tensor(n, m) => write!(f, "tensor `{n}`: {m}"),
        }
    }
}

impl std::error::Error for FieldError {}

struct TensorView<'a> {
    dtype: String,
    shape: Vec<usize>,
    data: &'a [u8],
}

fn read_f32(t: &TensorView, i: usize) -> f32 {
    match t.dtype.as_str() {
        "F16" => f16::from_le_bytes([t.data[2 * i], t.data[2 * i + 1]]).to_f32(),
        _ => f32::from_le_bytes(t.data[4 * i..4 * i + 4].try_into().unwrap()),
    }
}

impl VisibilityField {
    /// Parse a safetensors visibility field.
    pub fn from_safetensors(bytes: &[u8]) -> Result<Self, FieldError> {
        let fmt = |m: &str| FieldError::Format(m.to_string());
        if bytes.len() < 8 {
            return Err(fmt("too short"));
        }
        let n = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
        let header_end = 8usize.checked_add(n).filter(|&e| e <= bytes.len()).ok_or_else(|| fmt("bad header length"))?;
        let header: serde_json::Value =
            serde_json::from_slice(&bytes[8..header_end]).map_err(|e| FieldError::Format(e.to_string()))?;
        let body = &bytes[header_end..];
        let tensor = |name: &'static str| -> Result<TensorView, FieldError> {
            let t = header.get(name).ok_or(FieldError::Missing(name))?;
            let dtype = t["dtype"].as_str().unwrap_or_default().to_string();
            let shape: Vec<usize> = t["shape"]
                .as_array()
                .ok_or_else(|| FieldError::Tensor(name, "no shape".into()))?
                .iter()
                .map(|v| v.as_u64().unwrap_or(0) as usize)
                .collect();
            let off = t["data_offsets"].as_array().ok_or_else(|| FieldError::Tensor(name, "no offsets".into()))?;
            let (a, b) = (off[0].as_u64().unwrap_or(0) as usize, off[1].as_u64().unwrap_or(0) as usize);
            let elem = match dtype.as_str() {
                "F16" => 2,
                "F32" => 4,
                d => return Err(FieldError::Tensor(name, format!("unsupported dtype {d}"))),
            };
            if a > b || b > body.len() || b - a != shape.iter().product::<usize>() * elem {
                return Err(FieldError::Tensor(name, "data offsets do not match shape".into()));
            }
            Ok(TensorView { dtype, shape, data: &body[a..b] })
        };
        let sh = tensor("vis_sh")?;
        let peak = tensor("v_peak")?;
        if sh.shape.len() < 2 {
            return Err(FieldError::Tensor("vis_sh", "expected [N, K]".into()));
        }
        let (count, k) = (sh.shape[0], sh.shape[1..].iter().product::<usize>());
        let degree = (k as f64).sqrt() as u32 - 1;
        if (degree + 1) * (degree + 1) != k as u32 || degree > 4 {
            return Err(FieldError::Tensor("vis_sh", format!("{k} coefficients is not a degree <= 4 basis")));
        }
        if peak.shape.iter().product::<usize>() != count {
            return Err(FieldError::Tensor("v_peak", "length differs from vis_sh".into()));
        }
        let meta: serde_json::Value = header
            .get("__metadata__")
            .and_then(|m| m.get("field"))
            .and_then(|f| f.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        let mut packed = vec![0u32; count * PACKED_WORDS];
        let mut halves = [0u16; 2 * PACKED_WORDS];
        for i in 0..count {
            halves.fill(0);
            for c in 0..k {
                halves[c] = f16::from_f32(read_f32(&sh, i * k + c)).to_bits();
            }
            halves[SH_COEFFS] = f16::from_f32(read_f32(&peak, i)).to_bits();
            for w in 0..PACKED_WORDS {
                packed[i * PACKED_WORDS + w] = halves[2 * w] as u32 | (halves[2 * w + 1] as u32) << 16;
            }
        }
        Ok(Self {
            count,
            degree,
            rho: meta["rho"].as_f64().unwrap_or(0.3) as f32,
            kappa: meta["kappa"].as_f64().map(|v| v as f32),
            n_views: meta["n_views"].as_u64().map(|v| v as u32),
            packed,
        })
    }

    /// SH coefficients and `v_peak` of Gaussian `i`, unpacked.
    pub fn coefficients(&self, i: usize) -> ([f32; SH_COEFFS], f32) {
        let w = &self.packed[i * PACKED_WORDS..(i + 1) * PACKED_WORDS];
        let half = |j: usize| f16::from_bits((w[j / 2] >> (16 * (j % 2))) as u16).to_f32();
        (std::array::from_fn(half), half(SH_COEFFS))
    }

    /// CPU reference of the GPU query: visibility of Gaussian `i` seen along unit `dir`.
    pub fn visibility(&self, i: usize, dir: [f32; 3], rho: f32, gain: f32) -> f32 {
        let (sh, peak) = self.coefficients(i);
        let b = sh_basis4(dir);
        let v = sh.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<f32>();
        visibility_from_evidence(v, peak, rho, gain)
    }
}

/// Query rule: absolute evidence saturation times share of the best direction's evidence.
pub fn visibility_from_evidence(v: f32, v_peak: f32, rho: f32, gain: f32) -> f32 {
    let v = v.max(0.0);
    let rel = (v / v_peak.max(1e-6) / rho.max(1e-6)).clamp(0.0, 1.0);
    (1.0 - (-gain * v).exp()) * rel
}

/// gsplat real SH basis up to degree 4 (Sloan 2013), coefficient order as gsplat stores it.
pub fn sh_basis4([x, y, z]: [f32; 3]) -> [f32; SH_COEFFS] {
    let mut r = [0.0f32; SH_COEFFS];
    r[0] = 0.282_094_8;
    let a = -0.488_602_5;
    r[1] = a * y;
    r[2] = -a * z;
    r[3] = a * x;
    let z2 = z * z;
    let b = -1.092_548_4 * z;
    let a = 0.546_274_2;
    let (c1, s1) = (x * x - y * y, 2.0 * x * y);
    r[4] = a * s1;
    r[5] = b * y;
    r[6] = 0.946_174_7 * z2 - 0.315_391_57;
    r[7] = b * x;
    r[8] = a * c1;
    let c = -2.285_229 * z2 + 0.457_045_8;
    let b = 1.445_305_7 * z;
    let a = -0.590_043_6;
    let (c2, s2) = (x * c1 - y * s1, x * s1 + y * c1);
    r[9] = a * s2;
    r[10] = b * s1;
    r[11] = c * y;
    r[12] = z * (1.865_881_7 * z2 - 1.119_529);
    r[13] = c * x;
    r[14] = b * c1;
    r[15] = a * c2;
    let d = z * (-4.683_326 * z2 + 2.007_139_6);
    let c = 3.311_611_4 * z2 - 0.473_087_35;
    let b = -1.770_130_8 * z;
    let a = 0.625_835_7;
    let (c3, s3) = (x * c2 - y * s2, x * s2 + y * c2);
    r[16] = a * s3;
    r[17] = b * s2;
    r[18] = c * s1;
    r[19] = d * y;
    r[20] = 1.984_313_5 * z2 * (1.865_881_7 * z2 - 1.119_529) - 1.006_230_6 * (0.946_174_7 * z2 - 0.315_391_57);
    r[21] = d * x;
    r[22] = c * c1;
    r[23] = b * c2;
    r[24] = a * c3;
    r
}
