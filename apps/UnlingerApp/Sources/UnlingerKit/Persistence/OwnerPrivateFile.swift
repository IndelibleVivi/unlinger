import Foundation

enum OwnerPrivateFileError: Error, Equatable, Sendable {
    case unsafePath
    case ioFailure
}

/// Crash-durable, owner-private JSON/file primitive for App-local authority.
/// New bytes are created at 0600, fsynced, atomically renamed, then the parent
/// directory is fsynced. Existing targets are accepted only when they are an
/// exact regular file owned by the effective user with mode 0600.
enum OwnerPrivateFile {
    static func read(from fileURL: URL) throws -> Data? {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return nil }
        try verifyFile(fileURL)
        do {
            return try Data(contentsOf: fileURL, options: [.uncached])
        } catch {
            throw OwnerPrivateFileError.ioFailure
        }
    }

    static func write(_ data: Data, to fileURL: URL) throws {
        let directory = fileURL.deletingLastPathComponent()
        try prepareDirectory(directory)
        if FileManager.default.fileExists(atPath: fileURL.path) {
            try verifyFile(fileURL)
        }

        let temporaryURL = directory.appendingPathComponent(
            ".\(fileURL.lastPathComponent).\(UUID().uuidString.lowercased()).tmp",
            isDirectory: false
        )
        let fd = open(temporaryURL.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
        guard fd >= 0 else { throw OwnerPrivateFileError.ioFailure }
        var keepTemporary = true
        defer {
            close(fd)
            if keepTemporary { unlink(temporaryURL.path) }
        }

        var offset = 0
        let wroteAll = data.withUnsafeBytes { bytes -> Bool in
            guard let base = bytes.baseAddress else { return data.isEmpty }
            while offset < bytes.count {
                let written = Darwin.write(fd, base + offset, bytes.count - offset)
                if written < 0, errno == EINTR { continue }
                guard written > 0 else { return false }
                offset += written
            }
            return true
        }
        guard wroteAll, fsync(fd) == 0 else {
            throw OwnerPrivateFileError.ioFailure
        }
        guard rename(temporaryURL.path, fileURL.path) == 0 else {
            throw OwnerPrivateFileError.ioFailure
        }
        keepTemporary = false
        try syncDirectory(directory)
        try verifyFile(fileURL)
    }

    static func remove(_ fileURL: URL) throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        try verifyFile(fileURL)
        guard unlink(fileURL.path) == 0 else { throw OwnerPrivateFileError.ioFailure }
        try syncDirectory(fileURL.deletingLastPathComponent())
    }

    private static func prepareDirectory(_ directory: URL) throws {
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: NSNumber(value: Int16(0o700))]
            )
        } catch {
            throw OwnerPrivateFileError.ioFailure
        }

        var info = stat()
        guard lstat(directory.path, &info) == 0,
              (info.st_mode & S_IFMT) == S_IFDIR,
              info.st_uid == geteuid()
        else {
            throw OwnerPrivateFileError.unsafePath
        }
        guard chmod(directory.path, 0o700) == 0 else {
            throw OwnerPrivateFileError.ioFailure
        }
    }

    private static func verifyFile(_ fileURL: URL) throws {
        var info = stat()
        guard lstat(fileURL.path, &info) == 0,
              (info.st_mode & S_IFMT) == S_IFREG,
              info.st_uid == geteuid(),
              (info.st_mode & 0o777) == 0o600
        else {
            throw OwnerPrivateFileError.unsafePath
        }
    }

    private static func syncDirectory(_ directory: URL) throws {
        let fd = open(directory.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard fd >= 0 else { throw OwnerPrivateFileError.ioFailure }
        defer { close(fd) }
        guard fsync(fd) == 0 else { throw OwnerPrivateFileError.ioFailure }
    }
}
