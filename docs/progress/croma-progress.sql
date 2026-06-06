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
INSERT INTO "artifact" VALUES(18,'testbed-reproducibility','recipe','docs/testing/corpus-reproducibility.md','Committed recipe for recreating corpus/testbed artifacts.');
INSERT INTO "artifact" VALUES(19,'testbed-reproducibility','inventory','docs/reference/corpus-inventory.md','Corpus inventory updated to current local input/reference roots.');
INSERT INTO "artifact" VALUES(20,'testbed-reproducibility','progress_snapshot','docs/progress/croma-progress.sql','Progress tracker SQL snapshot including this documentation phase.');
INSERT INTO "artifact" VALUES(21,'bootstrap-hardening','script','tools/session_bootstrap.sh','Session bootstrap script hardened to fail fast and tolerate uv-missing tracker restore via python3.');
INSERT INTO "artifact" VALUES(22,'bootstrap-hardening','progress_snapshot','docs/progress/croma-progress.sql','Progress tracker SQL snapshot recording bootstrap hardening.');
INSERT INTO "artifact" VALUES(23,'bootstrap-hardening','script','tools/provision_corpus.py','Croma-owned corpus provisioner for Zenodo import, verified LFS archive import/build, and abc2xml reference generation.');
INSERT INTO "artifact" VALUES(24,'bootstrap-hardening','lfs_archive','docs/corpus/zenodo-10k-abc.tar.gz','Optional Git LFS ABC corpus cache archive, built from the Zenodo 10k dataset output.');
INSERT INTO "artifact" VALUES(25,'bootstrap-hardening','checksum','docs/corpus/zenodo-10k-abc.tar.gz.sha256','SHA-256 checksum used by provision_corpus.py import-archive.');
INSERT INTO "artifact" VALUES(26,'bootstrap-hardening','docs','docs/testing/corpus-reproducibility.md','Documents Zenodo provenance, LFS cache, bootstrap provisioning, reference XML generation, and archive rebuild commands.');
INSERT INTO "artifact" VALUES(27,'phase-10k','triage_md','docs/untracked/phase-10k/phase-10k-lyric-bar-marker-triage.md','Bar-marker + double-hyphen lyric triage and 100%-or-justify report');
INSERT INTO "artifact" VALUES(28,'phase-10k','after_report','docs/untracked/session-after/full-10k-report-only-compare-report.json','Full 10k report-only compare after the fix');
INSERT INTO "artifact" VALUES(29,'phase-10k','baseline_report','docs/untracked/session-baseline/full-10k-report-only-compare-report.json','Full 10k report-only compare before the fix');
CREATE TABLE memory (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  notes TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "memory" VALUES('pinned_toolchain','Rust 1.96.0 pinned by rust-toolchain.toml at repo root.','Use plain cargo/rustc; rust-toolchain.toml auto-selects 1.96.0 on any host. Linux cloud sandbox provisions via rustup (rustup show); local dev via Nix flake. No absolute toolchain path.','2026-06-06 02:23:59');
INSERT INTO "memory" VALUES('python_tooling','Use uv for Python/music21/Polars/corpus tooling.','Do not hand-roll venv setup.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('abc_reference_repo','/Users/rodox/dev/abc','Use as parser-policy/test/prover reference only; do not copy whole parser.','2026-06-06 06:01:44');
INSERT INTO "memory" VALUES('generated_artifacts_policy','Keep generated corpus XML/parquet/jsonl/reports under docs/untracked/.','Do not commit generated corpus artifacts unless explicitly requested.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('formatter_lsp_gate','Formatter and LSP wait until parser quality is proven.','Parser/corpus/music21 work remains priority.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('croma_core_crates_io','Keep croma-core crates.io-compatible.','Avoid path-only/local runtime assumptions in library code.','2026-06-06 01:33:56');
INSERT INTO "memory" VALUES('dev_environments','Two supported: Linux cloud sandbox (rustup + uv) and local Nix flake.','Build/validate from repo root: uv sync; cargo build -p croma-cli; cargo test --workspace. See docs/development-environment.md. Corpus roots are external; set ABC_ROOT/REF_ROOT per docs/testing/corpus-reproducibility.md.','2026-06-06 02:23:59');
INSERT INTO "memory" VALUES('phase_10k_outcome','Bar marker in w:/s: advances to next bar at block boundary and for consecutive markers; hyphen after space/hyphen holds a blank note (ABC 2.1 section 5.1).','Direct lyric rows 89->75; remaining 75 in tune_006403/001361/002325/006565 are downstream of measure-structure/overlay diffs, not lyric bugs.','2026-06-06 06:01:54');
INSERT INTO "memory" VALUES('next_target','Measure-model triage: leading bare-harmony empty measure (tune_006403) and inline meter-change empty measures (tune_002325).','High-value measure_alignment/barline targets surfaced by lyric triage.','2026-06-06 06:01:55');
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
INSERT INTO "metric" VALUES(44,'phase-10k','full_10k','direct_lyric_rows','89','75',NULL,'rows','mismatch_category=lyric; remaining 75 in 4 files are downstream of measure-structure/overlay diffs');
INSERT INTO "metric" VALUES(45,'phase-10k','full_10k','lyric_target_files','7','4',NULL,'files','tune_009204/005254/003682 reach 100% lyric match');
INSERT INTO "metric" VALUES(46,'phase-10k','full_10k','mismatch_rows','3578140','3577076',NULL,'rows','net -1064; no category increased');
INSERT INTO "metric" VALUES(47,'phase-10k','full_10k','structural_matches','3086','3093',NULL,'files','+7 fully-matching files');
INSERT INTO "metric" VALUES(48,'phase-10k','full_10k','missing_in_croma','1794379','1794191',NULL,'rows','-188 lyric/symbol extender realignment');
INSERT INTO "metric" VALUES(49,'phase-10k','full_10k','extra_in_croma','1643332','1642470',NULL,'rows','-862 lyric/symbol extender realignment');
INSERT INTO "metric" VALUES(50,'phase-10k','full_10k','exports_success_failure','9935/65','9935/65',NULL,'files','unchanged; no new panics/failures');
INSERT INTO "metric" VALUES(51,'phase-10k','full_10k','import_failures','0/0','0/0',NULL,'files','croma/reference musicxml import failures');
INSERT INTO "metric" VALUES(52,'phase-10l','full_10k','mismatch_rows','3577076','3536841',NULL,'rows','-40235 across all categories; no regressions');
INSERT INTO "metric" VALUES(53,'phase-10l','full_10k','affected_files','62','62',NULL,'files','files with |[X: inline-field-after-barline pattern');
INSERT INTO "metric" VALUES(54,'phase-10l','full_10k','pitch','20575','17524',NULL,'rows','-3051');
INSERT INTO "metric" VALUES(55,'phase-10l','full_10k','duration','35199','32448',NULL,'rows','-2751');
INSERT INTO "metric" VALUES(56,'phase-10l','full_10k','measure_alignment','36128','34278',NULL,'rows','-1850');
INSERT INTO "metric" VALUES(57,'phase-10l','full_10k','structural_matches','3093','3094',NULL,'files','+1');
INSERT INTO "metric" VALUES(58,'phase-10m','full_10k','mismatch_rows','3536841','3524300',NULL,'rows','-12541; no regressions');
INSERT INTO "metric" VALUES(59,'phase-10m','full_10k','structural_matches','3094','3124',NULL,'files','+30 fully-matching');
INSERT INTO "metric" VALUES(60,'phase-10m','full_10k','duration','32448','24610',NULL,'rows','-7838 inline L/M');
INSERT INTO "metric" VALUES(61,'phase-10m','full_10k','accidental','9267','6768',NULL,'rows','-2499 inline K');
INSERT INTO "metric" VALUES(62,'phase-10m','full_10k','measure_alignment','34278','32267',NULL,'rows','-2011');
INSERT INTO "metric" VALUES(63,'phase-10n','full_10k','mismatch_rows','3524300','393045',NULL,'rows','-3,131,255 (-89%)');
INSERT INTO "metric" VALUES(64,'phase-10n','full_10k','missing_in_croma','1781788','113548',NULL,'rows','-1668240');
INSERT INTO "metric" VALUES(65,'phase-10n','full_10k','extra_in_croma','1625986','147500',NULL,'rows','-1478486');
INSERT INTO "metric" VALUES(66,'phase-10n','full_10k','structural_matches','3124','3153',NULL,'files','+29');
INSERT INTO "metric" VALUES(67,'phase-10n','full_10k','octave_exposed','8030','29970',NULL,'rows','+21940 pre-existing clef-octave now visible');
INSERT INTO "metric" VALUES(68,'phase-10o','full_10k','mismatch_rows','393045','375387',NULL,'rows','-17658');
INSERT INTO "metric" VALUES(69,'phase-10o','full_10k','octave','29970','12347',NULL,'rows','-17623 clef octave + property merge');
INSERT INTO "metric" VALUES(70,'phase-10o','full_10k','structural_matches','3153','3153',NULL,'files','unchanged');
INSERT INTO "metric" VALUES(71,'phase-10p','full_10k','mismatch_rows','375387','353827',NULL,'rows','-21560');
INSERT INTO "metric" VALUES(72,'phase-10p','full_10k','structural_matches','3153','3326',NULL,'files','+173');
INSERT INTO "metric" VALUES(73,'phase-10p','full_10k','extra_in_croma','147500','126517',NULL,'rows','-20983 W: verses to credits');
INSERT INTO "metric" VALUES(74,'phase-10p','full_10k','direction','5779','5133',NULL,'rows','-646');
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
INSERT INTO "phase" VALUES('testbed-reproducibility','codex/testbed-reproducibility-recipe','complete',NULL,NULL,NULL,'Corpus/testbed recreation recipe','documentation/infrastructure','Added a committed recipe for recreating full 10k exports, report-only Music21/Polars comparisons, optional large table artifacts, targeted corpus selection, targeted comparisons, validation queries, and progress tracker restore/query/export workflow. Updated corpus inventory to the local trd_obsolete input/reference roots.','Use the recipe before the next parser phase; then triage the remaining 89 direct lyric rows in 7 files.','2026-06-06 02:01:00');
INSERT INTO "phase" VALUES('linux-sandbox-env','codex/phase-10j-linux-sandbox-env','complete',NULL,NULL,NULL,'Port dev-environment and corpus reproducibility docs from macOS-only paths to a portable Linux cloud sandbox + Nix flake setup; provision Rust 1.96.0 (rustup) and uv Python env','tooling/docs','Rewrote docs/development-environment.md for Linux cloud sandbox (rustup) and local Nix flake; made docs/testing/corpus-reproducibility.md environment-agnostic (ABC_ROOT/REF_ROOT vars, rust-toolchain.toml instead of absolute toolchain path); added provenance note to corpus-inventory.md; updated pinned_toolchain memory and added dev_environments memory. Built env: cargo build -p croma-cli OK, uv sync OK; cargo test --workspace green (cli 18, croma-core 145, croma-fmt 1).','Provision the external 10k corpus (ABC_ROOT/REF_ROOT) into the sandbox, then triage the residual 89 direct lyric rows in 7 files per phase-10i.','2026-06-06 02:24:17');
INSERT INTO "phase" VALUES('bootstrap-hardening','codex/bootstrap-hardening','complete',NULL,NULL,NULL,'Session bootstrap fail-fast behavior and uv fallback','tooling fix','Hardened tools/session_bootstrap.sh with fail-fast behavior, uv/python3 progress restore fallback, optional verified Git LFS ABC corpus cache import, Zenodo fallback provisioning, and explicit partial corpus status.','Continue Phase 10 parser work after provisioning ABC corpus and reference MusicXML; residual lyric policy triage remains next.','2026-06-06 04:35:22');
INSERT INTO "phase" VALUES('phase-10k','work/phase-10k-lyric-bar-marker','merged',25,'https://github.com/ro-ag/croma/pull/25',NULL,'Residual 89 direct lyric rows in 7 files (lyric cursor/bar-marker policy)','Croma bug fixed (two spec violations); residual rows justified as measure-model/overlay downstream artifacts','Fixed two ABC 2.1 section 5.1 violations: (1) a w:/s: bar marker now advances to the next bar at a block boundary and for consecutive markers instead of being ignored against the previous block note; (2) a hyphen preceded by a space or another hyphen now holds a blank note instead of exporting a literal dash. Direct lyric rows 89->75 across the full 10k; 3 files reach 100% lyric match and +7 files now fully match structurally with no category regressions. Remaining 75 rows in 4 files are downstream of measure-structure (empty leading harmony measure, cut-time grouping, inline meter-change empty measures) or voice-overlay divergences, which the positional comparison cannot align; lyric-to-note alignment is itself correct.','Triage measure-model differences: leading bare-harmony empty measure (tune_006403) and inline meter-change spurious empty measures (tune_002325); both are high-value measure_alignment/barline targets.','2026-06-06 06:34:21');
INSERT INTO "phase" VALUES('phase-10l','work/phase-10l-inline-field-barline','merged',NULL,NULL,NULL,'Inline information field after a barline mis-parsed as a liberal combined barline','Croma parser bug fixed','Fixed a parser bug where a barline immediately followed by an inline field (e.g. |[M:3/8], |[K:D]) swallowed the [ into a liberal |[ barline, dropping the field and inserting a spurious empty measure plus cascading garbage. The barline scan now stops before a [ that begins an inline field. Full 10k report-only mismatch rows drop 3,577,076 -> 3,536,841 (-40,235) with every category improved and none regressed; affects 62 corpus files. tune_002325 fully resolves (11 measures matching reference, 100 percent lyric match).','Apply inline [K:]/[M:]/[L:] changes (parsed but not yet applied): 377 files use inline key changes, 135 use inline meter; route them to the existing key/meter/unit change handlers and emit mid-tune key signatures.','2026-06-06 06:34:21');
INSERT INTO "phase" VALUES('phase-10m','work/phase-10m-inline-field-apply','merged',NULL,NULL,NULL,'Inline [K:]/[M:]/[L:] information fields parsed but never applied mid-tune','Croma parser bug fixed','Routed inline [M:]/[K:]/[L:] fields to the existing meter/key/unit change handlers so a mid-line change affects subsequent notes (accidentals, durations, meter). Hardened key tonic parsing so a clef shorthand or property token (bass, clef=bass) that happens to start with a note letter is no longer misread as a key change, and a clef-only inline [K:] leaves the signature untouched. Full 10k mismatch rows drop 3,536,841 -> 3,524,300 (-12,541): duration -7,838, accidental -2,499, measure_alignment -2,011, plus +30 fully-matching files and no category regressions.','Emit mid-tune <key>/<time> attributes in the MusicXML writer (currently applied to notes but not declared); then triage the remaining large missing_in_croma/extra_in_croma categories.','2026-06-06 07:01:17');
INSERT INTO "phase" VALUES('phase-10n','work/phase-10n-multipart-export','merged',NULL,NULL,NULL,'Single combined MusicXML part for multi-voice tunes vs one part per voice in abc2xml','Croma export model gap fixed (multi-voice multipart)','build_score_model now emits one MusicXML part per ABC voice (single score-partwise document, all parts), matching abc2xml/music21 and Finale-style export. Single-voice tunes are unchanged. Full 10k mismatch rows drop 3,524,300 -> 393,045 (-3,131,255, -89 percent): missing_in_croma -1,668,240, extra_in_croma -1,478,486, measure_alignment -10,957, slur -1,574, metadata -634, +29 fully-matching files. Correct part alignment EXPOSES pre-existing per-note mismatches previously hidden in the missing/extra bucket: octave +21,940 (clef octave transposition like clef=treble-8 not applied), duration +3,054, pitch +1,469, accidental +1,307. These are now visible and become the next targets.','Apply clef octave transposition (clef=treble-8/+8, octave=N) to voice note octaves to match abc2xml (octave +21,940 exposed by multipart).','2026-06-06 16:46:07');
INSERT INTO "phase" VALUES('phase-10o','work/phase-10o-clef-octave','merged',NULL,NULL,NULL,'Clef octave transposition (clef=treble-8 etc.) not applied; bare V: switch clobbered header voice properties','Croma export bug fixed','Apply the clef octave suffix (clef=treble-8/+8, +/-15) and octave= property as a per-voice octave shift on note octaves, and emit a matching clef-octave-change. Also fixed a pre-existing bug where a bare body V: switch overwrote a voices header-defined properties (clef/name/etc.) with empty values; properties now merge so each voice keeps its clef. Full 10k octave mismatches drop 29,970 -> 12,347 (-17,623), total rows 393,045 -> 375,387 (-17,658), no category regressions.','Triage remaining duration (27,664), measure_alignment (21,310), pitch (18,993), and the residual missing/extra (261k) categories.','2026-06-06 17:25:23');
INSERT INTO "phase" VALUES('phase-10p','work/phase-10p-direction-cleanup','in_progress',NULL,NULL,NULL,'W: post-tune words emitted as in-measure directions and duplicated per part','Croma export corrected to ABC + MusicXML specs','Per ABC 2.1 (W: = words printed after the tune) and MusicXML (page-level text = score-header <credit>), W: post-tune verses now export as <credit><credit-words> instead of in-measure <words> directions, and empty W: lines are skipped. Score-level directions (tempo, %% directives) are emitted once in the first part instead of once per voice-part. Full 10k structural matches +173 (3,153 -> 3,326), mismatch rows 375,387 -> 353,827 (-21,560): extra_in_croma -20,983, direction -646, with +69 missing_in_croma edge. abc2xml is only a baseline; this follows the ABC and MusicXML specs.','Map ABC decorations (!fermata!, articulations, ornaments) to MusicXML notation elements instead of <words> directions; then duration/pitch residuals.','2026-06-06 22:40:16');
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
INSERT INTO "validation" VALUES(28,'testbed-reproducibility','find "$ABC_ROOT" -type f -name ''*.abc'' | wc -l','passed','10000 files under /Users/rodox/dev/rs/trd_obsolete/test/real/abc.');
INSERT INTO "validation" VALUES(29,'testbed-reproducibility','find "$REF_ROOT" -type f \( -name ''*.musicxml'' -o -name ''*.xml'' \) | wc -l','passed','10000 reference XML/MusicXML files under /Users/rodox/dev/rs/trd_obsolete/test/real/musicxml.');
INSERT INTO "validation" VALUES(30,'testbed-reproducibility','uv run python tools/progress/progress.py status','passed','Tracker query works and shows Phase 10-i merged.');
INSERT INTO "validation" VALUES(31,'testbed-reproducibility','git diff --check','passed',NULL);
INSERT INTO "validation" VALUES(32,'linux-sandbox-env','cargo build -p croma-cli','pass','Built target/debug/croma on x86_64-unknown-linux-gnu with Rust 1.96.0.');
INSERT INTO "validation" VALUES(33,'linux-sandbox-env','cargo test --workspace','pass','cli 18, croma-core 145, croma-fmt 1 passed; 0 failed.');
INSERT INTO "validation" VALUES(34,'linux-sandbox-env','uv sync','pass','Resolved 30 / installed 28 packages; music21 10.3.0, polars 1.41.2 importable.');
INSERT INTO "validation" VALUES(35,'bootstrap-hardening','bash -n tools/session_bootstrap.sh','passed',NULL);
INSERT INTO "validation" VALUES(36,'bootstrap-hardening','tools/session_bootstrap.sh','passed','Normal local run with uv/cargo present.');
INSERT INTO "validation" VALUES(37,'bootstrap-hardening','PATH=/usr/bin:/bin tools/session_bootstrap.sh','passed','Simulated uv/cargo missing; progress tracker restored through python3 fallback.');
INSERT INTO "validation" VALUES(38,'bootstrap-hardening','git diff --check','passed',NULL);
INSERT INTO "validation" VALUES(39,'bootstrap-hardening','brew install git-lfs && git lfs install','passed','Installed git-lfs 3.7.1 and initialized Git LFS hooks.');
INSERT INTO "validation" VALUES(40,'bootstrap-hardening','uv run python tools/provision_corpus.py build-archive --output docs/untracked/corpus/zenodo-10k --archive docs/corpus/zenodo-10k-abc.tar.gz','passed','Built 2.5M ABC corpus archive with sha256 dfaff46a0af4bfc93b70a55dccff17d16dd1fbc8aeb5eff814bb9c2e0284cc92.');
INSERT INTO "validation" VALUES(41,'bootstrap-hardening','uv run python tools/provision_corpus.py import-archive --archive docs/corpus/zenodo-10k-abc.tar.gz','passed','Verified checksum and restored 10000 ABC files from the archive.');
INSERT INTO "validation" VALUES(42,'bootstrap-hardening','tools/session_bootstrap.sh --fetch-corpus','passed','Preferred verified LFS archive and reported ABC corpus available with 10000 files while reference XML remained missing.');
INSERT INTO "validation" VALUES(43,'bootstrap-hardening','uv run python -m py_compile tools/provision_corpus.py tools/music21_polars_corpus_compare.py','passed',NULL);
INSERT INTO "validation" VALUES(44,'bootstrap-hardening','uv run python -m pytest tests -q','passed','6 passed.');
INSERT INTO "validation" VALUES(45,'bootstrap-hardening','git diff --cached --check','passed',NULL);
INSERT INTO "validation" VALUES(46,'bootstrap-hardening','git lfs ls-files','passed','dfaff46a0a * docs/corpus/zenodo-10k-abc.tar.gz');
INSERT INTO "validation" VALUES(47,'phase-10k','cargo test --workspace','pass','148 core tests incl. 3 new no-happy-path lyric tests');
INSERT INTO "validation" VALUES(48,'phase-10k','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(49,'phase-10k','uv run python -m pytest tests -q','pass','6 passed');
INSERT INTO "validation" VALUES(50,'phase-10k','git diff --check','pass','no whitespace errors');
INSERT INTO "validation" VALUES(51,'phase-10k','full 10k report-only compare','pass','lyric 89->75, +7 matches, -1064 rows, no regressions');
INSERT INTO "validation" VALUES(52,'phase-10l','cargo test --workspace','pass','149 core tests incl. new inline-field-after-barline test');
INSERT INTO "validation" VALUES(53,'phase-10l','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(54,'phase-10l','full 10k report-only compare','pass','-40235 mismatch rows, no category regressions');
INSERT INTO "validation" VALUES(55,'phase-10m','cargo test --workspace','pass','151 core tests incl. inline key apply + clef-only preserve');
INSERT INTO "validation" VALUES(56,'phase-10m','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(57,'phase-10m','cargo fmt --all -- --check','pass','clean');
INSERT INTO "validation" VALUES(58,'phase-10m','full 10k report-only compare','pass','-12541 rows, +30 matches, no regressions');
INSERT INTO "validation" VALUES(59,'phase-10n','cargo test --workspace','pass','153 core tests incl. one-part-per-voice + single-voice');
INSERT INTO "validation" VALUES(60,'phase-10n','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(61,'phase-10n','cargo fmt --all -- --check','pass','clean');
INSERT INTO "validation" VALUES(62,'phase-10n','full 10k report-only compare','pass','-3,131,255 rows (-89%); exposes pre-existing per-note issues');
INSERT INTO "validation" VALUES(63,'phase-10o','cargo test --workspace','pass','155 core tests incl. clef-octave + bare-voice-switch');
INSERT INTO "validation" VALUES(64,'phase-10o','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(65,'phase-10o','cargo fmt --all -- --check','pass','clean');
INSERT INTO "validation" VALUES(66,'phase-10o','full 10k report-only compare','pass','-17658 rows, octave -17623, no regressions');
INSERT INTO "validation" VALUES(67,'phase-10p','cargo test --workspace','pass','156 core tests incl. W: credit');
INSERT INTO "validation" VALUES(68,'phase-10p','cargo clippy --workspace --all-targets -- -D warnings','pass','clean');
INSERT INTO "validation" VALUES(69,'phase-10p','cargo fmt --all -- --check','pass','clean');
INSERT INTO "validation" VALUES(70,'phase-10p','full 10k report-only compare','pass','+173 matches, -21560 rows');
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
INSERT INTO "sqlite_sequence" VALUES('metric',74);
INSERT INTO "sqlite_sequence" VALUES('artifact',29);
INSERT INTO "sqlite_sequence" VALUES('validation',70);
COMMIT;
