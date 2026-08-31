import SwiftUI

/// Fixture-driven previews: every documented state renders from the canonical
/// `Contract/v2` wire truth, never hand-built view data.

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

#Preview("All clear (report-only)") {
    MenuPopover()
        .environment(previewState("status-all-clear"))
}

#Preview("Needs attention") {
    MenuPopover()
        .environment(previewState(
            "status-needs-attention",
            historyFixture: "history-cleared-with-residue",
            incidentFixture: "incident-failed",
            incidentsFixture: "incidents-current"
        ))
}

#Preview("Current roster") {
    MenuPopover()
        .environment(previewState(
            "status-report-only",
            incidentFixture: "incident-protected",
            incidentsFixture: "incidents-current"
        ))
}

#Preview("Paused") {
    MenuPopover()
        .environment(previewState("status-paused"))
}

#Preview("Recently reclaimed") {
    MenuPopover()
        .environment(previewState("status-recently-reclaimed", historyFixture: "history-cleared"))
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
    MenuPopover()
        .environment(previewState(
            "status-all-clear",
            mutation: .pause(durationMillis: 7_200_000, label: "2h")
        ))
}
