use serde::{Deserialize, Serialize};

/// Describes whether a file's contents were hashed during project discovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HashStatus {
    Hashed,
    TooLarge,
    ReadError,
}

impl HashStatus {
    pub(crate) fn as_storage_value(&self) -> &'static str {
        match self {
            Self::Hashed => "hashed",
            Self::TooLarge => "too_large",
            Self::ReadError => "read_error",
        }
    }

    pub(crate) fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "hashed" => Some(Self::Hashed),
            "too_large" => Some(Self::TooLarge),
            "read_error" => Some(Self::ReadError),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HashStatus;

    #[test]
    fn storage_values_round_trip_for_every_hash_status() {
        for status in [
            HashStatus::Hashed,
            HashStatus::TooLarge,
            HashStatus::ReadError,
        ] {
            assert_eq!(
                HashStatus::from_storage_value(status.as_storage_value()),
                Some(status)
            );
        }
    }

    #[test]
    fn unknown_storage_value_is_rejected() {
        assert_eq!(HashStatus::from_storage_value("future_status"), None);
    }
}
