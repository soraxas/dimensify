"""Test vectors for the Gaussian visibility field: degree-4 real SH basis (gsplat convention) and the
visibility query rule. Needs torch, scipy and gsplat (CUDA).

Writes JSON {rho, gain, dirs [M*3], sh [M*25], v_peak [M], basis [M*25], v [M], vis [M]}.

    python tools/visibility/vis_golden.py vendor/bevy_gaussian_splatting/tests/fixtures/visibility_golden.json
"""

import json
import math
import sys
from pathlib import Path

import numpy as np
import torch
from gsplat import spherical_harmonics
from gsplat.cuda._torch_impl import _eval_sh_bases_fast
from scipy.special import spherical_in


def vmf_sh_scale(kappa, degree):
    """C_l with exp(k (d.mu - 1)) = sum_l C_l sum_m Y_lm(mu) Y_lm(d)."""
    l = np.arange(degree + 1)
    return torch.tensor(np.repeat(4 * np.pi * np.exp(-kappa) * spherical_in(l, kappa), 2 * l + 1))


def fib_sphere(n):
    i = torch.arange(n, dtype=torch.float64) + 0.5
    phi = torch.acos(1 - 2 * i / n)
    th = math.pi * (1 + 5 ** 0.5) * i
    return torch.stack([torch.cos(th) * torch.sin(phi), torch.sin(th) * torch.sin(phi), torch.cos(phi)], -1)


def query(v, v_peak, rho, gain):
    v = v.clamp(min=0)
    return (1 - torch.exp(-gain * v)) * (v / v_peak.clamp_min(1e-6) / rho).clamp(0, 1)


out = Path(sys.argv[1])
torch.manual_seed(0)
M = 64
dirs = torch.nn.functional.normalize(torch.randn(M, 3, dtype=torch.float64), dim=-1)
dirs[:6] = torch.tensor([[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]], dtype=torch.float64)
C = vmf_sh_scale(4.0, 4)
sh = torch.zeros(M, 25, dtype=torch.float64)
for _ in range(3):  # a few vMF lobes per Gaussian, random weights
    mu = torch.nn.functional.normalize(torch.randn(M, 3, dtype=torch.float64), dim=-1)
    sh += torch.rand(M, 1, dtype=torch.float64) * 20 * C * _eval_sh_bases_fast(25, mu)
sh = sh.half().double()  # stored as fp16
v_peak = (sh @ _eval_sh_bases_fast(25, fib_sphere(128)).T).max(1).values.clamp_min(1e-6).half().double()
basis = _eval_sh_bases_fast(25, dirs)
v = (basis * sh).sum(-1)
rho, gain = 0.3, 1.0
vis = query(v, v_peak, rho, gain)

# camera at the origin (identity view), so the SH direction is normalize(mean - camera) = dirs
v_cuda = spherical_harmonics(4, 2 * dirs.float().cuda(), torch.eye(4, device="cuda")[None],
                             sh.float().cuda()[..., None])[0, :, 0].double().cpu()
err = float((v_cuda - v).abs().max())
print(f"torch basis vs gsplat CUDA spherical_harmonics: max |dv| {err:.2e}")
assert err < 1e-3 * max(1.0, float(v.abs().max()))

r = lambda t: [round(float(x), 7) for x in t.flatten()]  # noqa: E731
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps({"note": "tools/visibility/vis_golden.py; gsplat real SH basis, degree 4",
                           "rho": rho, "gain": gain, "dirs": r(dirs), "sh": r(sh), "v_peak": r(v_peak),
                           "basis": r(basis), "v": r(v), "vis": r(vis)}))
print(f"{M} vectors -> {out}; vis {float(vis.min()):.3f}..{float(vis.max()):.3f}, "
      f"share < 0.3: {float((vis < 0.3).double().mean()):.2f}")
