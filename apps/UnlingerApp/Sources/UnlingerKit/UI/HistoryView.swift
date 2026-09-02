import SwiftUI

/// Incident-centric history index. Repeated observation events are summarized
/// once here and remain available on the incident timeline.
public struct HistoryView: View {
    @Environment(AppState.self) private var state

    public init() {}

    public var body: some View {
        let overview = state.browserOverview
        let entries = BrowserHistoryMapper.entries(
            events: state.history,
            currentSessions: overview.sessions,
            recentSettlement: overview.recentSettlement,
            mode: state.status?.effectiveMode
        )
        Group {
            if entries.isEmpty {
                ContentUnavailableView(
                    L10n.text("history.empty"),
                    systemImage: "clock",
                    description: Text(L10n.text("history.description"))
                )
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        Text(L10n.text("history.description"))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.bottom, 8)
                        ForEach(Array(entries.enumerated()), id: \.element.id) { index, entry in
                            NavigationLink(value: Route.incident(entry.incidentID)) {
                                BrowserHistoryRow(entry: entry)
                            }
                            .buttonStyle(.plain)
                            if index < entries.count - 1 {
                                Divider()
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                }
                .scrollIndicators(.never)
            }
        }
        .navigationTitle(L10n.text("history.title"))
        .frame(minHeight: 240)
    }
}
