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
mod tests {
    use super::*;

    #[test]
    fn diagnostics_carry_spec_and_recovery_metadata() {
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "abc.test.recovered",
            "test recovery",
            Span::new(2, 5),
        )
        .with_spec_reference(
            SpecReference::new("ABC 2.1 section 2")
                .with_url("https://abcnotation.com/wiki/abc:standard:v2.1"),
        )
        .with_recovery_note(RecoveryNote::new("continued after the malformed token"));

        assert_eq!(diagnostic.code, "abc.test.recovered");
        assert_eq!(
            diagnostic
                .spec_reference
                .as_ref()
                .map(|spec| spec.label.as_str()),
            Some("ABC 2.1 section 2")
        );
        assert_eq!(
            diagnostic
                .recovery_note
                .as_ref()
                .map(|note| note.message.as_str()),
            Some("continued after the malformed token")
        );
    }

    #[test]
    fn span_helpers_are_byte_based() {
        let span = Span::new(2, 6);

        assert_eq!(span.len(), 4);
        assert!(!span.is_empty());
        assert!(span.contains(2));
        assert!(span.contains(5));
        assert!(!span.contains(6));
        assert!(span.contains_span(Span::new(3, 5)));
    }
}
