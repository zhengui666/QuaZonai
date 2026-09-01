import SwiftUI

struct ResearchListView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var programs: [JSONValue] = []
    @State private var query = ""
    @State private var ascending = false
    @State private var error: String?

    private var visible: [JSONValue] {
        programs.filter { query.isEmpty || $0.searchableText.localizedCaseInsensitiveContains(query) }
            .sorted { ascending ? $0.listTitle < $1.listTitle : $0.listTitle > $1.listTitle }
    }

    var body: some View {
        List {
            if let error { Text(error).foregroundStyle(.red) }
            if programs.isEmpty && error == nil { ContentUnavailableView(L10n.text(.empty, session.language), systemImage: "tray") }
            ForEach(Array(visible.enumerated()), id: \.offset) { _, program in
                if let id = program.stableID {
                    NavigationLink { ResearchDetailView(programID: id) } label: { RecordRow(item: program) }
                }
            }
        }
        .navigationTitle(L10n.text(.research, session.language))
        .searchable(text: $query, prompt: L10n.text(.search, session.language))
        .toolbar { Button { ascending.toggle() } label: { Image(systemName: ascending ? "arrow.up" : "arrow.down") } }
        .task { await reload() }.refreshable { await reload() }
    }

    private func reload() async {
        do { programs = try await session.load(path: "/api/v1/research-programs", cacheKey: "programs").normalizedItems; error = nil }
        catch { self.error = error.localizedDescription }
    }
}

struct ResearchDetailView: View {
    @EnvironmentObject private var session: SessionStore
    let programID: String
    @State private var program: JSONValue?
    @State private var missions: JSONValue?
    @State private var activity: JSONValue?
    @State private var reason = ""
    @State private var error: String?
    @State private var runningAction: String?
    @State private var mutationSubmission = MutationSubmission()

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let program {
                    GroupBox("Frozen Charter / Program") { JSONDocumentView(value: program) }
                    actionPanel(program)
                }
                if let missions { GroupBox("Mission DAG") { JSONDocumentView(value: missions) } }
                if let activity { GroupBox("Experiment, evidence & agent activity") { JSONDocumentView(value: activity) } }
                if let error { Text(error).foregroundStyle(.red) }
            }.padding()
        }
        .navigationTitle("Program \(programID.prefix(8))")
        .task { await reload() }.refreshable { await reload() }
    }

    @ViewBuilder private func actionPanel(_ program: JSONValue) -> some View {
        let state = program.objectValue?.string("state") ?? ""
        GroupBox("Program administration") {
            VStack(alignment: .leading, spacing: 10) {
                TextField("Reason for pause/archive", text: $reason)
                HStack {
                    if state != "PAUSED" && state != "ARCHIVED" { Button("Pause") { Task { await act("pause", requiresReason: true) } } }
                    if state == "PAUSED" { Button("Resume") { Task { await act("resume", requiresReason: false) } } }
                    if state != "ARCHIVED" { Button("Archive") { Task { await act("archive", requiresReason: true) } } }
                    if state == "ARCHIVED" { Button("Restore") { Task { await act("restore", requiresReason: false) } } }
                }
                .buttonStyle(.bordered)
                .disabled(runningAction != nil)
            }
        }
    }

    private func act(_ action: String, requiresReason: Bool) async {
        if requiresReason && reason.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { error = "A reason is required."; return }
        runningAction = action; defer { runningAction = nil }
        let body: JSONValue = requiresReason ? .object(["reason": .string(reason.trimmingCharacters(in: .whitespacesAndNewlines))]) : .object([:])
        do {
            _ = try await session.mutate(
                path: "/api/v1/research-programs/\(programID)/\(action)",
                body: body,
                submission: mutationSubmission
            )
            mutationSubmission = MutationSubmission()
            reason = ""
            await reload()
        }
        catch { self.error = error.localizedDescription }
    }

    private func reload() async {
        do {
            async let p = session.load(path: "/api/v1/research-programs/\(programID)", cacheKey: "program-\(programID)")
            async let m = session.load(path: "/api/v1/research-programs/\(programID)/missions", cacheKey: "missions-\(programID)")
            async let a = session.load(path: "/api/v1/research-programs/\(programID)/activity", cacheKey: "activity-\(programID)")
            let values = try await (p, m, a); program = values.0; missions = values.1; activity = values.2; error = nil
        } catch { self.error = error.localizedDescription }
    }
}
