import SwiftData
import SwiftUI

struct RootView: View {
    @EnvironmentObject private var session: SessionStore
    @Environment(\.modelContext) private var modelContext

    var body: some View {
        Group {
            switch session.phase {
            case .serverSetup: ServerSetupView()
            case .connecting: ProgressView(L10n.text(.connecting, session.language))
            case .loginRequired: NativeLoginView()
            case .trustedUnlockAvailable: TrustedUnlockView()
            case .ready: WorkbenchShell()
            case let .incompatible(message): ContentUnavailableView(L10n.text(.incompatible, session.language), systemImage: "arrow.down.app", description: Text(message))
            }
        }
        .task { session.attachCache(modelContext); await session.begin() }
    }
}

private struct ServerSetupView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var serverURL = ""
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(L10n.text(.server, session.language), text: $serverURL)
                        .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                        .accessibilityIdentifier("server-url")
                    Text(L10n.text(.secureServerHint, session.language)).font(.caption).foregroundStyle(.secondary)
                }
                Section {
                    Button(L10n.text(.connect, session.language)) { Task { await session.connect(to: serverURL) } }
                        .disabled(serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        .accessibilityIdentifier("connect-button")
                }
                if let error = session.errorMessage { Section { Text(error).foregroundStyle(.red) } }
            }
            .navigationTitle("QuaZonai")
        }
    }
}

private struct NativeLoginView: View {
    @EnvironmentObject private var session: SessionStore
    @State private var totp = ""
    @State private var trustDevice = false
    @State private var busy = false
    var body: some View {
        NavigationStack {
            Form {
                Section("Native Operator Authentication") {
                    Text("This app authenticates with the current TOTP only. It never asks for or sends a username or password.").font(.callout)
                    TextField(L10n.text(.totp, session.language), text: $totp)
                        .keyboardType(.numberPad).textContentType(.oneTimeCode)
                        .accessibilityIdentifier("totp-code")
                    Toggle(L10n.text(.trustDevice, session.language), isOn: $trustDevice).accessibilityIdentifier("trust-device")
                }
                Section {
                    Button(L10n.text(.signIn, session.language)) {
                        let code = totp
                        totp = ""
                        busy = true
                        Task { _ = await session.login(totpCode: code, trustDevice: trustDevice); busy = false }
                    }
                    .disabled(totp.count != 6 || busy)
                    .accessibilityIdentifier("sign-in")
                }
                if let error = session.errorMessage { Section { Text(error).foregroundStyle(.red) } }
            }
            .navigationTitle("QuaZonai")
        }
    }
}

private struct TrustedUnlockView: View {
    @EnvironmentObject private var session: SessionStore
    var body: some View {
        NavigationStack {
            VStack(spacing: 18) {
                Image(systemName: "faceid").font(.system(size: 52))
                Text("A trusted-device refresh credential is protected by this device's Keychain and biometrics.").multilineTextAlignment(.center)
                Button(L10n.text(.unlock, session.language)) { Task { await session.unlockTrustedDevice() } }.buttonStyle(.borderedProminent)
                Button("Use TOTP instead") {
                    // This is an authentication-path choice, not logout. Keep the
                    // protected refresh credential until a successful TOTP login
                    // rotates the server generation and replaces or removes it.
                    session.errorMessage = nil
                    session.phase = .loginRequired
                }
                .buttonStyle(.bordered)
                if let error = session.errorMessage { Text(error).foregroundStyle(.red) }
            }.padding().navigationTitle("QuaZonai")
        }
    }
}

private enum CompactTab: Hashable { case home, research, approvals, portfolio, more }

private struct WorkbenchShell: View {
    @EnvironmentObject private var session: SessionStore
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var selectedSection: AppSection = .home
    @State private var compactTab: CompactTab = .home
    @State private var morePath: [AppSection] = []
    @State private var columns: NavigationSplitViewVisibility = .all

    var body: some View {
        Group {
            if sizeClass == .regular {
                NavigationSplitView(columnVisibility: $columns) {
                    List(AppSection.allCases) { section in
                        Button {
                            selectedSection = section
                        } label: {
                            Label(
                                L10n.text(section.titleKey, session.language),
                                systemImage: section.icon
                            )
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .listRowBackground(
                            selectedSection == section
                                ? Color.accentColor.opacity(0.14)
                                : Color.clear
                        )
                        .accessibilityAddTraits(
                            selectedSection == section ? .isSelected : []
                        )
                    }
                    .navigationTitle("QuaZonai")
                } content: {
                    NavigationStack { sectionView(selectedSection) }
                } detail: {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 12) {
                            Label(L10n.text(selectedSection.titleKey, session.language), systemImage: selectedSection.icon).font(.title2.bold())
                            Text("All server fields remain available in the collection/detail column. Resize, Split View and Stage Manager change layout only; they do not remove Operator capabilities.").foregroundStyle(.secondary)
                            Text("QuaZonai never exposes broker credentials, orders, positions, runtime stop or liquidation controls.").font(.callout)
                        }.padding()
                    }.navigationTitle("Inspector")
                }
            } else {
                TabView(selection: $compactTab) {
                    Tab(
                        L10n.text(.home, session.language),
                        systemImage: "house",
                        value: CompactTab.home
                    ) {
                        NavigationStack { HomeView(navigate: navigateCompact) }
                    }
                    Tab(
                        L10n.text(.research, session.language),
                        systemImage: "point.3.connected.trianglepath.dotted",
                        value: CompactTab.research
                    ) {
                        NavigationStack { ResearchListView() }
                    }
                    Tab(
                        L10n.text(.approvals, session.language),
                        systemImage: "checkmark.seal",
                        value: CompactTab.approvals
                    ) {
                        NavigationStack { ApprovalInboxView() }
                    }
                    Tab(
                        L10n.text(.portfolio, session.language),
                        systemImage: "chart.pie",
                        value: CompactTab.portfolio
                    ) {
                        NavigationStack { PortfolioLabView() }
                    }
                    Tab(
                        L10n.text(.more, session.language),
                        systemImage: "ellipsis",
                        value: CompactTab.more
                    ) {
                        NavigationStack(path: $morePath) {
                            MoreView()
                                .navigationDestination(for: AppSection.self) { section in
                                    sectionView(section)
                                }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder private func sectionView(_ section: AppSection) -> some View {
        switch section {
        case .home: HomeView { selectedSection = $0 }
        case .idea: IdeaComposerView { selectedSection = $0 }
        case .research: ResearchListView()
        case .alpha: AlphaLibraryView()
        case .portfolio: PortfolioLabView()
        case .approvals: ApprovalInboxView()
        case .handoff: HandoffFeedbackView()
        case .administration: AdministrationView()
        }
    }

    private func navigateCompact(_ section: AppSection) {
        switch section {
        case .home: compactTab = .home
        case .research: compactTab = .research
        case .approvals: compactTab = .approvals
        case .portfolio: compactTab = .portfolio
        default: compactTab = .more; morePath = [section]
        }
    }
}

private struct MoreView: View {
    @EnvironmentObject private var session: SessionStore
    var body: some View {
        List {
            NavigationLink(value: AppSection.idea) { Label(L10n.text(.idea, session.language), systemImage: AppSection.idea.icon) }
            NavigationLink(value: AppSection.alpha) { Label(L10n.text(.alpha, session.language), systemImage: AppSection.alpha.icon) }
            NavigationLink(value: AppSection.handoff) { Label(L10n.text(.handoff, session.language), systemImage: AppSection.handoff.icon) }
            NavigationLink(value: AppSection.administration) { Label(L10n.text(.administration, session.language), systemImage: AppSection.administration.icon) }
            Section {
                NavigationLink { LanguageSettingsView() } label: { Label(L10n.text(.language, session.language), systemImage: "globe") }
                NavigationLink { AppearanceSettingsView() } label: { Label(L10n.text(.appearance, session.language), systemImage: "circle.lefthalf.filled") }
                NavigationLink { DeviceSecurityView() } label: { Label(L10n.text(.accountSecurity, session.language), systemImage: "lock.shield") }
                NavigationLink { SettingsView() } label: { Label(L10n.text(.settings, session.language), systemImage: "gear") }
            }
        }.navigationTitle(L10n.text(.more, session.language))
    }
}

private struct LanguageSettingsView: View {
    @EnvironmentObject private var session: SessionStore
    var body: some View {
        List(AppLanguage.allCases) { language in
            Button { session.setLanguage(language) } label: { HStack { Text(language.displayName); Spacer(); if language == session.language { Image(systemName: "checkmark") } } }
        }.navigationTitle(L10n.text(.language, session.language))
    }
}

private struct AppearanceSettingsView: View {
    @EnvironmentObject private var session: SessionStore
    var body: some View {
        List(AppAppearance.allCases) { appearance in
            Button { session.setAppearance(appearance) } label: { HStack { Text(appearance.rawValue.capitalized); Spacer(); if appearance == session.appearance { Image(systemName: "checkmark") } } }
        }.navigationTitle(L10n.text(.appearance, session.language))
    }
}
