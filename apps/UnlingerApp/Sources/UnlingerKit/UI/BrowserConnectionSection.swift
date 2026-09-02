import SwiftUI

public struct BrowserConnectionSection: View {
    @Environment(AppState.self) private var state
    let connection: ConnectionState

    public init(connection: ConnectionState) {
        self.connection = connection
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch connection {
            case .connecting:
                ProgressView()
                    .controlSize(.small)
            case .unavailable:
                Text(L10n.text("unavailable.body"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Button(L10n.text("unavailable.retry"), action: retry)
            case .incompatibleDaemon:
                Text(L10n.text("incompatible.body"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Button(L10n.text("unavailable.retry"), action: retry)
            case .live:
                EmptyView()
            }
        }
    }

    private func retry() {
        Task { await state.refresh() }
    }
}
