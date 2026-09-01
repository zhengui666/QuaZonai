import UIKit
import XCTest

final class ClientParityUITests: XCTestCase {
    @MainActor
    private func launchFixtureApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments += [
            "--ui-testing",
            "--fixture-mode",
            "-AppleLanguages", "(en)",
            "-AppleLocale", "en_US"
        ]
        app.launchEnvironment["QUAZONAI_UI_SERVER"] = ProcessInfo.processInfo.environment["QUAZONAI_UI_SERVER"] ?? "http://127.0.0.1:8000"
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))
        return app
    }

    @MainActor
    private func assertDestination(_ name: String, in app: XCUIApplication) {
        let exists = app.buttons[name].waitForExistence(timeout: 5)
            || app.staticTexts[name].waitForExistence(timeout: 1)
            || app.cells[name].waitForExistence(timeout: 1)
        XCTAssertTrue(exists, "Missing native destination: \(name)")
    }

    @MainActor
    func testAllCapabilitiesRemainReachableOnIPhone() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else {
            throw XCTSkip("Executed only by the iPhone simulator matrix entry")
        }
        let app = launchFixtureApp()
        for destination in ["Home", "Research", "Approvals", "Portfolio", "More"] {
            assertDestination(destination, in: app)
        }
        let more = app.buttons["More"]
        if more.exists {
            more.tap()
        } else {
            app.staticTexts["More"].tap()
        }
        for destination in [
            "Idea Composer", "Alpha Library", "Handoff & Feedback", "Administration",
            "Language", "Appearance", "Account / Device Security"
        ] {
            assertDestination(destination, in: app)
        }
        for forbidden in ["Stop runtime", "Undeploy", "Close position", "Buy", "Sell"] {
            XCTAssertFalse(app.buttons[forbidden].exists, "Native client crossed downstream ownership")
        }
    }

    @MainActor
    func testAllCapabilitiesRemainReachableOnIPad() throws {
        guard UIDevice.current.userInterfaceIdiom == .pad else {
            throw XCTSkip("Executed only by the iPad simulator matrix entry")
        }
        let app = launchFixtureApp()
        for destination in [
            "Home", "Idea Composer", "Research Observatory", "Alpha Library",
            "Portfolio Lab", "Approval Inbox", "Handoff & Feedback", "Administration"
        ] {
            assertDestination(destination, in: app)
        }
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
    }
}
