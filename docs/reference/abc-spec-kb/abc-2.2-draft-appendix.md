# ABC 2.2 Draft Appendix

Primary source: `raw/abc-2.2-draft.dokuwiki.txt`.

This appendix exists so Croma can make deliberate compatibility choices without
accidentally treating draft behavior as ABC 2.1.

## Status

The downloaded source identifies itself as "The DRAFT abc music notation
standard 2.2" and begins with rationale/development notes. It says the draft
grew out of work intended to resolve volatile 2.1 clef and transposition issues.

Therefore:

- ABC 2.2 behavior is not Croma's default.
- Use `AbcSpecVersion::V22Draft` for explicit draft compatibility.
- If Croma supports a 2.2 feature in 2.1 mode for corpus pragmatism, document it
  as a compatibility policy decision.

## Main 2.2 Additions To Track

| Area | Draft signal | Croma handling |
| --- | --- | --- |
| `K:` and `V:` parameters | 2.2 rewrites voice modifiers, adds clef values, deprecates `middle`, and adds `clef=auto`. | Keep the model capable of these, but do not make all draft forms default 2.1 behavior. |
| Transposition | 2.2 incorporates a transposition proposal and adds section 13. | Keep score/sound/shift/instrument semantics out of the first 2.1 milestone except as diagnostics/model space. |
| Additional decorations | 2.2 adds more named decorations and navigation-like marks. | Parse conservatively; unsupported decoration should be diagnostic, not silent loss. |
| Multiple voices | Some multi-voice topics remain volatile or under review. | Preserve spans and source structure; avoid irreversible flattening. |
| Stylesheet directives | 2.2 still treats directive space as broad and not fully standardised. | Preserve or map only where MusicXML semantics are clear. |

## Compatibility Rules

- If 2.1 marks a section volatile and 2.2 resolves it, prefer a model that can
  express both before committing writer behavior.
- If a corpus file uses 2.2 syntax but does not declare it, recover only when
  deterministic and report the compatibility assumption.
- If a reference converter differs from both 2.1 and 2.2, classify the mismatch
  as a reference artifact unless MusicXML best practice gives a stronger reason
  to change Croma.

## Appendix Work Items

1. Add diagnostic metadata that can distinguish `abc-2.1` from
   `abc-2.2-draft` citations.
2. Add tests proving 2.2-only syntax is rejected or warned in 2.1 strict mode.
3. Add tests proving explicitly enabled 2.2 draft mode accepts chosen draft
   syntax.
4. Keep transposition and expanded clef support as a dedicated milestone after
   the 2.1 header/body/duration pipeline is reliable.

## Sources

- Primary ABC 2.2 draft raw export:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.2?do=export_raw`
- Primary ABC 2.2 draft rendered page:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.2`
- ABC standard route map:
  `https://abcnotation.com/wiki/abc%3Astandard%3Aroute-map`
- ABC standard index:
  `https://abcnotation.com/wiki/abc%3Astandard`
- Local downloaded raw source:
  `docs/reference/abc-spec-kb/raw/abc-2.2-draft.dokuwiki.txt`
- Local rendered snapshot:
  `docs/reference/abc-spec-kb/raw/abc-2.2-draft.html`
- Local download headers:
  `docs/reference/abc-spec-kb/raw/abc-2.2-draft.headers.txt`
- Local media assets referenced by the spec:
  `docs/reference/abc-spec-kb/raw/media/`
- Media manifest with SHA-256:
  `docs/reference/abc-spec-kb/generated/media-manifest.tsv`
- Download manifest with SHA-256:
  `docs/reference/abc-spec-kb/generated/source-manifest.json`
- License shown on rendered abcnotation.com wiki pages:
  `CC Attribution-Noncommercial-Share Alike 3.0 Unported`,
  `http://creativecommons.org/licenses/by-nc-sa/3.0/`
