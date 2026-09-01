import SwiftData
import SwiftUI

struct IdeaComposerView: View {
    @EnvironmentObject private var session: SessionStore
    @Environment(\.modelContext) private var modelContext
    let navigate: (AppSection) -> Void
    @State private var idea = ""
    @State private var preview: JSONValue?
    @State private var answers: [String: String] = [:]
    @State private var overlapAction = "recommended"
    @State private var loading = false
    @State private var error: String?
    @State private var started: JSONValue?
    @State private var previewSubmission = MutationSubmission()
    @State private var startSubmission = MutationSubmission()

    var body: some View {
        Form {
            Section("Research idea") {
                TextEditor(text: $idea).frame(minHeight: 130).accessibilityLabel("What should the research system investigate?")
                Text("Minimum 12 characters. Drafts are local-only until Start Research.").font(.caption).foregroundStyle(.secondary)
                Button(L10n.text(.preview, session.language)) { Task { await previewIdea() } }
                    .disabled(idea.trimmingCharacters(in: .whitespacesAndNewlines).count < 12 || loading)
            }
            if let preview {
                Section("Charter preview") { JSONDocumentView(value: preview) }
                if let questions = preview["clarification_questions"]?.arrayValue, !questions.isEmpty {
                    Section("Material clarification") {
                        ForEach(Array(questions.enumerated()), id: \.offset) { _, question in
                            if let object = question.objectValue, let key = object.string("key") {
                                TextField(object.string("question") ?? key, text: Binding(get: { answers[key] ?? "" }, set: { answers[key] = $0 }))
                            }
                        }
                    }
                }
                if preview["overlap"]?.objectValue != nil {
                    Section("Overlap handling") {
                        Picker("Handling", selection: $overlapAction) {
                            Text("Use recommendation").tag("recommended")
                            Text("Create related program").tag("new-program")
                            Text("Create independent program").tag("independent-program")
                        }.pickerStyle(.inline)
                    }
                }
                Section {
                    Button(L10n.text(.startResearch, session.language)) { Task { await startResearch() } }
                        .disabled(!answersComplete || loading)
                }
            }
            if let started { Section("Created program") { JSONDocumentView(value: started); Button(L10n.text(.research, session.language)) { navigate(.research) } } }
            if let error { Section { Text(error).foregroundStyle(.red) } }
        }
        .navigationTitle(L10n.text(.idea, session.language))
        .task { loadDraft() }
        .onChange(of: idea) { _, _ in saveDraft() }
    }

    private var answersComplete: Bool {
        guard let questions = preview?["clarification_questions"]?.arrayValue else { return true }
        return questions.allSatisfy { item in
            guard let key = item.objectValue?.string("key") else { return true }
            return !(answers[key] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    private func previewIdea() async {
        loading = true; defer { loading = false }
        do {
            preview = try await session.mutate(
                path: "/api/v1/ideas/preview",
                body: .object(["idea": .string(idea.trimmingCharacters(in: .whitespacesAndNewlines))]),
                submission: previewSubmission
            )
            previewSubmission = MutationSubmission()
            error = nil
        } catch { self.error = error.localizedDescription }
    }

    private func startResearch() async {
        loading = true; defer { loading = false }
        let answerJSON = JSONValue.object(answers.mapValues(JSONValue.string))
        do {
            started = try await session.mutate(path: "/api/v1/research-programs", body: .object([
                "idea": .string(idea.trimmingCharacters(in: .whitespacesAndNewlines)),
                "answers": answerJSON,
                "overlap_action": .string(overlapAction),
            ]), submission: startSubmission)
            startSubmission = MutationSubmission()
            error = nil
        } catch { self.error = error.localizedDescription }
    }

    private var draftKey: String { "\(session.profile)|idea-draft" }
    private func loadDraft() {
        guard !session.profile.isEmpty else { return }
        let key = draftKey
        let descriptor = FetchDescriptor<IdeaDraft>(predicate: #Predicate { $0.draftKey == key })
        if let draft = try? modelContext.fetch(descriptor).first { idea = draft.text }
        else { modelContext.insert(IdeaDraft(serverProfile: session.profile)) }
    }
    private func saveDraft() {
        guard !session.profile.isEmpty else { return }
        let key = draftKey
        let descriptor = FetchDescriptor<IdeaDraft>(predicate: #Predicate { $0.draftKey == key })
        if let draft = try? modelContext.fetch(descriptor).first { draft.text = idea; draft.updatedAt = .now }
        else { modelContext.insert(IdeaDraft(serverProfile: session.profile, text: idea)) }
        try? modelContext.save()
    }
}
