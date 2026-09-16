import AppKit
import SwiftUI

public struct SettingsView: View {
    @Environment(AppSettings.self) private var settings

    public init() {}

    public var body: some View {
        Form {
            Section(L10n.text("settings.notifications")) {
                NotificationModeFields(
                    mode: settings.notificationMode,
                    authorization: settings.notificationAuthorization,
                    language: LanguageSettings.shared.preference
                )
                .equatable()
            }

            Section(L10n.text("settings.startup")) {
                Toggle(L10n.text("settings.launch_at_login"), isOn: launchAtLogin)
                if let statusKey = loginStatusKey {
                    Text(L10n.text(statusKey))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section(L10n.text("settings.about")) {
                LabeledContent(L10n.text("settings.app_version"), value: appVersion)
                DaemonVersionRow()
            }

            Section {
                Text(L10n.text("settings.quit_daemon_continues"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button(L10n.text("action.quit"), role: .destructive) {
                    NSApplication.shared.terminate(nil)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(L10n.text("settings.title"))
        .frame(minWidth: 310)
        .task {
            await settings.refreshLaunchAtLogin()
        }
    }

    private var launchAtLogin: Binding<Bool> {
        Binding(
            get: { settings.launchAtLoginRequested },
            set: { enabled in
                Task { await settings.setLaunchAtLogin(enabled) }
            }
        )
    }

    private var loginStatusKey: String? {
        switch settings.launchAtLoginStatus {
        case .enabled, .disabled: nil
        case .requiresApproval: "settings.login_requires_approval"
        case .notFound: "settings.login_not_found"
        case .failed: "settings.login_failed"
        case .unknown: "settings.login_unknown"
        }
    }

    private var appVersion: String {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
        let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String
        return switch (version, build) {
        case let (version?, build?) where version != build: "\(version) (\(build))"
        case let (version?, _): version
        case let (_, build?): build
        default: "development"
        }
    }
}

private struct NotificationModeFields: View, Equatable {
    @Environment(AppSettings.self) private var settings

    let mode: NotificationMode
    let authorization: NotificationAuthorization
    let language: LanguageSettings.Preference

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.mode == rhs.mode
            && lhs.authorization == rhs.authorization
            && lhs.language == rhs.language
    }

    var body: some View {
        LabeledContent(L10n.text("settings.notification_mode")) {
            StablePopUpButton(
                style: .selection,
                title: L10n.text("settings.notification_mode"),
                accessibilityLabel: L10n.text("settings.notification_mode"),
                selectedID: mode.rawValue,
                items: [
                    StablePopUpItem(
                        id: NotificationMode.off.rawValue,
                        title: L10n.text("settings.notification.off")
                    ),
                    StablePopUpItem(
                        id: NotificationMode.attention.rawValue,
                        title: L10n.text("settings.notification.attention")
                    ),
                    StablePopUpItem(
                        id: NotificationMode.attentionAndReclaims.rawValue,
                        title: L10n.text("settings.notification.reclaims")
                    )
                ]
            ) { identifier in
                guard let mode = NotificationMode(rawValue: identifier) else { return }
                settings.setNotificationMode(mode)
            }
            .fixedSize()
        }
        if authorization == .denied {
            Text(L10n.text("settings.notifications_denied"))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}

private struct DaemonVersionRow: View {
    @Environment(AppState.self) private var state

    var body: some View {
        LabeledContent(
            L10n.text("settings.daemon_version"),
            value: state.status?.daemonVersion ?? L10n.text("settings.not_connected")
        )
    }
}
