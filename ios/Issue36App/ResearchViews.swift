import SwiftData
import SwiftUI

// MARK: - Home

@MainActor
private final class HomeLoader: ObservableObject {
    @Published var readiness: JSONValue?
    @Published var health: JSONValue?
    @Published var programs: [JSONValue] = []
    @Published var approvals: [JSONValue] = []
    @Published var handoffs: [JSONValue] = []
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var cached = false

    func load(model: AppModel, context: ModelContext) async {
        isLoading = readiness == nil
        errorMessage = nil
        defer { isLoading = false }
        guard let profile = model.profile else { return }
        do {
            async let readinessValue = model.request(path: "/api/v1/readiness")
            async let healthValue = model.request(path: "/api/v1/system/health")
            async let programValue = model.request(path: "/api/v1/research-programs")
            async let approvalValue = model.request(path: "/api/v1/approvals")
            async let handoffValue = model.request(path: "/api/v1/handoffs")
            let values = try await (
                readinessValue,
                healthValue,
                programValue,
                approvalValue,
                handoffValue
            )
            readiness = values.0
            health = values.1
            programs = values.2.arrayValue ?? []
            approvals = values.3.arrayValue ?? []
            handoffs = values.4.arrayValue ?? []
            try OfflineCache.store(values.0, profileID: profile.id, key: "home.readiness", context: context)
            try OfflineCache.store(values.1, profileID: profile.id, key: "home.health", context: context)
            try OfflineCache.store(values.2, profileID: profile.id, key: "home.programs", context: context)
            try OfflineCache.store(values.3, profileID: profile.id, key: "home.approvals", context: context)
            try OfflineCache.store(values.4, profileID: profile.id, key: "home.handoffs", context: context)
            cached = false
        } catch {
            readiness = try? OfflineCache.resource(profileID: profile.id, key: "home.readiness", context: context)
            health = try? OfflineCache.resource(profileID: profile.id, key: "home.health", context: context)
            programs = (try? OfflineCache.resource(profileID: profile.id, key: "home.programs", context: context))?.arrayValue ?? []
            approvals = (try? OfflineCache.resource(profileID: profile.id, key: "home.approvals", context: context))?.arrayValue ?? []
            handoffs = (try? OfflineCache.resource(profileID: profile.id, key: "home.handoffs", context: context))?.arrayValue ?? []
            cached = readiness != nil || health != nil || !programs.isEmpty
            if !cached { errorMessage = error.localizedDescription }
        }
    }
}

struct HomeView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader = HomeLoader()

    private func countPrograms(_ states: Set<String>) -> Int {
        loader.programs.filter { item in
            guard let state = item.resourceState else { return false }
            return states.contains(state)
        }.count
    }

    private var pendingApprovals: Int {
        loader.approvals.filter { $0.resourceState == "PENDING" }.count
    }

    private var availableHandoffs: Int {
        loader.handoffs.filter { ["AVAILABLE", "FEEDBACK_PENDING"].contains($0.resourceState ?? "") }.count
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.readiness == nil {
                LoadingStateView()
            } else if let error = loader.errorMessage {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("home.title").font(.largeTitle.bold())
                                Text(model.profile?.name ?? "QuaZonai")
                                    .font(.callout.monospaced())
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            StreamStatusBadge(status: model.streamStatus)
                                .font(.caption)
                        }
                        if loader.cached { OfflineReadOnlyBanner() }
                        actionCenter
                        researchPulse
                        systemHealth
                        recentEvents
                    }
                    .padding()
                }
            }
        }
        .navigationTitle("nav.home")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { await loader.load(model: model, context: modelContext) }
                } label: {
                    Label("common.refresh", systemImage: "arrow.clockwise")
                }
            }
        }
        .refreshable { await loader.load(model: model, context: modelContext) }
        .task { await loader.load(model: model, context: modelContext) }
        .accessibilityIdentifier("home-screen")
    }

    private var actionCenter: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("home.actionCenter").font(.title2.bold())
            HStack(spacing: 12) {
                NavigationLink {
                    IdeaComposerView()
                } label: {
                    Label("home.propose", systemImage: "plus.circle")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                NavigationLink {
                    ApprovalView()
                } label: {
                    Label("home.review", systemImage: "checkmark.seal")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150))], spacing: 12) {
                MetricCard(title: String(localized: "nav.approvals"), value: "\(pendingApprovals)", systemImage: "checkmark.seal")
                MetricCard(title: String(localized: "nav.handoff"), value: "\(availableHandoffs)", systemImage: "arrow.left.arrow.right")
            }
        }
    }

    private var researchPulse: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("home.researchPulse").font(.title2.bold())
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 135))], spacing: 12) {
                MetricCard(title: "Active", value: "\(countPrograms(["ACTIVE"]))")
                MetricCard(title: "Cooling", value: "\(countPrograms(["COOLING"]))")
                MetricCard(title: "Blocked", value: "\(countPrograms(["BLOCKED"]))")
                MetricCard(title: "Paused", value: "\(countPrograms(["PAUSED"]))")
                MetricCard(title: "Programs", value: "\(loader.programs.count)")
            }
        }
    }

    private var systemHealth: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("System Health").font(.title2.bold())
            if let health = loader.health {
                JSONInspector(value: health)
                    .padding()
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
            }
            if let readiness = loader.readiness {
                JSONInspector(value: readiness)
                    .padding()
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
            }
        }
    }

    private var recentEvents: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("home.events").font(.title2.bold())
            if model.recentEvents.isEmpty {
                EmptyStateView(title: String(localized: "home.events"), description: "No material events have arrived in this session.")
                    .frame(minHeight: 180)
            } else {
                ForEach(model.recentEvents.prefix(12), id: \.id) { event in
                    VStack(alignment: .leading, spacing: 5) {
                        HStack {
                            Text(event.event).font(.headline)
                            Spacer()
                            Text("#\(event.id)").font(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                        Text(event.data)
                            .font(.caption.monospaced())
                            .lineLimit(4)
                            .textSelection(.enabled)
                    }
                    .padding(12)
                    .background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
                }
            }
        }
    }
}

// MARK: - Idea Composer

struct IdeaComposerView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @State private var idea = ""
    @State private var preview: JSONValue?
    @State private var answers: [String: String] = [:]
    @State private var overlapAction = "recommended"
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var createdProgramID: String?
    @State private var previewKey = UUID()
    @State private var startKey = UUID()

    private var questions: [JSONValue] {
        preview?["clarification_questions"]?.arrayValue ?? []
    }

    private var answersComplete: Bool {
        questions.allSatisfy { question in
            guard let key = question["key"]?.stringValue else { return true }
            return !(answers[key] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("idea.title").font(.largeTitle.bold())
                VStack(alignment: .leading, spacing: 10) {
                    Text("idea.prompt").font(.headline)
                    TextEditor(text: $idea)
                        .frame(minHeight: 170)
                        .padding(10)
                        .scrollContentBackground(.hidden)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 14))
                        .accessibilityIdentifier("idea-text")
                    if idea.trimmingCharacters(in: .whitespacesAndNewlines).count < 12 {
                        Text("idea.minimum").font(.caption).foregroundStyle(.secondary)
                    } else {
                        Label("idea.draft", systemImage: "checkmark.circle")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Button {
                        Task { await previewIdea() }
                    } label: {
                        Label("idea.preview", systemImage: "doc.text.magnifyingglass")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(idea.trimmingCharacters(in: .whitespacesAndNewlines).count < 12 || isWorking || !model.network.isOnline)
                    .accessibilityIdentifier("preview-idea")
                }

                if let preview {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("Charter Preview").font(.title2.bold())
                        JSONInspector(value: preview["charter"] ?? preview)
                        if !questions.isEmpty {
                            Divider()
                            Text("Clarification Questions").font(.headline)
                            ForEach(Array(questions.enumerated()), id: \.offset) { _, question in
                                let key = question["key"]?.stringValue ?? UUID().uuidString
                                TextField(
                                    question["question"]?.stringValue ?? "Clarification",
                                    text: Binding(
                                        get: { answers[key] ?? "" },
                                        set: { answers[key] = $0 }
                                    )
                                )
                                .textFieldStyle(.roundedBorder)
                            }
                        }
                        if preview["overlap"] != nil {
                            Text("Overlap Detection").font(.headline)
                            JSONInspector(value: preview["overlap"] ?? .null)
                            Picker("Recommended handling", selection: $overlapAction) {
                                Text("Recommended").tag("recommended")
                                Text("Related Program").tag("new-program")
                                Text("Independent Program").tag("independent-program")
                            }
                            .pickerStyle(.segmented)
                        }
                        Button {
                            Task { await startResearch() }
                        } label: {
                            Label("idea.start", systemImage: "play.fill")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .disabled(isWorking || !answersComplete || !model.network.isOnline)
                        .accessibilityIdentifier("start-research")
                    }
                    .padding()
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                }
                if let errorMessage {
                    ErrorStateView(message: errorMessage, retry: nil).frame(minHeight: 180)
                }
            }
            .padding()
        }
        .navigationTitle("nav.idea")
        .navigationDestination(isPresented: Binding(
            get: { createdProgramID != nil },
            set: { if !$0 { createdProgramID = nil } }
        )) {
            if let createdProgramID {
                ResearchProgramDetailView(programID: createdProgramID, seed: nil)
            }
        }
        .task {
            guard let profile = model.profile else { return }
            idea = (try? OfflineCache.draft(profileID: profile.id, context: modelContext)) ?? ""
        }
        .onChange(of: idea) { _, value in
            guard let profile = model.profile else { return }
            try? OfflineCache.storeDraft(value, profileID: profile.id, context: modelContext)
        }
        .accessibilityIdentifier("idea-screen")
    }

    private func previewIdea() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            preview = try await model.request(
                path: "/api/v1/ideas/preview",
                method: .post,
                body: .object(["idea": .string(idea.trimmingCharacters(in: .whitespacesAndNewlines))]),
                idempotencyKey: previewKey
            )
            previewKey = UUID()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func startResearch() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        let answerBody = answers.mapValues(JSONValue.string)
        do {
            let response = try await model.request(
                path: "/api/v1/research-programs",
                method: .post,
                body: .object([
                    "idea": .string(idea.trimmingCharacters(in: .whitespacesAndNewlines)),
                    "answers": .object(answerBody),
                    "overlap_action": .string(overlapAction),
                ]),
                idempotencyKey: startKey
            )
            startKey = UUID()
            createdProgramID = response.firstString(for: ["id"])
            if let profile = model.profile {
                try? OfflineCache.storeDraft("", profileID: profile.id, context: modelContext)
            }
            idea = ""
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Research Observatory

struct ResearchView: View {
    var body: some View {
        ResourceCollectionScreen(
            title: String(localized: "research.title"),
            endpoint: "/api/v1/research-programs",
            cacheKey: "research.programs",
            emptyDescription: "No Research Program has been created."
        ) { program in
            if let id = program.resourceID {
                ResearchProgramDetailView(programID: id, seed: program)
            } else {
                JSONInspector(value: program).padding()
            }
        }
        .accessibilityIdentifier("research-screen")
    }
}

@MainActor
private final class ResearchDetailLoader: ObservableObject {
    @Published var program: JSONValue?
    @Published var missions: [JSONValue] = []
    @Published var activity: [JSONValue] = []
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var cached = false

    func load(programID: String, seed: JSONValue?, model: AppModel, context: ModelContext) async {
        isLoading = program == nil
        errorMessage = nil
        if program == nil { program = seed }
        defer { isLoading = false }
        guard let profile = model.profile else { return }
        do {
            async let programValue = model.request(path: "/api/v1/research-programs/\(programID)")
            async let missionValue = model.request(path: "/api/v1/research-programs/\(programID)/missions")
            async let activityValue = model.request(path: "/api/v1/research-programs/\(programID)/activity")
            let values = try await (programValue, missionValue, activityValue)
            program = values.0
            missions = values.1.arrayValue ?? []
            activity = values.2.arrayValue ?? []
            try OfflineCache.store(values.0, profileID: profile.id, key: "research.program.\(programID)", context: context)
            try OfflineCache.store(values.1, profileID: profile.id, key: "research.missions.\(programID)", context: context)
            try OfflineCache.store(values.2, profileID: profile.id, key: "research.activity.\(programID)", context: context)
            cached = false
        } catch {
            program = (try? OfflineCache.resource(profileID: profile.id, key: "research.program.\(programID)", context: context)) ?? program
            missions = (try? OfflineCache.resource(profileID: profile.id, key: "research.missions.\(programID)", context: context))?.arrayValue ?? []
            activity = (try? OfflineCache.resource(profileID: profile.id, key: "research.activity.\(programID)", context: context))?.arrayValue ?? []
            cached = program != nil
            if !cached { errorMessage = error.localizedDescription }
        }
    }
}

struct ResearchProgramDetailView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader = ResearchDetailLoader()
    @State private var action: String?
    @State private var reason = ""
    @State private var isMutating = false
    @State private var mutationError: String?
    @State private var idempotencyKey = UUID()

    let programID: String
    let seed: JSONValue?

    var body: some View {
        Group {
            if loader.isLoading && loader.program == nil {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.program == nil {
                ErrorStateView(message: error) { reload() }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        if loader.cached { OfflineReadOnlyBanner() }
                        if let program = loader.program {
                            HStack {
                                Text(program.firstString(for: ["title", "id"]) ?? programID)
                                    .font(.title.bold())
                                    .textSelection(.enabled)
                                Spacer()
                                if let state = program.resourceState { StatePill(state: state) }
                            }
                            programActions(state: program.resourceState ?? "")
                            GroupBox("Frozen Charter & Program") {
                                JSONInspector(value: program).padding(.top, 6)
                            }
                        }
                        GroupBox(String(localized: "research.missions")) {
                            MissionGraphView(missions: loader.missions).padding(.top, 8)
                        }
                        GroupBox("Experiment & Evidence Ledger") {
                            if let ledger = loader.program?["experiment_ledger"]
                                ?? loader.program?["evidence"]
                                ?? loader.program?["search_ledger"]
                            {
                                JSONInspector(value: ledger).padding(.top, 8)
                            } else {
                                Text("No ledger projection is attached to this Program response.")
                                    .foregroundStyle(.secondary).padding(.top, 8)
                            }
                        }
                        GroupBox(String(localized: "research.activity")) {
                            ActivityTimeline(events: loader.activity).padding(.top, 8)
                        }
                        if let mutationError {
                            ErrorStateView(message: mutationError, retry: nil).frame(minHeight: 160)
                        }
                    }
                    .padding()
                }
            }
        }
        .navigationTitle("Program")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: reload) {
                    Label("common.refresh", systemImage: "arrow.clockwise")
                }
            }
        }
        .sheet(item: Binding(
            get: { action.map(ActionSheetValue.init) },
            set: { action = $0?.value }
        )) { wrapped in
            NavigationStack {
                Form {
                    Section {
                        TextField("common.reason", text: $reason, axis: .vertical)
                    } footer: {
                        Text("Pause and Archive require an operator reason. Resume and Restore do not alter downstream runtimes.")
                    }
                    if let mutationError { Text(mutationError).foregroundStyle(.red) }
                }
                .navigationTitle(wrapped.value.capitalized)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("common.cancel") { action = nil }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("common.submit") {
                            Task { await performAction(wrapped.value) }
                        }
                        .disabled(isMutating || (["pause", "archive"].contains(wrapped.value) && reason.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty))
                    }
                }
            }
            .presentationDetents([.medium])
        }
        .task {
            await loader.load(programID: programID, seed: seed, model: model, context: modelContext)
        }
    }

    private func reload() {
        Task { await loader.load(programID: programID, seed: seed, model: model, context: modelContext) }
    }

    @ViewBuilder
    private func programActions(state: String) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack {
                if state != "PAUSED" && state != "ARCHIVED" {
                    Button("research.pause") { action = "pause" }.buttonStyle(.bordered)
                }
                if state == "PAUSED" {
                    Button("research.resume") { action = "resume" }.buttonStyle(.borderedProminent)
                }
                if state != "ARCHIVED" {
                    Button("research.archive", role: .destructive) { action = "archive" }.buttonStyle(.bordered)
                }
                if state == "ARCHIVED" {
                    Button("research.restore") { action = "restore" }.buttonStyle(.borderedProminent)
                }
            }
        }
        .disabled(!model.network.isOnline || isMutating)
    }

    private func performAction(_ value: String) async {
        isMutating = true
        mutationError = nil
        defer { isMutating = false }
        do {
            let body: JSONValue = ["pause", "archive"].contains(value)
                ? .object(["reason": .string(reason.trimmingCharacters(in: .whitespacesAndNewlines))])
                : .object([:])
            _ = try await model.request(
                path: "/api/v1/research-programs/\(programID)/\(value)",
                method: .post,
                body: body,
                idempotencyKey: idempotencyKey
            )
            idempotencyKey = UUID()
            action = nil
            reason = ""
            await loader.load(programID: programID, seed: nil, model: model, context: modelContext)
        } catch {
            mutationError = error.localizedDescription
        }
    }
}

private struct ActionSheetValue: Identifiable {
    let value: String
    var id: String { value }
}

private struct MissionGraphView: View {
    let missions: [JSONValue]

    var body: some View {
        if missions.isEmpty {
            Text("No Missions").foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(Array(missions.enumerated()), id: \.offset) { index, mission in
                    HStack(alignment: .top, spacing: 12) {
                        VStack(spacing: 0) {
                            Circle().fill(.tint).frame(width: 12, height: 12)
                            if index < missions.count - 1 {
                                Rectangle().fill(.quaternary).frame(width: 2, height: 54)
                            }
                        }
                        VStack(alignment: .leading, spacing: 5) {
                            HStack {
                                Text(mission.firstString(for: ["type", "objective", "id"]) ?? "Mission")
                                    .font(.headline)
                                Spacer()
                                if let state = mission.resourceState { StatePill(state: state) }
                            }
                            if let objective = mission["objective"]?.stringValue {
                                Text(objective).font(.subheadline).foregroundStyle(.secondary)
                            }
                            if let dependencies = mission["dependencies"]?.arrayValue, !dependencies.isEmpty {
                                Text("Depends on: \(dependencies.map(\.displayText).joined(separator: ", "))")
                                    .font(.caption.monospaced()).foregroundStyle(.secondary)
                            }
                        }
                    }
                    .accessibilityElement(children: .combine)
                }
            }
        }
    }
}

private struct ActivityTimeline: View {
    let events: [JSONValue]

    var body: some View {
        if events.isEmpty {
            Text("No Agent activity").foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(Array(events.enumerated()), id: \.offset) { _, event in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(event.firstString(for: ["kind", "aggregate_type"]) ?? "Event")
                            .font(.headline)
                        if let timestamp = event.firstString(for: ["created_at"]) {
                            Text(timestamp).font(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                        if let payload = event["payload"] {
                            JSONInspector(value: payload)
                        }
                    }
                    .padding(10)
                    .background(.quaternary, in: RoundedRectangle(cornerRadius: 10))
                }
            }
        }
    }
}
