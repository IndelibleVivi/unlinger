import SwiftUI

public struct BrowserSessionDetailHeader: View {
    let session: BrowserSessionPresentation

    public init(session: BrowserSessionPresentation) {
        self.session = session
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.text(
                session.isPreviousObservation
                    ? "browser.detail.recorded_session"
                    : "browser.detail.current_session"
            ))
                .font(.caption)
                .foregroundStyle(.tertiary)
            Text(L10n.text(session.familyKey))
                .font(.headline)
            if let identity = BrowserProductCopy.browserIdentity(
                productKey: session.productKey,
                observedVersion: session.observedVersion
            ) {
                Text(identity)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Text(L10n.text(session.stateKey))
                .font(.subheadline)
                .bold()
            if let reasonKey = session.reasonKey {
                Text(L10n.text(reasonKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Divider()
            Text(L10n.text(
                "browser.session.metrics",
                session.memberCount,
                Format.bytes(session.residentMemoryBytes)
            ))
            .font(.caption)
            .foregroundStyle(.secondary)
            if session.isPreviousObservation {
                Text(L10n.text("browser.session.previous_observation"))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(0.065), in: .rect(cornerRadius: 12))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(BrowserProductCopy.sessionAccessibilityLabel(session))
    }
}
