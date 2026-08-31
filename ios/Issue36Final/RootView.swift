import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Group {
            switch model.phase {
            case .serverSetup:
                ServerSetupView()
            case .connecting:
                LoadingStateView()
            case .login:
                MobileLoginView()
            case .ready:
                AdaptiveWorkbenchView()
            case let .upgradeRequired(version):
                ContentUnavailableView {
                    Label("auth.upgrade", systemImage: "arrow.down.app")
                } description: {
                    Text("Minimum app version: \(version)")
                }
            case let .failed(message):
                ErrorStateView(message: message) {
                    Task { await model.connect() }
                }
            }
        }
        .overlay(alignment: .top) {
            if let banner = model.bannerMessage {
                Text(banner)
                    .font(.footnote)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .background(.regularMaterial, in: Capsule())
                    .padding(.top, 8)
                    .accessibilityAddTraits(.isStaticText)
            }
        }
    }
}

private struct ServerSetupView: View {
    @EnvironmentObject private var model: AppModel
    @FocusState private var serverFocused: Bool

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    Image(systemName: "chart.xyaxis.line")
                        .font(.system(size: 54, weight: .semibold))
                        .symbolRenderingMode(.hierarchical)
                        .accessibilityHidden(true)
                    Text("setup.title")
                        .font(.largeTitle.bold())
                    Text("setup.secureHint")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    TextField(
                        "setup.server",
                        text: $model.serverInput,
                        prompt: Text("https://quazonai.example.com")
                    )
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .textContentType(.URL)
                    .focused($serverFocused)
                    .submitLabel(.go)
                    .onSubmit { Task { await model.connect() } }
                    .padding(14)
                    .background(.quaternary, in: RoundedRectangle(cornerRadius: 14))
                    .accessibilityIdentifier("server-url")
                    Button {
                        Task { await model.connect() }
                    } label: {
                        Label("setup.connect", systemImage: "network")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .disabled(
                        model.serverInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    )
                    .accessibilityIdentifier("connect-server")
                }
                .frame(maxWidth: 620, alignment: .leading)
                .padding(28)
                .frame(maxWidth: .infinity)
            }
            .navigationTitle("app.name")
            .onAppear { serverFocused = model.serverInput.isEmpty }
        }
    }
}

private struct MobileLoginView: View {
    @EnvironmentObject private var model: AppModel
    @State private var code = ""
    @State private var trustDevice = true
    @State private var isSubmitting = false
    @State private var errorMessage: String?
    @FocusState private var codeFocused: Bool

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Label("auth.title", systemImage: "lock.shield")
                        .font(.largeTitle.bold())
                    if let host = model.profile?.baseURL.host {
                        Text(host)
                            .font(.callout.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    SecureField("auth.totp", text: $code)
                        .keyboardType(.numberPad)
                        .textContentType(.oneTimeCode)
                        .focused($codeFocused)
                        .onChange(of: code) { _, value in
                            let digits = value.filter(\.isNumber)
                            let normalized = String(digits.prefix(6))
                            if normalized != code { code = normalized }
                        }
                        .padding(14)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 14))
                        .accessibilityIdentifier("totp-code")
                    Toggle("auth.trustDevice", isOn: $trustDevice)
                        .accessibilityIdentifier("trust-device")
                    if let errorMessage {
                        Label(errorMessage, systemImage: "exclamationmark.triangle")
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .accessibilityIdentifier("login-error")
                    }
                    Button(action: submit) {
                        if isSubmitting {
                            ProgressView().frame(maxWidth: .infinity)
                        } else {
                            Label("auth.login", systemImage: "checkmark.shield")
                                .frame(maxWidth: .infinity)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .disabled(code.count != 6 || isSubmitting)
                    .accessibilityIdentifier("verify-totp")
                    Button("auth.unlock") {
                        Task {
                            isSubmitting = true
                            defer { isSubmitting = false }
                            do {
                                try await model.unlockTrustedDevice()
                                errorMessage = nil
                            } catch {
                                errorMessage = error.localizedDescription
                            }
                        }
                    }
                    .disabled(isSubmitting)
                    .frame(maxWidth: .infinity)
                    Button("Change server", role: .destructive) {
                        Task { await model.forgetServer() }
                    }
                    .frame(maxWidth: .infinity)
                }
                .frame(maxWidth: 520, alignment: .leading)
                .padding(28)
                .frame(maxWidth: .infinity)
            }
            .navigationTitle("app.name")
            .onAppear { codeFocused = true }
        }
    }

    private func submit() {
        Task {
            isSubmitting = true
            errorMessage = nil
            let submittedCode = code
            code = ""
            defer { isSubmitting = false }
            do {
                try await model.login(totpCode: submittedCode, trustDevice: trustDevice)
            } catch {
                errorMessage = error.localizedDescription
                codeFocused = true
            }
        }
    }
}

private enum PhoneTab: Hashable {
    case home
    case research
    case approvals
    case portfolio
    case more
}

private struct AdaptiveWorkbenchView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @State private var phoneTab: PhoneTab = .home

    private var useSplitView: Bool {
        UIDevice.current.userInterfaceIdiom == .pad && horizontalSizeClass == .regular
    }

    var body: some View {
        Group {
            if useSplitView {
                IPadWorkbenchView()
            } else {
                IPhoneWorkbenchView(selection: $phoneTab)
            }
        }
        .safeAreaInset(edge: .top) {
            VStack(spacing: 6) {
                if model.isDirectAccess { DirectAccessBanner() }
                if !model.network.isOnline { OfflineReadOnlyBanner() }
            }
            .padding(.horizontal)
            .padding(.top, 4)
        }
    }
}

private struct IPhoneWorkbenchView: View {
    @Binding var selection: PhoneTab

    var body: some View {
        TabView(selection: $selection) {
            NavigationStack { HomeView() }
                .tabItem { Label("nav.home", systemImage: "house") }
                .tag(PhoneTab.home)
            NavigationStack { ResearchView() }
                .tabItem { Label("nav.research", systemImage: "binoculars") }
                .tag(PhoneTab.research)
            NavigationStack { ApprovalView() }
                .tabItem { Label("nav.approvals", systemImage: "checkmark.seal") }
                .tag(PhoneTab.approvals)
            NavigationStack { PortfolioView() }
                .tabItem { Label("nav.portfolio", systemImage: "chart.pie") }
                .tag(PhoneTab.portfolio)
            NavigationStack { MoreView() }
                .tabItem { Label("nav.more", systemImage: "ellipsis.circle") }
                .tag(PhoneTab.more)
        }
        .accessibilityIdentifier("iphone-tab-shell")
    }
}

private struct MoreView: View {
    private let destinations: [AppDestination] = [
        .idea,
        .alpha,
        .handoff,
        .administration,
        .security,
        .settings,
    ]

    var body: some View {
        List(destinations) { destination in
            NavigationLink {
                DestinationContent(destination: destination)
            } label: {
                Label(
                    String(localized: destination.localizationKey),
                    systemImage: destination.symbol
                )
            }
        }
        .navigationTitle("nav.more")
        .accessibilityIdentifier("more-screen")
    }
}

private struct IPadWorkbenchView: View {
    @EnvironmentObject private var model: AppModel
    private let destinations = AppDestination.allCases

    var body: some View {
        NavigationSplitView {
            List {
                ForEach(destinations) { destination in
                    Button {
                        model.destination = destination
                    } label: {
                        HStack {
                            Label(
                                String(localized: destination.localizationKey),
                                systemImage: destination.symbol
                            )
                            Spacer()
                            if model.destination == destination {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(.tint)
                                    .accessibilityHidden(true)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("sidebar-\(destination.rawValue)")
                    .accessibilityAddTraits(
                        model.destination == destination ? .isSelected : []
                    )
                }
            }
            .navigationTitle("app.name")
            .toolbar {
                ToolbarItem(placement: .bottomBar) {
                    StreamStatusBadge(status: model.streamStatus)
                }
            }
        } detail: {
            NavigationStack {
                DestinationContent(destination: model.destination)
            }
        }
        .navigationSplitViewStyle(.balanced)
        .accessibilityIdentifier("ipad-split-shell")
    }
}

struct DestinationContent: View {
    let destination: AppDestination

    @ViewBuilder
    var body: some View {
        switch destination {
        case .home: HomeView()
        case .idea: IdeaComposerView()
        case .research: ResearchView()
        case .alpha: AlphaLibraryView()
        case .portfolio: PortfolioView()
        case .approvals: ApprovalView()
        case .handoff: HandoffView()
        case .administration: AdministrationView()
        case .security: DeviceSecurityView()
        case .settings: SettingsView()
        }
    }
}
