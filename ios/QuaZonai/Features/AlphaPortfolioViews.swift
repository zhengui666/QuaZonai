import SwiftUI

struct AlphaLibraryView: View {
    var body: some View {
        RemoteCollectionView(
            title: "Alpha Library",
            path: "/api/v1/alpha-library",
            cacheKey: "alpha-library",
            detailPath: { "/api/v1/alpha-library/\($0)" }
        )
    }
}

struct PortfolioLabView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var mandates: [JSONValue] = []
    @State private var programs: [JSONValue] = []
    @State private var query = ""
    @State private var error: String?

    private var filteredPrograms: [JSONValue] {
        programs.filter { query.isEmpty || $0.searchableText.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        List {
            if let error { Text(error).foregroundStyle(.red) }
            Section("Mandates") {
                if mandates.isEmpty { Text(L10n.text(.empty, session.language)).foregroundStyle(.secondary) }
                ForEach(Array(mandates.enumerated()), id: \.offset) { _, mandate in
                    NavigationLink { ScrollView { JSONDocumentView(value: mandate).padding() }.navigationTitle(mandate.listTitle) } label: { RecordRow(item: mandate) }
                }
            }
            Section("Portfolio Programs / Candidates") {
                if programs.isEmpty { Text(L10n.text(.empty, session.language)).foregroundStyle(.secondary) }
                ForEach(Array(filteredPrograms.enumerated()), id: \.offset) { _, program in
                    VStack(alignment: .leading, spacing: 8) {
                        RecordRow(item: program)
                        if let candidateID = program.objectValue?.string("current_candidate_id"), !candidateID.isEmpty {
                            NavigationLink("Open current candidate") {
                                RemoteDetailView(
                                    title: "Candidate \(candidateID.prefix(8))",
                                    path: "/api/v1/portfolio-candidates/\(candidateID)",
                                    cacheKey: "candidate-\(candidateID)"
                                )
                            }
                            .font(.subheadline)
                        }
                        DisclosureGroup("All program fields") { JSONTreeView(value: program) }
                    }
                }
            }
        }
        .navigationTitle(L10n.text(.portfolio, session.language))
        .searchable(text: $query, prompt: L10n.text(.search, session.language))
        .task { await reload() }
        .refreshable { await reload() }
    }

    private func reload() async {
        do {
            async let m = session.load(path: "/api/v1/portfolio-mandates", cacheKey: "portfolio-mandates")
            async let p = session.load(path: "/api/v1/portfolio-programs", cacheKey: "portfolio-programs")
            let values = try await (m, p)
            mandates = values.0.normalizedItems
            programs = values.1.normalizedItems
            error = nil
        } catch { self.error = error.localizedDescription }
    }
}
