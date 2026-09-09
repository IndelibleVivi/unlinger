/// A presentation of the daemon's explicit attribution reason. The App does
/// not infer cleanup authority from an empty action list or process count.
extension CleanupReceipt {
    var endedWithoutIntervention: Bool {
        state == .cleared
            && processOutcome == .cleared
            && overallOutcome == .cleared
            && artifactOutcome == .notApplicable && reasonId == "cleanup.tree_gone_without_signal"
    }
}
