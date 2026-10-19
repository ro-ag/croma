//! Engraving rules for the `--engrave` MusicXML profile: beam grouping, tuplet
//! brackets, stem direction, multi-voice rest placement, and slur/tie
//! placement. A clean-room implementation of the rules in
//! `docs/engraving/rules.md`, which are drawn from published engraving
//! references; the module boundary is fixed by `docs/engraving/contract.md`.

pub(crate) mod beam;
pub(crate) mod stem;
