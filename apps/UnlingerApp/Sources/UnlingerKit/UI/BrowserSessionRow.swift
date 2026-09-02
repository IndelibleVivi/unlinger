import SwiftUI

public struct BrowserSessionRow: View {
    let session: BrowserSessionPresentation

    public init(session: BrowserSessionPresentation) {
        self.session = session
    }

    public var body: some View {
        NavigationLink(value: Route.incident(session.incidentID)) {
            HStack(alignment: .firstTextBaseline, spacing: 9) {
                Image(systemName: symbolName)
                    .foregroundStyle(symbolColor)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(L10n.text(session.familyKey))
                        .font(.subheadline)
                    Text(L10n.text(
                        "browser.session.summary",
                        L10n.text(session.stateKey),
                        session.memberCount,
                        Format.bytes(session.residentMemoryBytes)
                    ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    if let reasonKey = session.reasonKey {
                        Text(L10n.text(reasonKey))
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if session.isPreviousObservation {
                        Text(L10n.text("browser.session.previous_observation"))
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(BrowserProductCopy.sessionAccessibilityLabel(session))
        .accessibilityAddTraits(.isButton)
    }

    private var symbolName: String {
        switch session.state {
        case .active: "play.circle"
        case .cooling: "hourglass.circle"
        case .confirmed: "exclamationmark.circle"
        case .reclaiming: "arrow.triangle.2.circlepath"
        case .protected: "hand.raised.circle"
        case .ambiguous: "questionmark.circle"
        case .cleared: "checkmark.circle"
        case .revived: "arrow.uturn.forward.circle"
        case .failed: "exclamationmark.circle"
        case .unknown: "questionmark.circle"
        }
    }

    private var symbolColor: Color {
        switch session.state {
        case .cooling, .confirmed, .reclaiming: .accentColor
        case .ambiguous, .revived, .failed, .unknown: .orange
        case .active, .protected, .cleared: .secondary
        }
    }
}
