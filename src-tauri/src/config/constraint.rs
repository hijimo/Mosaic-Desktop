use std::fmt;
use std::sync::Arc;

use super::config_requirements::RequirementSource;

#[derive(Debug, PartialEq, Eq)]
pub enum ConstraintError {
    InvalidValue {
        field_name: &'static str,
        candidate: String,
        allowed: String,
        requirement_source: RequirementSource,
    },
    EmptyField {
        field_name: String,
    },
    ExecPolicyParse {
        requirement_source: RequirementSource,
        reason: String,
    },
}

impl fmt::Display for ConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue {
                field_name,
                candidate,
                allowed,
                requirement_source,
            } => write!(
                f,
                "invalid value for `{field_name}`: `{candidate}` is not in the allowed set {allowed} (set by {requirement_source})"
            ),
            Self::EmptyField { field_name } => {
                write!(f, "field `{field_name}` cannot be empty")
            }
            Self::ExecPolicyParse {
                requirement_source,
                reason,
            } => write!(
                f,
                "invalid rules in requirements (set by {requirement_source}): {reason}"
            ),
        }
    }
}

impl std::error::Error for ConstraintError {}

impl ConstraintError {
    pub fn empty_field(field_name: impl Into<String>) -> Self {
        Self::EmptyField {
            field_name: field_name.into(),
        }
    }
}

pub type ConstraintResult<T> = Result<T, ConstraintError>;

impl From<ConstraintError> for std::io::Error {
    fn from(err: ConstraintError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, err.to_string())
    }
}

type ConstraintValidator<T> = dyn Fn(&T) -> ConstraintResult<()> + Send + Sync;
type ConstraintNormalizer<T> = dyn Fn(T) -> T + Send + Sync;

#[derive(Clone)]
pub struct Constrained<T> {
    value: T,
    validator: Arc<ConstraintValidator<T>>,
    normalizer: Option<Arc<ConstraintNormalizer<T>>>,
}

impl<T: Send + Sync> Constrained<T> {
    pub fn new(
        initial_value: T,
        validator: impl Fn(&T) -> ConstraintResult<()> + Send + Sync + 'static,
    ) -> ConstraintResult<Self> {
        let validator: Arc<ConstraintValidator<T>> = Arc::new(validator);
        validator(&initial_value)?;
        Ok(Self {
            value: initial_value,
            validator,
            normalizer: None,
        })
    }

    pub fn normalized(
        initial_value: T,
        normalizer: impl Fn(T) -> T + Send + Sync + 'static,
    ) -> ConstraintResult<Self> {
        let normalizer: Arc<ConstraintNormalizer<T>> = Arc::new(normalizer);
        let normalized = normalizer(initial_value);
        Ok(Self {
            value: normalized,
            validator: Arc::new(|_| Ok(())),
            normalizer: Some(normalizer),
        })
    }

    pub fn allow_any(initial_value: T) -> Self {
        Self {
            value: initial_value,
            validator: Arc::new(|_| Ok(())),
            normalizer: None,
        }
    }

    pub fn allow_only(only_value: T) -> Self
    where
        T: Clone + fmt::Debug + PartialEq + 'static,
    {
        let allowed_value = only_value.clone();
        Self {
            value: only_value,
            validator: Arc::new(move |candidate| {
                if candidate == &allowed_value {
                    Ok(())
                } else {
                    Err(ConstraintError::InvalidValue {
                        field_name: "<unknown>",
                        candidate: format!("{candidate:?}"),
                        allowed: format!("[{allowed_value:?}]"),
                        requirement_source: RequirementSource::Unknown,
                    })
                }
            }),
            normalizer: None,
        }
    }

    pub fn allow_any_from_default() -> Self
    where
        T: Default,
    {
        Self::allow_any(T::default())
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn value(&self) -> T
    where
        T: Copy,
    {
        self.value
    }

    pub fn can_set(&self, candidate: &T) -> ConstraintResult<()> {
        (self.validator)(candidate)
    }

    pub fn set(&mut self, value: T) -> ConstraintResult<()> {
        let value = if let Some(normalizer) = &self.normalizer {
            normalizer(value)
        } else {
            value
        };
        (self.validator)(&value)?;
        self.value = value;
        Ok(())
    }
}

impl<T> std::ops::Deref for Constrained<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for Constrained<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Constrained")
            .field("value", &self.value)
            .finish()
    }
}

impl<T: PartialEq> PartialEq for Constrained<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_value(candidate: impl Into<String>, allowed: impl Into<String>) -> ConstraintError {
        ConstraintError::InvalidValue {
            field_name: "<unknown>",
            candidate: candidate.into(),
            allowed: allowed.into(),
            requirement_source: RequirementSource::Unknown,
        }
    }

    #[test]
    fn allow_any_accepts_all() {
        let mut c = Constrained::allow_any(5);
        c.set(-10).unwrap();
        assert_eq!(c.value(), -10);
    }

    #[test]
    fn allow_only_rejects_different() {
        let mut c = Constrained::allow_only(5);
        c.set(5).unwrap();
        let err = c.set(6).unwrap_err();
        assert_eq!(err, invalid_value("6", "[5]"));
    }

    #[test]
    fn normalizer_applies() {
        let mut c = Constrained::normalized(-1, |v| v.max(0)).unwrap();
        assert_eq!(c.value(), 0);
        c.set(-5).unwrap();
        assert_eq!(c.value(), 0);
        c.set(10).unwrap();
        assert_eq!(c.value(), 10);
    }

    #[test]
    fn can_set_probes_without_setting() {
        let c = Constrained::new(1, |v| {
            if *v > 0 {
                Ok(())
            } else {
                Err(invalid_value(v.to_string(), "positive values"))
            }
        })
        .unwrap();
        c.can_set(&2).unwrap();
        assert!(c.can_set(&-1).is_err());
        assert_eq!(c.value(), 1);
    }
}
