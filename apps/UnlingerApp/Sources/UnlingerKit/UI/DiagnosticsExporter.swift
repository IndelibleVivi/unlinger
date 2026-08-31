import AppKit
import Darwin
import Foundation

/// Writes a diagnostics bundle to a user-chosen private file (0600, current
/// user only). The save panel's own replacement prompt is the explicit
/// overwrite consent; nothing is written automatically.
@MainActor
public enum DiagnosticsExporter {
    public static func export(_ export: DiagnosticsExport) -> Bool {
        guard let rawJSON = export.rawJSON else { return false }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = L10n.text("export.filename")
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let url = panel.url else { return false }
        return writePrivateAtomically(rawJSON, to: url)
    }

    /// Writes beside the target with owner-only permissions, then atomically
    /// replaces the selected pathname. A failed write therefore leaves an
    /// existing export intact.
    nonisolated static func writePrivateAtomically(_ data: Data, to url: URL) -> Bool {
        let directory = url.deletingLastPathComponent()
        let temporaryURL = directory.appendingPathComponent(
            ".\(url.lastPathComponent).\(UUID().uuidString).tmp"
        )
        let temporaryPath = temporaryURL.path(percentEncoded: false)
        let targetPath = url.path(percentEncoded: false)

        let descriptor = open(
            temporaryPath,
            O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW,
            S_IRUSR | S_IWUSR
        )
        guard descriptor >= 0 else { return false }

        var isOpen = true
        defer {
            if isOpen {
                close(descriptor)
            }
            unlink(temporaryPath)
        }

        let wroteAllBytes = data.withUnsafeBytes { bytes in
            var offset = 0
            while offset < bytes.count {
                let written = Darwin.write(
                    descriptor,
                    bytes.baseAddress!.advanced(by: offset),
                    bytes.count - offset
                )
                if written < 0 {
                    if errno == EINTR { continue }
                    return false
                }
                offset += written
            }
            return true
        }

        guard wroteAllBytes, fsync(descriptor) == 0 else { return false }
        let closeResult = close(descriptor)
        isOpen = false
        guard closeResult == 0 else { return false }

        return temporaryPath.withCString { temporaryPointer in
            targetPath.withCString { targetPointer in
                rename(temporaryPointer, targetPointer) == 0
            }
        }
    }
}
