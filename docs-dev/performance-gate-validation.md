# Temporary performance gate validation

This disposable pull request validates Bencher regression reporting. It must not
be merged. The initial commit deliberately triples timed SOG decoding work; the
follow-up restores the original workload to verify the gate recovers.
