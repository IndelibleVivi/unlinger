import ApplicationServices
import Foundation

private enum ProbeFailure: Error, CustomStringConvertible {
    case usage
    case noWindow
    case attribute(String, AXError)
    case malformedAttribute(String)
    case oversizedTree

    var description: String {
        switch self {
        case .usage:
            "usage: accessibility-tree-probe <pid> [duration-seconds]"
        case .noWindow:
            "target window is unavailable"
        case .attribute(let name, let error):
            "Accessibility attribute \(name) failed with AXError \(error.rawValue)"
        case .malformedAttribute(let name):
            "Accessibility attribute \(name) returned an unexpected value"
        case .oversizedTree:
            "Accessibility tree exceeded the 10000-node safety bound"
        }
    }
}

private func elements(
    of element: AXUIElement,
    attribute: CFString
) throws -> [AXUIElement] {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, attribute, &value)
    if result == .noValue || result == .attributeUnsupported {
        return []
    }
    guard result == .success else {
        throw ProbeFailure.attribute(attribute as String, result)
    }
    guard let children = value as? [AXUIElement] else {
        throw ProbeFailure.malformedAttribute(attribute as String)
    }
    return children
}

private func treeCount(window: AXUIElement) throws -> Int {
    var count = 0
    var pending = [window]
    while let element = pending.popLast() {
        count += 1
        guard count <= 10_000 else {
            throw ProbeFailure.oversizedTree
        }
        pending.append(contentsOf: try elements(
            of: element,
            attribute: kAXChildrenAttribute as CFString
        ))
    }
    return count
}

private func emit(_ value: String, to handle: FileHandle) {
    handle.write(Data("\(value)\n".utf8))
}

private func run() throws {
    guard (2 ... 3).contains(CommandLine.arguments.count),
          let rawPID = Int32(CommandLine.arguments[1]),
          rawPID > 0
    else {
        throw ProbeFailure.usage
    }

    let application = AXUIElementCreateApplication(pid_t(rawPID))
    AXUIElementSetMessagingTimeout(application, 2.0)
    let windows = try elements(of: application, attribute: kAXWindowsAttribute as CFString)
    guard let window = windows.first else {
        throw ProbeFailure.noWindow
    }

    guard CommandLine.arguments.count == 3 else {
        emit(String(try treeCount(window: window)), to: .standardOutput)
        return
    }
    guard let durationSeconds = Double(CommandLine.arguments[2]),
          durationSeconds > 0
    else {
        throw ProbeFailure.usage
    }

    let deadline = ProcessInfo.processInfo.systemUptime + durationSeconds
    var consecutiveFailures = 0
    while ProcessInfo.processInfo.systemUptime < deadline {
        do {
            emit(String(try treeCount(window: window)), to: .standardOutput)
            consecutiveFailures = 0
        } catch {
            emit(String(describing: error), to: .standardError)
            consecutiveFailures += 1
            if consecutiveFailures >= 4 {
                throw error
            }
        }
        Thread.sleep(forTimeInterval: 0.25)
    }
}

do {
    try run()
} catch {
    emit(String(describing: error), to: .standardError)
    exit(1)
}
