import SwiftUI

/// Status-level actions: pause presets and resume, gated purely by backend
/// capabilities. Also hosts the mutation-state banner so confirmation and
/// delivery-uncertainty are visible wherever the action was started.
public struct ActionControls: View {
    @Environment(AppState.self) private var state
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
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.text("actions.hint"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            HStack(spacing: 12) {
                pauseMenu
                resumeButton
                Spacer(minLength: 0)
                languageMenu
            }
            MutationBanner()
        }
    }

    /// In-app UI language override. Not a backend capability — display only.
    private var languageMenu: some View {
        Menu {
            ForEach(LanguageSettings.Preference.allCases, id: \.self) { preference in
                Button {
                    LanguageSettings.shared.preference = preference
                } label: {
                    if LanguageSettings.shared.preference == preference {
                        Label(L10n.text(preference.copyKey), systemImage: "checkmark")
                    } else {
                        Text(L10n.text(preference.copyKey))
                    }
                }
            }
        } label: {
            Label(L10n.text("language.menu"), systemImage: "globe")
                .labelStyle(.iconOnly)
        }
        .help(L10n.text("language.menu"))
    }

    private var pauseMenu: some View {
        Menu {
            ForEach(Self.pausePresets, id: \.millis) { preset in
                Button(L10n.text(preset.key)) {
                    let label = L10n.text(preset.key)
                    Task {
                        await state.perform(.pause(durationMillis: preset.millis, label: label))
                    }
                }
            }
        } label: {
            Text(L10n.text("action.pause"))
        }
        .disabled(!(capabilities?.pause.available ?? false))
        .help(capabilities?.pause.available == false
              ? CapabilityCopy.unavailableReason(capabilities?.pause.unavailableReasonId)
              : "")
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
        case .uncertain:
            VStack(alignment: .leading, spacing: 6) {
                Label(L10n.text("mutation.uncertain.title"), systemImage: "questionmark.circle")
                    .font(.subheadline)
                Text(L10n.text("mutation.uncertain.body"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button(L10n.text("mutation.dismiss")) {
                    state.dismissMutationState()
                }
                .font(.caption)
            }
            .padding(8)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
        case .failed:
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
