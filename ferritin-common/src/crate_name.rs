use std::{
    borrow::{Borrow, Cow},
    cmp::Ordering,
    fmt::Display,
    hash::{Hash, Hasher},
    ops::Deref,
};

#[derive(Clone, Eq, Ord)]
pub struct CrateName<'a>(Cow<'a, str>);

impl CrateName<'_> {
    pub fn to_static(&self) -> CrateName<'static> {
        self.0.to_string().into()
    }
}

impl<'a> std::fmt::Debug for CrateName<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.0)
    }
}

impl Borrow<str> for CrateName<'_> {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

impl Hash for CrateName<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for byte in self.0.bytes() {
            if byte == b'-' {
                state.write_u8(b'_');
            } else {
                state.write_u8(byte);
            }
        }
    }
}

impl<'a> From<Cow<'a, str>> for CrateName<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        Self(value)
    }
}

impl<'a> From<&'a str> for CrateName<'a> {
    fn from(value: &'a str) -> Self {
        Self(Cow::Borrowed(value))
    }
}

impl From<String> for CrateName<'_> {
    fn from(value: String) -> Self {
        Self(Cow::Owned(value))
    }
}

impl Display for CrateName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

impl Deref for CrateName<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq for CrateName<'_> {
    fn eq(&self, other: &Self) -> bool {
        eq_ignoring_dash_underscore(&self.0, &other.0)
    }
}

impl PartialOrd for CrateName<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if eq_ignoring_dash_underscore(&self.0, &other.0) {
            return Some(Ordering::Equal);
        }
        self.0.partial_cmp(&other.0)
    }
}

/// Helper function to compare strings ignoring dash/underscore differences
fn eq_ignoring_dash_underscore(a: &str, b: &str) -> bool {
    let mut a = a.chars();
    let mut b = b.chars();
    loop {
        match (a.next(), b.next()) {
            (Some('_'), Some('-')) | (Some('-'), Some('_')) => {}
            (Some(a_char), Some(b_char)) if a_char == b_char => {}
            (None, None) => break true,
            _ => break false,
        }
    }
}
