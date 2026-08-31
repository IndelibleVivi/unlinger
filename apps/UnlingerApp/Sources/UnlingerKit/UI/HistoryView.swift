import SwiftUI

/// Recent events from `history { limit }`. Cleanup rows lead with their
/// overall outcome; `cleared_with_residue` is phrased as a process success.
public struct HistoryView: View {
    @Environment(AppState.self) private var state

    public init() {}

    public var body: some View {
        Group {
            if state.history.isEmpty {
                Text(L10n.text("history.empty"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(state.history) { event in
                    NavigationLink(value: Route.incident(event.incidentId)) {
                        HistoryRow(event: event)
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle(L10n.text("history.title"))
        .frame(minHeight: 240)
    }
}

struct HistoryRow: View {
    let event: HistoryEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(label)
                    .font(.subheadline)
                Spacer()
                if let bytes = reclaimedBytes {
                    Text(Format.bytes(bytes))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Text(Format.relativeTime(Date(unixMillis: event.occurredAtUnixMillis)))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 2)
    }

    private var label: String {
        switch event.payload {
        case .cleanup(let receipt):
            OutcomeCopy.label(for: receipt.overallOutcome)
        case .observation(let record):
            if record.state == .protected {
                L10n.text("outcome.protected")
            } else {
                OutcomeCopy.label(for: event.state)
            }
        case .unknown:
            OutcomeCopy.label(for: event.state)
        }
    }

    private var reclaimedBytes: UInt64? {
        guard case .cleanup(let receipt) = event.payload else { return nil }
        return receipt.resources?.estimatedReclaimedMemoryBytes
    }
}
