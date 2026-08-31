import SwiftData
import SwiftUI

@MainActor
final class ResourceLoader: ObservableObject {
    @Published var items: [JSONValue] = []
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var loadedFromCache = false

    let endpoint: String
    let cacheKey: String

    init(endpoint: String, cacheKey: String) {
        self.endpoint = endpoint
        self.cacheKey = cacheKey
    }

    func load(model: AppModel, context: ModelContext) async {
        isLoading = items.isEmpty
        errorMessage = nil
        defer { isLoading = false }
        guard let profile = model.profile else {
            errorMessage = APIError.invalidServer.localizedDescription
            return
        }
        do {
            let response = try await model.request(path: endpoint)
            items = response.arrayValue ?? [response]
            try OfflineCache.store(response, profileID: profile.id, key: cacheKey, context: context)
            loadedFromCache = false
        } catch {
            if let cached = try? OfflineCache.resource(
                profileID: profile.id,
                key: cacheKey,
                context: context
            ) {
                items = cached.arrayValue ?? [cached]
                loadedFromCache = true
                errorMessage = nil
            } else {
                errorMessage = error.localizedDescription
            }
        }
    }
}

@MainActor
final class ResourceDetailLoader: ObservableObject {
    @Published var value: JSONValue?
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var loadedFromCache = false

    let endpoint: String
    let cacheKey: String

    init(endpoint: String, cacheKey: String) {
        self.endpoint = endpoint
        self.cacheKey = cacheKey
    }

    func load(model: AppModel, context: ModelContext) async {
        isLoading = value == nil
        errorMessage = nil
        defer { isLoading = false }
        guard let profile = model.profile else {
            errorMessage = APIError.invalidServer.localizedDescription
            return
        }
        do {
            let response = try await model.request(path: endpoint)
            value = response
            try OfflineCache.store(response, profileID: profile.id, key: cacheKey, context: context)
            loadedFromCache = false
        } catch {
            if let cached = try? OfflineCache.resource(
                profileID: profile.id,
                key: cacheKey,
                context: context
            ) {
                value = cached
                loadedFromCache = true
            } else {
                errorMessage = error.localizedDescription
            }
        }
    }
}

struct LoadingStateView: View {
    var body: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text("common.loading").foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
    }
}

struct ErrorStateView: View {
    let message: String
    let retry: (() -> Void)?

    var body: some View {
        ContentUnavailableView {
            Label("common.error", systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            if let retry {
                Button("common.retry", action: retry)
            }
        }
    }
}

struct EmptyStateView: View {
    let title: String
    let description: String

    var body: some View {
        ContentUnavailableView(title, systemImage: "tray", description: Text(description))
    }
}

struct OfflineReadOnlyBanner: View {
    var body: some View {
        Label("offline.readOnly", systemImage: "icloud.slash")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12))
            .accessibilityElement(children: .combine)
    }
}

struct DirectAccessBanner: View {
    var body: some View {
        Label("auth.direct", systemImage: "exclamationmark.shield")
            .font(.footnote)
            .foregroundStyle(.orange)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
            .accessibilityElement(children: .combine)
    }
}

struct StreamStatusBadge: View {
    let status: EventStreamStatus

    var body: some View {
        switch status {
        case .connected:
            Label("home.streamConnected", systemImage: "dot.radiowaves.left.and.right")
                .foregroundStyle(.green)
        case .reconnecting:
            Label("home.streamReconnecting", systemImage: "arrow.triangle.2.circlepath")
                .foregroundStyle(.orange)
        case .disconnected:
            Label("common.offline", systemImage: "wifi.slash")
                .foregroundStyle(.secondary)
        }
    }
}

struct MetricCard: View {
    let title: String
    let value: String
    var systemImage: String = "chart.bar"

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: systemImage)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title2.monospacedDigit().weight(.semibold))
                .lineLimit(2)
                .minimumScaleFactor(0.7)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .combine)
    }
}

struct StatePill: View {
    let state: String

    var body: some View {
        Text(state.replacingOccurrences(of: "_", with: " ").capitalized)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(.quaternary, in: Capsule())
            .accessibilityLabel(Text("State: \(state)"))
    }
}

struct ResourceRow: View {
    let value: JSONValue

    private var title: String {
        value.firstString(for: ["name", "title", "kind", "plugin_id", "universe_key", "id"])
            ?? value.displayText
    }

    private var subtitle: String? {
        value.firstString(for: ["state", "role", "purpose", "environment_type", "provider"])
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.headline).textSelection(.enabled)
            if let subtitle {
                StatePill(state: subtitle)
            }
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .combine)
    }
}

struct JSONInspector: View {
    let value: JSONValue
    var depth = 0

    var body: some View {
        switch value {
        case let .object(object):
            VStack(alignment: .leading, spacing: 8) {
                ForEach(object.keys.sorted(), id: \.self) { key in
                    let child = object[key] ?? .null
                    if child.objectValue != nil || child.arrayValue != nil {
                        DisclosureGroup {
                            JSONInspector(value: child, depth: depth + 1)
                                .padding(.leading, 8)
                        } label: {
                            Text(key.replacingOccurrences(of: "_", with: " ").capitalized)
                                .font(.subheadline.weight(.semibold))
                        }
                    } else {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(key.replacingOccurrences(of: "_", with: " ").capitalized)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Spacer(minLength: 8)
                            Text(child.displayText)
                                .font(.body.monospacedDigit())
                                .multilineTextAlignment(.trailing)
                                .textSelection(.enabled)
                        }
                        .accessibilityElement(children: .combine)
                    }
                }
            }
        case let .array(array):
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(array.enumerated()), id: \.offset) { index, child in
                    DisclosureGroup("Item \(index + 1)") {
                        JSONInspector(value: child, depth: depth + 1)
                    }
                }
            }
        default:
            Text(value.displayText).textSelection(.enabled)
        }
    }
}

struct ResourceDetailScreen: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader: ResourceDetailLoader
    let title: String
    let headerValue: JSONValue?

    init(title: String, endpoint: String, cacheKey: String, headerValue: JSONValue? = nil) {
        self.title = title
        self.headerValue = headerValue
        _loader = StateObject(wrappedValue: ResourceDetailLoader(endpoint: endpoint, cacheKey: cacheKey))
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.value == nil {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.value == nil {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else if let value = loader.value ?? headerValue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        if loader.loadedFromCache { OfflineReadOnlyBanner() }
                        JSONInspector(value: value)
                    }
                    .padding()
                }
            } else {
                EmptyStateView(title: title, description: "No resource data is available.")
            }
        }
        .navigationTitle(title)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { await loader.load(model: model, context: modelContext) }
                } label: {
                    Label("common.refresh", systemImage: "arrow.clockwise")
                }
            }
        }
        .task { await loader.load(model: model, context: modelContext) }
    }
}

struct ResourceCollectionScreen<Detail: View>: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader: ResourceLoader
    @State private var searchText = ""
    @State private var ascending = true

    let title: String
    let emptyDescription: String
    @ViewBuilder let detail: (JSONValue) -> Detail

    init(
        title: String,
        endpoint: String,
        cacheKey: String,
        emptyDescription: String,
        @ViewBuilder detail: @escaping (JSONValue) -> Detail
    ) {
        self.title = title
        self.emptyDescription = emptyDescription
        self.detail = detail
        _loader = StateObject(wrappedValue: ResourceLoader(endpoint: endpoint, cacheKey: cacheKey))
    }

    private var visibleItems: [JSONValue] {
        let filtered = searchText.isEmpty
            ? loader.items
            : loader.items.filter {
                $0.searchableText.localizedStandardContains(searchText)
            }
        return filtered.sorted {
            ascending
                ? $0.searchableText.localizedStandardCompare($1.searchableText) == .orderedAscending
                : $0.searchableText.localizedStandardCompare($1.searchableText) == .orderedDescending
        }
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.items.isEmpty {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.items.isEmpty {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else if visibleItems.isEmpty {
                EmptyStateView(title: title, description: emptyDescription)
            } else {
                List {
                    if loader.loadedFromCache {
                        OfflineReadOnlyBanner().listRowSeparator(.hidden)
                    }
                    ForEach(Array(visibleItems.enumerated()), id: \.offset) { _, item in
                        NavigationLink {
                            detail(item)
                        } label: {
                            ResourceRow(value: item)
                        }
                    }
                }
                .listStyle(.insetGrouped)
            }
        }
        .navigationTitle(title)
        .searchable(text: $searchText, prompt: Text("common.search"))
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button("Ascending") { ascending = true }
                    Button("Descending") { ascending = false }
                } label: {
                    Label("common.sort", systemImage: "arrow.up.arrow.down")
                }
            }
        }
        .refreshable { await loader.load(model: model, context: modelContext) }
        .task { await loader.load(model: model, context: modelContext) }
    }
}

extension JSONValue {
    var resourceID: String? { firstString(for: ["id"]) }
    var resourceState: String? { firstString(for: ["state"]) }
}
