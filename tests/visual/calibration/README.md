# Calibration captures

`native-vulkan-second-run` contains the clean captures from hosted run
[40709856837](https://github.com/soraxas/dimensify/actions/runs/40709856837).
Its repeat was byte-identical. Compare these against the original Vulkan
reference from run 40707382978 to reproduce the recorded one-level variation.
The reference manifest records both runs, all four capture hashes and measured
error bounds. The exact numerical cause is not established.

The comparator contract test accepts this measured variation and rejects one
channel error of two, or 525 channel errors of one. It accepts 524 errors of
one, verifying the separate total-error limit. Semantic assertions are unchanged.
