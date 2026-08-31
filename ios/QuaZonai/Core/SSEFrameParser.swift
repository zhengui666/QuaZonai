import Foundation

struct SSEFrame: Equatable, Sendable {
    let id: Int?
    let event: String
    let data: String
    let retryMilliseconds: Int?
}

struct SSEFrameParser: Sendable {
    private var eventID: Int?
    private var eventName = "qz-event"
    private var dataLines: [String] = []
    private var retryMilliseconds: Int?

    mutating func consume(line: String) -> SSEFrame? {
        if line.isEmpty {
            guard !dataLines.isEmpty else {
                resetEventFields()
                return nil
            }
            let frame = SSEFrame(
                id: eventID,
                event: eventName,
                data: dataLines.joined(separator: "\n"),
                retryMilliseconds: retryMilliseconds
            )
            resetEventFields()
            return frame
        }
        if line.hasPrefix(":") { return nil }

        let field: String
        var value: Substring
        if let separator = line.firstIndex(of: ":") {
            field = String(line[..<separator])
            value = line[line.index(after: separator)...]
            if value.first == " " { value = value.dropFirst() }
        } else {
            field = line
            value = ""
        }

        switch field {
        case "id":
            let text = String(value)
            if !text.contains("\0") { eventID = Int(text) }
        case "event":
            eventName = value.isEmpty ? "qz-event" : String(value)
        case "data":
            dataLines.append(String(value))
        case "retry":
            if let parsed = Int(value), parsed >= 0 { retryMilliseconds = parsed }
        default:
            break
        }
        return nil
    }

    private mutating func resetEventFields() {
        eventID = nil
        eventName = "qz-event"
        dataLines.removeAll(keepingCapacity: true)
        retryMilliseconds = nil
    }
}

struct EventCursor: Equatable, Sendable {
    private(set) var value: Int

    init(_ value: Int = 0) {
        self.value = max(0, value)
    }

    mutating func accept(_ id: Int) -> Bool {
        guard id > value else { return false }
        value = id
        return true
    }
}

enum EventStreamBackoff {
    static func delaySeconds(attempt: Int, jitterUnit: Double) -> Double {
        let boundedAttempt = min(max(attempt, 1), 5)
        let ceiling = min(30.0, pow(2.0, Double(boundedAttempt)))
        let boundedJitter = min(max(jitterUnit, 0), 1)
        return ceiling + (ceiling * 0.25 * boundedJitter)
    }
}
