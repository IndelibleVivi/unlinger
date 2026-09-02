import SwiftUI

public struct BrowserHomeView: View {
    @Environment(AppState.self) private var state

    public init() {}

    public var body: some View {
        let overview = state.browserOverview
        let sections = overview.visibleSections(connection: state.connection)
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                ForEach(Array(sections.enumerated()), id: \.element) { index, section in
                    if index > 0 { Divider() }
                    sectionView(section, overview: overview)
                }
            }
        }
        .scrollIndicators(.never)
        .frame(maxHeight: 520)
    }

    @ViewBuilder
    private func sectionView(
        _ section: BrowserPopoverSection,
        overview: BrowserOverview
    ) -> some View {
        switch section {
        case .overview:
            BrowserOverviewSection(overview: overview)
        case .connection:
            BrowserConnectionSection(connection: state.connection)
        case .sessions:
            BrowserSessionsSection(sessions: overview.sessions)
        case .coverage:
            BrowserCoverageSection(notices: overview.coverageNotices)
        case .savedProtections:
            SavedProtectionsSection(protections: overview.savedProtections)
        case .attention:
            AttentionList(items: overview.attention, overflow: overview.attentionOverflow)
        case .recentSettlement:
            if let settlement = overview.recentSettlement {
                RecentSettlementSection(settlement: settlement)
            }
        case .history:
            NavigationLink(value: Route.history) {
                VStack(alignment: .leading, spacing: 2) {
                    Label(L10n.text("browser.history.title"), systemImage: "clock")
                        .font(.subheadline)
                    Text(L10n.text("browser.history.hint"))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
            .buttonStyle(.plain)
        case .settings:
            NavigationLink(value: Route.settings) {
                Label(L10n.text("settings.title"), systemImage: "gearshape")
                    .font(.subheadline)
            }
            .buttonStyle(.plain)
        case .actions:
            ActionControls(capabilities: state.status?.capabilities)
        }
    }
}
