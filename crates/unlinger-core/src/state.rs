use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IncidentState {
    Protected,
    Active,
    Cooling,
    Confirmed,
    Ambiguous,
    Reclaiming,
    Cleared,
    Revived,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionError {
    pub from: IncidentState,
    pub to: IncidentState,
}

impl Display for TransitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid incident transition {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl Error for TransitionError {}

impl IncidentState {
    pub fn transition(self, to: Self) -> Result<Self, TransitionError> {
        let allowed = self == to
            || matches!(
                (self, to),
                (
                    Self::Active,
                    Self::Cooling | Self::Protected | Self::Ambiguous
                ) | (
                    Self::Cooling,
                    Self::Confirmed | Self::Active | Self::Protected | Self::Ambiguous
                ) | (
                    Self::Confirmed,
                    Self::Reclaiming | Self::Active | Self::Protected | Self::Ambiguous
                ) | (
                    Self::Ambiguous,
                    Self::Protected | Self::Active | Self::Cooling
                ) | (
                    Self::Reclaiming,
                    Self::Cleared | Self::Revived | Self::Failed
                ) | (Self::Revived, Self::Cooling | Self::Failed)
                    | (
                        Self::Failed,
                        Self::Cooling | Self::Protected | Self::Ambiguous
                    )
            );
        allowed
            .then_some(to)
            .ok_or(TransitionError { from: self, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_active_and_ambiguous_never_jump_to_reclaiming() {
        for state in [
            IncidentState::Protected,
            IncidentState::Active,
            IncidentState::Ambiguous,
        ] {
            assert!(state.transition(IncidentState::Reclaiming).is_err());
        }
    }

    #[test]
    fn cooling_requires_confirmed_before_reclaiming() {
        assert!(
            IncidentState::Cooling
                .transition(IncidentState::Reclaiming)
                .is_err()
        );
        assert_eq!(
            IncidentState::Cooling.transition(IncidentState::Confirmed),
            Ok(IncidentState::Confirmed)
        );
        assert_eq!(
            IncidentState::Confirmed.transition(IncidentState::Reclaiming),
            Ok(IncidentState::Reclaiming)
        );
    }

    #[test]
    fn transitions_are_idempotent() {
        for state in [
            IncidentState::Protected,
            IncidentState::Active,
            IncidentState::Cooling,
            IncidentState::Confirmed,
            IncidentState::Ambiguous,
            IncidentState::Reclaiming,
            IncidentState::Cleared,
            IncidentState::Revived,
            IncidentState::Failed,
        ] {
            assert_eq!(state.transition(state), Ok(state));
        }
    }
}
