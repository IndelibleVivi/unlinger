// Entry point: scene type must be chosen before SwiftUI builds the scene
// graph, so dispatch between two App structs here instead of branching
// inside `body` (SceneBuilder does not support control flow).
if LaunchMode.windowed {
    WindowedUnlingerApp.main()
} else {
    MenuBarUnlingerApp.main()
}
