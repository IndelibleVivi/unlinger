import Foundation

struct GateCheckPresentation: Identifiable {
    let copyKey: String
    let passed: Bool

    var id: String { copyKey }
}
