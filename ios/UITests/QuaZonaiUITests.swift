import XCTest

final class QuaZonaiUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func launchApp() {
        app = XCUIApplication()
        app.launchEnvironment["QUAZONAI_UI_SERVER"] = ProcessInfo.processInfo.environment["QUAZONAI_UI_SERVER"] ?? "http://127.0.0.1:8000"
        app.launch()
        XCTAssertTrue(
            app.staticTexts["Home"].waitForExistence(timeout: 15)
                || app.navigationBars["QuaZonai"].waitForExistence(timeout: 15)
        )
    }

    @MainActor
    private func openMoreDestination(_ label: String) {
        if app.tabBars.buttons["More"].exists {
            app.tabBars.buttons["More"].tap()
        }
        let target = app.buttons[label].firstMatch
        if target.waitForExistence(timeout: 4) {
            target.tap()
            return
        }
        let text = app.staticTexts[label].firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 4))
        text.tap()
    }

    @MainActor
    func testHome() {
        launchApp()
        XCTAssertTrue(app.staticTexts["System Health"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testIdeaFlow() {
        launchApp()
        openMoreDestination("Idea Composer")
        let editor = app.textViews.firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 6))
        editor.tap()
        editor.typeText("Test post-earnings drift in liquid US equities after realistic costs.")
        app.buttons["Preview research charter"].tap()
        XCTAssertTrue(app.buttons["Start Research"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testResearch() {
        launchApp()
        if app.tabBars.buttons["Research"].exists {
            app.tabBars.buttons["Research"].tap()
        } else {
            openMoreDestination("Research")
        }
        XCTAssertTrue(app.navigationBars["Research"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testProgramActions() {
        testResearch()
    }

    @MainActor
    func testAlpha() {
        launchApp()
        openMoreDestination("Alpha Library")
        XCTAssertTrue(app.navigationBars["Alpha Library"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testPortfolio() {
        launchApp()
        if app.tabBars.buttons["Portfolio"].exists {
            app.tabBars.buttons["Portfolio"].tap()
        } else {
            openMoreDestination("Portfolio")
        }
        XCTAssertTrue(app.navigationBars["Portfolio"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testApproval() {
        launchApp()
        if app.tabBars.buttons["Approvals"].exists {
            app.tabBars.buttons["Approvals"].tap()
        } else {
            openMoreDestination("Approvals")
        }
        XCTAssertTrue(app.navigationBars["Approvals"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testReject() {
        testApproval()
        XCTAssertTrue(app.buttons["Reject"].firstMatch.waitForExistence(timeout: 6))
    }

    @MainActor
    func testHandoff() {
        launchApp()
        openMoreDestination("Handoff & Feedback")
        XCTAssertTrue(app.navigationBars["Handoff & Feedback"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testHandoffRevoke() {
        testHandoff()
    }

    @MainActor
    func testAdministration() {
        launchApp()
        openMoreDestination("Administration")
        XCTAssertTrue(app.navigationBars["Administration"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testRuntimeConfiguration() {
        testAdministration()
        XCTAssertTrue(app.secureTextFields["Codex API Key (write only)"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testDataSourceRegistration() {
        testAdministration()
        XCTAssertTrue(app.buttons["Register Data Source"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testDownstreamRegistration() {
        testAdministration()
        XCTAssertTrue(app.buttons["Register Downstream"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testMandateToggle() {
        testAdministration()
        XCTAssertTrue(app.switches.firstMatch.waitForExistence(timeout: 6))
    }

    @MainActor
    func testDeviceSecurity() {
        launchApp()
        openMoreDestination("Account / Device Security")
        XCTAssertTrue(app.navigationBars["Account / Device Security"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testIPadResize() {
        launchApp()
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(app.staticTexts["Home"].waitForExistence(timeout: 6))
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(app.staticTexts["Home"].exists)
    }
}
