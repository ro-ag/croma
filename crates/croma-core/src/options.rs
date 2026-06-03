#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbcSpecVersion {
    #[default]
    V21,
    V22Draft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseMode {
    #[default]
    Strict,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub spec: AbcSpecVersion,
    pub parse_mode: ParseMode,
}
