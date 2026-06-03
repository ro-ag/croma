# ABC Specification Knowledge Base

This directory is a tracked reference knowledge base for Croma's ABC parser and
MusicXML exporter work.

## Source Priority

1. `raw/abc-2.1.dokuwiki.txt`
   - Authoritative default for Croma.
   - Downloaded from the abcnotation.com DokuWiki raw export for
     `abc:standard:v2.1`.
   - Treat this as the source of truth for `AbcSpecVersion::V21`.

2. `raw/abc-2.2-draft.dokuwiki.txt`
   - Draft/development appendix.
   - Downloaded from the abcnotation.com DokuWiki raw export for
     `abc:standard:v2.2`.
   - Use only behind explicit draft-compatibility decisions.

3. `raw/abc-standard-route-map.dokuwiki.txt`
   - Provenance for the development status of 2.2.

4. `raw/abc-standard-index.dokuwiki.txt`
   - Provenance for the standard index page.

Generated metadata:

- `generated/source-manifest.json`: source URLs, local paths, line counts, byte
  counts, and SHA-256 hashes.
- `generated/media-manifest.tsv`: image asset URLs, local paths, byte counts,
  and SHA-256 hashes.
- `generated/section-index.md`: heading index with line numbers into the raw
  DokuWiki snapshots.

## Working Rule

When implementing parser behavior, cite the 2.1 section first. If behavior
exists only in the 2.2 draft, keep it behind `AbcSpecVersion::V22Draft` or a
documented compatibility decision. If corpus evidence conflicts with a
reference converter, classify the mismatch as one of:

- Croma bug
- reference-converter artifact
- documented ABC/MusicXML policy decision

Do not silently promote draft 2.2 behavior into default ABC 2.1 mode.

## Sources

- ABC 2.1 standard raw export:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.1?do=export_raw`
- ABC 2.1 rendered source page:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.1`
- ABC 2.2 draft raw export:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.2?do=export_raw`
- ABC 2.2 draft rendered source page:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.2`
- ABC standard index:
  `https://abcnotation.com/wiki/abc%3Astandard`
- ABC standard route map:
  `https://abcnotation.com/wiki/abc%3Astandard%3Aroute-map`
- ABC wiki media assets referenced by the 2.1 and 2.2 pages:
  `https://abcnotation.com/wiki/_media/abc:standard:<asset-name>`
- License shown on rendered abcnotation.com wiki pages:
  `CC Attribution-Noncommercial-Share Alike 3.0 Unported`,
  `http://creativecommons.org/licenses/by-nc-sa/3.0/`
