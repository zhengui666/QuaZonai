import SwiftData
import SwiftUI

@main
struct QuaZonaiApp: App {
    @StateObject private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase
    private let container: ModelContainer

    init() {
        do {
            container = try ModelContainer(for: CachedResource.self, IdeaDraft.self, EventCursor.self)
        } catch {
            fatalError("Unable to initialize the isolated SwiftData cache: \(error)")
        }
    }

    var body: some Scene {
        WindowGroup {
            ContextBridgeView()
                .environmentObject(model)
                .environment(\.locale, Locale(identifier: model.localeIdentifier))
                .environment(
                    \.layoutDirection,
                    model.localeIdentifier.hasPrefix("ar") ? .rightToLeft : .leftToRight
                )
                .preferredColorScheme(model.appearance.colorScheme)
                .overlay {
                    if scenePhase != .active {
                        PrivacyCoverView()
                    }
                }
                .task {
                    let arguments = ProcessInfo.processInfo.arguments
                    if let index = arguments.firstIndex(of: "--server-url"),
                       arguments.indices.contains(index + 1)
                    {
                        model.serverInput = arguments[index + 1]
                    }
                    if arguments.contains("--ui-testing") && !model.serverInput.isEmpty {
                        await model.connect()
                    }
                }
                .onChange(of: scenePhase) { _, phase in
                    Task {
                        if phase == .active {
                            await model.appWillEnterForeground()
                        } else {
                            await model.appDidEnterBackground()
                        }
                    }
                }
        }
        .modelContainer(container)
    }
}

private struct ContextBridgeView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext

    var body: some View {
        RootView()
            .task { model.attach(modelContext: modelContext) }
    }
}

private struct PrivacyCoverView: View {
    var body: some View {
        ZStack {
            Rectangle().fill(.ultraThickMaterial).ignoresSafeArea()
            VStack(spacing: 14) {
                Image(systemName: "lock.shield.fill")
                    .font(.system(size: 44, weight: .semibold))
                    .accessibilityHidden(true)
                Text("privacy.cover")
                    .font(.headline)
                    .multilineTextAlignment(.center)
            }
            .padding(32)
        }
        .accessibilityElement(children: .combine)
    }
}
