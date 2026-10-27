# Splat fixtures

`tiny.ply` is an authored eight-Gaussian cube with axis-dependent RGB SH0
coefficients, uniform `exp(-1.5)` scales and sigmoid(3) opacity. Positions are
the eight combinations of -1 and +1 on each source axis. It contains no captured
or third-party scene data.

`tiny.sog` represents the same eight centers and scales as a SOG v2 ZIP containing
`meta.json` and five lossless WebP textures. Quantized quaternion/opacity values
approximate the PLY values. The means bounds are +/- ln(2), with endpoint pixels
0/65535. The color codebook spans [-3,3]; alpha is 243/255. Generated with Python
Pillow's lossless WebP encoder. Both fixtures are used by the browser acceptance
script; decoder tests also create their own independent one-point SOG fixture.
