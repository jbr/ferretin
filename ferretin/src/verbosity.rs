use clap::ValueEnum;

/// Controls the verbosity level of documentation display
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Verbosity {
    Minimal,
    Brief,
    Full,
}

impl Verbosity {
    pub(crate) fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}

impl Default for Verbosity {
    fn default() -> Self {
        Self::Full // For humans, default to full documentation
    }
}
