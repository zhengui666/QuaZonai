import XCTest

final class ParityUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        let serverURL = ProcessInfo.processInfo.environment["QUAZONAI_UI_TEST_SERVER_URL"]
            ?? "http://127.0.0.1:8000"
        app.launchArguments = ["--ui-testing", "--server-url", serverURL]
        app.launchEnvironment["AppleLanguages"] = "(en)"
        app.launchEnvironment["AppleLocale"] = "en_US"
        app.launch()
    }

    func testIPhoneCapabilityRegistry() throws {
        let shell = app.descendants(matching: .any)["iphone-tab-shell"]
        guard shell.waitForExistence(timeout: 30) else {
            if app.descendants(matching: .any)["ipad-split-shell"].exists {
                throw XCTSkip("This capability test is scoped to the iPhone simulator.")
            }
            XCTFail("The iPhone operator shell did not become ready.")
            return
        }

        assertVisible("home-screen")
        tapTab("Research")
        assertVisible("research-screen")
        tapTab("Approvals")
        assertVisible("approval-screen")
        tapTab("Portfolio")
        assertVisible("portfolio-screen")
        tapTab("More")
        assertVisible("more-screen")

        openMoreDestination("Idea Composer", expectedIdentifier: "idea-screen")
        navigateBackToMore()
        openMoreDestination("Alpha Library", expectedIdentifier: "alpha-screen")
        navigateBackToMore()
        openMoreDestination("Handoff & Feedback", expectedIdentifier: "handoff-screen")
        navigateBackToMore()
        openMoreDestination("Administration", expectedIdentifier: "administration-screen")
        navigateBackToMore()
        openMoreDestination("Device Security", expectedIdentifier: "security-screen")
        navigateBackToMore()
        openMoreDestination("Language & Appearance", expectedIdentifier: "settings-screen")
    }

    func testIPadCapabilityRegistry() throws {
        let shell = app.descendants(matching: .any)["ipad-split-shell"]
        guard shell.waitForExistence(timeout: 30) else {
            if app.descendants(matching: .any)["iphone-tab-shell"].exists {
                throw XCTSkip("This capability test is scoped to the iPad simulator.")
            }
            XCTFail("The iPad operator shell did not become ready.")
            return
        }

        let destinations: [(String, String)] = [
            ("home", "home-screen"),
            ("idea", "idea-screen"),
            ("research", "research-screen"),
            ("alpha", "alpha-screen"),
            ("portfolio", "portfolio-screen"),
            ("approvals", "approval-screen"),
            ("handoff", "handoff-screen"),
            ("administration", "administration-screen"),
            ("security", "security-screen"),
            ("settings", "settings-screen"),
        ]
        for (destination, screen) in destinations {
            let sidebar = app.descendants(matching: .any)["sidebar-\(destination)"]
            XCTAssertTrue(sidebar.waitForExistence(timeout: 10), "Missing iPad entry: \(destination)")
            sidebar.tap()
            assertVisible(screen)
        }
    }

    func testDirectAccessIdeaPreviewAndResearchNavigation() throws {
        guard app.descendants(matching: .any)["iphone-tab-shell"].waitForExistence(timeout: 30) else {
            throw XCTSkip("Mutation journey currently runs on the iPhone simulator.")
        }
        tapTab("More")
        openMoreDestination("Idea Composer", expectedIdentifier: "idea-screen")
        let text = app.descendants(matching: .any)["idea-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        text.tap()
        text.typeText("Test post-earnings drift in liquid US equities after realistic costs.")
        let preview = app.descendants(matching: .any)["preview-idea"]
        XCTAssertTrue(preview.waitForExistence(timeout: 10))
        preview.tap()
        let start = app.descendants(matching: .any)["start-research"]
        XCTAssertTrue(start.waitForExistence(timeout: 20))
        start.tap()
        XCTAssertTrue(app.navigationBars["Program"].waitForExistence(timeout: 20))
    }

    private func tapTab(_ title: String) {
        let button = app.tabBars.buttons[title]
        XCTAssertTrue(button.waitForExistence(timeout: 10), "Missing tab: \(title)")
        button.tap()
    }

    private func openMoreDestination(_ title: String, expectedIdentifier: String) {
        let link = app.buttons[title].firstMatch.exists
            ? app.buttons[title].firstMatch
            : app.staticTexts[title].firstMatch
        XCTAssertTrue(link.waitForExistence(timeout: 10), "Missing More destination: \(title)")
        link.tap()
        assertVisible(expectedIdentifier)
    }

    private func navigateBackToMore() {
        let back = app.navigationBars.buttons.element(boundBy: 0)
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        back.tap()
        assertVisible("more-screen")
    }

    private func assertVisible(_ identifier: String) {
        let element = app.descendants(matching: .any)[identifier]
        XCTAssertTrue(element.waitForExistence(timeout: 20), "Missing screen: \(identifier)")
    }
}
