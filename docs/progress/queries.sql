-- Common read-only progress tracker queries.
-- Usage:
--   sqlite3 -header -column docs/untracked/croma-progress.sqlite < docs/progress/queries.sql

.headers on
.mode column

SELECT
  phase_id,
  status,
  branch,
  pr_number,
  selected_target,
  next_recommended_target
FROM phase_summary
ORDER BY phase_id;

SELECT
  phase_id,
  scope,
  name,
  before_value,
  after_value,
  delta,
  unit
FROM metric
WHERE scope = 'full_10k'
ORDER BY phase_id, name;

SELECT
  key,
  value,
  notes
FROM memory
ORDER BY key;
