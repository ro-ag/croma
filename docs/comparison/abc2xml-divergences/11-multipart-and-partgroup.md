# 11 — Multipart model and `<part-group>` brackets

## Part-per-voice: no divergence (Croma feature, both agree)

Across all 9,935 comparable files the **part count is identical** between Croma
and the reference — both emit strictly one `<part>` per ABC voice (`V:`), and no
reference file uses a multi-staff single part. The often-stated premise that
"abc2xml combines voices differently" is **not borne out** by this corpus.

Verified: `tune_001763` = 4 parts / 4 voices, `tune_006030` = 2 parts,
`tune_000482` = 1 part — identical content (parts, voices, measures, notes) in
both tools.

Croma's one-part-per-voice model is the correct MusicXML content representation
and is an intentional feature.

## `<part-group>` brackets: a cosmetic gap

The real structural difference is presentation: the reference emits MusicXML
`<part-group type="start"/>` / `…"stop"/>` bracket elements derived from the
ABC `%%staves` / `%%score` grouping directives; Croma emits none.

- **342** reference files contain `<part-group>`; Croma has it in **0**.
- `tune_001763` (`%%staves [1 2 3 4]`): both emit 4 parts / 60 measures with
  identical voice content; the reference additionally emits 2 `<part-group>`
  elements (bracket start/stop). The note content is identical.

## ABC 2.1 basis

`%%staves` / `%%score` are **stylesheet directives** (presentation / staff
grouping), not music code (§2.2.1 lines 264–266). They affect visual staff
bracketing, not the part-per-voice content model.

## Verdict

**CROMA_FEATURE** for the part-per-voice model (no divergence — both agree).
The `<part-group>` omission is a **cosmetic presentation gap** (visual staff
brackets), not a structural or content correctness issue: parts, voices,
measures, and notes are identical. It is a candidate future enhancement (emit
`<part-group>` from `%%staves`/`%%score`), tracked separately, not a bug.
