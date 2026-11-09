//! Visibility field loading, SH basis and query rule against gsplat golden vectors.
use bevy_gaussian_splatting::visibility::{field, *};

const GOLDEN: &str = include_str!("../../../vendor/bevy_gaussian_splatting/tests/fixtures/visibility_golden.json");

fn floats(v: &serde_json::Value, key: &str) -> Vec<f32> {
    v[key].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
}

/// Minimal safetensors writer for tests.
fn safetensors(sh: &[f32], k: usize, peak: &[f32], meta: &str) -> Vec<u8> {
    let n = peak.len();
    let mut body = Vec::new();
    for x in sh {
        body.extend_from_slice(&field::f16::from_f32(*x).to_le_bytes());
    }
    let a = body.len();
    for x in peak {
        body.extend_from_slice(&x.to_le_bytes());
    }
    let header = serde_json::json!({
        "__metadata__": {"field": meta},
        "vis_sh": {"dtype": "F16", "shape": [n, k], "data_offsets": [0, a]},
        "v_peak": {"dtype": "F32", "shape": [n], "data_offsets": [a, body.len()]},
    })
    .to_string();
    let mut out = (header.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&body);
    out
}

#[test]
fn basis_matches_gsplat() {
    let g: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let (dirs, basis) = (floats(&g, "dirs"), floats(&g, "basis"));
    for (i, d) in dirs.chunks(3).enumerate() {
        let b = sh_basis4([d[0], d[1], d[2]]);
        for c in 0..SH {
            let e = basis[i * SH + c];
            assert!((b[c] - e).abs() < 1e-5, "dir {i} coeff {c}: {} vs {e}", b[c]);
        }
    }
}

const SH: usize = field::SH_COEFFS;

#[test]
fn packed_field_query_matches_reference() {
    let g: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let (dirs, sh, peak, vis) = (floats(&g, "dirs"), floats(&g, "sh"), floats(&g, "v_peak"), floats(&g, "vis"));
    let rho = g["rho"].as_f64().unwrap() as f32;
    let bytes = safetensors(&sh, SH, &peak, r#"{"degree": 4, "rho": 0.3, "kappa": 4.0, "n_views": 7}"#);
    let f = VisibilityField::from_safetensors(&bytes).unwrap();
    assert_eq!((f.count, f.degree, f.n_views), (peak.len(), 4, Some(7)));
    assert!((f.rho - 0.3).abs() < 1e-6);
    for (i, d) in dirs.chunks(3).enumerate() {
        let got = f.visibility(i, [d[0], d[1], d[2]], rho, 1.0);
        assert!((got - vis[i]).abs() < 2e-3, "gaussian {i}: {got} vs {}", vis[i]);
    }
}

#[test]
fn lower_degree_fields_pad_with_zeros() {
    let sh = [1.0f32, 0.5, -0.25, 0.125];
    let f = VisibilityField::from_safetensors(&safetensors(&sh, 4, &[2.0], "{}")).unwrap();
    let (c, peak) = f.coefficients(0);
    assert_eq!((f.degree, peak), (1, 2.0));
    assert_eq!(&c[..4], &sh);
    assert!(c[4..].iter().all(|x| *x == 0.0));
}

#[test]
fn rejects_mismatched_shapes() {
    let mut bytes = safetensors(&[0.0; 25], 25, &[1.0, 1.0], "{}");
    assert!(VisibilityField::from_safetensors(&bytes).is_err());
    bytes.truncate(4);
    assert!(VisibilityField::from_safetensors(&bytes).is_err());
    assert!(VisibilityField::from_safetensors(&safetensors(&[0.0; 6], 6, &[1.0], "{}")).is_err());
}

#[test]
fn curves() {
    let mut s = VisibilitySettings::default();
    assert_eq!((s.keep(0.2), s.keep(0.8)), (0.0, 1.0));
    assert!((s.keep(0.5) - 0.5).abs() < 1e-6);
    s.gamma = 2.0;
    assert!((s.keep(0.5) - 0.25).abs() < 1e-6);
    s.curve = VisibilityCurve::Linear;
    s.gamma = 1.0;
    assert!((s.keep(0.4) - 0.25).abs() < 1e-6);
    s.curve = VisibilityCurve::Hide;
    assert_eq!((s.keep(0.49), s.keep(0.5)), (0.0, 1.0));
    s.curve = VisibilityCurve::Smooth;
    s.lo = 0.6;
    s.hi = 0.6;
    assert_eq!((s.keep(0.59), s.keep(0.6)), (0.0, 1.0));
}

#[test]
fn fit_residual_is_normalised_and_caps_visibility() {
    // Isotropic evidence (only the l=0 coefficient) so visibility is ~1 from any direction.
    let mut sh = vec![0.0f32; 2 * SH];
    sh[0] = 200.0;
    sh[SH] = 200.0;
    let peak = [200.0 * 0.282_094_8f32; 2];
    let mut bytes = safetensors(&sh, SH, &peak, r#"{"degree": 4, "rho": 0.3, "fit_e_ref": 0.2}"#);
    // Append a fit_resid tensor by rebuilding the header.
    let n = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let mut header: serde_json::Value = serde_json::from_slice(&bytes[8..8 + n]).unwrap();
    let mut body = bytes.split_off(8 + n);
    let a = body.len();
    for x in [0.05f32, 0.4] {
        body.extend_from_slice(&x.to_le_bytes());
    }
    header["fit_resid"] = serde_json::json!({"dtype": "F32", "shape": [2], "data_offsets": [a, body.len()]});
    let h = header.to_string();
    let mut out = (h.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(h.as_bytes());
    out.extend_from_slice(&body);
    let f = VisibilityField::from_safetensors(&out).unwrap();
    assert!((f.fit(0) - 0.25).abs() < 1e-3 && (f.fit(1) - 1.0).abs() < 1e-3);
    let d = [0.0, 0.0, 1.0];
    assert!(f.visibility(0, d, 0.3, 1.0) > 0.99);
    assert!((f.visibility_fit(0, d, 0.3, 1.0, 1.0) - 0.75).abs() < 1e-3);
    assert!(f.visibility_fit(1, d, 0.3, 1.0, 1.0).abs() < 1e-3);
    assert!((f.visibility_fit(1, d, 0.3, 1.0, 0.5) - 0.5).abs() < 1e-3);
}
