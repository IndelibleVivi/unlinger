import SwiftUI

/// Status-level actions: pause presets and resume, gated purely by backend
/// capabilities. Also hosts the mutation-state banner so confirmation and
/// delivery-uncertainty are visible wherever the action was started.
public struct ActionControls: View {
    let capabilities: GlobalCapabilities?

    static let pausePresets: [(key: String, millis: UInt64)] = [
        ("action.pause.2h", 7_200_000),
        ("action.pause.8h", 28_800_000),
        ("action.pause.24h", 86_400_000)
    ]

    public init(capabilities: GlobalCapabilities?) {
        self.capabilities = capabilities
    }

    public var body: some View {
        let language = LanguageSettings.shared.preference
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.text("actions.hint"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            ActionBarControls(capabilities: capabilities, language: language)
                .equatable()
            MutationBanner()
        }
    }
}

private struct ActionBarControls: View, Equatable {
    @Environment(AppState.self) private var state

    let capabilities: GlobalCapabilities?
    let language: LanguageSettings.Preference

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.capabilities == rhs.capabilities && lhs.language == rhs.language
    }

    var body: some View {
        HStack(spacing: 12) {
            pauseMenu
            resumeButton
            Spacer(minLength: 0)
            languageMenu
        }
    }

    /// In-app UI language override. Not a backend capability — display only.
    private var languageMenu: some View {
        StablePopUpButton(
            style: .action,
            title: L10n.text("language.menu"),
            systemImageName: "globe",
            showsTitle: false,
            accessibilityLabel: L10n.text("language.menu"),
            toolTip: L10n.text("language.menu"),
            items: LanguageSettings.Preference.allCases.map { preference in
                StablePopUpItem(
                    id: preference.rawValue,
                    title: L10n.text(preference.copyKey),
                    isMarked: language == preference
                )
            }
        ) { identifier in
            guard let preference = LanguageSettings.Preference(rawValue: identifier) else {
                return
            }
            LanguageSettings.shared.setPreference(preference)
        }
        .fixedSize()
    }

    private var pauseMenu: some View {
        StablePopUpButton(
            style: .action,
            title: L10n.text("action.pause"),
            accessibilityLabel: L10n.text("action.pause"),
            toolTip: capabilities?.pause.available == false
                ? CapabilityCopy.unavailableReason(capabilities?.pause.unavailableReasonId)
                : "",
            isEnabled: capabilities?.pause.available ?? false,
            items: ActionControls.pausePresets.map { preset in
                StablePopUpItem(id: String(preset.millis), title: L10n.text(preset.key))
            }
        ) { identifier in
            guard let preset = ActionControls.pausePresets.first(where: {
                String($0.millis) == identifier
            }) else {
                return
            }
            let label = L10n.text(preset.key)
            Task {
                await state.perform(.pause(durationMillis: preset.millis, label: label))
            }
        }
        .fixedSize()
    }

    private var resumeButton: some View {
        Button(L10n.text("action.resume")) {
            Task { await state.perform(.resume) }
        }
        .disabled(!(capabilities?.resume.available ?? false))
        .help(capabilities?.resume.available == false
              ? CapabilityCopy.unavailableReason(capabilities?.resume.unavailableReasonId)
              : "")
    }
}

/// Reflects mutation progress/results. An uncertain delivery never exposes a
/// shortcut that could bypass the freshly read backend capability state; the
/// user returns to the ordinary capability-gated action surface instead.
public struct MutationBanner: View {
    @Environment(AppState.self) private var state

    public init() {}

    public var body: some View {
        switch state.mutationState {
        case .idle:
            EmptyView()
        case .inFlight:
            Label(L10n.text("mutation.in_flight"), systemImage: "ellipsis.circle")
                .font(.caption)
                .foregroundStyle(.secondary)
        case .confirmed:
            Label(L10n.text("mutation.confirmed"), systemImage: "checkmark.circle")
                .font(.caption)
                .foregroundStyle(.secondary)
        case .unresolved(let pending):
            VStack(alignment: .leading, spacing: 6) {
                Label(L10n.text("mutation.uncertain.title"), systemImage: "questionmark.circle")
                    .font(.subheadline)
                Text(L10n.text("mutation.uncertain.body"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    Button(L10n.text("mutation.check_again")) {
                        Task { await state.checkAgain(mutationID: pending.mutationID) }
                    }
                    Button(L10n.text("mutation.dismiss")) {
                        state.dismissMutationState()
                    }
                }
                .font(.caption)
            }
            .padding(8)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
        case .authorityLost:
            HStack(spacing: 12) {
                Label(L10n.text("mutation.authority_lost"), systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.orange)
                Button(L10n.text("mutation.dismiss")) {
                    state.dismissMutationState()
                }
                .font(.caption)
            }
        case .definitelyNotApplied:
            HStack(spacing: 12) {
                Label(L10n.text("mutation.not_applied"), systemImage: "minus.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button(L10n.text("mutation.dismiss")) {
                    state.dismissMutationState()
                }
                .font(.caption)
            }
        case .rejected, .failedBeforeSend:
            HStack(spacing: 12) {
                Label(L10n.text("mutation.failed"), systemImage: "xmark.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button(L10n.text("mutation.dismiss")) {
                    state.dismissMutationState()
                }
                .font(.caption)
            }
        }
    }
}
