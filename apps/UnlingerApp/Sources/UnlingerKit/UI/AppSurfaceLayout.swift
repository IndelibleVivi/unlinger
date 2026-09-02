import CoreGraphics

/// Both AppKit hosts render the same SwiftUI tree. Keeping one content size
/// prevents the hosted tree from extending beyond either window's clip rect.
public enum AppSurfaceLayout {
    public static let contentSize = CGSize(width: 340, height: 420)
}
