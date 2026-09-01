import SwiftUI

private struct IdentifiedApproval: Identifiable {
    let id: String
    let value: JSONValue

    init?(_ value: JSONValue) {
        guard let id = value.stableID else { return nil }
        self.id = id
        self.value = value
    }
}

private let approvalRejectReasons = [
    "RESEARCH_EVIDENCE_INSUFFICIENT",
    "RISK_PROFILE_UNACCEPTABLE",
    "DRAWDOWN_TOO_HIGH",
    "TURNOVER_TOO_HIGH",
    "CAPACITY_TOO_LOW",
    "COMPLEXITY_TOO_HIGH",
    "INTERPRETABILITY_INSUFFICIENT",
    "MARKET_SCOPE_UNACCEPTABLE",
    "PAPER_EVIDENCE_INSUFFICIENT",
    "LIVE_READINESS_INSUFFICIENT",
    "NOT_ALIGNED_WITH_ORIGINAL_IDEA",
    "OTHER",
]

struct ApprovalInboxView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var approvals: [JSONValue] = []
    @State private var downstreams: [JSONValue] = []
    @State private var query = ""
    @State private var error: String?

    private var visible: [JSONValue] {
        approvals.filter { query.isEmpty || $0.searchableText.localizedCaseInsensitiveContains(query) }
            .sorted { lhs, rhs in
                let lp = lhs.objectValue?.string("state") == "PENDING"
                let rp = rhs.objectValue?.string("state") == "PENDING"
                if lp != rp { return lp && !rp }
                return lhs.listTitle < rhs.listTitle
            }
    }

    var body: some View {
        List {
            if let error { Text(error).foregroundStyle(.red) }
            if approvals.isEmpty && error == nil { ContentUnavailableView(L10n.text(.empty, session.language), systemImage: "checkmark.seal") }
            ForEach(visible.compactMap(IdentifiedApproval.init)) { item in
                ApprovalCard(approval: item.value, downstreams: downstreams) { await reload() }
            }
        }
        .navigationTitle(L10n.text(.approvals, session.language))
        .searchable(text: $query, prompt: L10n.text(.search, session.language))
        .task { await reload() }
        .refreshable { await reload() }
    }

    private func reload() async {
        do {
            async let a = session.load(path: "/api/v1/approvals", cacheKey: "approvals")
            async let d = session.load(path: "/api/v1/downstream-systems", cacheKey: "downstreams")
            let values = try await (a, d)
            approvals = values.0.normalizedItems
            downstreams = values.1.normalizedItems
            error = nil
        } catch { self.error = error.localizedDescription }
    }
}

private struct ApprovalCard: View {
    @EnvironmentObject private var session: SessionStore
    let approval: JSONValue
    let downstreams: [JSONValue]
    let reload: () async -> Void
    @State private var downstreamID = ""
    @State private var rejectReason = ""
    @State private var rejectNote = ""
    @State private var showReject = false
    @State private var error: String?
    @State private var busy = false
    @State private var mutationSubmission = MutationSubmission()

    private var object: [String: JSONValue] { approval.objectValue ?? [:] }
    private var pending: Bool { object.string("state") == "PENDING" }
    private var purpose: String { object.string("purpose") ?? "" }
    private var compatible: [JSONValue] {
        downstreams.filter { item in
            guard let value = item.objectValue, value.bool("enabled") != false else { return false }
            guard value.string("preflight_state") == "READY" else { return false }
            if purpose == "PAPER" { return value.string("environment_type") == "PAPER" }
            if purpose == "LIVE" { return value.string("environment_type") == "LIVE" }
            return true
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack { Text(purpose).font(.caption).bold(); Text(object.string("state") ?? "").font(.caption); Spacer(); Text((object.string("valid_until") ?? object.string("expires_at") ?? "")).font(.caption2).foregroundStyle(.secondary) }
            Text(object.string("recommendation_rationale") ?? "Candidate \((object.string("candidate_id") ?? "").prefix(8))").font(.headline)
            if pending {
                Picker("Compatible downstream", selection: $downstreamID) {
                    Text("Select downstream").tag("")
                    ForEach(Array(compatible.enumerated()), id: \.offset) { _, system in
                        if let id = system.stableID { Text("\(system.listTitle) · \(system.objectValue?.string("environment_type") ?? "")").tag(id) }
                    }
                }
                .onAppear { if downstreamID.isEmpty { downstreamID = object.string("downstream_system_id") ?? compatible.first?.stableID ?? "" } }
                HStack {
                    Button(L10n.text(.reject, session.language), role: .destructive) { showReject = true }.disabled(busy)
                    Button(L10n.text(.approve, session.language)) { Task { await approve() } }
                        .buttonStyle(.borderedProminent).disabled(downstreamID.isEmpty || busy)
                }
            }
            DisclosureGroup("Level 2 evidence / capital / report") { JSONDocumentView(value: approval) }
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .padding(.vertical, 5)
        .sheet(isPresented: $showReject) {
            NavigationStack {
                Form {
                    Picker("Reason code", selection: $rejectReason) {
                        Text("Select reason").tag("")
                        ForEach(approvalRejectReasons, id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) }
                    }
                    TextField("Optional note", text: $rejectNote, axis: .vertical)
                }
                .navigationTitle(L10n.text(.reject, session.language))
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button(L10n.text(.cancel, session.language)) { showReject = false } }
                    ToolbarItem(placement: .confirmationAction) { Button(L10n.text(.reject, session.language), role: .destructive) { Task { await reject() } }.disabled(rejectReason.isEmpty) }
                }
            }
        }
    }

    private func approve() async {
        guard await BiometricGate.authorize(reason: "Approve this portfolio handoff candidate") else {
            error = "Biometric confirmation was not completed."
            return
        }
        guard let id = approval.stableID else { return }
        busy = true; defer { busy = false }
        do {
            _ = try await session.mutate(path: "/api/v1/approvals/\(id)/approve", body: .object([
                "downstream_system_id": .string(downstreamID),
                "expected_state": .string("PENDING"),
            ]), submission: mutationSubmission)
            mutationSubmission = MutationSubmission()
            await reload()
        } catch { self.error = error.localizedDescription }
    }

    private func reject() async {
        guard let id = approval.stableID else { return }
        busy = true; defer { busy = false }
        var body: [String: JSONValue] = ["reason_code": .string(rejectReason), "expected_state": .string("PENDING")]
        if !rejectNote.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { body["note"] = .string(rejectNote.trimmingCharacters(in: .whitespacesAndNewlines)) }
        do {
            _ = try await session.mutate(path: "/api/v1/approvals/\(id)/reject", body: .object(body), submission: mutationSubmission)
            mutationSubmission = MutationSubmission()
            showReject = false; rejectReason = ""; rejectNote = ""; await reload()
        } catch { self.error = error.localizedDescription }
    }
}

struct HandoffFeedbackView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var handoffs: [JSONValue] = []
    @State private var query = ""
    @State private var ascending = false
    @State private var error: String?

    private var visible: [JSONValue] {
        handoffs.filter { query.isEmpty || $0.searchableText.localizedCaseInsensitiveContains(query) }
            .sorted { ascending ? $0.listTitle < $1.listTitle : $0.listTitle > $1.listTitle }
    }

    var body: some View {
        List {
            Section {
                HStack {
                    MetricChip(label: "Available", count: handoffs.filter { $0.objectValue?.string("state") == "AVAILABLE" }.count)
                    MetricChip(label: "Claimed", count: handoffs.filter { $0.objectValue?.string("state") == "CLAIMED" }.count)
                    MetricChip(label: "Accepted", count: handoffs.filter { $0.objectValue?.string("state") == "DOWNSTREAM_ACCEPTED" }.count)
                }
            }
            if let error { Text(error).foregroundStyle(.red) }
            ForEach(Array(visible.enumerated()), id: \.offset) { _, offer in HandoffCard(offer: offer) { await reload() } }
        }
        .navigationTitle(L10n.text(.handoff, session.language))
        .searchable(text: $query, prompt: L10n.text(.search, session.language))
        .toolbar { Button { ascending.toggle() } label: { Image(systemName: ascending ? "arrow.up" : "arrow.down") } }
        .task { await reload() }.refreshable { await reload() }
    }

    private func reload() async {
        do { handoffs = try await session.load(path: "/api/v1/handoffs", cacheKey: "handoffs").normalizedItems; error = nil }
        catch { self.error = error.localizedDescription }
    }
}

private struct HandoffCard: View {
    @EnvironmentObject private var session: SessionStore
    let offer: JSONValue
    let reload: () async -> Void
    @State private var error: String?
    @State private var busy = false
    @State private var mutationSubmission = MutationSubmission()
    private var state: String { offer.objectValue?.string("state") ?? "" }
    private var revocable: Bool { ["APPROVED", "PUBLISHING", "AVAILABLE"].contains(state) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            RecordRow(item: offer)
            Text(state).font(.caption).accessibilityIdentifier("handoff-state")
            if revocable {
                Button(L10n.text(.revoke, session.language), role: .destructive) { Task { await revoke() } }.disabled(busy)
            } else if ["CLAIMED", "DOWNSTREAM_ACCEPTED", "FEEDBACK_PENDING", "FEEDBACK_IN_PROGRESS", "FEEDBACK_PARTIAL", "FEEDBACK_COMPLETE"].contains(state) {
                Text("Downstream owns runtime control after claim.").font(.caption).foregroundStyle(.secondary)
            }
            DisclosureGroup("Package / feedback / forward evidence") { JSONDocumentView(value: offer) }
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }.padding(.vertical, 4)
    }

    private func revoke() async {
        guard let id = offer.stableID else { return }
        guard await BiometricGate.authorize(reason: "Revoke an unclaimed handoff offer") else { error = "Biometric confirmation was not completed."; return }
        busy = true; defer { busy = false }
        do {
            _ = try await session.mutate(
                path: "/api/v1/handoffs/\(id)/revoke",
                body: .object(["reason_code": .string("OPERATOR_REVOKE")]),
                submission: mutationSubmission
            )
            mutationSubmission = MutationSubmission()
            await reload()
        }
        catch { self.error = error.localizedDescription }
    }
}

private struct MetricChip: View {
    let label: String; let count: Int
    var body: some View { VStack { Text(String(count)).font(.headline.monospacedDigit()); Text(label).font(.caption2).foregroundStyle(.secondary) }.frame(maxWidth: .infinity) }
}
