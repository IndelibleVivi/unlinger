import SwiftUI

/// One incident's retained timeline from `explain`, with the incident-level
/// capability actions (retry / protect / unprotect / export) the backend
/// explicitly offers for it.
public struct IncidentDetailView: View {
    @Environment(AppState.self) private var state
    let incidentID: String

    @State private var loadState: DetailLoadState = .loading
    @State private var exported = false
    @State private var exportFailed = false
    @State private var loadGeneration: UInt64 = 0

    private enum DetailLoadState {
        case loading
        case loaded(IncidentDetail, staleError: IncidentDetailFailure?)
        case notFound
        case unavailable
        case incompatible
        case localHistoryUnavailable
        case protocolFailure
    }

    public init(incidentID: String) {
        self.incidentID = incidentID
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                switch loadState {
                case .loaded(let detail, let staleError):
                    let timeline = BrowserHistoryMapper.timelineEntries(events: detail.events)
                    if let staleError {
                        staleBanner(staleError)
                    }
                    let overview = state.browserOverview
                    if let summary = BrowserOverviewMapper.detailPresentation(
                        incidentID: incidentID,
                        currentSessions: overview.sessions,
                        events: detail.events,
                        mode: state.status?.effectiveMode
                    ) {
                        BrowserSessionDetailHeader(session: summary)
                    } else if let settlement = overview.recentSettlement,
                              settlement.incidentID == incidentID
                    {
                        BrowserSettlementDetailHeader(settlement: settlement)
                    }
                    Text(L10n.text("browser.detail.timeline"))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Text(L10n.text("browser.detail.timeline.hint"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    ForEach(timeline) { entry in
                        EventCard(entry: entry)
                    }
                    Divider()
                    Text(L10n.text("browser.detail.actions"))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    actions(detail.capabilities)
                    MutationBanner()
                case .notFound:
                    Text(L10n.text("detail.not_found"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                case .unavailable:
                    Text(L10n.text("detail.unavailable"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    retryButton
                case .incompatible:
                    Text(L10n.text("detail.incompatible"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                case .localHistoryUnavailable:
                    Text(L10n.text("detail.local_history_unavailable"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    retryButton
                case .protocolFailure:
                    Text(L10n.text("detail.protocol_failure"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    retryButton
                case .loading:
                    ProgressView()
                        .controlSize(.small)
                        .frame(maxWidth: .infinity, minHeight: 120)
                }
            }
            .padding()
        }
        .scrollIndicators(.never)
        .navigationTitle(L10n.text("browser.detail.title"))
        .task { await reload() }
        // A mutation confirmed from the shared banner (e.g. explicit retry)
        // also leaves capabilities stale; reload on confirmation.
        .onChange(of: state.mutationState) { _, new in
            if case .confirmed(_, let receipt) = new,
               receipt.affectsIncident(incidentID)
            {
                Task { await reload() }
            }
        }
    }

    private func reload() async {
        loadGeneration &+= 1
        let generation = loadGeneration
        let previousDetail: IncidentDetail? = if case .loaded(let detail, _) = loadState {
            detail
        } else {
            nil
        }
        if previousDetail == nil { loadState = .loading }
        do {
            let detail = try await state.explain(incidentID: incidentID)
            guard generation == loadGeneration, !Task.isCancelled else { return }
            loadState = .loaded(detail, staleError: nil)
        } catch let error {
            guard generation == loadGeneration, !Task.isCancelled else { return }
            let failure = IncidentDetailFailure.classify(error)
            if failure == .notFound {
                loadState = .notFound
            } else if let previousDetail {
                loadState = .loaded(previousDetail, staleError: failure)
            } else {
                loadState = switch failure {
                case .notFound: .notFound
                case .unavailable: .unavailable
                case .incompatible: .incompatible
                case .localHistoryUnavailable: .localHistoryUnavailable
                case .protocolFailure: .protocolFailure
                }
            }
        }
    }

    private var retryButton: some View {
        Button(L10n.text("detail.retry")) {
            Task { await reload() }
        }
    }

    private func staleBanner(_ failure: IncidentDetailFailure) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(L10n.text("detail.stale"))
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            retryButton
                .font(.caption)
        }
        .help(L10n.text(failure.copyKey))
    }

    @ViewBuilder
    private func actions(_ capabilities: IncidentCapabilities) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            capabilityButton(
                L10n.text("action.retry"),
                capability: capabilities.retryFailedCleanup
            ) {
                await state.perform(.retryFailedCleanup(incidentID: incidentID))
                await reload()
            }
            capabilityButton(
                L10n.text("action.protect"),
                capability: capabilities.protectIncident
            ) {
                await state.perform(.protect(incidentID: incidentID))
                await reload()
            }
            capabilityButton(
                L10n.text("action.unprotect"),
                capability: capabilities.unprotectIncident
            ) {
                await state.perform(.unprotect(incidentID: incidentID))
                await reload()
            }
            capabilityButton(
                L10n.text("action.export"),
                capability: capabilities.exportDiagnostics
            ) {
                exported = false
                exportFailed = false
                do {
                    let export = try await state.exportDiagnostics(incidentID: incidentID)
                    exported = DiagnosticsExporter.export(export)
                    exportFailed = !exported
                } catch {
                    exported = false
                    exportFailed = true
                }
            }
            if exported {
                Text(L10n.text("action.exported"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if exportFailed {
                Text(L10n.text("action.export_failed"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func capabilityButton(
        _ title: String,
        capability: Capability,
        action: @escaping @MainActor () async -> Void
    ) -> some View {
        Button(title) {
            Task { await action() }
        }
        .disabled(!capability.available)
        .help(capability.available ? "" : CapabilityCopy.unavailableReason(capability.unavailableReasonId))
    }
}

public enum IncidentDetailFailure: Equatable, Sendable {
    case notFound
    case unavailable
    case incompatible
    case localHistoryUnavailable
    case protocolFailure

    public static func classify(_ error: ClientError) -> IncidentDetailFailure {
        switch error {
        case .serverError(let code, _) where code == "not_found": .notFound
        case .serverError(let code, _) where code == "store_error": .localHistoryUnavailable
        case .unavailable: .unavailable
        case .incompatibleDaemon: .incompatible
        default: .protocolFailure
        }
    }

    var copyKey: String {
        switch self {
        case .notFound: "detail.not_found"
        case .unavailable: "detail.unavailable"
        case .incompatible: "detail.incompatible"
        case .localHistoryUnavailable: "detail.local_history_unavailable"
        case .protocolFailure: "detail.protocol_failure"
        }
    }
}

private extension MutationReceipt {
    func affectsIncident(_ incidentID: String) -> Bool {
        guard case .applied(let result) = outcome else { return false }
        return switch result {
        case .retryScheduled(let affected): affected == incidentID
        case .incidentProtected(let protection): protection.incidentId == incidentID
        case .incidentUnprotected(let affected): affected == incidentID
        case .paused, .resumed, .unknown: false
        }
    }
}

struct EventCard: View {
    let entry: BrowserTimelineEntryPresentation

    private var event: HistoryEvent { entry.latestEvent }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(OutcomeCopy.label(for: event))
                    .font(.subheadline.weight(.medium))
                Spacer()
                if entry.eventCount > 1 {
                    Text(L10n.text("detail.observations_grouped", entry.eventCount))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text(Format.shortTime(Date(unixMillis: event.occurredAtUnixMillis)))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            switch event.payload {
            case .observation(let record):
                observationBody(record)
            case .cleanup(let receipt):
                cleanupBody(receipt)
            case .unknown:
                EmptyView()
            }
        }
        .padding(10)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
    }

    @ViewBuilder
    private func observationBody(_ record: ObservationRecord) -> some View {
        Text(L10n.text("detail.members", record.memberCount,
                       Format.bytes(record.residentMemoryBytes)))
            .font(.caption)
            .foregroundStyle(.secondary)
        if !record.roles.isEmpty {
            Text(record.roles.map { "\(OutcomeCopy.roleLabel($0.role)) × \($0.count)" }.joined(separator: ", "))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        GateLedgerView(gates: record.gates)
    }

    @ViewBuilder
    private func cleanupBody(_ receipt: CleanupReceipt) -> some View {
        // Show the outcome line only when it adds a fact the state label
        // doesn't — e.g. state FAILED with overall cleared_with_residue is the
        // dual truth "processes cleared, residue kept" and must not read as a
        // process failure.
        let outcomeLabel = OutcomeCopy.label(for: receipt.overallOutcome)
        if receipt.endedWithoutIntervention {
            Text(L10n.text("detail.cleanup.without_intervention"))
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if outcomeLabel != OutcomeCopy.label(for: event.state) {
            Text(outcomeLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if !receipt.endedWithoutIntervention,
           let reclaimed = receipt.resources.estimatedReclaimedMemoryBytes, reclaimed > 0
        {
            Text(L10n.text("detail.resources.reclaimed", Format.bytes(reclaimed)))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if !receipt.processActions.isEmpty || !receipt.artifactActions.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(receipt.processActions) { action in
                    Text(actionLabel(signal: action.signal, disposition: action.disposition))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                ForEach(receipt.artifactActions) { action in
                    Text(artifactLabel(disposition: action.disposition))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func actionLabel(signal: CleanupSignal, disposition: SignalDisposition) -> String {
        let base = switch signal {
        case .term: L10n.text("detail.action.term")
        case .kill: L10n.text("detail.action.kill")
        case .unknown(let raw): raw
        }
        return switch disposition {
        case .delivered: base
        case .deliveryUnknown: L10n.text("detail.action.delivery_unknown", base)
        default: L10n.text("detail.action.not_delivered", base)
        }
    }

    private func artifactLabel(disposition: ArtifactDisposition) -> String {
        switch disposition {
        case .removed: L10n.text("detail.action.artifact.removed")
        default: L10n.text("detail.action.artifact.kept")
        }
    }
}

struct GateLedgerView: View {
    let gates: GateLedger

    var body: some View {
        let entries = [
            GateCheckPresentation(copyKey: "detail.gate.same_user", passed: gates.sameUser),
            GateCheckPresentation(copyKey: "detail.gate.abandoned", passed: gates.confirmedAbandonment),
            GateCheckPresentation(copyKey: "detail.gate.isolated", passed: gates.isolatedSession),
            GateCheckPresentation(copyKey: "detail.gate.stable", passed: gates.stableAcrossTwoObservations),
            GateCheckPresentation(copyKey: "detail.gate.identity", passed: gates.processIdentityUnchanged),
            GateCheckPresentation(copyKey: "detail.gate.provenance", passed: gates.strongAutomationProvenance),
            GateCheckPresentation(copyKey: "detail.gate.unprotected", passed: gates.noProtectionRule)
        ]
        let passedCount = entries.filter(\.passed).count

        DisclosureGroup(L10n.text("detail.gates.summary", passedCount, entries.count)) {
            VStack(alignment: .leading, spacing: 5) {
                ForEach(entries, id: \GateCheckPresentation.id) { entry in
                    HStack(spacing: 6) {
                        Image(systemName: entry.passed ? "checkmark.circle" : "xmark.circle")
                            .foregroundStyle(entry.passed ? Color.secondary : Color.orange)
                            .accessibilityHidden(true)
                        Text(L10n.text(entry.copyKey))
                        Spacer(minLength: 8)
                        Text(L10n.text(entry.passed ? "detail.gate.passed" : "detail.gate.blocked"))
                            .foregroundStyle(entry.passed ? Color.secondary : Color.orange)
                    }
                }
            }
            .padding(.top, 5)
        }
        .font(.caption)
    }
}
