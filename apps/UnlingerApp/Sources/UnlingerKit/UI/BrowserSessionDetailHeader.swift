import SwiftUI

public struct BrowserSessionDetailHeader: View {
    let session: BrowserSessionPresentation

    public init(session: BrowserSessionPresentation) {
        self.session = session
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(L10n.text("browser.detail.title"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            Text(L10n.text(session.familyKey))
                .font(.headline)
            Text(L10n.text(session.stateKey))
                .font(.subheadline)
            Text(L10n.text(
                "browser.session.metrics",
                session.memberCount,
                Format.bytes(session.residentMemoryBytes)
            ))
            .font(.caption)
            .foregroundStyle(.secondary)
            if let reasonKey = session.reasonKey {
                Text(L10n.text(reasonKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if session.isPreviousObservation {
                Text(L10n.text("browser.session.previous_observation"))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(12)
        .background(Color.secondary.opacity(0.065), in: .rect(cornerRadius: 12))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(BrowserProductCopy.sessionAccessibilityLabel(session))
    }
}
