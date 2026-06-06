BEGIN TRANSACTION;
CREATE TABLE artifact (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  description TEXT,
  UNIQUE (phase_id, kind, path)
);
INSERT INTO "artifact" VALUES(1,'phase-10a','triage_md','docs/untracked/phase-10a-mismatch-root-cause-triage.md','Phase 10-a Polars/root-cause triage.');
INSERT INTO "artifact" VALUES(2,'phase-10a','triage_json','docs/untracked/phase-10a-mismatch-root-cause-triage.json','Phase 10-a machine-readable triage.');
INSERT INTO "artifact" VALUES(3,'phase-10e','full_report','docs/untracked/phase-10e/full-10k-report-only-compare-report.json','Full 10k report-only comparison with parallel tooling.');
INSERT INTO "artifact" VALUES(4,'phase-10f','triage_md','docs/untracked/phase-10f/phase-10f-polars-parser-target-triage.md','Cross-bar tie target triage.');
INSERT INTO "artifact" VALUES(5,'phase-10f','full_report','docs/untracked/phase-10f/full-10k-after-report-only-compare-report.json','Full 10k after cross-bar tie fix.');
INSERT INTO "artifact" VALUES(6,'phase-10g','triage_md','docs/untracked/phase-10g/phase-10g-text-direction-triage.md','Malformed quoted chord/text-direction triage.');
INSERT INTO "artifact" VALUES(7,'phase-10g','full_report','docs/untracked/phase-10g/full-10k-after-report-only-compare-report.json','Full 10k after malformed chord export fix.');
INSERT INTO "artifact" VALUES(8,'phase-10h','triage_md','docs/untracked/phase-10h/phase-10h-lyric-alignment-triage.md','Lyric hyphen control triage.');
INSERT INTO "artifact" VALUES(9,'phase-10h','full_report','docs/untracked/phase-10h/full-10k-after-report-only-compare-report.json','Full 10k after lyric hyphen export fix.');
INSERT INTO "artifact" VALUES(11,'phase-10i','triage_md','docs/untracked/phase-10i/phase-10i-lyric-melisma-triage.md','Lyric melisma/NBSP/empty-extender triage.');
INSERT INTO "artifact" VALUES(12,'phase-10i','triage_json','docs/untracked/phase-10i/phase-10i-lyric-melisma-triage.json','Machine-readable Phase 10-i triage.');
INSERT INTO "artifact" VALUES(13,'phase-10i','target_before_report','docs/untracked/phase-10i/target-before-compare-report.json','Target 107-file comparison before Phase 10-i fix.');
INSERT INTO "artifact" VALUES(14,'phase-10i','target_after_report','docs/untracked/phase-10i/target-after-compare-report.json','Target 107-file comparison after Phase 10-i fix.');
INSERT INTO "artifact" VALUES(15,'phase-10i','full_report','docs/untracked/phase-10i/full-10k-after-report-only-compare-report.json','Full 10k report-only comparison after Phase 10-i fix.');
INSERT INTO "artifact" VALUES(16,'phase-10i','residual_file_list','docs/untracked/phase-10i/residual-lyric-files.txt','Residual lyric target file list.');
INSERT INTO "artifact" VALUES(17,'phase-10i','target_corpus','docs/untracked/phase-10i/residual-lyric-target-corpus/','Target corpus for residual lyric/melisma analysis.');
CREATE TABLE memory (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  notes TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "memory" VALUES('pinned_toolchain','/Users/rodox/.rustup/toolchains/1.96.0-aarch64-apple-darwin','Use PATH="$TOOLCHAIN/bin:$PATH" RUSTC="$TOOLCHAIN/bin/rustc" "$TOOLCHAIN/bin/cargo" ...','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('python_tooling','Use uv for Python/music21/Polars/corpus tooling.','Do not hand-roll venv setup.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('abc_reference_repo','/Users/rodox/dev/abc','Use as parser-policy/test/prover reference only; do not copy whole parser.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('generated_artifacts_policy','Keep generated corpus XML/parquet/jsonl/reports under docs/untracked/.','Do not commit generated corpus artifacts unless explicitly requested.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('formatter_lsp_gate','Formatter and LSP wait until parser quality is proven.','Parser/corpus/music21 work remains priority.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('croma_core_crates_io','Keep croma-core crates.io-compatible.','Avoid path-only/local runtime assumptions in library code.','2026-06-06 01:33:56');
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "meta" VALUES('db_path','docs/untracked/croma-progress.sqlite','2026-06-06 01:33:56');
INSERT INTO "meta" VALUES('purpose','Local ignored Croma phase ledger for compact status recall and prompt generation.','2026-06-06 01:33:56');
INSERT INTO "meta" VALUES('schema_version','1','2026-06-06 01:33:56');
CREATE TABLE metric (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  scope TEXT NOT NULL,
  name TEXT NOT NULL,
  before_value TEXT,
  after_value TEXT,
  delta TEXT,
  unit TEXT,
  notes TEXT,
  UNIQUE (phase_id, scope, name)
);
INSERT INTO "metric" VALUES(1,'phase-9','full_10k_smoke','export_successes','unknown','9935',NULL,'files',NULL);
INSERT INTO "metric" VALUES(2,'phase-9','full_10k_smoke','export_failures','66','65','-1','files','Remaining failures classified as malformed/reference artifacts.');
INSERT INTO "metric" VALUES(3,'phase-10','full_10k','matches',NULL,'1335',NULL,'files',NULL);
INSERT INTO "metric" VALUES(4,'phase-10','full_10k','mismatches',NULL,'8600',NULL,'files',NULL);
INSERT INTO "metric" VALUES(5,'phase-10','full_10k','music21_import_failures',NULL,'0',NULL,'files','Croma and reference import failures both zero.');
INSERT INTO "metric" VALUES(6,'phase-10b','leading_subset','matches_mismatches','0/1932','636/1296',NULL,'files',NULL);
INSERT INTO "metric" VALUES(7,'phase-10b','leading_subset','mismatch_rows','1386236','61849','-1327387','rows',NULL);
INSERT INTO "metric" VALUES(8,'phase-10b','full_10k','matches_mismatches','2363/7572','2363/7572','0','files','File counts stable; row count improved.');
INSERT INTO "metric" VALUES(9,'phase-10b','full_10k','mismatch_rows','2759584','2759218','-366','rows',NULL);
INSERT INTO "metric" VALUES(10,'phase-10c','leading_subset','matches_mismatches','636/1296','785/1147',NULL,'files',NULL);
INSERT INTO "metric" VALUES(11,'phase-10c','leading_subset','mismatch_rows','61849','60907','-942','rows',NULL);
INSERT INTO "metric" VALUES(12,'phase-10c','full_10k','matches_mismatches','2363/7572','2897/7038',NULL,'files',NULL);
INSERT INTO "metric" VALUES(13,'phase-10c','full_10k','mismatch_rows','2759218','2755572','-3646','rows',NULL);
INSERT INTO "metric" VALUES(14,'phase-10e','full_10k_report_only','matches_mismatches',NULL,'2848/7087',NULL,'files','New tool schema/report-only run.');
INSERT INTO "metric" VALUES(15,'phase-10e','full_10k_report_only','mismatch_rows',NULL,'3652690',NULL,'rows',NULL);
INSERT INTO "metric" VALUES(16,'phase-10e','full_10k_report_only','elapsed',NULL,'28.912',NULL,'seconds','jobs=16 worker_chunk_size=16');
INSERT INTO "metric" VALUES(17,'phase-10f','target_tie_10_files','matches_mismatches','0/10','8/2',NULL,'files',NULL);
INSERT INTO "metric" VALUES(18,'phase-10f','target_tie_10_files','mismatch_rows','128','40','-88','rows',NULL);
INSERT INTO "metric" VALUES(19,'phase-10f','target_tie_10_files','tie_rows','88','0','-88','rows',NULL);
INSERT INTO "metric" VALUES(20,'phase-10f','full_10k','matches_mismatches','2848/7087','2945/6990',NULL,'files',NULL);
INSERT INTO "metric" VALUES(21,'phase-10f','full_10k','mismatch_rows','3652690','3649963','-2727','rows',NULL);
INSERT INTO "metric" VALUES(22,'phase-10f','full_10k','tie_rows','3398','671','-2727','rows',NULL);
INSERT INTO "metric" VALUES(23,'phase-10g','target_malformed_chord_1553_files','matches_mismatches','0/1553','128/1425',NULL,'files',NULL);
INSERT INTO "metric" VALUES(24,'phase-10g','target_malformed_chord_1553_files','mismatch_rows','64817','59154','-5663','rows',NULL);
INSERT INTO "metric" VALUES(25,'phase-10g','full_10k','matches_mismatches','2945/6990','3044/6891',NULL,'files',NULL);
INSERT INTO "metric" VALUES(26,'phase-10g','full_10k','mismatch_rows','3649963','3644300','-5663','rows',NULL);
INSERT INTO "metric" VALUES(27,'phase-10h','target_hyphen_777_files','matches_mismatches','0/777','42/735',NULL,'files',NULL);
INSERT INTO "metric" VALUES(28,'phase-10h','target_hyphen_777_files','mismatch_rows','346879','280647','-66232','rows',NULL);
INSERT INTO "metric" VALUES(29,'phase-10h','target_hyphen_777_files','lyric_rows','28984','627','-28357','rows',NULL);
INSERT INTO "metric" VALUES(30,'phase-10h','full_10k','matches_mismatches','3044/6891','3086/6849',NULL,'files',NULL);
INSERT INTO "metric" VALUES(31,'phase-10h','full_10k','mismatch_rows','3644300','3578068','-66232','rows',NULL);
INSERT INTO "metric" VALUES(32,'phase-10h','full_10k','lyric_rows','28985','628','-28357','rows',NULL);
INSERT INTO "metric" VALUES(33,'phase-10i','target_107_files','matches_mismatches','0/107','0/107','0','files','Target still structurally mismatches because of larger non-lyric categories; selected direct lyric issue improved.');
INSERT INTO "metric" VALUES(34,'phase-10i','target_107_files','mismatch_rows','266464','265905','-559','rows',NULL);
INSERT INTO "metric" VALUES(35,'phase-10i','target_107_files','lyric_component_rows','10063','9504','-559','rows',NULL);
INSERT INTO "metric" VALUES(36,'phase-10i','target_107_files','direct_lyric_rows',NULL,'89',NULL,'rows','After selected fix, direct lyric rows remain in 7 files.');
INSERT INTO "metric" VALUES(37,'phase-10i','target_107_files','improved_regressed_files',NULL,'17/0',NULL,'files','17 improved, 0 regressed in selected target evidence.');
INSERT INTO "metric" VALUES(38,'phase-10i','full_10k','matches_mismatches','3086/6849','3086/6849','0','files','No full-file match count change; selected rows reduced in target and full comparison.');
INSERT INTO "metric" VALUES(39,'phase-10i','full_10k','mismatch_rows','3578068','3578140','+72','rows','Report-only full run row accounting differs from Phase 10-h baseline; use triage details for selected direct lyric fix.');
INSERT INTO "metric" VALUES(40,'phase-10i','full_10k','direct_lyric_rows','628','89','-539','rows','Direct lyric rows after Phase 10-i: 89 in 7 files.');
INSERT INTO "metric" VALUES(41,'phase-10i','full_10k','exports_success_failure','9935/65','9935/65','0','files',NULL);
INSERT INTO "metric" VALUES(42,'phase-10i','full_10k','music21_import_failures','0/0','0/0','0','files','Croma/reference import failures.');
INSERT INTO "metric" VALUES(43,'phase-10i','full_10k','elapsed',NULL,'124.773',NULL,'seconds',NULL);
CREATE TABLE phase (
  phase_id TEXT PRIMARY KEY,
  branch TEXT,
  status TEXT NOT NULL CHECK (status IN ('planned','in_progress','complete','merged','blocked','unknown')),
  pr_number INTEGER,
  pr_url TEXT,
  commit_hash TEXT,
  selected_target TEXT,
  classification TEXT,
  summary TEXT,
  next_recommended_target TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "phase" VALUES('phase-9','codex/phase-9-cli-corpus-gate','complete',NULL,NULL,NULL,'CLI/corpus smoke gate and 66-failure evidence review','mixed: one Croma bug fixed; rest malformed/reference artifacts','10k corpus smoke gate rerun: 9935 successes, 65 failures, 0 panics/hard errors/timeouts after fixing body V: same-line music tail.','Move to full 10k music21 structural comparison.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10','codex/phase-10-full-music21-compare','merged',11,NULL,NULL,'Full 10k music21 structural comparison','comparison baseline','Croma XML importability bugs addressed; 10000 attempted, 9935 exports, 0 music21 import failures, 1335 matches, 8600 mismatches.','Use mismatch categories to choose focused parser fixes.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10a','codex/phase-10a-mismatch-root-causes','merged',12,NULL,'7d7ab46','Polars/root-cause triage','triage/evidence','Identified major clusters: V: voices collapsed, leading first-body barline, tuplet/broken drift, grace/key policy, chord/tie residual alignment, inline state drift, hidden rests.','Fix leading first-measure barline policy first.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10b','codex/phase-10b-leading-barline-policy','merged',13,NULL,'41f5c39','Leading first-body barline policy','Croma bug fixed with known policy edge cases','Leading |, [|, |:, ||, |], liberal first-body barlines no longer create empty measure 1; targeted subset improved sharply.','Define liberal combined barline spellings.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10c','codex/phase-10c-liberal-barlines','merged',14,NULL,'accf7dc','Liberal combined barlines','Croma policy fix','Defined ||:, [|:, :||, :|], :||:, :|:, ::, |::, ::| behavior; full 10k improved to 2897 matches / 7038 mismatches.','Small repeat-policy classification follow-up.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10d','codex/phase-10d-repeat-policy-classification','merged',15,NULL,'d26b318','Repeat policy tests/classification','policy/tests','Added Phase 10-d repeat policy tests after Phase 10-c; details are in PR #15/local reports if needed.','Improve comparison tooling throughput and queryability.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10e','codex/phase-10e-polars-parallel-comparison-tools','merged',16,NULL,'f22a9d1','Parallel music21 + Polars comparison tooling','tooling enhancement','Added jobs/chunk/progress/component/file filters, normalized fact/comparison/mismatch tables, summaries, baseline delta support. Full 10k report-only run took 28.912s with jobs=16.','Use Polars evidence to select focused parser fixes.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10f','codex/phase-10f-polars-guided-parser-fix','merged',17,NULL,'2fbfae2','Cross-bar tie propagation','Croma bug fixed','Legal same-pitch ties across barlines no longer finalize before resolution. Target tie rows 88 -> 0; full tie rows 3398 -> 671.','Text/direction leakage or residual tie mixed cases.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10g','codex/phase-10g-text-direction-leakage','merged',18,'https://github.com/ro-ag/croma/pull/18','640c7a6','Malformed unprefixed quoted chord strings','Croma MusicXML export bug fixed','Strings like "(A7)" and "C/" export as words instead of fake harmony; valid chord symbols still export as harmony. Full mismatch rows -5663.','Body w: lyric alignment/control mismatches.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10h','codex/phase-10h-lyric-alignment-controls','merged',19,NULL,'ce6c160','Lyric hyphen control MusicXML export','Croma MusicXML export bug fixed','ABC lyric - controls no longer export as standalone sung text; escaped literal hyphens remain text. Full mismatch rows -66232; lyric rows 28985 -> 628.','Residual _ melisma/hold lyric alignment, especially tune_000509.abc.','2026-06-06 01:33:56');
INSERT INTO "phase" VALUES('phase-10i','codex/phase-10i-lyric-melisma-hold','merged',20,'https://github.com/ro-ag/croma/pull/20','51440fb','NBSP lyric tokenization plus empty-extender comparison for _ melisma holds','Croma bug plus comparison harness bug fixed','Fixed tune_000509-style lyric _ melisma/hold evidence: Croma no longer treats U+00A0 inside body w: lyrics as a separator, and comparison tooling preserves empty MusicXML lyric extender slots. Target direct lyric rows reduced; no targeted regressions.','Remaining direct lyric rows: 89 rows in 7 files, likely lyric cursor/reference-policy behavior; triage separately.','2026-06-06 01:46:30');
CREATE TABLE validation (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  command TEXT NOT NULL,
  status TEXT NOT NULL,
  notes TEXT,
  UNIQUE (phase_id, command)
);
INSERT INTO "validation" VALUES(1,'phase-10e','uv run python -m py_compile tools/music21_polars_corpus_compare.py','passed',NULL);
INSERT INTO "validation" VALUES(2,'phase-10e','uv run python -m pytest tests -q','passed','5 passed in reported run.');
INSERT INTO "validation" VALUES(3,'phase-10e','git diff --check','passed',NULL);
INSERT INTO "validation" VALUES(4,'phase-10f','cargo test --workspace','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(5,'phase-10f','cargo clippy --workspace --all-targets -- -D warnings','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(6,'phase-10f','cargo doc -p croma-core --no-deps','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(7,'phase-10f','cargo package -p croma-core --allow-dirty','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(8,'phase-10f','git diff --check','passed',NULL);
INSERT INTO "validation" VALUES(9,'phase-10g','cargo test --workspace','passed','Rerun before PR #18.');
INSERT INTO "validation" VALUES(10,'phase-10g','cargo clippy --workspace --all-targets -- -D warnings','passed','Rerun before PR #18.');
INSERT INTO "validation" VALUES(11,'phase-10g','cargo doc -p croma-core --no-deps','passed','Rerun before PR #18.');
INSERT INTO "validation" VALUES(12,'phase-10g','cargo package -p croma-core --allow-dirty','passed','Rerun before PR #18.');
INSERT INTO "validation" VALUES(13,'phase-10g','git diff --check','passed','Rerun before PR #18.');
INSERT INTO "validation" VALUES(14,'phase-10h','uv run python -m py_compile tools/music21_polars_corpus_compare.py','passed',NULL);
INSERT INTO "validation" VALUES(15,'phase-10h','uv run python -m pytest tests -q','passed',NULL);
INSERT INTO "validation" VALUES(16,'phase-10h','cargo test --workspace','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(17,'phase-10h','cargo clippy --workspace --all-targets -- -D warnings','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(18,'phase-10h','cargo doc -p croma-core --no-deps','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(19,'phase-10h','cargo package -p croma-core --allow-dirty','passed','Pinned Rust toolchain.');
INSERT INTO "validation" VALUES(20,'phase-10h','git diff --check','passed',NULL);
INSERT INTO "validation" VALUES(21,'phase-10i','uv run python -m py_compile tools/music21_polars_corpus_compare.py tools/music21_compare.py','passed','Reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(22,'phase-10i','uv run python -m pytest tests -q','passed','Reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(23,'phase-10i','cargo test --workspace','passed','Pinned Rust toolchain, reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(24,'phase-10i','cargo clippy --workspace --all-targets -- -D warnings','passed','Pinned Rust toolchain, reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(25,'phase-10i','cargo doc -p croma-core --no-deps','passed','Pinned Rust toolchain, reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(26,'phase-10i','cargo package -p croma-core --allow-dirty','passed','Pinned Rust toolchain, reported by Phase 10-i agent.');
INSERT INTO "validation" VALUES(27,'phase-10i','git diff --check','passed','Reported by Phase 10-i agent.');
CREATE VIEW phase_summary AS
SELECT
  phase_id,
  status,
  branch,
  pr_number,
  commit_hash,
  selected_target,
  classification,
  summary,
  next_recommended_target
FROM phase
ORDER BY phase_id;
DELETE FROM "sqlite_sequence";
INSERT INTO "sqlite_sequence" VALUES('metric',43);
INSERT INTO "sqlite_sequence" VALUES('artifact',17);
INSERT INTO "sqlite_sequence" VALUES('validation',27);
COMMIT;
