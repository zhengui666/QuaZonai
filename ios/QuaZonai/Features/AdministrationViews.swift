import SwiftUI

struct AdministrationView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var readiness: JSONValue?
    @State private var health: JSONValue?
    @State private var runtime: JSONValue?
    @State private var mandates: [JSONValue] = []
    @State private var sources: [JSONValue] = []
    @State private var datasets: [JSONValue] = []
    @State private var universes: [JSONValue] = []
    @State private var downstreams: [JSONValue] = []
    @State private var plugins: [JSONValue] = []
    @State private var approvals: [JSONValue] = []
    @State private var devices: [JSONValue] = []
    @State private var error: String?
    @State private var showDataSource = false
    @State private var showDownstream = false

    var body: some View {
        List {
            if let error { Text(error).foregroundStyle(.red) }
            Section("Readiness / Health") {
                if let readiness { JSONDocumentView(value: readiness) }
                if let health { JSONDocumentView(value: health) }
            }
            Section("Runtime Configuration") {
                if let runtime { RuntimeConfigurationEditor(configuration: runtime) { await reload() } }
                else { ProgressView() }
            }
            Section("Mandates") {
                ForEach(Array(mandates.enumerated()), id: \.offset) { _, mandate in MandateAdminRow(mandate: mandate) { await reload() } }
            }
            Section("Data Sources") {
                Button("Register Data Source") { showDataSource = true }
                JSONRecords(records: sources)
            }
            Section("Datasets") { JSONRecords(records: datasets) }
            Section("Universes") { JSONRecords(records: universes) }
            Section("Downstreams") {
                Button("Register Downstream") { showDownstream = true }
                JSONRecords(records: downstreams)
            }
            Section("Plugin Releases") { JSONRecords(records: plugins) }
            Section("Capital Context") {
                let contexts = approvals.compactMap { approval -> JSONValue? in
                    guard let context = approval["capital_context"], context.objectValue != nil else { return nil }
                    var object = context.objectValue ?? [:]
                    object["purpose"] = approval["purpose"] ?? .null
                    object["candidate_id"] = approval["candidate_id"] ?? .null
                    return .object(object)
                }
                JSONRecords(records: contexts)
            }
            Section("Mobile Devices") { JSONRecords(records: devices) }
        }
        .navigationTitle(L10n.text(.administration, session.language))
        .task { await reload() }
        .refreshable { await reload() }
        .sheet(isPresented: $showDataSource) { DataSourceRegistrationSheet { showDataSource = false; await reload() } }
        .sheet(isPresented: $showDownstream) { DownstreamRegistrationSheet { showDownstream = false; await reload() } }
    }

    private func reload() async {
        do {
            async let r = session.load(path: "/api/v1/readiness", cacheKey: "admin-readiness")
            async let h = session.load(path: "/api/v1/system/health", cacheKey: "admin-health")
            async let rc = session.load(path: "/api/v1/system/runtime-configuration", cacheKey: "runtime-config")
            async let m = session.load(path: "/api/v1/portfolio-mandates", cacheKey: "portfolio-mandates")
            async let s = session.load(path: "/api/v1/data-sources", cacheKey: "data-sources")
            async let ds = session.load(path: "/api/v1/datasets", cacheKey: "datasets")
            async let u = session.load(path: "/api/v1/universes", cacheKey: "universes")
            async let d = session.load(path: "/api/v1/downstream-systems", cacheKey: "downstreams")
            async let a = session.load(path: "/api/v1/approvals", cacheKey: "approvals")
            async let md = session.load(path: "/api/v1/auth/mobile/devices", cacheKey: nil, offlineReadable: false)
            let values = try await (r, h, rc, m, s, ds, u, d, a, md)
            readiness = values.0; health = values.1; runtime = values.2
            mandates = values.3.normalizedItems; sources = values.4.normalizedItems; datasets = values.5.normalizedItems
            universes = values.6.normalizedItems; downstreams = values.7.normalizedItems; approvals = values.8.normalizedItems; devices = values.9.normalizedItems
            do { plugins = try await session.load(path: "/api/v1/plugin-releases", cacheKey: "plugins").normalizedItems } catch { plugins = [] }
            error = nil
        } catch { self.error = error.localizedDescription }
    }
}

private struct JSONRecords: View {
    let records: [JSONValue]
    var body: some View {
        if records.isEmpty { Text("—").foregroundStyle(.secondary) }
        ForEach(Array(records.enumerated()), id: \.offset) { _, record in
            DisclosureGroup(record.listTitle) { JSONDocumentView(value: record) }
        }
    }
}

private struct MandateAdminRow: View {
    @EnvironmentObject private var session: SessionStore
    let mandate: JSONValue
    let reload: () async -> Void
    @State private var busy = false
    @State private var error: String?
    var body: some View {
        let enabled = mandate.objectValue?.bool("enabled") ?? false
        VStack(alignment: .leading, spacing: 6) {
            Toggle(isOn: Binding(get: { enabled }, set: { _ in Task { await toggle(enabled: enabled) } })) {
                VStack(alignment: .leading) { Text(mandate.listTitle); Text(mandate.objectValue?.string("objective") ?? mandate["spec_json"]?.objectValue?.string("objective") ?? "Versioned portfolio mandate").font(.caption).foregroundStyle(.secondary) }
            }.disabled(busy)
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }
    }
    private func toggle(enabled: Bool) async {
        guard let id = mandate.stableID else { return }
        busy = true; defer { busy = false }
        do { _ = try await session.mutate(path: "/api/v1/portfolio-mandates/\(id)/\(enabled ? "disable" : "enable")"); await reload() }
        catch { self.error = error.localizedDescription }
    }
}

private struct RuntimeConfigurationEditor: View {
    @EnvironmentObject private var session: SessionStore
    let configuration: JSONValue
    let reload: () async -> Void
    @State private var model = ""
    @State private var baseURL = ""
    @State private var apiKey = ""
    @State private var clearAPIKey = false
    @State private var maxWheel = ""
    @State private var pluginValidation = ""
    @State private var bundleBuild = ""
    @State private var pluginJob = ""
    @State private var missionJob = ""
    @State private var pollSeconds = ""
    @State private var leaseSeconds = ""
    @State private var initialized = false
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let object = configuration.objectValue {
                LabeledContent("Revision", value: object.string("revision") ?? "—")
                LabeledContent("Codex auth configured", value: (object.bool("codex_api_key_configured") == true || object.bool("codex_login_configured") == true) ? "YES" : "NO")
            }
            TextField("Codex Model", text: $model)
            TextField("Codex Base URL", text: $baseURL).textInputAutocapitalization(.never).autocorrectionDisabled()
            SecureField("Codex API Key (write only)", text: $apiKey).textContentType(.password)
            Toggle("Clear Codex API Key", isOn: $clearAPIKey)
            Group {
                TextField("Max Plugin Wheel Bytes", text: $maxWheel).keyboardType(.numberPad)
                TextField("Plugin Validation Timeout", text: $pluginValidation).keyboardType(.numberPad)
                TextField("Bundle Build Timeout", text: $bundleBuild).keyboardType(.numberPad)
                TextField("Plugin Job Timeout", text: $pluginJob).keyboardType(.numberPad)
                TextField("Mission Job Timeout", text: $missionJob).keyboardType(.numberPad)
                TextField("Job Poll Seconds", text: $pollSeconds).keyboardType(.decimalPad)
                TextField("Job Lease Seconds", text: $leaseSeconds).keyboardType(.numberPad)
            }
            Button(L10n.text(.save, session.language)) { Task { await save() } }.disabled(busy).buttonStyle(.borderedProminent)
            DisclosureGroup("All runtime fields") { JSONTreeView(value: configuration) }
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .onAppear { initialize() }
        .onChange(of: configuration) { _, _ in initialized = false; initialize() }
    }

    private func initialize() {
        guard !initialized, let object = configuration.objectValue else { return }
        model = object.string("codex_model") ?? ""; baseURL = object.string("codex_base_url") ?? ""
        maxWheel = object.string("max_plugin_wheel_bytes") ?? ""; pluginValidation = object.string("plugin_validation_timeout_seconds") ?? ""
        bundleBuild = object.string("bundle_build_timeout_seconds") ?? ""; pluginJob = object.string("plugin_job_timeout_seconds") ?? ""
        missionJob = object.string("mission_job_timeout_seconds") ?? ""; pollSeconds = object.string("job_poll_seconds") ?? ""; leaseSeconds = object.string("job_lease_seconds") ?? ""
        initialized = true
    }

    private func save() async {
        guard let object = configuration.objectValue, let revision = object.number("revision") else { return }
        if (!apiKey.isEmpty || clearAPIKey) && !(await BiometricGate.authorize(reason: "Change the Codex provider credential")) { error = "Biometric confirmation was not completed."; return }
        busy = true
        defer { busy = false; apiKey = "" }
        var payload: [String: JSONValue] = [
            "expected_revision": .number(revision),
            "codex_model": model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .null : .string(model.trimmingCharacters(in: .whitespacesAndNewlines)),
            "codex_base_url": baseURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .null : .string(baseURL.trimmingCharacters(in: .whitespacesAndNewlines)),
            "clear_codex_api_key": .bool(clearAPIKey),
            "max_plugin_wheel_bytes": .number(Double(maxWheel) ?? object.number("max_plugin_wheel_bytes") ?? 0),
            "plugin_validation_timeout_seconds": .number(Double(pluginValidation) ?? object.number("plugin_validation_timeout_seconds") ?? 0),
            "bundle_build_timeout_seconds": .number(Double(bundleBuild) ?? object.number("bundle_build_timeout_seconds") ?? 0),
            "plugin_job_timeout_seconds": .number(Double(pluginJob) ?? object.number("plugin_job_timeout_seconds") ?? 0),
            "mission_job_timeout_seconds": .number(Double(missionJob) ?? object.number("mission_job_timeout_seconds") ?? 0),
            "job_poll_seconds": .number(Double(pollSeconds) ?? object.number("job_poll_seconds") ?? 0),
            "job_lease_seconds": .number(Double(leaseSeconds) ?? object.number("job_lease_seconds") ?? 0),
        ]
        if !apiKey.isEmpty { payload["codex_api_key"] = .string(apiKey) }
        do { _ = try await session.mutate(path: "/api/v1/system/runtime-configuration", method: .put, body: .object(payload)); clearAPIKey = false; error = nil; await reload() }
        catch { self.error = error.localizedDescription }
    }
}

private struct DataSourceRegistrationSheet: View {
    @EnvironmentObject private var session: SessionStore
    let completed: () async -> Void
    @State private var name = ""
    @State private var provider = ""
    @State private var fields = ""
    @State private var error: String?
    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name)
                TextField("Provider", text: $provider)
                TextField("Canonical Fields", text: $fields, prompt: Text("event_time, available_time, close, volume"))
                if let error { Text(error).foregroundStyle(.red) }
            }.navigationTitle("Register Data Source").toolbar {
                ToolbarItem(placement: .confirmationAction) { Button(L10n.text(.register, session.language)) { Task { await submit() } }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
            }
        }
    }
    private func submit() async {
        let canonical = fields.split(separator: ",").map { JSONValue.string($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
        do { _ = try await session.mutate(path: "/api/v1/data-sources", body: .object(["name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)), "provider": .string(provider.trimmingCharacters(in: .whitespacesAndNewlines)), "fields": .array(canonical), "state": .string("STAGED")])); await completed() }
        catch { self.error = error.localizedDescription }
    }
}

private struct DownstreamRegistrationSheet: View {
    @EnvironmentObject private var session: SessionStore
    let completed: () async -> Void
    @State private var name = ""
    @State private var environment = "PAPER"
    @State private var packageContract = ""
    @State private var feedbackContract = ""
    @State private var error: String?
    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name)
                Picker("Environment", selection: $environment) { ForEach(["PAPER", "LIVE", "EXTERNAL_BACKTEST"], id: \.self) { Text($0).tag($0) } }
                TextField("Package Contract Version", text: $packageContract)
                TextField("Feedback Contract Version", text: $feedbackContract)
                if let error { Text(error).foregroundStyle(.red) }
            }.navigationTitle("Register Downstream").toolbar {
                ToolbarItem(placement: .confirmationAction) { Button(L10n.text(.register, session.language)) { Task { await submit() } }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
            }
        }
    }
    private func submit() async {
        if environment == "LIVE" && !(await BiometricGate.authorize(reason: "Register a Live downstream")) { error = "Biometric confirmation was not completed."; return }
        var payload: [String: JSONValue] = ["name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)), "environment_type": .string(environment), "enabled": .bool(true)]
        if !packageContract.isEmpty { payload["package_contract_version"] = .string(packageContract) }
        if !feedbackContract.isEmpty { payload["feedback_contract_version"] = .string(feedbackContract) }
        do { _ = try await session.mutate(path: "/api/v1/downstream-systems", body: .object(payload)); await completed() }
        catch { self.error = error.localizedDescription }
    }
}

struct DeviceSecurityView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var devices: [JSONValue] = []
    @State private var error: String?
    var body: some View {
        List {
            ForEach(Array(devices.enumerated()), id: \.offset) { _, device in
                VStack(alignment: .leading, spacing: 6) {
                    RecordRow(item: device)
                    DisclosureGroup("Session details") { JSONTreeView(value: device) }
                    if device.objectValue?.string("revoked_at") == nil, let id = device.stableID {
                        Button("Revoke device", role: .destructive) { Task { await revoke(id) } }
                    }
                }
            }
            if devices.isEmpty && error == nil { Text(L10n.text(.empty, session.language)).foregroundStyle(.secondary) }
            if let error { Text(error).foregroundStyle(.red) }
        }.navigationTitle(L10n.text(.accountSecurity, session.language)).task { await reload() }.refreshable { await reload() }
    }
    private func reload() async {
        do { devices = try await session.load(path: "/api/v1/auth/mobile/devices", cacheKey: nil, offlineReadable: false).normalizedItems; error = nil }
        catch { self.error = error.localizedDescription }
    }
    private func revoke(_ id: String) async {
        guard await BiometricGate.authorize(reason: "Revoke a trusted native device") else { error = "Biometric confirmation was not completed."; return }
        do { _ = try await session.mutate(path: "/api/v1/auth/mobile/devices/\(id)/revoke"); await reload() }
        catch { self.error = error.localizedDescription }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var session: SessionStore
    var body: some View {
        Form {
            Section(L10n.text(.server, session.language)) { Text(session.profile).textSelection(.enabled) }
            Section(L10n.text(.language, session.language)) {
                Picker(L10n.text(.language, session.language), selection: Binding(get: { session.language }, set: { session.setLanguage($0) })) {
                    ForEach(AppLanguage.allCases) { Text($0.displayName).tag($0) }
                }
            }
            Section(L10n.text(.appearance, session.language)) {
                Picker(L10n.text(.appearance, session.language), selection: Binding(get: { session.appearance }, set: { session.setAppearance($0) })) {
                    ForEach(AppAppearance.allCases) { Text($0.rawValue.capitalized).tag($0) }
                }.pickerStyle(.segmented)
            }
            Section {
                NavigationLink(L10n.text(.accountSecurity, session.language)) { DeviceSecurityView() }
                Button(L10n.text(.logout, session.language), role: .destructive) { Task { await session.logout() } }
                Button("Forget server", role: .destructive) { Task { await session.forgetServer() } }
            }
        }.navigationTitle(L10n.text(.settings, session.language))
    }
}
