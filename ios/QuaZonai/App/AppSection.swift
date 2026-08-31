import SwiftUI

enum AppSection: String, CaseIterable, Identifiable, Hashable {
    case home, idea, research, alpha, portfolio, approvals, handoff, administration
    var id: String { rawValue }

    var titleKey: L10nKey {
        switch self {
        case .home: .home
        case .idea: .idea
        case .research: .research
        case .alpha: .alpha
        case .portfolio: .portfolio
        case .approvals: .approvals
        case .handoff: .handoff
        case .administration: .administration
        }
    }

    var icon: String {
        switch self {
        case .home: "house"
        case .idea: "lightbulb"
        case .research: "point.3.connected.trianglepath.dotted"
        case .alpha: "waveform.path.ecg"
        case .portfolio: "chart.pie"
        case .approvals: "checkmark.seal"
        case .handoff: "arrow.left.arrow.right"
        case .administration: "gearshape.2"
        }
    }
}
