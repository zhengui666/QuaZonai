import SwiftData
import SwiftUI

@main
struct QuaZonaiApp: App {
    @StateObject private var session = SessionStore()
    @Environment(\.scenePhase) private var scenePhase

    private let modelContainer: ModelContainer = {
        let schema = Schema([CachedPayload.self, IdeaDraft.self, EventCursor.self])
        do { return try ModelContainer(for: schema) }
        catch { fatalError("Unable to initialize the local read cache: \(error)") }
    }()

    var body: some Scene {
        WindowGroup {
            ZStack {
                RootView()
                    .environmentObject(session)
                    .environment(\.locale, Locale(identifier: session.language.rawValue))
                    .environment(\.layoutDirection, session.language.isRTL ? .rightToLeft : .leftToRight)
                    .preferredColorScheme(session.appearance.colorScheme)
                if session.privacyCovered {
                    Rectangle().fill(.background).ignoresSafeArea().overlay(Image(systemName: "lock.shield").font(.largeTitle).accessibilityLabel("QuaZonai is hidden while in the background"))
                }
            }
            .onChange(of: scenePhase) { _, phase in session.privacyCovered = phase != .active }
        }
        .modelContainer(modelContainer)
    }
}
