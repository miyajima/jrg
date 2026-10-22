# Google API key alerts in a saved documentation page

On 2026-10-22, GitHub reported nine Google API key alerts, all at line 7269
of `experiments/2026-10-22-web-selection-rules/raw/gcs-retry.html` in
commit `901747c47ca48e087e2e2da96fc02cb09a3ea860`.

## Source and use

The file is a downloaded copy of the public
[Google Cloud Storage retry strategy documentation](https://docs.cloud.google.com/storage/docs/retry-strategy).
The nine strings occur in Google DevSite's JavaScript bootstrap configuration.
A fresh unauthenticated HTTP GET returned 200 and contained all nine identical
strings. The values were compared in memory and were not printed or saved in
this report. These are third-party public page configuration values; they were
not issued or configured as credentials for jrg. No key was submitted to a
Google API to test its validity. Their validity and API restrictions are unknown.

## Sanitization

All nine occurrences were replaced with `[REDACTED_GOOGLE_API_KEY]`. The article's
rendered Markdown and title are exactly identical before and after redaction.
The selection inputs and recorded evaluation outputs were not regenerated.

- Original downloaded HTML SHA-256:
  `47104e05c598ad9a1287fa77909f05c63f389c2770b81214d94544bf8e4e5268`
- Sanitized HTML SHA-256:
  `87539a085e24a35f4b41036eff87692ad93dc455660725e3109e127eb9fca4aa`

The fetch manifest retains the original download hash and records the redaction,
current hash and byte length. File integrity mappings were updated transitively.
The results retain their original input-manifest hash in `sanitization` metadata
and explicitly record that the evaluation was not rerun. The ablation's
`original_frozen_inputs_sha256` remains a historical provenance reference.

## Prevention and verification

Run `python3 -B tests/secret_scan.py` and
`python3 -B scripts/check-secrets.py` before committing downloaded pages.
CI runs both commands. The scanner checks tracked working-tree files for Google
API key patterns, reports only paths and line numbers, and fails on read errors.
It is a targeted check; it does not cover other credential formats or Git history.

The replacement commit must have `2f246c74bcd03ed099b307d9d4747a37eb9a7f9b`
as its parent. Update the remote with an explicit `--force-with-lease` expectation
and verify the resulting remote head and absence of the original commit from
branch and tag ancestry. Removing branch references does not purge GitHub's
cached commit objects or other people's clones.
