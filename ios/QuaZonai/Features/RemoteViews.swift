import SwiftUI

struct RemoteDetailView: View {
    @EnvironmentObject private var session: SessionStore
    let title: String
    let path: String
    let cacheKey: String
    @State private var value: JSONValue?
    @State private var error: String?

    var body: some View {
        ScrollView {
            Group {
                if let value { JSONDocumentView(value: value) }
                else if let error { ContentUnavailableView(L10n.text(.error, session.language), systemImage: "exclamationmark.triangle", description: Text(error)) }
                else { ProgressView(L10n.text(.loading, session.language)) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .navigationTitle(title)
        .task { await reload() }
        .refreshable { await reload() }
    }

    private func reload() async {
        do { value = try await session.load(path: path, cacheKey: cacheKey) ; error = nil }
        catch { self.error = error.localizedDescription }
    }
}

struct RemoteCollectionView: View {
    @EnvironmentObject private var session: SessionStore
    let title: String
    let path: String
    let cacheKey: String
    let detailPath: ((String) -> String)?
    @State private var value: JSONValue?
    @State private var error: String?
    @State private var query = ""
    @State private var ascending = true

    private var items: [JSONValue] {
        let source = value?.normalizedItems ?? []
        return source
            .filter { query.isEmpty || $0.searchableText.localizedCaseInsensitiveContains(query) }
            .sorted { ascending ? $0.listTitle.localizedCompare($1.listTitle) == .orderedAscending : $0.listTitle.localizedCompare($1.listTitle) == .orderedDescending }
    }

    var body: some View {
        List {
            if let error {
                ContentUnavailableView(L10n.text(.error, session.language), systemImage: "exclamationmark.triangle", description: Text(error))
            } else if value == nil {
                ProgressView(L10n.text(.loading, session.language))
            } else if items.isEmpty {
                ContentUnavailableView(L10n.text(.empty, session.language), systemImage: "tray")
            } else {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    if let id = item.stableID, let detailPath {
                        NavigationLink {
                            RemoteDetailView(title: item.listTitle, path: detailPath(id), cacheKey: "\(cacheKey)-\(id)")
                        } label: { RecordRow(item: item) }
                    } else {
                        NavigationLink { ScrollView { JSONDocumentView(value: item).padding() } } label: { RecordRow(item: item) }
                    }
                }
            }
        }
        .navigationTitle(title)
        .searchable(text: $query, prompt: L10n.text(.search, session.language))
        .toolbar {
            Button { ascending.toggle() } label: { Label(L10n.text(.sort, session.language), systemImage: ascending ? "arrow.up" : "arrow.down") }
        }
        .task { await reload() }
        .refreshable { await reload() }
    }

    private func reload() async {
        do { value = try await session.load(path: path, cacheKey: cacheKey); error = nil }
        catch { self.error = error.localizedDescription }
    }
}

struct RecordRow: View {
    let item: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(item.listTitle).font(.headline).lineLimit(2)
            if let object = item.objectValue {
                HStack(spacing: 8) {
                    if let state = object.string("state") { Text(state).font(.caption).foregroundStyle(.secondary) }
                    if let purpose = object.string("purpose") { Text(purpose).font(.caption).foregroundStyle(.secondary) }
                    if let created = object.string("created_at") { Text(created).font(.caption2).foregroundStyle(.tertiary).lineLimit(1) }
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}
