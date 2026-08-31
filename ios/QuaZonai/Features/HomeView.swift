import SwiftUI

struct HomeView: View {
    @EnvironmentObject private var session: SessionStore
    let navigate: (AppSection) -> Void
    @State private var readiness: JSONValue?
    @State private var health: JSONValue?
    @State private var programs: [JSONValue] = []
    @State private var alphas: [JSONValue] = []
    @State private var approvals: [JSONValue] = []
    @State private var handoffs: [JSONValue] = []
    @State private var error: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if session.directAccessWarning {
                    Label(L10n.text(.directWarning, session.language), systemImage: "exclamationmark.shield")
                        .font(.callout).padding().frame(maxWidth: .infinity, alignment: .leading)
                        .background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 12))
                }
                HStack {
                    Button { navigate(.idea) } label: { Label(L10n.text(.idea, session.language), systemImage: "plus") }
                        .buttonStyle(.borderedProminent).keyboardShortcut("n", modifiers: .command)
                    Button { navigate(.approvals) } label: { Label(L10n.text(.approvals, session.language), systemImage: "checkmark.seal") }
                        .buttonStyle(.bordered)
                }
                .accessibilityLabel(L10n.text(.actionCenter, session.language))

                if let error { Text(error).foregroundStyle(.red) }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 145))], spacing: 12) {
                    MetricCard(label: "Active / Cooling / Blocked", value: programStateSummary)
                    MetricCard(label: "Running missions", value: runningMissionCount)
                    MetricCard(label: L10n.text(.alpha, session.language), value: String(alphas.count))
                    MetricCard(label: L10n.text(.approvals, session.language), value: String(approvals.filter { $0.objectValue?.string("state") == "PENDING" }.count))
                    MetricCard(label: L10n.text(.handoff, session.language), value: String(handoffs.filter { $0.objectValue?.string("state") == "AVAILABLE" }.count))
                    MetricCard(label: "SSE", value: eventStateText)
                }

                GroupBox(L10n.text(.systemHealth, session.language)) {
                    if let readiness { JSONTreeView(value: readiness) }
                    if let health { JSONTreeView(value: health) }
                }
                GroupBox(L10n.text(.recentEvents, session.language)) {
                    if session.recentEvents.isEmpty { Text(L10n.text(.empty, session.language)).foregroundStyle(.secondary) }
                    else {
                        ForEach(session.recentEvents.prefix(8), id: \.id) { event in
                            DisclosureGroup("#\(event.id) · \(event.name)") { JSONTreeView(value: event.data) }
                        }
                    }
                }
            }
            .padding()
        }
        .navigationTitle(L10n.text(.home, session.language))
        .task { await reload() }
        .refreshable { await reload() }
    }

    private var programStateSummary: String {
        let counts = Dictionary(grouping: programs.compactMap { $0.objectValue?.string("state") }, by: { $0 }).mapValues(\.count)
        return "\(counts["ACTIVE", default: 0]) / \(counts["COOLING", default: 0]) / \(counts["BLOCKED", default: 0])"
    }
    private var runningMissionCount: String {
        String(programs.reduce(0) { $0 + Int($1.objectValue?.number("mission_count") ?? 0) })
    }
    private var eventStateText: String {
        switch session.eventState {
        case .disconnected: "Disconnected"
        case .catchingUp: "Catching up"
        case .connected: "Connected"
        case let .reconnecting(attempt): "Reconnect \(attempt)"
        }
    }

    private func reload() async {
        do {
            async let r = session.load(path: "/api/v1/readiness", cacheKey: "home-readiness")
            async let h = session.load(path: "/api/v1/system/health", cacheKey: "home-health")
            async let p = session.load(path: "/api/v1/research-programs", cacheKey: "programs")
            async let a = session.load(path: "/api/v1/alpha-library", cacheKey: "alphas")
            async let ap = session.load(path: "/api/v1/approvals", cacheKey: "approvals")
            async let ho = session.load(path: "/api/v1/handoffs", cacheKey: "handoffs")
            let values = try await (r, h, p, a, ap, ho)
            readiness = values.0; health = values.1; programs = values.2.normalizedItems; alphas = values.3.normalizedItems; approvals = values.4.normalizedItems; handoffs = values.5.normalizedItems
            error = nil
        } catch { self.error = error.localizedDescription }
    }
}

private struct MetricCard: View {
    let label: String
    let value: String
    var body: some View {
        VStack(alignment: .leading, spacing: 8) { Text(label).font(.caption).foregroundStyle(.secondary); Text(value).font(.title2.monospacedDigit()) }
            .padding().frame(maxWidth: .infinity, alignment: .leading).background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12))
    }
}
