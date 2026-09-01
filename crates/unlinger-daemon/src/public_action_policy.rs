use crate::OrdinaryMutation;

/// Volatile lifecycle facts are stabilized by the ControlPlane status lock
/// before a public mutation transaction begins.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimePolicyFacts {
    pub draining: bool,
    pub failed: bool,
}

/// Exact durable facts are re-read inside the mutation transaction. Fields
/// that do not apply to an action remain false.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StorePolicyFacts {
    pub paused: bool,
    pub cleanup_blocked: bool,
    pub incident_observed: bool,
    pub incident_protected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PolicyDecision {
    Allow,
    NoChange(&'static str),
    Reject(&'static str),
}

impl PolicyDecision {
    pub(crate) const fn unavailable_reason(self) -> Option<&'static str> {
        match self {
            Self::Allow => None,
            Self::NoChange(reason_id) | Self::Reject(reason_id) => Some(reason_id),
        }
    }
}

/// The single ordinary-action policy shared by capability projection and the
/// authoritative v3 transaction. A score or UI affordance never authorizes a
/// mutation; this typed decision is recomputed from stabilized runtime facts
/// and current durable facts immediately before the transaction applies it.
pub(crate) fn evaluate_action(
    runtime: RuntimePolicyFacts,
    store: StorePolicyFacts,
    mutation: &OrdinaryMutation,
) -> PolicyDecision {
    if runtime.draining {
        return PolicyDecision::Reject("action.daemon_draining");
    }
    if runtime.failed {
        return PolicyDecision::Reject("action.daemon_failed");
    }

    match mutation {
        OrdinaryMutation::Pause { .. } => PolicyDecision::Allow,
        OrdinaryMutation::Resume if store.paused => PolicyDecision::Allow,
        OrdinaryMutation::Resume => PolicyDecision::NoChange("action.not_paused"),
        OrdinaryMutation::RetryFailedCleanup { .. } if store.cleanup_blocked => {
            PolicyDecision::Allow
        }
        OrdinaryMutation::RetryFailedCleanup { .. } => {
            PolicyDecision::Reject("action.no_blocked_cleanup")
        }
        OrdinaryMutation::ProtectIncident { .. } if !store.incident_observed => {
            PolicyDecision::Reject("action.incident_not_observed")
        }
        OrdinaryMutation::ProtectIncident { .. } if store.incident_protected => {
            PolicyDecision::NoChange("action.already_protected")
        }
        OrdinaryMutation::ProtectIncident { .. } => PolicyDecision::Allow,
        OrdinaryMutation::UnprotectIncident { .. } if store.incident_protected => {
            PolicyDecision::Allow
        }
        OrdinaryMutation::UnprotectIncident { .. } => {
            PolicyDecision::NoChange("action.not_protected")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pause() -> OrdinaryMutation {
        OrdinaryMutation::Pause {
            duration_millis: 60_000,
        }
    }

    fn retry() -> OrdinaryMutation {
        OrdinaryMutation::RetryFailedCleanup {
            incident_id: "incident-1".to_owned(),
        }
    }

    fn protect() -> OrdinaryMutation {
        OrdinaryMutation::ProtectIncident {
            incident_id: "incident-1".to_owned(),
        }
    }

    fn unprotect() -> OrdinaryMutation {
        OrdinaryMutation::UnprotectIncident {
            incident_id: "incident-1".to_owned(),
        }
    }

    #[test]
    fn lifecycle_denial_is_shared_by_every_ordinary_action() {
        let actions = [
            pause(),
            OrdinaryMutation::Resume,
            retry(),
            protect(),
            unprotect(),
        ];

        for action in &actions {
            assert_eq!(
                evaluate_action(
                    RuntimePolicyFacts {
                        draining: true,
                        failed: false,
                    },
                    StorePolicyFacts::default(),
                    action,
                ),
                PolicyDecision::Reject("action.daemon_draining"),
            );
            assert_eq!(
                evaluate_action(
                    RuntimePolicyFacts {
                        draining: false,
                        failed: true,
                    },
                    StorePolicyFacts::default(),
                    action,
                ),
                PolicyDecision::Reject("action.daemon_failed"),
            );
        }
    }

    #[test]
    fn durable_facts_form_one_action_policy_matrix() {
        let ready = RuntimePolicyFacts::default();

        assert_eq!(
            evaluate_action(ready, StorePolicyFacts::default(), &pause()),
            PolicyDecision::Allow,
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts::default(),
                &OrdinaryMutation::Resume
            ),
            PolicyDecision::NoChange("action.not_paused"),
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts {
                    paused: true,
                    ..StorePolicyFacts::default()
                },
                &OrdinaryMutation::Resume,
            ),
            PolicyDecision::Allow,
        );
        assert_eq!(
            evaluate_action(ready, StorePolicyFacts::default(), &retry()),
            PolicyDecision::Reject("action.no_blocked_cleanup"),
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts {
                    cleanup_blocked: true,
                    ..StorePolicyFacts::default()
                },
                &retry(),
            ),
            PolicyDecision::Allow,
        );
        assert_eq!(
            evaluate_action(ready, StorePolicyFacts::default(), &protect()),
            PolicyDecision::Reject("action.incident_not_observed"),
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts {
                    incident_observed: true,
                    ..StorePolicyFacts::default()
                },
                &protect(),
            ),
            PolicyDecision::Allow,
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts {
                    incident_observed: true,
                    incident_protected: true,
                    ..StorePolicyFacts::default()
                },
                &protect(),
            ),
            PolicyDecision::NoChange("action.already_protected"),
        );
        assert_eq!(
            evaluate_action(ready, StorePolicyFacts::default(), &unprotect()),
            PolicyDecision::NoChange("action.not_protected"),
        );
        assert_eq!(
            evaluate_action(
                ready,
                StorePolicyFacts {
                    incident_protected: true,
                    ..StorePolicyFacts::default()
                },
                &unprotect(),
            ),
            PolicyDecision::Allow,
        );
    }
}
