use crate::{
    AttentionKind, ControlError, ControlPlane, DaemonMode, DaemonStatus, EventPayload,
    HistoryEvent, IpcCommand, IpcPayload, ObservationRecord, ProtectedIncidentSummary,
    StartupState, StorageRecoveryReason,
};
use unlinger_core::{
    ArtifactDisposition, ArtifactOutcome, BrowserCompatibility, BrowserCompatibilityDecision,
    BrowserProduct, IncidentReport, IncidentState, OverallOutcome, ProcessOutcome,
    SignalDisposition,
};
use unlinger_protocol as public;
use unlinger_rules::{BrowserAutomaticActionLevel, RuleSet};

const MAX_PUBLIC_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub(crate) fn handle_at(
    control: &ControlPlane,
    command: public::Command,
    now_unix_millis: u64,
    schema_version: u32,
) -> Result<public::Payload, ControlError> {
    match command {
        public::Command::Status => project_payload(
            control,
            control.handle_at(IpcCommand::Status, now_unix_millis)?,
            schema_version,
        ),
        public::Command::BrowserOverview => {
            project_browser_overview(control, now_unix_millis, schema_version)
        }
        public::Command::History { limit } => project_payload(
            control,
            control.handle_at(IpcCommand::History { limit }, now_unix_millis)?,
            schema_version,
        ),
        public::Command::Explain { incident_id } => project_payload(
            control,
            control.handle_at(IpcCommand::Explain { incident_id }, now_unix_millis)?,
            schema_version,
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
                    .map(|report| {
                        project_current_incident(
                            report,
                            schema_version >= public::PREVIOUS_SCHEMA_VERSION,
                        )
                    })
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
            schema_version,
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

fn project_browser_overview(
    control: &ControlPlane,
    now_unix_millis: u64,
    schema_version: u32,
) -> Result<public::Payload, ControlError> {
    let source = control.browser_source_snapshot_at(now_unix_millis)?;
    let phase = browser_overview_phase(&source.status, &source.roster);
    let impact = control.store().impact_summary(0)?;
    let storage_residue = if schema_version == public::SCHEMA_VERSION {
        control
            .store()
            .latest_storage_residue_observation()?
            .map(project_storage_residue)
    } else {
        None
    };
    let settlement_impact = source
        .status
        .most_recent_reclaim
        .as_ref()
        .and_then(|reclaim| reclaim.event_token.as_deref())
        .map(|event_token| control.store().cleanup_impact_for_event_token(event_token))
        .transpose()?
        .flatten();
    let recent_settlement = project_recent_browser_settlement(
        settlement_impact.as_ref(),
        source.status.most_recent_reclaim.as_ref(),
    );
    let projected_status = project_status(control, source.status)?;
    let sessions = source
        .roster
        .reports
        .into_iter()
        .map(|report| public::BrowserSessionSummary {
            incident_id: report.incident_id,
            family: report.signature_pack,
            state: project_incident_state(report.state),
            member_count: report.member_count,
            resident_memory_bytes: report.resident_memory_bytes,
            compatibility: project_browser_compatibility(report.browser_compatibility),
            capabilities: public::BrowserSessionCapabilities {
                open_detail: public::Capability::available(),
            },
        })
        .collect::<Vec<_>>();
    let coverage_notices = sessions
        .iter()
        .filter_map(|session| {
            session.compatibility.reason_id.as_ref().map(|reason_id| {
                public::BrowserCoverageSummary {
                    incident_id: session.incident_id.clone(),
                    decision: session.compatibility.decision,
                    reason_id: reason_id.clone(),
                }
            })
        })
        .collect();
    let catalog = RuleSet::embedded()
        .map_err(|_| {
            ControlError::Unavailable("embedded browser support catalog is unavailable".to_owned())
        })?
        .browser_support_catalog();
    let support_catalog = public::BrowserSupportCatalog {
        support_revision: catalog.support_revision,
        families: catalog
            .families
            .into_iter()
            .map(|family| public::BrowserFamilySupport {
                family: family.family,
                product: project_browser_product(family.product),
                admitted_versions: family.admitted_versions,
                automatic_action_level: match family.automatic_action_level {
                    BrowserAutomaticActionLevel::Automatic => {
                        public::BrowserAutomaticActionLevel::Automatic
                    }
                    BrowserAutomaticActionLevel::ObserveOnly => {
                        public::BrowserAutomaticActionLevel::ObserveOnly
                    }
                    BrowserAutomaticActionLevel::Unsupported => {
                        public::BrowserAutomaticActionLevel::Unsupported
                    }
                },
            })
            .collect(),
    };
    Ok(public::Payload::BrowserOverview(
        public::BrowserOverviewSnapshot {
            generated_at_unix_millis: now_unix_millis,
            cycle_token: source.roster.cycle_token,
            observed_at_unix_millis: source.roster.observed_at_unix_millis,
            freshness: project_roster_freshness(source.roster.freshness),
            healthy: projected_status.healthy,
            effective_mode: projected_status.effective_mode,
            paused_until_unix_millis: projected_status.paused_until_unix_millis,
            phase,
            sessions,
            coverage_notices,
            recent_settlement,
            impact: (schema_version == public::SCHEMA_VERSION).then_some({
                public::BrowserImpactSummary {
                    tracking_started_at_unix_millis: impact.tracking_started_at_unix_millis,
                    historical_completeness: match impact.historical_completeness {
                        crate::ImpactHistoryCompleteness::Complete => {
                            public::ImpactHistoryCompleteness::Complete
                        }
                        crate::ImpactHistoryCompleteness::PartialBackfill => {
                            public::ImpactHistoryCompleteness::PartialBackfill
                        }
                    },
                    terminal_cleanup_count: impact.terminal_cleanup_count,
                    proved_reclaim_count: impact.proved_reclaim_count,
                    reclaimed_process_count: impact.reclaimed_process_count,
                    estimated_reclaimed_memory_bytes: impact.estimated_reclaimed_memory_bytes,
                }
            }),
            storage_residue,
            attention: projected_status.attention,
            protection: projected_status.protection,
            support_catalog,
        },
    ))
}

fn project_storage_residue(
    observation: unlinger_core::StorageResidueObservation,
) -> public::StorageResidueSummary {
    public::StorageResidueSummary {
        kind: match observation.kind {
            unlinger_core::StorageResidueKind::ChromeCodeSignClone => {
                public::StorageResidueKind::ChromeCodeSignClone
            }
        },
        status: match observation.status {
            unlinger_core::StorageResidueStatus::Clear => public::StorageResidueStatus::Clear,
            unlinger_core::StorageResidueStatus::Detected => public::StorageResidueStatus::Detected,
            unlinger_core::StorageResidueStatus::Unavailable => {
                public::StorageResidueStatus::Unavailable
            }
        },
        observed_at_unix_millis: observation.observed_at_unix_millis,
        candidate_count: observation.candidate_count,
        logical_bytes: observation.logical_bytes,
        shape_complete: observation.shape_complete,
        reference_check: match observation.reference_check {
            unlinger_core::StorageResidueReferenceCheck::Incomplete => {
                public::StorageResidueReferenceCheck::Incomplete
            }
            unlinger_core::StorageResidueReferenceCheck::CompleteNoReferences => {
                public::StorageResidueReferenceCheck::CompleteNoReferences
            }
            unlinger_core::StorageResidueReferenceCheck::Referenced => {
                public::StorageResidueReferenceCheck::Referenced
            }
        },
        automatic_cleanup_eligible: observation.automatic_cleanup_eligible,
        reason_ids: observation.reason_ids,
    }
}

fn browser_overview_phase(
    status: &DaemonStatus,
    roster: &crate::ipc::RosterSnapshot,
) -> public::BrowserOverviewPhase {
    let readiness_is_ready = matches!(
        status.startup_state,
        StartupState::ReadyReportOnly | StartupState::ReadyEnforce
    ) && status.ready;
    let coherent = status.latest_observation_at_unix_millis.is_some()
        && status.latest_observation_at_unix_millis == roster.observed_at_unix_millis;
    if !status.healthy
        || !readiness_is_ready
        || status.scan_in_progress
        || roster.freshness != crate::ipc::RosterFreshness::Current
        || !coherent
    {
        return public::BrowserOverviewPhase::Unknown;
    }
    if status.attention.blocked_cleanup_count > 0
        || !status.attention.items.is_empty()
        || !status.event_source_healthy
        || status.storage_recovery.is_some()
        || roster
            .reports
            .iter()
            .any(|report| matches!(report.state, IncidentState::Failed | IncidentState::Revived))
    {
        return public::BrowserOverviewPhase::Attention;
    }
    if status.cleanup_in_progress
        || roster
            .reports
            .iter()
            .any(|report| report.state == IncidentState::Reclaiming)
    {
        return public::BrowserOverviewPhase::Reclaiming;
    }
    if roster
        .reports
        .iter()
        .any(|report| report.state == IncidentState::Confirmed)
    {
        return public::BrowserOverviewPhase::Confirmed;
    }
    if roster
        .reports
        .iter()
        .any(|report| report.state == IncidentState::Cooling)
    {
        return public::BrowserOverviewPhase::Verifying;
    }
    if roster
        .reports
        .iter()
        .any(|report| report.state == IncidentState::Active)
    {
        return public::BrowserOverviewPhase::Active;
    }
    if roster.reports.iter().any(|report| {
        matches!(
            report.state,
            IncidentState::Protected | IncidentState::Ambiguous
        )
    }) {
        return public::BrowserOverviewPhase::Protected;
    }
    public::BrowserOverviewPhase::Clear
}

fn project_roster_freshness(
    freshness: crate::ipc::RosterFreshness,
) -> public::ObservationFreshness {
    match freshness {
        crate::ipc::RosterFreshness::Current => public::ObservationFreshness::Current,
        crate::ipc::RosterFreshness::ScanInProgress => public::ObservationFreshness::ScanInProgress,
        crate::ipc::RosterFreshness::StaleAfterFailure => {
            public::ObservationFreshness::StaleAfterFailure
        }
        crate::ipc::RosterFreshness::NeverObserved => public::ObservationFreshness::NeverObserved,
    }
}

fn project_recent_browser_settlement(
    impact: Option<&crate::CleanupImpact>,
    reclaim: Option<&crate::RecentReclaim>,
) -> Option<public::BrowserSettlementSummary> {
    let reclaim = reclaim?;
    let (Some(event_token), Some(expected_outcome)) =
        (reclaim.event_token.as_ref(), reclaim.outcome.as_ref())
    else {
        return None;
    };
    let impact = impact.filter(|impact| {
        impact.event_token == *event_token
            && impact.incident_id == reclaim.incident_id
            && impact.occurred_at_unix_millis == reclaim.occurred_at_unix_millis
    })?;
    if impact.outcome != *expected_outcome {
        return None;
    }
    Some(public::BrowserSettlementSummary {
        event_token: event_token.clone(),
        incident_id: reclaim.incident_id.clone(),
        family: impact.family.clone(),
        occurred_at_unix_millis: impact.occurred_at_unix_millis,
        process_count: impact.process_count,
        estimated_reclaimed_memory_bytes: impact.estimated_reclaimed_memory_bytes,
        revival_checks_completed: impact.revival_checks_completed,
        artifact_outcome: project_artifact_outcome(impact.outcome.artifact),
        overall_outcome: project_overall_outcome(impact.outcome.overall),
    })
}

fn project_payload(
    control: &ControlPlane,
    payload: IpcPayload,
    schema_version: u32,
) -> Result<public::Payload, ControlError> {
    Ok(match payload {
        IpcPayload::Status(status) => public::Payload::Status(project_status(control, status)?),
        IpcPayload::History(events) => public::Payload::History(
            events
                .into_iter()
                .map(|event| project_history_event(event, schema_version))
                .collect(),
        ),
        IpcPayload::Incident(detail) => {
            public::Payload::Incident(project_incident(control, detail, schema_version)?)
        }
        IpcPayload::Pause { .. }
        | IpcPayload::Resumed
        | IpcPayload::RetryScheduled { .. }
        | IpcPayload::IncidentProtected { .. }
        | IpcPayload::IncidentUnprotected { .. } => {
            return Err(ControlError::Unavailable(
                "frontend mutations require durable receipts".to_owned(),
            ));
        }
        IpcPayload::Diagnostics(bundle) => {
            let incident = project_incident(control, bundle.incident, schema_version)?;
            public::Payload::Diagnostics(public::DiagnosticsBundle {
                document_schema_version: schema_version,
                generated_at_unix_millis: bundle.generated_at_unix_millis,
                status: project_status(control, bundle.status)?,
                incident,
            })
        }
        IpcPayload::Lifecycle(_) => {
            return Err(ControlError::Unavailable(
                "service lifecycle responses are not part of frontend IPC".to_owned(),
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
    schema_version: u32,
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
            .map(|event| project_history_event(event, schema_version))
            .collect(),
        capabilities: public::IncidentCapabilities {
            retry_failed_cleanup,
            protect_incident,
            unprotect_incident,
            export_diagnostics: public::Capability::available(),
        },
    })
}

fn project_history_event(event: HistoryEvent, schema_version: u32) -> public::HistoryEvent {
    let event_token = event.event_token;
    public::HistoryEvent {
        event_token: event_token.clone(),
        incident_id: event.incident_id,
        occurred_at_unix_millis: event.occurred_at_unix_millis,
        observation_span: (schema_version == public::SCHEMA_VERSION
            && event.kind == crate::EventKind::Observation)
            .then_some(public::ObservationSpan {
                first_observed_at_unix_millis: event.first_occurred_at_unix_millis,
                observation_count: event.observation_count,
            }),
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
fn project_current_incident(
    report: IncidentReport,
    include_browser_compatibility: bool,
) -> public::CurrentIncident {
    let record = ObservationRecord::from(&report);
    public::CurrentIncident {
        incident_id: report.incident_id,
        observation: project_observation_record_with_compatibility(
            record,
            include_browser_compatibility
                .then(|| project_browser_compatibility(report.browser_compatibility)),
        ),
    }
}

fn project_observation_record(report: ObservationRecord) -> public::Observation {
    project_observation_record_with_compatibility(report, None)
}

fn project_observation_record_with_compatibility(
    report: ObservationRecord,
    browser_compatibility: Option<public::BrowserCompatibility>,
) -> public::Observation {
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
        browser_compatibility,
    }
}

fn project_browser_compatibility(
    compatibility: BrowserCompatibility,
) -> public::BrowserCompatibility {
    public::BrowserCompatibility {
        product: project_browser_product(compatibility.product),
        observed_version: compatibility.observed_version,
        decision: project_browser_compatibility_decision(compatibility.decision),
        reason_id: compatibility.reason_id,
    }
}

fn project_browser_product(product: BrowserProduct) -> public::BrowserProduct {
    match product {
        BrowserProduct::ChromeForTesting => public::BrowserProduct::ChromeForTesting,
        BrowserProduct::Chromium => public::BrowserProduct::Chromium,
        BrowserProduct::GoogleChrome => public::BrowserProduct::GoogleChrome,
        BrowserProduct::Other => public::BrowserProduct::Other,
        BrowserProduct::Unknown => public::BrowserProduct::Unknown,
    }
}

fn project_browser_compatibility_decision(
    decision: BrowserCompatibilityDecision,
) -> public::BrowserCompatibilityDecision {
    match decision {
        BrowserCompatibilityDecision::Automatic => public::BrowserCompatibilityDecision::Automatic,
        BrowserCompatibilityDecision::ObserveOnly => {
            public::BrowserCompatibilityDecision::ObserveOnly
        }
        BrowserCompatibilityDecision::Protected => public::BrowserCompatibilityDecision::Protected,
        BrowserCompatibilityDecision::Unknown => public::BrowserCompatibilityDecision::Unknown,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{RosterFreshness, RosterSnapshot};
    use unlinger_core::{GateLedger, RootSummary};

    fn ready_status() -> DaemonStatus {
        let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
        status.healthy = true;
        status.ready = true;
        status.startup_state = StartupState::ReadyReportOnly;
        status.latest_observation_at_unix_millis = Some(100);
        status
    }

    fn report(state: IncidentState) -> IncidentReport {
        IncidentReport {
            incident_id: format!("inc-{state:?}"),
            tracking_key: "transient-tracking".to_owned(),
            session_fingerprint: "transient-session".to_owned(),
            signature_pack: "playwright".to_owned(),
            signature_version: "0.2.0".to_owned(),
            state,
            root: RootSummary {
                pid: 42,
                started_at_unix_micros: 100,
                executable_basename: "node".to_owned(),
                identity_fingerprint: "transient-identity".to_owned(),
            },
            member_count: 1,
            resident_memory_bytes: 4096,
            member_fingerprint: "transient-members".to_owned(),
            roles: Vec::new(),
            evidence: Vec::new(),
            gates: GateLedger {
                same_user: true,
                strong_automation_provenance: true,
                confirmed_abandonment: true,
                isolated_session: true,
                stable_across_two_observations: true,
                process_identity_unchanged: true,
                no_protection_rule: true,
            },
            browser_compatibility: BrowserCompatibility::default(),
            targets: Vec::new(),
            runtime_artifacts: Vec::new(),
        }
    }

    fn roster(reports: Vec<IncidentReport>) -> RosterSnapshot {
        RosterSnapshot {
            cycle_token: Some("cycle".to_owned()),
            observed_at_unix_millis: Some(100),
            freshness: RosterFreshness::Current,
            reports,
        }
    }

    #[test]
    fn browser_phase_fails_unknown_before_applying_state_precedence() {
        let mut status = ready_status();
        status.latest_observation_at_unix_millis = Some(99);
        assert_eq!(
            browser_overview_phase(&status, &roster(vec![report(IncidentState::Confirmed)])),
            public::BrowserOverviewPhase::Unknown
        );

        let mut stale = roster(Vec::new());
        stale.freshness = RosterFreshness::StaleAfterFailure;
        assert_eq!(
            browser_overview_phase(&ready_status(), &stale),
            public::BrowserOverviewPhase::Unknown
        );
    }

    #[test]
    fn browser_phase_uses_one_server_owned_precedence_table() {
        let status = ready_status();
        assert_eq!(
            browser_overview_phase(&status, &roster(Vec::new())),
            public::BrowserOverviewPhase::Clear
        );
        assert_eq!(
            browser_overview_phase(&status, &roster(vec![report(IncidentState::Protected)])),
            public::BrowserOverviewPhase::Protected
        );
        assert_eq!(
            browser_overview_phase(
                &status,
                &roster(vec![
                    report(IncidentState::Active),
                    report(IncidentState::Protected),
                ]),
            ),
            public::BrowserOverviewPhase::Active
        );
        assert_eq!(
            browser_overview_phase(
                &status,
                &roster(vec![
                    report(IncidentState::Confirmed),
                    report(IncidentState::Cooling),
                ]),
            ),
            public::BrowserOverviewPhase::Confirmed
        );

        let mut reclaiming = ready_status();
        reclaiming.cleanup_in_progress = true;
        assert_eq!(
            browser_overview_phase(&reclaiming, &roster(vec![report(IncidentState::Confirmed)]),),
            public::BrowserOverviewPhase::Reclaiming
        );

        let mut attention = ready_status();
        attention.event_source_healthy = false;
        attention.cleanup_in_progress = true;
        assert_eq!(
            browser_overview_phase(&attention, &roster(Vec::new())),
            public::BrowserOverviewPhase::Attention
        );
        assert_eq!(
            browser_overview_phase(
                &status,
                &roster(vec![
                    report(IncidentState::Failed),
                    report(IncidentState::Reclaiming),
                ]),
            ),
            public::BrowserOverviewPhase::Attention
        );
    }
}
