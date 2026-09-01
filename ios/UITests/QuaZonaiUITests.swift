import XCTest

final class QuaZonaiUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func launchApp() {
        app = XCUIApplication()
        app.launchArguments += ["--ui-testing", "--fixture-mode", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launchEnvironment["QUAZONAI_UI_SERVER"] = ProcessInfo.processInfo.environment["QUAZONAI_UI_SERVER"] ?? "http://127.0.0.1:8000"
        app.launch()
        XCTAssertTrue(
            app.staticTexts["Home"].waitForExistence(timeout: 15)
                || app.navigationBars["QuaZonai"].waitForExistence(timeout: 15)
        )
    }

    @MainActor
    private func openMoreDestination(_ label: String) {
        if UIDevice.current.userInterfaceIdiom == .pad {
            let target = app.buttons[label].firstMatch
            XCTAssertTrue(target.waitForExistence(timeout: 6), "Missing iPad destination: \(label)")
            target.tap()
            return
        }
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
    private func openPrimarySection(phoneLabel: String, iPadLabel: String) {
        if UIDevice.current.userInterfaceIdiom == .pad {
            let target = app.buttons[iPadLabel].firstMatch
            XCTAssertTrue(target.waitForExistence(timeout: 6), "Missing iPad destination: \(iPadLabel)")
            target.tap()
        } else {
            XCTAssertTrue(app.tabBars.buttons[phoneLabel].waitForExistence(timeout: 6))
            app.tabBars.buttons[phoneLabel].tap()
        }
    }

    @MainActor
    private func waitForHittable(_ element: XCUIElement, swipes: Int = 8) -> Bool {
        for _ in 0...swipes {
            if element.exists && element.isHittable { return true }
            app.swipeUp()
        }
        return element.exists
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
        let start = app.buttons["Start Research"]
        XCTAssertTrue(start.waitForExistence(timeout: 10))
        start.tap()
        XCTAssertTrue(app.staticTexts["Created program"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testResearch() {
        launchApp()
        openPrimarySection(phoneLabel: "Research", iPadLabel: "Research Observatory")
        XCTAssertTrue(app.navigationBars["Research"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testProgramActions() {
        launchApp()
        openPrimarySection(phoneLabel: "Research", iPadLabel: "Research Observatory")
        let firstRecord = app.staticTexts["Fixture research program"].firstMatch
        XCTAssertTrue(waitForHittable(firstRecord, swipes: 4))
        firstRecord.tap()
        let reason = app.textFields["Reason for pause/archive"]
        XCTAssertTrue(waitForHittable(reason))
        reason.tap(); reason.typeText("UI action test")
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 8))
        app.buttons["Resume"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 8))
        reason.tap(); reason.typeText("UI archive test")
        app.buttons["Archive"].tap()
        XCTAssertTrue(app.buttons["Restore"].waitForExistence(timeout: 8))
        app.buttons["Restore"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 8))
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
        openPrimarySection(phoneLabel: "Portfolio", iPadLabel: "Portfolio Lab")
        XCTAssertTrue(app.navigationBars["Portfolio"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testApproval() {
        launchApp()
        openPrimarySection(phoneLabel: "Approvals", iPadLabel: "Approval Inbox")
        XCTAssertTrue(app.navigationBars["Approvals"].waitForExistence(timeout: 6))
        let approve = app.buttons["Approve"].firstMatch
        XCTAssertTrue(approve.waitForExistence(timeout: 8))
        approve.tap()
        XCTAssertTrue(app.staticTexts["APPROVED"].waitForExistence(timeout: 8))
    }

    @MainActor
    func testReject() {
        launchApp()
        openPrimarySection(phoneLabel: "Approvals", iPadLabel: "Approval Inbox")
        let reject = app.buttons["Reject"].firstMatch
        XCTAssertTrue(reject.waitForExistence(timeout: 8))
        reject.tap()
        XCTAssertTrue(app.navigationBars["Reject"].waitForExistence(timeout: 6))
        let picker = app.buttons["Reason code"]
        XCTAssertTrue(picker.waitForExistence(timeout: 6))
        picker.tap()
        XCTAssertTrue(app.buttons["Risk Profile Unacceptable"].waitForExistence(timeout: 6))
        app.buttons["Risk Profile Unacceptable"].tap()
        let rejectButtons = app.buttons.matching(identifier: "Reject")
        XCTAssertGreaterThan(rejectButtons.count, 1)
        rejectButtons.element(boundBy: rejectButtons.count - 1).tap()
        XCTAssertTrue(app.staticTexts["REJECTED"].waitForExistence(timeout: 8))
    }

    @MainActor
    func testHandoff() {
        launchApp()
        openMoreDestination("Handoff & Feedback")
        XCTAssertTrue(app.navigationBars["Handoff & Feedback"].waitForExistence(timeout: 6))
    }

    @MainActor
    func testHandoffRevoke() {
        launchApp()
        openMoreDestination("Handoff & Feedback")
        let revoke = app.buttons["Revoke"].firstMatch
        XCTAssertTrue(revoke.waitForExistence(timeout: 8))
        revoke.tap()
        XCTAssertTrue(app.staticTexts["REVOKED"].waitForExistence(timeout: 8))
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
        let save = app.buttons["Save"]
        XCTAssertTrue(waitForHittable(save))
        save.tap()
    }

    @MainActor
    func testDataSourceRegistration() {
        testAdministration()
        let register = app.buttons["register-data-source"]
        XCTAssertTrue(waitForHittable(register))
        register.tap()
        XCTAssertTrue(app.navigationBars["Register Data Source"].waitForExistence(timeout: 6))
        app.textFields["Name"].tap(); app.textFields["Name"].typeText("iOS UI Data")
        app.textFields["Provider"].tap(); app.textFields["Provider"].typeText("Fixture Provider")
        app.textFields["Canonical Fields"].tap(); app.textFields["Canonical Fields"].typeText("event_time, available_time, close, volume")
        app.buttons["Register"].tap()
        XCTAssertTrue(app.staticTexts["iOS UI Data"].waitForExistence(timeout: 8))
    }

    @MainActor
    func testDownstreamRegistration() {
        testAdministration()
        let register = app.buttons["register-downstream"]
        XCTAssertTrue(waitForHittable(register))
        register.tap()
        XCTAssertTrue(app.navigationBars["Register Downstream"].waitForExistence(timeout: 6))
        app.textFields["Name"].tap(); app.textFields["Name"].typeText("iOS UI Downstream")
        app.buttons["Register"].tap()
        XCTAssertTrue(app.staticTexts["Service token — shown once"].waitForExistence(timeout: 8))
    }

    @MainActor
    func testMandateToggle() {
        testAdministration()
        let toggle = app.switches.firstMatch
        XCTAssertTrue(waitForHittable(toggle))
        toggle.tap()
        XCTAssertTrue(toggle.waitForExistence(timeout: 8))
    }

    @MainActor
    func testDeviceSecurity() {
        launchApp()
        openMoreDestination("Account / Device Security")
        XCTAssertTrue(app.navigationBars["Account / Device Security"].waitForExistence(timeout: 6))
        let revoke = app.buttons["Revoke device"].firstMatch
        XCTAssertTrue(revoke.waitForExistence(timeout: 8))
        revoke.tap()
        XCTAssertTrue(app.staticTexts["REVOKED"].waitForExistence(timeout: 8))
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
