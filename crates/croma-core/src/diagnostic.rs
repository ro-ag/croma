#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub spec_reference: Option<SpecReference>,
    pub recovery_note: Option<RecoveryNote>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: &'static str,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            span,
            spec_reference: None,
            recovery_note: None,
        }
    }

    pub fn with_spec_reference(mut self, spec_reference: SpecReference) -> Self {
        self.spec_reference = Some(spec_reference);
        self
    }

    pub fn with_recovery_note(mut self, recovery_note: RecoveryNote) -> Self {
        self.recovery_note = Some(recovery_note);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecReference {
    pub label: String,
    pub url: Option<String>,
}

impl SpecReference {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryNote {
    pub message: String,
}

impl RecoveryNote {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    pub fn contains_span(self, span: Span) -> bool {
        self.start <= span.start && span.end <= self.end
    }
}

#[cfg(test)]
#[path = "diagnostic_tests.rs"]
mod tests;
