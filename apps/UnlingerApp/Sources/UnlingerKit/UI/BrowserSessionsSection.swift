import SwiftUI

public struct BrowserSessionsSection: View {
    let sessions: [BrowserSessionPresentation]

    public init(sessions: [BrowserSessionPresentation]) {
        self.sessions = sessions
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            Label(L10n.text("browser.sessions.title"), systemImage: "safari")
                .font(.caption)
                .foregroundStyle(.tertiary)
            LazyVStack(alignment: .leading, spacing: 10) {
                ForEach(sessions) { session in
                    BrowserSessionRow(session: session)
                }
            }
        }
    }
}
