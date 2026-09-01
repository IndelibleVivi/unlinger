import SwiftUI

/// Fixture-driven previews: every documented state renders from the canonical
/// `Contract/v3` wire truth, never hand-built view data.

@MainActor
private func previewState(
    _ statusFixture: String,
    historyFixture: String? = nil,
    incidentFixture: String? = nil,
    incidentsFixture: String? = nil,
    mutation: Mutation? = nil
) -> AppState {
    let state = AppState(client: FixtureClient(
        statusFixture: statusFixture,
        historyFixture: historyFixture,
        incidentFixture: incidentFixture,
        incidentsFixture: incidentsFixture,
        mutationResults: mutation == nil ? [] : [.deliveryUncertain]
    ))
    state.startPolling()
    if let mutation {
        Task { await state.perform(mutation) }
    }
    return state
}

@MainActor
private func previewRoot(_ state: AppState) -> some View {
    MenuPopover()
        .environment(state)
        .environment(AppRouter())
        .environment(AppSettings())
}

#Preview("All clear (report-only)") {
    previewRoot(previewState("status-all-clear"))
}

#Preview("Needs attention") {
    previewRoot(previewState(
            "status-needs-attention",
            historyFixture: "history-cleared-with-residue",
            incidentFixture: "incident-failed",
            incidentsFixture: "incidents-current"
        ))
}

#Preview("Current roster") {
    previewRoot(previewState(
            "status-report-only",
            incidentFixture: "incident-protected",
            incidentsFixture: "incidents-current"
        ))
}

#Preview("Paused") {
    previewRoot(previewState("status-paused"))
}

#Preview("Recently reclaimed") {
    previewRoot(previewState("status-recently-reclaimed", historyFixture: "history-cleared"))
}

#Preview("History") {
    NavigationStack {
        HistoryView()
    }
    .environment(previewState("status-recently-reclaimed", historyFixture: "history-cleared"))
    .frame(width: 340)
}

#Preview("Incident: protected") {
    NavigationStack {
        IncidentDetailView(incidentID: "redacted-incident-1")
    }
    .environment(AppState(client: FixtureClient(
        statusFixture: "status-report-only",
        incidentFixture: "incident-protected"
    )))
    .frame(width: 340)
}

#Preview("Mutation delivery uncertain") {
    previewRoot(previewState(
            "status-all-clear",
            mutation: .pause(durationMillis: 7_200_000, label: "2h")
        ))
}
