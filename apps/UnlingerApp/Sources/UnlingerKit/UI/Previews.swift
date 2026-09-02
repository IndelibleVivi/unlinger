import SwiftUI

/// Fixture-driven previews: every documented state renders from the canonical
/// `Contract/v3` wire truth, never hand-built view data.

@MainActor
private func previewState(
    _ scenario: String
) -> AppState {
    let state = AppState(client: FixtureClient.scenario(scenario))
    state.startPolling()
    return state
}

@MainActor
private func previewRoot(_ state: AppState) -> some View {
    MenuPopover()
        .environment(state)
        .environment(AppRouter())
        .environment(AppSettings())
}

@MainActor
private func previewMutationState() -> AppState {
    let state = AppState(client: FixtureClient.scenario("delivery-uncertain"))
    state.startPolling()
    Task {
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
    }
    return state
}

#Preview("Browser: clear") {
    previewRoot(previewState("browser-clear"))
}

#Preview("Browser: active") {
    previewRoot(previewState("browser-active"))
}

#Preview("Browser: verifying") {
    previewRoot(previewState("browser-verifying"))
}

#Preview("Browser: confirmed (report-only)") {
    previewRoot(previewState("browser-confirmed-report-only"))
}

#Preview("Browser: reclaiming") {
    previewRoot(previewState("browser-reclaiming"))
}

#Preview("Browser: protected unsupported") {
    previewRoot(previewState("browser-protected-unsupported"))
}

#Preview("Browser: attention") {
    previewRoot(previewState("browser-attention"))
}

#Preview("Browser: recent settlement") {
    previewRoot(previewState("browser-recent-settlement"))
}

#Preview("History") {
    NavigationStack {
        HistoryView()
    }
    .environment(previewState("browser-recent-settlement"))
    .frame(width: 360)
}

#Preview("Incident: protected") {
    NavigationStack {
        IncidentDetailView(incidentID: "redacted-incident-1")
    }
    .environment(AppState(client: FixtureClient(
        statusFixture: "status-report-only",
        incidentFixture: "incident-protected"
    )))
    .frame(width: 360)
}

#Preview("Mutation delivery uncertain") {
    previewRoot(previewMutationState())
}
