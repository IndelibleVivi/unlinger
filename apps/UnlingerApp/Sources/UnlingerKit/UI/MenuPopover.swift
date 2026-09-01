import SwiftUI

/// Root of the menu-bar popover: status, attention, recent reclaim,
/// protections, history link, and status-level actions.
public struct MenuPopover: View {
    @Environment(AppState.self) private var state
    @Environment(AppRouter.self) private var router
    private let allowsWindowPresentation: Bool

    public init(allowsWindowPresentation: Bool = false) {
        self.allowsWindowPresentation = allowsWindowPresentation
    }

    public var body: some View {
        @Bindable var router = router
        VStack(spacing: 0) {
            if !router.path.isEmpty, allowsWindowPresentation {
                routeControls
                Divider()
            }

            NavigationStack(path: $router.path) {
                Group {
                    switch state.connection {
                    case .connecting:
                        VStack(spacing: 8) {
                            ProgressView()
                                .controlSize(.small)
                            Text(L10n.text("status.headline.activity"))
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, minHeight: 120)
                    case .unavailable:
                        unavailable
                    case .incompatibleDaemon:
                        incompatible
                    case .live:
                        live
                    }
                }
                .padding()
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .history:
                        HistoryView()
                    case .settings:
                        SettingsView()
                    case .incident(let incidentID):
                        IncidentDetailView(incidentID: incidentID)
                    }
                }
            }

            if router.path.isEmpty, allowsWindowPresentation {
                Divider()
                windowButton
                    .padding(.horizontal)
                    .padding(.vertical, 10)
            }
        }
        .frame(width: 340)
    }

    private var routeControls: some View {
        HStack(spacing: 12) {
            Button(L10n.text("navigation.back"), systemImage: "chevron.left") {
                router.goBack()
            }
            Spacer(minLength: 0)
            if allowsWindowPresentation {
                windowButton
            }
        }
        .controlSize(.small)
        .padding(.horizontal)
        .padding(.vertical, 10)
    }

    private var windowButton: some View {
        Button(L10n.text("navigation.open_window"), systemImage: "macwindow") {
            router.presentCurrentRoute()
        }
    }

    private var unavailable: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(L10n.text("unavailable.title"), systemImage: "circle.slash")
                .font(.headline)
            Text(L10n.text("unavailable.body"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Button(L10n.text("unavailable.retry")) {
                Task { await state.refresh() }
            }
            .padding(.top, 4)
        }
    }

    private var incompatible: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(L10n.text("incompatible.title"), systemImage: "exclamationmark.triangle")
                .font(.headline)
            Text(L10n.text("incompatible.body"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Button(L10n.text("unavailable.retry")) {
                Task { await state.refresh() }
            }
            .padding(.top, 4)
        }
    }

    private var live: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let viewModel = state.viewModel {
                    StatusSection(viewModel: viewModel)

                    if !state.currentIncidents.isEmpty {
                        Divider()
                        RosterSection(roster: state.observationRoster)
                    }

                    if !viewModel.attention.isEmpty {
                        Divider()
                        AttentionList(items: viewModel.attention, overflow: viewModel.attentionOverflow)
                    }

                    if let reclaim = viewModel.recentReclaim {
                        Divider()
                        reclaimRow(reclaim)
                    }

                    if !viewModel.protections.isEmpty {
                        Divider()
                        VStack(alignment: .leading, spacing: 6) {
                            Label(L10n.text("protection.count", viewModel.protections.count), systemImage: "hand.raised")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            ForEach(viewModel.protections.prefix(3)) { protection in
                                NavigationLink(value: Route.incident(protection.incidentId)) {
                                    HStack {
                                        Text(L10n.text("protection.item", Format.relativeTime(Date(unixMillis: protection.protectedAtUnixMillis))))
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                        Spacer(minLength: 0)
                                        Image(systemName: "chevron.right")
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                            .accessibilityHidden(true)
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }

                    Divider()
                    VStack(alignment: .leading, spacing: 2) {
                        NavigationLink(value: Route.history) {
                            Label(L10n.text("history.title"), systemImage: "clock")
                                .font(.subheadline)
                        }
                        Text(L10n.text("history.hint"))
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }

                    Divider()
                    ActionControls(capabilities: state.status?.capabilities)

                    Divider()
                    NavigationLink(value: Route.settings) {
                        Label(L10n.text("settings.title"), systemImage: "gearshape")
                            .font(.subheadline)
                    }
                }
            }
        }
        .scrollIndicators(.never)
        .frame(maxHeight: 480)
    }

    private func reclaimRow(_ reclaim: ReclaimViewData) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L10n.text("reclaim.section"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "arrow.down.circle")
                    .foregroundStyle(.secondary)
                    .font(.subheadline)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.text(reclaim.copyKey))
                        .font(.subheadline)
                    Text(Format.relativeTime(reclaim.occurredAt))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }
}
