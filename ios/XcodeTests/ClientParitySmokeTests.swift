import UIKit
import XCTest

final class ClientParitySmokeTests: XCTestCase {
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "--ui-testing",
            "--fixture-mode",
            "-AppleLanguages", "(en)",
            "-AppleLocale", "en_US"
        ]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))
        return app
    }

    func testIPhoneAndIPadExposeTheSameOperatorDomains() {
        let app = launch()
        let compact = UIDevice.current.userInterfaceIdiom == .phone
        let expected = compact
            ? ["Home", "Research", "Approvals", "Portfolio", "More"]
            : ["Home", "Idea Composer", "Research Observatory", "Alpha Library", "Portfolio Lab", "Approval Inbox", "Handoff & Feedback", "Administration"]
        for label in expected {
            XCTAssertTrue(
                app.buttons[label].waitForExistence(timeout: 5)
                    || app.cells[label].waitForExistence(timeout: 1)
                    || app.staticTexts[label].waitForExistence(timeout: 1),
                "Missing native Operator domain: \(label)"
            )
        }
    }

    func testExecutionControlNeverAppears() {
        let app = launch()
        for forbidden in ["Stop runtime", "Undeploy", "Close position", "Emergency liquidate"] {
            XCTAssertFalse(app.buttons[forbidden].exists)
        }
    }
}
