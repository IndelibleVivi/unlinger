use crate::{
    AttentionKind, ControlError, ControlPlane, DaemonMode, DaemonStatus, EventPayload,
    HistoryEvent, IpcCommand, IpcPayload, ObservationRecord, ProtectedIncidentSummary,
    StartupState, StorageRecoveryReason,
};
use unlinger_core::{
    ArtifactDisposition, ArtifactOutcome, IncidentReport, IncidentState, OverallOutcome,
    ProcessOutcome, SignalDisposition,
};
use unlinger_protocol as public;

const MAX_PUBLIC_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub(crate) fn handle_at(
    control: &ControlPlane,
    command: public::Command,
    now_unix_millis: u64,
) -> Result<public::Payload, ControlError> {
    match command {
        public::Command::Status => project_payload(
            control,
            control.handle_at(IpcCommand::Status, now_unix_millis)?,
        ),
        public::Command::History { limit } => project_payload(
            control,
            control.handle_at(IpcCommand::History { limit }, now_unix_millis)?,
        ),
        public::Command::Explain { incident_id } => project_payload(
            control,
            control.handle_at(IpcCommand::Explain { incident_id }, now_unix_millis)?,
        ),
        public::Command::Incidents => {
            let roster = control.roster_snapshot();
            Ok(public::Payload::Incidents(public::ObservationRoster {
                cycle_token: roster.cycle_token,
                observed_at_unix_millis: roster.observed_at_unix_millis,
                freshness: match roster.freshness {
                    crate::ipc::RosterFreshness::Current => public::ObservationFreshness::Current,
                    crate::ipc::RosterFreshness::ScanInProgress => {
                        public::ObservationFreshness::ScanInProgress
                    }
                    crate::ipc::RosterFreshness::StaleAfterFailure => {
                        public::ObservationFreshness::StaleAfterFailure
                    }
                    crate::ipc::RosterFreshness::NeverObserved => {
                        public::ObservationFreshness::NeverObserved
                    }
                },
                items: roster
                    .reports
                    .into_iter()
                    .map(project_current_incident)
                    .collect(),
            }))
        }
        public::Command::MutationStatus { context } => {
            validate_mutation_context(&context)?;
            let status = match control.mutation_lookup(&context)? {
                crate::MutationLookup::Committed(receipt) => {
                    public::MutationStatus::Committed { receipt }
                }
                crate::MutationLookup::NotFound => public::MutationStatus::NotFound { context },
                crate::MutationLookup::AuthorityLost => {
                    public::MutationStatus::AuthorityLost { context }
                }
            };
            Ok(public::Payload::MutationStatus(status))
        }
        public::Command::Pause {
            context,
            duration_millis,
        } => {
            validate_mutation_context(&context)?;
            if duration_millis == 0 || duration_millis > MAX_PUBLIC_PAUSE_MILLIS {
                return Err(ControlError::InvalidArgument(format!(
                    "pause duration must be between 1 and {MAX_PUBLIC_PAUSE_MILLIS} milliseconds"
                )));
            }
            commit_mutation(
                control,
                context,
                crate::OrdinaryMutation::Pause { duration_millis },
                now_unix_millis,
            )
        }
        public::Command::Resume { context } => {
            validate_mutation_context(&context)?;
            commit_mutation(
                control,
                context,
                crate::OrdinaryMutation::Resume,
                now_unix_millis,
            )
        }
        public::Command::RetryFailedCleanup {
            context,
            incident_id,
        } => {
            validate_mutation_context(&context)?;
            validate_incident_id(&incident_id)?;
            commit_mutation(
                control,
                context,
                crate::OrdinaryMutation::RetryFailedCleanup { incident_id },
                now_unix_millis,
            )
        }
        public::Command::ProtectIncident {
            context,
            incident_id,
        } => {
            validate_mutation_context(&context)?;
            validate_incident_id(&incident_id)?;
            commit_mutation(
                control,
                context,
                crate::OrdinaryMutation::ProtectIncident { incident_id },
                now_unix_millis,
            )
        }
        public::Command::UnprotectIncident {
            context,
            incident_id,
        } => {
            validate_mutation_context(&context)?;
            validate_incident_id(&incident_id)?;
            commit_mutation(
                control,
                context,
                crate::OrdinaryMutation::UnprotectIncident { incident_id },
                now_unix_millis,
            )
        }
        public::Command::ExportDiagnostics { incident_id } => project_payload(
            control,
            control.handle_at(
                IpcCommand::ExportDiagnostics { incident_id },
                now_unix_millis,
            )?,
        ),
    }
}

fn commit_mutation(
    control: &ControlPlane,
    context: public::MutationContext,
    mutation: crate::OrdinaryMutation,
    now_unix_millis: u64,
) -> Result<public::Payload, ControlError> {
    let commit = control.commit_public_mutation(&context, &mutation, now_unix_millis)?;
    Ok(public::Payload::MutationCommitted(commit.receipt))
}

fn validate_mutation_context(context: &public::MutationContext) -> Result<(), ControlError> {
    if !public::is_valid_namespace_token(&context.namespace_token) {
        return Err(ControlError::InvalidArgument(
            "mutation namespace must use the canonical 32-byte opaque token representation"
                .to_owned(),
        ));
    }
    if !public::is_valid_mutation_id(&context.mutation_id) {
        return Err(ControlError::InvalidArgument(
            "mutation ID must use canonical 36-byte UUID representation".to_owned(),
        ));
    }
    Ok(())
}

fn validate_incident_id(incident_id: &str) -> Result<(), ControlError> {
    if incident_id.is_empty() || incident_id.len() > 128 {
        Err(ControlError::InvalidArgument(
            "incident ID must contain 1 to 128 bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn project_payload(
    control: &ControlPlane,
    payload: IpcPayload,
) -> Result<public::Payload, ControlError> {
    Ok(match payload {
        IpcPayload::Status(status) => public::Payload::Status(project_status(control, status)?),
        IpcPayload::History(events) => {
            public::Payload::History(events.into_iter().map(project_history_event).collect())
        }
        IpcPayload::Incident(detail) => {
            public::Payload::Incident(project_incident(control, detail)?)
        }
        IpcPayload::Pause { .. }
        | IpcPayload::Resumed
        | IpcPayload::RetryScheduled { .. }
        | IpcPayload::IncidentProtected { .. }
        | IpcPayload::IncidentUnprotected { .. } => {
            return Err(ControlError::Unavailable(
                "public mutations require schema-v3 durable receipts".to_owned(),
            ));
        }
        IpcPayload::Diagnostics(bundle) => {
            let incident = project_incident(control, bundle.incident)?;
            public::Payload::Diagnostics(public::DiagnosticsBundle {
                document_schema_version: public::SCHEMA_VERSION,
                generated_at_unix_millis: bundle.generated_at_unix_millis,
                status: project_status(control, bundle.status)?,
                incident,
            })
        }
        IpcPayload::Lifecycle(_) => {
            return Err(ControlError::Unavailable(
                "service lifecycle responses are not part of public IPC v3".to_owned(),
            ));
        }
    })
}

fn project_status(
    control: &ControlPlane,
    status: DaemonStatus,
) -> Result<public::PublicStatus, ControlError> {
    let blocked_cleanup_count = status.attention.blocked_cleanup_count;
    let effective_mode = project_mode(status.effective_mode());
    let readiness = match status.startup_state {
        StartupState::Draining => public::Readiness::Draining,
        StartupState::Failed => public::Readiness::Failed,
        StartupState::ReadyReportOnly | StartupState::ReadyEnforce if status.ready => {
            public::Readiness::Ready
        }
        StartupState::Legacy if status.healthy => public::Readiness::Ready,
        StartupState::Legacy
        | StartupState::Booting
        | StartupState::Recovering
        | StartupState::FirstScanReportOnly
        | StartupState::ReadyReportOnly
        | StartupState::ReadyEnforce => public::Readiness::Starting,
    };
    let had_storage_recovery = status.storage_recovery.is_some();
    let pause = project_mutation_capability(
        control,
        &crate::OrdinaryMutation::Pause { duration_millis: 1 },
    )?;
    let resume = project_mutation_capability(control, &crate::OrdinaryMutation::Resume)?;
    let attention_items = status
        .attention
        .items
        .into_iter()
        .map(|item| {
            let outcome = item.outcome;
            let residue = outcome
                .as_ref()
                .is_some_and(|value| value.overall == OverallOutcome::ClearedWithResidue);
            public::AttentionItem {
                event_token: item.event_token,
                kind: if residue {
                    public::AttentionKind::CleanupResidue
                } else {
                    match item.kind {
                        AttentionKind::CleanupFailed => public::AttentionKind::CleanupFailed,
                        AttentionKind::CleanupRevived => public::AttentionKind::CleanupRevived,
                        AttentionKind::DaemonUnhealthy => public::AttentionKind::DaemonUnhealthy,
                        AttentionKind::EventSourceDegraded => {
                            public::AttentionKind::EventSourceDegraded
                        }
                        AttentionKind::StorageRecovered => public::AttentionKind::StorageRecovered,
                    }
                },
                reason_id: item.reason_id,
                incident_id: item.incident_id,
                overall_outcome: outcome.map(|value| project_overall_outcome(value.overall)),
                occurred_at_unix_millis: item.occurred_at_unix_millis,
            }
        })
        .collect();
    let storage_recovery = status
        .storage_recovery
        .map(|recovery| public::StorageRecovery {
            recovery_token: recovery.public_token,
            occurred_at_unix_millis: recovery.occurred_at_unix_millis,
            reason_id: match recovery.reason {
                StorageRecoveryReason::IntegrityCheckFailed => "storage.integrity_recovered",
                StorageRecoveryReason::RequiredSchemaInvalid => "storage.schema_recovered",
            }
            .to_owned(),
            quarantined_sidecar_count: recovery.quarantined_sidecar_count,
        });
    let most_recent_reclaim = status.most_recent_reclaim.and_then(|reclaim| {
        let outcome = reclaim.outcome?;
        let event_token = reclaim.event_token?;
        Some(public::RecentReclaim {
            event_token,
            incident_id: reclaim.incident_id,
            occurred_at_unix_millis: reclaim.occurred_at_unix_millis,
            process_outcome: project_process_outcome(outcome.process),
            artifact_outcome: project_artifact_outcome(outcome.artifact),
            overall_outcome: project_overall_outcome(outcome.overall),
        })
    });
    let mutation_authority = public::MutationAuthority {
        namespace_token: control.store().mutation_namespace_token()?,
        minimum_reconciliation_window_millis: crate::store::MUTATION_RECONCILIATION_WINDOW_MILLIS,
    };
    Ok(public::PublicStatus {
        daemon_version: status.daemon_version,
        healthy: status.healthy,
        readiness,
        effective_mode,
        scan_in_progress: status.scan_in_progress,
        cleanup_in_progress: status.cleanup_in_progress,
        paused_until_unix_millis: status.paused_until_unix_millis,
        cycle_started_at_unix_millis: status.cycle_started_at_unix_millis,
        latest_observation_at_unix_millis: status.latest_observation_at_unix_millis,
        last_scan_at_unix_millis: status.last_scan_at_unix_millis,
        confirmed_incident_count: status.confirmed_incidents,
        ambiguous_incident_count: status.ambiguous_incidents,
        most_recent_reclaim,
        event_source: public::EventSourceHealth {
            healthy: status.event_source_healthy,
            reason_id: (!status.event_source_healthy)
                .then(|| "runtime.event_source_degraded".to_owned()),
        },
        storage: public::StorageHealth {
            healthy: true,
            last_recovery: storage_recovery,
        },
        attention: public::AttentionProjection {
            total_count: blocked_cleanup_count
                + usize::from(!status.healthy)
                + usize::from(!status.event_source_healthy)
                + usize::from(had_storage_recovery),
            items: attention_items,
        },
        protection: public::ProtectionProjection {
            total_count: status.protected_incident_count,
            items: status
                .protected_incidents
                .into_iter()
                .map(project_protection)
                .collect(),
        },
        capabilities: public::GlobalCapabilities { pause, resume },
        mutation_authority,
    })
}

fn project_mutation_capability(
    control: &ControlPlane,
    mutation: &crate::OrdinaryMutation,
) -> Result<public::Capability, ControlError> {
    Ok(
        match control.ordinary_mutation_unavailable_reason(mutation)? {
            Some(reason_id) => public::Capability::unavailable(reason_id),
            None => public::Capability::available(),
        },
    )
}

fn project_incident(
    control: &ControlPlane,
    detail: crate::IncidentDetail,
) -> Result<public::IncidentDetail, ControlError> {
    let incident_id = detail.incident_id;
    let retry_failed_cleanup = project_mutation_capability(
        control,
        &crate::OrdinaryMutation::RetryFailedCleanup {
            incident_id: incident_id.clone(),
        },
    )?;
    let protect_incident = project_mutation_capability(
        control,
        &crate::OrdinaryMutation::ProtectIncident {
            incident_id: incident_id.clone(),
        },
    )?;
    let unprotect_incident = project_mutation_capability(
        control,
        &crate::OrdinaryMutation::UnprotectIncident {
            incident_id: incident_id.clone(),
        },
    )?;
    Ok(public::IncidentDetail {
        incident_id,
        events: detail
            .events
            .into_iter()
            .map(project_history_event)
            .collect(),
        capabilities: public::IncidentCapabilities {
            retry_failed_cleanup,
            protect_incident,
            unprotect_incident,
            export_diagnostics: public::Capability::available(),
        },
    })
}

fn project_history_event(event: HistoryEvent) -> public::HistoryEvent {
    let event_token = event.event_token;
    public::HistoryEvent {
        event_token: event_token.clone(),
        incident_id: event.incident_id,
        occurred_at_unix_millis: event.occurred_at_unix_millis,
        state: project_incident_state(event.state),
        payload: match event.payload {
            EventPayload::Observation { report } => public::EventPayload::Observation {
                observation: project_observation_record(report),
            },
            EventPayload::Cleanup { receipt } => {
                let outcome = receipt.outcome();
                public::EventPayload::Cleanup {
                    cleanup: public::Cleanup {
                        state: project_incident_state(receipt.state),
                        reason_id: receipt.reason_id,
                        process_outcome: project_process_outcome(outcome.process),
                        artifact_outcome: project_artifact_outcome(outcome.artifact),
                        overall_outcome: project_overall_outcome(outcome.overall),
                        attention_required: outcome.attention_required,
                        process_actions: receipt
                            .actions
                            .into_iter()
                            .enumerate()
                            .map(|(index, action)| public::ProcessAction {
                                action_token: format!("{event_token}:p:{index}"),
                                stage: project_cleanup_stage(action.stage),
                                signal: project_cleanup_signal(action.signal),
                                disposition: project_signal_disposition(action.disposition),
                            })
                            .collect(),
                        artifact_actions: receipt
                            .artifact_actions
                            .into_iter()
                            .enumerate()
                            .map(|(index, action)| public::ArtifactAction {
                                action_token: format!("{event_token}:a:{index}"),
                                kind: project_artifact_kind(action.kind),
                                disposition: project_artifact_disposition(action.disposition),
                            })
                            .collect(),
                        survivor_count: receipt.survivor_pids.len(),
                        revival_checks_completed: receipt.revival_checks_completed,
                        resources: public::CleanupResources {
                            before: receipt.resources.before.map(|snapshot| {
                                public::ResourceSnapshot {
                                    process_count: snapshot.process_count,
                                    resident_memory_bytes: snapshot.resident_memory_bytes,
                                }
                            }),
                            after: receipt.resources.after.map(|snapshot| {
                                public::ResourceSnapshot {
                                    process_count: snapshot.process_count,
                                    resident_memory_bytes: snapshot.resident_memory_bytes,
                                }
                            }),
                            estimated_reclaimed_memory_bytes: receipt
                                .resources
                                .estimated_reclaimed_memory_bytes,
                        },
                    },
                }
            }
        },
    }
}

/// Projects one in-memory cycle report into the public roster entry. The
/// store's `ObservationRecord` redaction step runs first, so tracking keys,
/// fingerprints, targets, and artifact candidates never leave the daemon.
fn project_current_incident(report: IncidentReport) -> public::CurrentIncident {
    let record = ObservationRecord::from(&report);
    public::CurrentIncident {
        incident_id: report.incident_id,
        observation: project_observation_record(record),
    }
}

fn project_observation_record(report: ObservationRecord) -> public::Observation {
    public::Observation {
        family: report.signature_pack,
        family_version: report.signature_version,
        state: project_incident_state(report.state),
        executable_basename: report.root.executable_basename,
        member_count: report.member_count,
        resident_memory_bytes: report.resident_memory_bytes,
        roles: report
            .roles
            .into_iter()
            .map(|role| public::RoleCount {
                role: project_process_role(role.role),
                count: role.count,
            })
            .collect(),
        evidence: report
            .evidence
            .into_iter()
            .map(|evidence| public::Evidence {
                id: evidence.id,
                family: project_evidence_family(evidence.family),
            })
            .collect(),
        gates: public::GateLedger {
            same_user: report.gates.same_user,
            strong_automation_provenance: report.gates.strong_automation_provenance,
            confirmed_abandonment: report.gates.confirmed_abandonment,
            isolated_session: report.gates.isolated_session,
            stable_across_two_observations: report.gates.stable_across_two_observations,
            process_identity_unchanged: report.gates.process_identity_unchanged,
            no_protection_rule: report.gates.no_protection_rule,
        },
    }
}

fn project_protection(summary: ProtectedIncidentSummary) -> public::ProtectionSummary {
    public::ProtectionSummary {
        incident_id: summary.incident_id,
        protected_at_unix_millis: summary.protected_at_unix_millis,
        last_exact_observed_at_unix_millis: summary.last_exact_observed_at_unix_millis,
        exact_absence_since_unix_millis: summary.exact_absence_since_unix_millis,
    }
}

fn project_mode(mode: DaemonMode) -> public::Mode {
    match mode {
        DaemonMode::ReportOnly => public::Mode::ReportOnly,
        DaemonMode::Enforce => public::Mode::Enforce,
    }
}

fn project_incident_state(state: IncidentState) -> public::IncidentState {
    match state {
        IncidentState::Protected => public::IncidentState::Protected,
        IncidentState::Active => public::IncidentState::Active,
        IncidentState::Cooling => public::IncidentState::Cooling,
        IncidentState::Confirmed => public::IncidentState::Confirmed,
        IncidentState::Ambiguous => public::IncidentState::Ambiguous,
        IncidentState::Reclaiming => public::IncidentState::Reclaiming,
        IncidentState::Cleared => public::IncidentState::Cleared,
        IncidentState::Revived => public::IncidentState::Revived,
        IncidentState::Failed => public::IncidentState::Failed,
    }
}

fn project_process_outcome(outcome: ProcessOutcome) -> public::ProcessOutcome {
    match outcome {
        ProcessOutcome::Cleared => public::ProcessOutcome::Cleared,
        ProcessOutcome::Revived => public::ProcessOutcome::Revived,
        ProcessOutcome::Failed => public::ProcessOutcome::Failed,
        ProcessOutcome::DeliveryUnknown => public::ProcessOutcome::DeliveryUnknown,
    }
}

fn project_artifact_outcome(outcome: ArtifactOutcome) -> public::ArtifactOutcome {
    match outcome {
        ArtifactOutcome::NotApplicable => public::ArtifactOutcome::NotApplicable,
        ArtifactOutcome::Reconciled => public::ArtifactOutcome::Reconciled,
        ArtifactOutcome::Residue => public::ArtifactOutcome::Residue,
        ArtifactOutcome::DeliveryUnknown => public::ArtifactOutcome::DeliveryUnknown,
    }
}

fn project_overall_outcome(outcome: OverallOutcome) -> public::OverallOutcome {
    match outcome {
        OverallOutcome::Cleared => public::OverallOutcome::Cleared,
        OverallOutcome::ClearedWithResidue => public::OverallOutcome::ClearedWithResidue,
        OverallOutcome::Revived => public::OverallOutcome::Revived,
        OverallOutcome::Failed => public::OverallOutcome::Failed,
    }
}

fn project_process_role(role: unlinger_core::ProcessRole) -> public::ProcessRole {
    match role {
        unlinger_core::ProcessRole::Controller => public::ProcessRole::Controller,
        unlinger_core::ProcessRole::BrowserRoot => public::ProcessRole::BrowserRoot,
        unlinger_core::ProcessRole::Renderer => public::ProcessRole::Renderer,
        unlinger_core::ProcessRole::Gpu => public::ProcessRole::Gpu,
        unlinger_core::ProcessRole::Utility => public::ProcessRole::Utility,
        unlinger_core::ProcessRole::BrowserHelper => public::ProcessRole::BrowserHelper,
        unlinger_core::ProcessRole::CrashHandler => public::ProcessRole::CrashHandler,
        unlinger_core::ProcessRole::Recorder => public::ProcessRole::Recorder,
        unlinger_core::ProcessRole::IncidentMember => public::ProcessRole::IncidentMember,
    }
}

fn project_evidence_family(family: unlinger_core::EvidenceFamily) -> public::EvidenceFamily {
    match family {
        unlinger_core::EvidenceFamily::AutomationProvenance => {
            public::EvidenceFamily::AutomationProvenance
        }
        unlinger_core::EvidenceFamily::Abandonment => public::EvidenceFamily::Abandonment,
        unlinger_core::EvidenceFamily::Isolation => public::EvidenceFamily::Isolation,
        unlinger_core::EvidenceFamily::Protection => public::EvidenceFamily::Protection,
    }
}

fn project_cleanup_stage(stage: unlinger_core::CleanupStage) -> public::CleanupStage {
    match stage {
        unlinger_core::CleanupStage::PrimaryTerm => public::CleanupStage::PrimaryTerm,
        unlinger_core::CleanupStage::MemberTerm => public::CleanupStage::MemberTerm,
        unlinger_core::CleanupStage::ExactKill => public::CleanupStage::ExactKill,
    }
}

fn project_cleanup_signal(signal: unlinger_core::CleanupSignal) -> public::CleanupSignal {
    match signal {
        unlinger_core::CleanupSignal::Term => public::CleanupSignal::Term,
        unlinger_core::CleanupSignal::Kill => public::CleanupSignal::Kill,
    }
}

fn project_signal_disposition(disposition: SignalDisposition) -> public::SignalDisposition {
    match disposition {
        SignalDisposition::Delivered => public::SignalDisposition::Delivered,
        SignalDisposition::AlreadyExited => public::SignalDisposition::AlreadyExited,
        SignalDisposition::IdentityMismatch => public::SignalDisposition::IdentityMismatch,
        SignalDisposition::Rejected => public::SignalDisposition::Rejected,
        SignalDisposition::CancelledBeforeDelivery => {
            public::SignalDisposition::CancelledBeforeDelivery
        }
        SignalDisposition::DeliveryUnknown => public::SignalDisposition::DeliveryUnknown,
    }
}

fn project_artifact_kind(kind: unlinger_core::RuntimeArtifactKind) -> public::RuntimeArtifactKind {
    match kind {
        unlinger_core::RuntimeArtifactKind::DevToolsActivePort => {
            public::RuntimeArtifactKind::DevToolsActivePort
        }
    }
}

fn project_artifact_disposition(disposition: ArtifactDisposition) -> public::ArtifactDisposition {
    match disposition {
        ArtifactDisposition::Removed => public::ArtifactDisposition::Removed,
        ArtifactDisposition::AlreadyAbsent => public::ArtifactDisposition::AlreadyAbsent,
        ArtifactDisposition::IdentityMismatch => public::ArtifactDisposition::IdentityMismatch,
        ArtifactDisposition::Referenced => public::ArtifactDisposition::Referenced,
        ArtifactDisposition::Unsafe => public::ArtifactDisposition::Unsafe,
        ArtifactDisposition::Rejected => public::ArtifactDisposition::Rejected,
        ArtifactDisposition::CancelledBeforeDelivery => {
            public::ArtifactDisposition::CancelledBeforeDelivery
        }
        ArtifactDisposition::DeliveryUnknown => public::ArtifactDisposition::DeliveryUnknown,
    }
}
