import Combine
import SwiftData
import SwiftUI

struct AdministrationView: View {
    private let sections: [(LocalizedStringKey, String, AnyView)] = [
        ("admin.runtime", "slider.horizontal.3", AnyView(RuntimeConfigurationView())),
        ("Runtime Health", "heart.text.square", AnyView(AdminResourceScreen(
            title: "Runtime Health",
            endpoint: "/api/v1/system/health",
            cacheKey: "admin.health"
        ))),
        ("Readiness", "checkmark.circle", AnyView(AdminResourceScreen(
            title: "Readiness",
            endpoint: "/api/v1/readiness",
            cacheKey: "admin.readiness"
        ))),
        ("admin.mandates", "scope", AnyView(MandateAdministrationView())),
        ("admin.sources", "externaldrive.connected.to.line.below", AnyView(DataSourceAdministrationView())),
        ("admin.datasets", "cylinder.split.1x2", AnyView(AdminCollectionScreen(
            title: String(localized: "admin.datasets"),
            endpoint: "/api/v1/datasets",
            cacheKey: "admin.datasets"
        ))),
        ("admin.universes", "globe", AnyView(AdminCollectionScreen(
            title: String(localized: "admin.universes"),
            endpoint: "/api/v1/universes",
            cacheKey: "admin.universes"
        ))),
        ("admin.downstreams", "arrowshape.turn.up.right", AnyView(DownstreamAdministrationView())),
        ("admin.plugins", "shippingbox", AnyView(AdminCollectionScreen(
            title: String(localized: "admin.plugins"),
            endpoint: "/api/v1/plugin-releases",
            cacheKey: "admin.plugins"
        ))),
        ("admin.capital", "banknote", AnyView(CapitalContextView())),
        ("security.title", "lock.shield", AnyView(DeviceSecurityView())),
    ]

    var body: some View {
        List {
            Section {
                ForEach(Array(sections.enumerated()), id: \.offset) { _, section in
                    NavigationLink {
                        section.2
                    } label: {
                        Label {
                            Text(section.0)
                        } icon: {
                            Image(systemName: section.1)
                        }
                    }
                }
            } footer: {
                Text("Administration changes operator configuration and registries only. It never controls orders, positions, or downstream runtime lifecycle.")
            }
        }
        .navigationTitle("admin.title")
        .accessibilityIdentifier("administration-screen")
    }
}

private struct AdminResourceScreen: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader: ResourceDetailLoader
    let title: String

    init(title: String, endpoint: String, cacheKey: String) {
        self.title = title
        _loader = StateObject(
            wrappedValue: ResourceDetailLoader(endpoint: endpoint, cacheKey: cacheKey)
        )
    }

    var body: some View {
        Group {
            if loader.isLoading && loader.value == nil {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.value == nil {
                ErrorStateView(message: error) {
                    Task { await loader.load(model: model, context: modelContext) }
                }
            } else if let value = loader.value {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        if loader.loadedFromCache { OfflineReadOnlyBanner() }
                        JSONInspector(value: value)
                    }
                    .padding()
                }
            } else {
                EmptyStateView(title: title, description: "No data is available.")
            }
        }
        .navigationTitle(title)
        .task { await loader.load(model: model, context: modelContext) }
    }
}

private struct AdminCollectionScreen: View {
    let title: String
    let endpoint: String
    let cacheKey: String

    var body: some View {
        ResourceCollectionScreen(
            title: title,
            endpoint: endpoint,
            cacheKey: cacheKey,
            emptyDescription: "No registered resources."
        ) { value in
            ScrollView { JSONInspector(value: value).padding() }
                .navigationTitle(title)
        }
    }
}

// MARK: - Runtime Configuration

@MainActor
private final class RuntimeConfigurationLoader: ObservableObject {
    @Published var value: JSONValue?
    @Published var isLoading = false
    @Published var errorMessage: String?

    func load(model: AppModel) async {
        isLoading = value == nil
        errorMessage = nil
        defer { isLoading = false }
        do {
            value = try await model.request(path: "/api/v1/system/runtime-configuration")
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct RuntimeConfigurationView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var loader = RuntimeConfigurationLoader()
    @State private var codexModel = ""
    @State private var codexBaseURL = ""
    @State private var codexAPIKey = ""
    @State private var clearAPIKey = false
    @State private var maxWheelBytes = ""
    @State private var pluginValidationTimeout = ""
    @State private var bundleBuildTimeout = ""
    @State private var pluginJobTimeout = ""
    @State private var missionJobTimeout = ""
    @State private var jobPollSeconds = ""
    @State private var jobLeaseSeconds = ""
    @State private var populatedRevision: Int?
    @State private var isSaving = false
    @State private var saveError: String?
    @State private var saveKey = UUID()

    var body: some View {
        Group {
            if loader.isLoading && loader.value == nil {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.value == nil {
                ErrorStateView(message: error) { Task { await loader.load(model: model) } }
            } else {
                Form {
                    Section("admin.runtime") {
                        TextField("Codex Model", text: $codexModel)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        TextField("Codex Base URL", text: $codexBaseURL)
                            .keyboardType(.URL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        SecureField("admin.apiKey", text: $codexAPIKey)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .accessibilityIdentifier("codex-api-key")
                        Toggle("admin.clearApiKey", isOn: $clearAPIKey)
                        LabeledContent("Codex auth configured") {
                            let configured = loader.value?["codex_api_key_configured"]?.boolValue == true
                                || loader.value?["codex_login_configured"]?.boolValue == true
                            StatePill(state: configured ? "CONFIGURED" : "NOT_CONFIGURED")
                        }
                        LabeledContent("Revision") {
                            Text(loader.value?["revision"]?.displayText ?? "—")
                                .monospacedDigit()
                        }
                    }
                    Section("Limits and timeouts") {
                        numericField("Max Plugin Wheel Bytes", text: $maxWheelBytes)
                        numericField("Plugin Validation Timeout", text: $pluginValidationTimeout)
                        numericField("Bundle Build Timeout", text: $bundleBuildTimeout)
                        numericField("Plugin Job Timeout", text: $pluginJobTimeout)
                        numericField("Mission Job Timeout", text: $missionJobTimeout)
                        numericField("Job Poll Seconds", text: $jobPollSeconds, decimal: true)
                        numericField("Job Lease Seconds", text: $jobLeaseSeconds)
                    }
                    Section {
                        Button {
                            Task { await save() }
                        } label: {
                            if isSaving {
                                ProgressView().frame(maxWidth: .infinity)
                            } else {
                                Label("common.save", systemImage: "square.and.arrow.down")
                                    .frame(maxWidth: .infinity)
                            }
                        }
                        .disabled(isSaving || !model.network.isOnline || !formIsValid)
                        .accessibilityIdentifier("save-runtime-configuration")
                    } footer: {
                        Text("The API key stays only in this SecureField until the request completes. It is never placed in Keychain, SwiftData, or logs.")
                    }
                    if let saveError {
                        Section { Text(saveError).foregroundStyle(.red) }
                    }
                }
            }
        }
        .navigationTitle("admin.runtime")
        .task {
            await loader.load(model: model)
            populateIfNeeded()
        }
        .onChange(of: loader.value) { _, _ in populateIfNeeded() }
    }

    @ViewBuilder
    private func numericField(
        _ title: String,
        text: Binding<String>,
        decimal: Bool = false
    ) -> some View {
        TextField(title, text: text)
            .keyboardType(decimal ? .decimalPad : .numberPad)
            .multilineTextAlignment(.trailing)
    }

    private var formIsValid: Bool {
        Int(maxWheelBytes) != nil
            && Int(pluginValidationTimeout) != nil
            && Int(bundleBuildTimeout) != nil
            && Int(pluginJobTimeout) != nil
            && Int(missionJobTimeout) != nil
            && Double(jobPollSeconds) != nil
            && Int(jobLeaseSeconds) != nil
            && populatedRevision != nil
            && !(clearAPIKey && !codexAPIKey.isEmpty)
    }

    private func populateIfNeeded() {
        guard let value = loader.value,
              let revisionNumber = value["revision"]?.numberValue
        else { return }
        let revision = Int(revisionNumber)
        guard populatedRevision != revision else { return }
        populatedRevision = revision
        codexModel = value["codex_model"]?.stringValue ?? ""
        codexBaseURL = value["codex_base_url"]?.stringValue ?? ""
        maxWheelBytes = value["max_plugin_wheel_bytes"]?.displayText ?? ""
        pluginValidationTimeout = value["plugin_validation_timeout_seconds"]?.displayText ?? ""
        bundleBuildTimeout = value["bundle_build_timeout_seconds"]?.displayText ?? ""
        pluginJobTimeout = value["plugin_job_timeout_seconds"]?.displayText ?? ""
        missionJobTimeout = value["mission_job_timeout_seconds"]?.displayText ?? ""
        jobPollSeconds = value["job_poll_seconds"]?.displayText ?? ""
        jobLeaseSeconds = value["job_lease_seconds"]?.displayText ?? ""
    }

    private func save() async {
        guard let expectedRevision = populatedRevision,
              let maxWheel = Int(maxWheelBytes),
              let pluginValidation = Int(pluginValidationTimeout),
              let bundleTimeout = Int(bundleBuildTimeout),
              let pluginTimeout = Int(pluginJobTimeout),
              let missionTimeout = Int(missionJobTimeout),
              let pollSeconds = Double(jobPollSeconds),
              let leaseSeconds = Int(jobLeaseSeconds)
        else { return }
        isSaving = true
        saveError = nil
        defer {
            codexAPIKey = ""
            isSaving = false
        }
        do {
            let latest = try await model.request(path: "/api/v1/system/runtime-configuration")
            guard let latestNumber = latest["revision"]?.numberValue,
                  Int(latestNumber) == expectedRevision
            else {
                loader.value = latest
                populatedRevision = nil
                populateIfNeeded()
                throw APIError.conflict(
                    code: "RUNTIME_CONFIGURATION_REVISION_CONFLICT",
                    message: "Runtime Configuration changed on the server. Review the refreshed values before saving."
                )
            }
            var payload: [String: JSONValue] = [
                "expected_revision": .number(Double(expectedRevision)),
                "codex_model": codexModel.isEmpty ? .null : .string(codexModel),
                "codex_base_url": codexBaseURL.isEmpty ? .null : .string(codexBaseURL),
                "clear_codex_api_key": .bool(clearAPIKey),
                "max_plugin_wheel_bytes": .number(Double(maxWheel)),
                "plugin_validation_timeout_seconds": .number(Double(pluginValidation)),
                "bundle_build_timeout_seconds": .number(Double(bundleTimeout)),
                "plugin_job_timeout_seconds": .number(Double(pluginTimeout)),
                "mission_job_timeout_seconds": .number(Double(missionTimeout)),
                "job_poll_seconds": .number(pollSeconds),
                "job_lease_seconds": .number(Double(leaseSeconds)),
            ]
            if !codexAPIKey.isEmpty { payload["codex_api_key"] = .string(codexAPIKey) }
            let result = try await model.performSensitive(
                reason: String(localized: "security.biometric")
            ) {
                try await model.request(
                    path: "/api/v1/system/runtime-configuration",
                    method: .put,
                    body: .object(payload),
                    idempotencyKey: saveKey
                )
            }
            loader.value = result
            populatedRevision = nil
            clearAPIKey = false
            saveKey = UUID()
            populateIfNeeded()
        } catch {
            saveError = error.localizedDescription
        }
    }
}

// MARK: - Mandates

struct MandateAdministrationView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader = ResourceLoader(
        endpoint: "/api/v1/portfolio-mandates",
        cacheKey: "admin.mandates"
    )
    @State private var errorMessage: String?

    var body: some View {
        Group {
            if loader.isLoading && loader.items.isEmpty {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.items.isEmpty {
                ErrorStateView(message: error) { reload() }
            } else {
                List {
                    if loader.loadedFromCache { OfflineReadOnlyBanner() }
                    ForEach(Array(loader.items.enumerated()), id: \.offset) { _, mandate in
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                ResourceRow(value: mandate)
                                Spacer()
                                Toggle(
                                    "",
                                    isOn: Binding(
                                        get: { mandate["enabled"]?.boolValue == true },
                                        set: { enabled in
                                            Task { await setEnabled(enabled, mandate: mandate) }
                                        }
                                    )
                                )
                                .labelsHidden()
                                .disabled(!model.network.isOnline)
                            }
                            if let spec = mandate["spec_json"] { JSONInspector(value: spec) }
                        }
                    }
                    if let errorMessage { Text(errorMessage).foregroundStyle(.red) }
                }
            }
        }
        .navigationTitle("admin.mandates")
        .task { await loader.load(model: model, context: modelContext) }
    }

    private func reload() {
        Task { await loader.load(model: model, context: modelContext) }
    }

    private func setEnabled(_ enabled: Bool, mandate: JSONValue) async {
        guard let id = mandate.resourceID else { return }
        errorMessage = nil
        do {
            _ = try await model.request(
                path: "/api/v1/portfolio-mandates/\(id)/\(enabled ? "enable" : "disable")",
                method: .post,
                body: .object([:])
            )
            await loader.load(model: model, context: modelContext)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Data Sources

struct DataSourceAdministrationView: View {
    @EnvironmentObject private var model: AppModel
    @State private var showRegistration = false

    var body: some View {
        AdminCollectionScreen(
            title: String(localized: "admin.sources"),
            endpoint: "/api/v1/data-sources",
            cacheKey: "admin.sources"
        )
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showRegistration = true
                } label: {
                    Label("Register", systemImage: "plus")
                }
                .disabled(!model.network.isOnline)
            }
        }
        .sheet(isPresented: $showRegistration) {
            DataSourceRegistrationView { showRegistration = false }
        }
    }
}

private struct DataSourceRegistrationView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var provider = ""
    @State private var fields = ""
    @State private var cadence = ""
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var idempotencyKey = UUID()
    let completed: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name)
                TextField("Provider", text: $provider)
                TextField("Canonical Fields", text: $fields, axis: .vertical)
                TextField("Update Cadence", text: $cadence)
                if let errorMessage { Text(errorMessage).foregroundStyle(.red) }
            }
            .navigationTitle("Register Data Source")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("common.cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Register") { Task { await register() } }
                        .disabled(
                            name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                || fields.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                || isWorking
                        )
                }
            }
        }
    }

    private func register() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        let canonicalFields = fields.split(separator: ",").map {
            JSONValue.string($0.trimmingCharacters(in: .whitespacesAndNewlines))
        }.filter { $0.stringValue?.isEmpty == false }
        var payload: [String: JSONValue] = [
            "name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)),
            "provider": .string(provider.trimmingCharacters(in: .whitespacesAndNewlines)),
            "fields": .array(canonicalFields),
            "state": .string("STAGED"),
        ]
        if !cadence.isEmpty { payload["update_cadence"] = .string(cadence) }
        do {
            _ = try await model.request(
                path: "/api/v1/data-sources",
                method: .post,
                body: .object(payload),
                idempotencyKey: idempotencyKey
            )
            idempotencyKey = UUID()
            completed()
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Downstreams

struct DownstreamAdministrationView: View {
    @EnvironmentObject private var model: AppModel
    @State private var showRegistration = false

    var body: some View {
        AdminCollectionScreen(
            title: String(localized: "admin.downstreams"),
            endpoint: "/api/v1/downstream-systems",
            cacheKey: "admin.downstreams"
        )
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showRegistration = true
                } label: {
                    Label("Register", systemImage: "plus")
                }
                .disabled(!model.network.isOnline)
            }
        }
        .sheet(isPresented: $showRegistration) {
            DownstreamRegistrationView { showRegistration = false }
        }
    }
}

private struct DownstreamRegistrationView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var environment = "PAPER"
    @State private var enabled = true
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var idempotencyKey = UUID()
    let completed: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name)
                Picker("Environment", selection: $environment) {
                    Text("PAPER").tag("PAPER")
                    Text("LIVE").tag("LIVE")
                    Text("EXTERNAL BACKTEST").tag("EXTERNAL_BACKTEST")
                }
                Toggle("Enabled", isOn: $enabled)
                if let errorMessage { Text(errorMessage).foregroundStyle(.red) }
            }
            .navigationTitle("Register Downstream")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("common.cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Register") { Task { await register() } }
                        .disabled(
                            name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                || isWorking
                        )
                }
            }
        }
    }

    private func register() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        let operation: @MainActor () async throws -> JSONValue = {
            try await model.request(
                path: "/api/v1/downstream-systems",
                method: .post,
                body: .object([
                    "name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)),
                    "environment_type": .string(environment),
                    "enabled": .bool(enabled),
                ]),
                idempotencyKey: idempotencyKey
            )
        }
        do {
            if environment == "LIVE" {
                _ = try await model.performSensitive(
                    reason: String(localized: "security.biometric"),
                    operation: operation
                )
            } else {
                _ = try await operation()
            }
            idempotencyKey = UUID()
            completed()
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Capital Context

struct CapitalContextView: View {
    var body: some View {
        ResourceCollectionScreen(
            title: String(localized: "admin.capital"),
            endpoint: "/api/v1/approvals",
            cacheKey: "admin.capital-context",
            emptyDescription: "No Approval carries a Capital Context."
        ) { approval in
            ScrollView {
                JSONInspector(value: approval["capital_context"] ?? approval).padding()
            }
            .navigationTitle("admin.capital")
        }
    }
}

// MARK: - Device Security

struct DeviceSecurityView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.modelContext) private var modelContext
    @StateObject private var loader = ResourceLoader(
        endpoint: "/api/v1/auth/mobile/devices",
        cacheKey: "security.devices"
    )
    @State private var errorMessage: String?
    @State private var isWorking = false

    var body: some View {
        Group {
            if model.isDirectAccess {
                ContentUnavailableView {
                    Label("auth.direct", systemImage: "exclamationmark.shield")
                } description: {
                    Text("Trusted-device sessions exist only when Operator Authentication is enabled.")
                }
            } else if loader.isLoading && loader.items.isEmpty {
                LoadingStateView()
            } else if let error = loader.errorMessage, loader.items.isEmpty {
                ErrorStateView(message: error) { reload() }
            } else if loader.items.isEmpty {
                EmptyStateView(
                    title: String(localized: "security.title"),
                    description: "No trusted native devices are registered."
                )
            } else {
                List {
                    Section {
                        Label("security.biometric", systemImage: "faceid")
                    }
                    ForEach(Array(loader.items.enumerated()), id: \.offset) { _, device in
                        VStack(alignment: .leading, spacing: 10) {
                            ResourceRow(value: device)
                            JSONInspector(value: device)
                            if device["revoked_at"] == nil || device["revoked_at"] == .null {
                                Button("security.revoke", role: .destructive) {
                                    Task { await revoke(device) }
                                }
                                .disabled(isWorking || !model.network.isOnline)
                            }
                        }
                        .padding(.vertical, 5)
                    }
                    if let errorMessage { Text(errorMessage).foregroundStyle(.red) }
                }
            }
        }
        .navigationTitle("security.title")
        .task {
            if !model.isDirectAccess {
                await loader.load(model: model, context: modelContext)
            }
        }
        .accessibilityIdentifier("security-screen")
    }

    private func reload() {
        Task { await loader.load(model: model, context: modelContext) }
    }

    private func revoke(_ device: JSONValue) async {
        guard let id = device.resourceID else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            _ = try await model.performSensitive(
                reason: String(localized: "security.biometric")
            ) {
                try await model.request(
                    path: "/api/v1/auth/mobile/devices/\(id)/revoke",
                    method: .post,
                    body: .object([:])
                )
            }
            await loader.load(model: model, context: modelContext)
        } catch APIError.unauthorized {
            await model.logout()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
