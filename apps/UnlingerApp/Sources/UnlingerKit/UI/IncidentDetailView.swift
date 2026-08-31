import SwiftUI

/// One incident's retained timeline from `explain`, with the incident-level
/// capability actions (retry / protect / unprotect / export) the backend
/// explicitly offers for it.
public struct IncidentDetailView: View {
    @Environment(AppState.self) private var state
    let incidentID: String

    @State private var detail: IncidentDetail?
    @State private var loadFailed = false
    @State private var exported = false

    public init(incidentID: String) {
        self.incidentID = incidentID
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let detail {
                    ForEach(detail.events) { event in
                        EventCard(event: event)
                    }
                    Divider()
                    actions(detail.capabilities)
                    MutationBanner()
                } else if loadFailed {
                    Text(L10n.text("detail.not_found"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    ProgressView()
                        .controlSize(.small)
                        .frame(maxWidth: .infinity, minHeight: 120)
                }
            }
            .padding()
        }
        .scrollIndicators(.never)
        .task { await reload() }
        // A mutation confirmed from the shared banner (e.g. explicit retry)
        // also leaves capabilities stale; reload on confirmation.
        .onChange(of: state.mutationState) { _, new in
            if case .confirmed = new {
                Task { await reload() }
            }
        }
    }

    private func reload() async {
        do {
            detail = try await state.explain(incidentID: incidentID)
            loadFailed = false
        } catch {
            loadFailed = true
        }
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
                await state.perform(.exportDiagnostics(incidentID: incidentID))
                if let export = state.lastExport, DiagnosticsExporter.export(export) {
                    exported = true
                }
            }
            if exported {
                Text(L10n.text("action.exported"))
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

struct EventCard: View {
    let event: HistoryEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(OutcomeCopy.label(for: event.state))
                    .font(.subheadline.weight(.medium))
                Spacer()
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
        if let memberCount = record.memberCount {
            Text(L10n.text("detail.members", memberCount,
                           Format.bytes(record.residentMemoryBytes ?? 0)))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if let roles = record.roles, !roles.isEmpty {
            Text(roles.map { "\(OutcomeCopy.roleLabel($0.role)) × \($0.count)" }.joined(separator: ", "))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if let gates = record.gates {
            GateLedgerView(gates: gates)
        }
    }

    @ViewBuilder
    private func cleanupBody(_ receipt: CleanupReceipt) -> some View {
        // Show the outcome line only when it adds a fact the state label
        // doesn't — e.g. state FAILED with overall cleared_with_residue is the
        // dual truth "processes cleared, residue kept" and must not read as a
        // process failure.
        let outcomeLabel = OutcomeCopy.label(for: receipt.overallOutcome)
        if outcomeLabel != OutcomeCopy.label(for: event.state) {
            Text(outcomeLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if let resources = receipt.resources,
           let reclaimed = resources.estimatedReclaimedMemoryBytes, reclaimed > 0
        {
            Text(L10n.text("detail.resources.reclaimed", Format.bytes(reclaimed)))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if !receipt.processActions.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(receipt.processActions, id: \.stage) { action in
                    Text(actionLabel(signal: action.signal, disposition: action.disposition))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                ForEach(receipt.artifactActions, id: \.kind) { action in
                    Text(artifactLabel(disposition: action.disposition))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func actionLabel(signal: String, disposition: String) -> String {
        let base = switch signal {
        case "term": L10n.text("detail.action.term")
        case "kill": L10n.text("detail.action.kill")
        default: signal
        }
        return switch disposition {
        case "delivered": base
        case "delivery_unknown": L10n.text("detail.action.delivery_unknown", base)
        default: L10n.text("detail.action.not_delivered", base)
        }
    }

    private func artifactLabel(disposition: String) -> String {
        switch disposition {
        case "removed": L10n.text("detail.action.artifact.removed")
        default: L10n.text("detail.action.artifact.kept")
        }
    }
}

struct GateLedgerView: View {
    let gates: GateLedger

    var body: some View {
        let entries: [(String, Bool)] = [
            ("same_user", gates.sameUser),
            ("abandoned", gates.confirmedAbandonment),
            ("isolated", gates.isolatedSession),
            ("stable", gates.stableAcrossTwoObservations),
            ("identity", gates.processIdentityUnchanged),
            ("provenance", gates.strongAutomationProvenance)
        ].compactMap { name, value in value.map { (name, $0) } }

        if !entries.isEmpty {
            HStack(spacing: 6) {
                ForEach(entries, id: \.0) { name, passed in
                    Image(systemName: passed ? "checkmark.circle" : "xmark.circle")
                        .foregroundStyle(passed ? Color.secondary : Color.orange)
                        .help("\(name): \(passed ? L10n.text("detail.gate.passed") : L10n.text("detail.gate.blocked"))")
                }
            }
            .font(.caption)
        }
    }
}
