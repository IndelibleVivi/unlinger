import SwiftUI

public struct BrowserCoverageSection: View {
    let notices: [BrowserCoverageNotice]

    public init(notices: [BrowserCoverageNotice]) {
        self.notices = notices
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label(L10n.text("browser.coverage.title"), systemImage: "shield.lefthalf.filled")
                .font(.caption)
                .foregroundStyle(.tertiary)
            ForEach(notices, id: \.self) { notice in
                Text(L10n.text(notice.copyKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
