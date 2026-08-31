import Charts
import SwiftData
import SwiftUI

// MARK: - Charts

private struct MetricDatum: Identifiable {
    let id = UUID()
    let label: String
    let value: Double
}

struct NumericMetricsChart: View {
    let value: JSONValue

    private var data: [MetricDatum] {
        guard let object = value.objectValue else { return [] }
        return object.compactMap { key, value in
            guard let number = value.numberValue, number.isFinite else { return nil }
            return MetricDatum(label: key.replacingOccurrences(of: "_", with: " "), value: number)
        }
        .sorted { $0.label < $1.label }
    }

    var body: some View {
        if data.isEmpty {
            JSONInspector(value: value)
        } else {
            Chart(data) { datum in
                BarMark(
                    x: .value("Metric", datum.label),
                    y: .value("Value", datum.value)
                )
                .accessibilityLabel(datum.label)
                .accessibilityValue(datum.value.formatted())
            }
            .frame(minHeight: 240)
            .chartXAxis { AxisMarks(values: .automatic) { _ in AxisGridLine(); AxisTick() } }
            .accessibilityLabel("Metric chart")
            .accessibilityValue(data.map { "\($0.label) \($0.value.formatted())" }.joined(separator: ", "))
        }
    }
}

private struct SeriesPoint: Identifiable {
    let id = UUID()
    let series: String
    let index: Int
    let value: Double
}

struct EquitySeriesChart: View {
    let candidate: JSONValue

    private var points: [SeriesPoint] {
        let keys = ["portfolio_equity", "equity", "benchmark", "drawdown"]
        return keys.flatMap { key -> [SeriesPoint] in
            guard let values = candidate[key]?.arrayValue else { return [] }
            return values.enumerated().compactMap { index, item in
                let number = item.numberValue
                    ?? item["value"]?.numberValue
                    ?? item["close"]?.numberValue
                    ?? item["equity"]?.numberValue
                guard let number, number.isFinite else { return nil }
                return SeriesPoint(series: key, index: index, value: number)
            }
        }
    }

    var body: some View {
        if points.isEmpty {
            Text("No equity series in this projection.").foregroundStyle(.secondary)
        } else {
            Chart(points) { point in
                LineMark(
                    x: .value("Observation", point.index),
                    y: .value("Value", point.value),
                    series: .value("Series", point.series)
                )
                .interpolationMethod(.monotone)
            }
            .frame(minHeight: 260)
            .accessibilityLabel("Portfolio equity, benchmark, and drawdown chart")
            .accessibilityValue("\(points.count) plotted observations")
        }
    }
}

struct CorrelationMatrixView: View {
    let value: JSONValue

    var body: some View {
        if let rows = value.arrayValue {
            ScrollView(.horizontal) {
                Grid(horizontalSpacing: 3, verticalSpacing: 3) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { rowIndex, row in
                        GridRow {
                            Text("\(rowIndex + 1)").font(.caption.monospaced())
                            ForEach(Array((row.arrayValue ?? []).enumerated()), id: \.offset) { _, cell in
                                Text(cell.numberValue?.formatted(.number.precision(.fractionLength(2))) ?? "—")
                                    .font(.caption2.monospacedDigit())
                                    .frame(width: 52, height: 34)
                                    .background(.quaternary, in: RoundedRectangle(cornerRadius: 5))
                            }
                        }
                    }
                }
            }
            .accessibilityLabel("Correlation matrix")
        } else {
            JSONInspector(value: value)
        }
    }
}

// MARK: - Alpha Library

struct AlphaLibraryView: View {
    var body: some View {
        ResourceCollectionScreen(
            title: String(localized: "alpha.title"),
            endpoint: "/api/v1/alpha-library",
            cacheKey: "alpha.library",
            emptyDescription: "No Alpha Qualification has entered the library."
        ) { alpha in
            if let id = alpha.resourceID {
                AlphaDetailView(alphaID: id, seed: alpha)
            } else {
                JSONInspector(value: alpha).padding()
            }
        }
        .accessibilityIdentifier("alpha-screen")
    }
}

struct AlphaDetailView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader: ResourceDetailLoader
    let seed: JSONValue

    init(alphaID: String, seed: JSONValue) {
        self.seed = seed
        _loader = StateObject(wrappedValue: ResourceDetailLoader(
            endpoint: "/api/v1/alpha-library/\(alphaID)",
            cacheKey: "alpha.\(alphaID)"
        ))
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.value == nil { LoadingStateView() }
            else if let error = loader.errorMessage, loader.value == nil {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else {
                let value = loader.value ?? seed
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        if loader.loadedFromCache { OfflineReadOnlyBanner() }
                        HStack {
                            Text(value.firstString(for: ["name", "id"]) ?? "Alpha")
                                .font(.title.bold()).textSelection(.enabled)
                            Spacer()
                            if let state = value.resourceState { StatePill(state: state) }
                        }
                        if let metrics = value["metrics"] {
                            GroupBox("Metrics & Robustness") {
                                NumericMetricsChart(value: metrics).padding(.top, 8)
                            }
                        }
                        GroupBox("Qualification, Universe, Horizon, Calibration, Lineage & Evidence") {
                            JSONInspector(value: value).padding(.top, 8)
                        }
                    }
                    .padding()
                }
            }
        }
        .navigationTitle("Alpha")
        .task { await loader.load(model: model, context: modelContext) }
    }
}

// MARK: - Portfolio Lab

private enum PortfolioSegment: String, CaseIterable, Identifiable {
    case mandates
    case programs
    var id: String { rawValue }
}

struct PortfolioView: View {
    @State private var segment: PortfolioSegment = .programs

    var body: some View {
        VStack(spacing: 0) {
            Picker("Portfolio section", selection: $segment) {
                Text("Mandates").tag(PortfolioSegment.mandates)
                Text("Programs & Candidates").tag(PortfolioSegment.programs)
            }
            .pickerStyle(.segmented)
            .padding()
            switch segment {
            case .mandates:
                ResourceCollectionScreen(
                    title: String(localized: "admin.mandates"),
                    endpoint: "/api/v1/portfolio-mandates",
                    cacheKey: "portfolio.mandates",
                    emptyDescription: "No Portfolio Mandate is registered."
                ) { mandate in
                    ScrollView { JSONInspector(value: mandate).padding() }
                        .navigationTitle("Mandate")
                }
            case .programs:
                ResourceCollectionScreen(
                    title: String(localized: "portfolio.title"),
                    endpoint: "/api/v1/portfolio-programs",
                    cacheKey: "portfolio.programs",
                    emptyDescription: "No Portfolio Program is active."
                ) { program in
                    PortfolioProgramDetailView(program: program)
                }
            }
        }
        .accessibilityIdentifier("portfolio-screen")
    }
}

struct PortfolioProgramDetailView: View {
    let program: JSONValue

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                JSONInspector(value: program)
                if let candidateID = program.firstString(for: ["current_candidate_id"]) {
                    NavigationLink {
                        PortfolioCandidateDetailView(candidateID: candidateID)
                    } label: {
                        Label("Open current Candidate", systemImage: "chart.pie.fill")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .padding()
        }
        .navigationTitle("Portfolio Program")
    }
}

struct PortfolioCandidateDetailView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader: ResourceDetailLoader

    init(candidateID: String) {
        _loader = StateObject(wrappedValue: ResourceDetailLoader(
            endpoint: "/api/v1/portfolio-candidates/\(candidateID)",
            cacheKey: "portfolio.candidate.\(candidateID)"
        ))
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.value == nil { LoadingStateView() }
            else if let error = loader.errorMessage, loader.value == nil {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else if let candidate = loader.value {
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        if loader.loadedFromCache { OfflineReadOnlyBanner() }
                        EquitySeriesChart(candidate: candidate)
                        if let metrics = candidate["metrics"] {
                            GroupBox("Candidate Metrics") {
                                NumericMetricsChart(value: metrics).padding(.top, 8)
                            }
                        }
                        if let correlation = candidate["correlation_matrix"] {
                            GroupBox("Correlation Matrix") {
                                CorrelationMatrixView(value: correlation).padding(.top, 8)
                            }
                        }
                        GroupBox("Members, Allocation, Risk, Cost, Capacity, Policy & Constraints") {
                            JSONInspector(value: candidate).padding(.top, 8)
                        }
                    }
                    .padding()
                }
            }
        }
        .navigationTitle("Candidate")
        .task { await loader.load(model: model, context: modelContext) }
    }
}

// MARK: - Approval

@MainActor
private final class ApprovalLoader: ObservableObject {
    @Published var approvals: [JSONValue] = []
    @Published var downstreams: [JSONValue] = []
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var cached = false

    func load(model: AppModel, context: ModelContext) async {
        isLoading = approvals.isEmpty
        errorMessage = nil
        defer { isLoading = false }
        guard let profile = model.profile else { return }
        do {
            async let approvalsValue = model.request(path: "/api/v1/approvals")
            async let downstreamValue = model.request(path: "/api/v1/downstream-systems")
            let values = try await (approvalsValue, downstreamValue)
            approvals = values.0.arrayValue ?? []
            downstreams = values.1.arrayValue ?? []
            try OfflineCache.store(values.0, profileID: profile.id, key: "approval.list", context: context)
            try OfflineCache.store(values.1, profileID: profile.id, key: "admin.downstreams", context: context)
            cached = false
        } catch {
            approvals = (try? OfflineCache.resource(profileID: profile.id, key: "approval.list", context: context))?.arrayValue ?? []
            downstreams = (try? OfflineCache.resource(profileID: profile.id, key: "admin.downstreams", context: context))?.arrayValue ?? []
            cached = !approvals.isEmpty
            if !cached { errorMessage = error.localizedDescription }
        }
    }
}

struct ApprovalView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader = ApprovalLoader()
    @State private var searchText = ""

    private var visible: [JSONValue] {
        let values = searchText.isEmpty
            ? loader.approvals
            : loader.approvals.filter { $0.searchableText.localizedStandardContains(searchText) }
        return values.sorted {
            let leftPending = $0.resourceState == "PENDING"
            let rightPending = $1.resourceState == "PENDING"
            if leftPending != rightPending { return leftPending }
            return $0.searchableText < $1.searchableText
        }
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.approvals.isEmpty { LoadingStateView() }
            else if let error = loader.errorMessage { ErrorStateView(message: error) { reload() } }
            else if visible.isEmpty {
                EmptyStateView(title: String(localized: "approval.title"), description: "No candidate decision currently requires operator action.")
            } else {
                List {
                    if loader.cached { OfflineReadOnlyBanner().listRowSeparator(.hidden) }
                    ForEach(Array(visible.enumerated()), id: \.offset) { _, approval in
                        NavigationLink {
                            ApprovalDecisionView(
                                approval: approval,
                                downstreams: loader.downstreams,
                                onChanged: reload
                            )
                        } label: {
                            ResourceRow(value: approval)
                        }
                    }
                }
            }
        }
        .navigationTitle("approval.title")
        .searchable(text: $searchText)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: reload) { Label("common.refresh", systemImage: "arrow.clockwise") }
            }
        }
        .refreshable { await loader.load(model: model, context: modelContext) }
        .task { await loader.load(model: model, context: modelContext) }
        .accessibilityIdentifier("approval-screen")
    }

    private func reload() {
        Task { await loader.load(model: model, context: modelContext) }
    }
}

private let rejectionReasonCodes = [
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

struct ApprovalDecisionView: View {
    @EnvironmentObject private var model: AppModel
    @State private var current: JSONValue
    @State private var downstreamID = ""
    @State private var rejectReason = ""
    @State private var note = ""
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var decisionKey = UUID()
    @State private var showReject = false

    let downstreams: [JSONValue]
    let onChanged: () -> Void

    init(approval: JSONValue, downstreams: [JSONValue], onChanged: @escaping () -> Void) {
        _current = State(initialValue: approval)
        self.downstreams = downstreams
        self.onChanged = onChanged
    }

    private var approvalID: String { current.resourceID ?? "" }
    private var purpose: String { current.firstString(for: ["purpose"]) ?? "PAPER" }
    private var compatibleDownstreams: [JSONValue] {
        downstreams.filter { downstream in
            guard downstream["enabled"]?.boolValue != false else { return false }
            let environment = downstream.firstString(for: ["environment_type"]) ?? ""
            if purpose == "PAPER" && environment != "PAPER" { return false }
            if purpose == "LIVE" && environment != "LIVE" { return false }
            let preflight = downstream.firstString(for: ["preflight_state"]) ?? "READY"
            return preflight.range(of: "READY|PASS|VALID", options: .regularExpression) != nil
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    StatePill(state: purpose)
                    if let state = current.resourceState { StatePill(state: state) }
                    Spacer()
                    if let expiry = current.firstString(for: ["valid_until", "expires_at"]) {
                        Text(expiry).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                Text(current.firstString(for: ["recommendation_rationale"]) ?? "System recommendation")
                    .font(.title3.weight(.semibold))
                GroupBox("Level 2 Evidence, Capital Context & Human Report") {
                    JSONInspector(value: current).padding(.top, 8)
                }
                if current.resourceState == "PENDING" {
                    Picker("approval.downstream", selection: $downstreamID) {
                        Text("Select…").tag("")
                        ForEach(Array(compatibleDownstreams.enumerated()), id: \.offset) { _, downstream in
                            Text(downstream.firstString(for: ["name", "id"]) ?? "Downstream")
                                .tag(downstream.resourceID ?? "")
                        }
                    }
                    .pickerStyle(.menu)
                    Text("approval.expectedState").font(.footnote).foregroundStyle(.secondary)
                    HStack {
                        Button("approval.reject", role: .destructive) { showReject = true }
                            .buttonStyle(.bordered)
                        Button("approval.approve") {
                            Task { await approve() }
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(downstreamID.isEmpty || isWorking || !model.network.isOnline)
                        .accessibilityIdentifier("approve-candidate")
                    }
                }
                if let errorMessage {
                    ErrorStateView(message: errorMessage, retry: nil).frame(minHeight: 160)
                }
            }
            .padding()
        }
        .navigationTitle("approval.title")
        .sheet(isPresented: $showReject) {
            NavigationStack {
                Form {
                    Picker("common.reason", selection: $rejectReason) {
                        Text("Select…").tag("")
                        ForEach(rejectionReasonCodes, id: \.self) { code in
                            Text(code.replacingOccurrences(of: "_", with: " ").capitalized).tag(code)
                        }
                    }
                    TextField("common.note", text: $note, axis: .vertical)
                    if let errorMessage { Text(errorMessage).foregroundStyle(.red) }
                }
                .navigationTitle("approval.reject")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("common.cancel") { showReject = false }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("approval.reject", role: .destructive) {
                            Task { await reject() }
                        }
                        .disabled(rejectReason.isEmpty || isWorking || !model.network.isOnline)
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
        .task {
            if downstreamID.isEmpty {
                downstreamID = compatibleDownstreams.first?.resourceID ?? ""
            }
        }
    }

    private func refreshedPendingApproval() async throws -> JSONValue {
        let response = try await model.request(path: "/api/v1/approvals")
        guard let refreshed = response.arrayValue?.first(where: { $0.resourceID == approvalID }) else {
            throw APIError.server(status: 404, code: "APPROVAL_NOT_FOUND", message: "Approval no longer exists.")
        }
        current = refreshed
        guard refreshed.resourceState == "PENDING" else {
            throw APIError.conflict(code: "APPROVAL_STATE_CONFLICT", message: "Approval is no longer pending.")
        }
        if let expiry = refreshed.firstString(for: ["valid_until", "expires_at"]),
           let date = ISO8601DateFormatter().date(from: expiry),
           date <= .now
        {
            throw APIError.conflict(code: "APPROVAL_EXPIRED", message: "Approval has expired.")
        }
        return refreshed
    }

    private func approve() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            _ = try await refreshedPendingApproval()
            let result = try await model.performSensitive(
                reason: String(localized: "security.biometric")
            ) {
                try await model.request(
                    path: "/api/v1/approvals/\(approvalID)/approve",
                    method: .post,
                    body: .object([
                        "downstream_system_id": .string(downstreamID),
                        "expected_state": .string("PENDING"),
                    ]),
                    idempotencyKey: decisionKey
                )
            }
            if result != .null { current = result }
            decisionKey = UUID()
            onChanged()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func reject() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            _ = try await refreshedPendingApproval()
            var payload: [String: JSONValue] = [
                "reason_code": .string(rejectReason),
                "expected_state": .string("PENDING"),
            ]
            if !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                payload["note"] = .string(note.trimmingCharacters(in: .whitespacesAndNewlines))
            }
            let result = try await model.performSensitive(
                reason: String(localized: "security.biometric")
            ) {
                try await model.request(
                    path: "/api/v1/approvals/\(approvalID)/reject",
                    method: .post,
                    body: .object(payload),
                    idempotencyKey: decisionKey
                )
            }
            if result != .null { current = result }
            decisionKey = UUID()
            showReject = false
            onChanged()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Handoff & Feedback

struct HandoffView: View {
    var body: some View {
        ResourceCollectionScreen(
            title: String(localized: "handoff.title"),
            endpoint: "/api/v1/handoffs",
            cacheKey: "handoff.list",
            emptyDescription: "No Handoff Offer or Forward Feedback is available."
        ) { handoff in
            HandoffDetailView(handoff: handoff)
        }
        .accessibilityIdentifier("handoff-screen")
    }
}

struct HandoffDetailView: View {
    @EnvironmentObject private var model: AppModel
    @State private var current: JSONValue
    @State private var reason = "OPERATOR_REVOKED"
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var idempotencyKey = UUID()

    init(handoff: JSONValue) {
        _current = State(initialValue: handoff)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    if let state = current.resourceState { StatePill(state: state) }
                    Spacer()
                    if let deadline = current.firstString(for: ["claim_deadline"]) {
                        Text(deadline).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                GroupBox("Candidate, Downstream, Package Contract, Feedback Contract & Forward Performance") {
                    JSONInspector(value: current).padding(.top, 8)
                }
                if current.resourceState == "AVAILABLE" {
                    TextField("common.reason", text: $reason)
                        .textFieldStyle(.roundedBorder)
                    Button("handoff.revoke", role: .destructive) {
                        Task { await revoke() }
                    }
                    .buttonStyle(.bordered)
                    .disabled(reason.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isWorking || !model.network.isOnline)
                    .accessibilityIdentifier("revoke-handoff")
                }
                if let errorMessage {
                    ErrorStateView(message: errorMessage, retry: nil).frame(minHeight: 160)
                }
            }
            .padding()
        }
        .navigationTitle("handoff.title")
    }

    private func revoke() async {
        guard let id = current.resourceID else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let list = try await model.request(path: "/api/v1/handoffs")
            guard let refreshed = list.arrayValue?.first(where: { $0.resourceID == id }),
                  refreshed.resourceState == "AVAILABLE"
            else {
                throw APIError.conflict(code: "HANDOFF_STATE_CONFLICT", message: "Only an unclaimed AVAILABLE offer can be revoked.")
            }
            current = refreshed
            let result = try await model.performSensitive(
                reason: String(localized: "security.biometric")
            ) {
                try await model.request(
                    path: "/api/v1/handoffs/\(id)/revoke",
                    method: .post,
                    body: .object(["reason_code": .string(reason)]),
                    idempotencyKey: idempotencyKey
                )
            }
            if result != .null { current = result }
            idempotencyKey = UUID()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Language and Appearance

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    private let locales: [(String, String)] = [
        ("en", "English"),
        ("zh-Hans", "简体中文"),
        ("zh-Hant", "繁體中文"),
        ("ja", "日本語"),
        ("ko", "한국어"),
        ("es", "Español"),
        ("ar", "العربية"),
    ]

    var body: some View {
        Form {
            Section("settings.language") {
                Picker("settings.language", selection: Binding(
                    get: { model.localeIdentifier },
                    set: { model.setLocale($0) }
                )) {
                    ForEach(locales, id: \.0) { locale in
                        Text(locale.1).tag(locale.0)
                    }
                }
            }
            Section("settings.appearance") {
                Picker("settings.appearance", selection: Binding(
                    get: { model.appearance },
                    set: { model.setAppearance($0) }
                )) {
                    Text("settings.system").tag(AppearanceMode.system)
                    Text("settings.light").tag(AppearanceMode.light)
                    Text("settings.dark").tag(AppearanceMode.dark)
                }
                .pickerStyle(.segmented)
            }
            Section {
                Button("Sign Out", role: .destructive) {
                    Task { await model.logout() }
                }
                Button("Forget Server", role: .destructive) {
                    Task { await model.forgetServer() }
                }
            }
        }
        .navigationTitle("settings.title")
        .accessibilityIdentifier("settings-screen")
    }
}
