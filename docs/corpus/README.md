# Corpus LFS Cache

This directory contains optional Git LFS cache artifacts for the external real
ABC corpus used by the phase 10 parser work.

- `zenodo-10k-abc.tar.gz`: compressed ABC source corpus cache, tracked through
  Git LFS.
- `zenodo-10k-abc.tar.gz.sha256`: tracked checksum used by
  `tools/provision_corpus.py import-archive`.

The archive is an integrity-checked cache, not the source of truth. Provenance
remains the Zenodo record documented in `docs/reference/corpus-inventory.md` and
`docs/testing/corpus-reproducibility.md`.

To provision local ignored corpus files:

```sh
tools/session_bootstrap.sh --fetch-corpus
```
