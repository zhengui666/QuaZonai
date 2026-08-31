import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public enum EventStreamState: Sendable, Equatable {
    case disconnected
    case catchingUp
    case connected
    case reconnecting(Int)
}

public struct ServerEvent: Sendable, Equatable {
    public let id: Int
    public let name: String
    public let data: JSONValue
}

public actor EventStreamActor {
    private let client: APIClient
    private let session: URLSession
    private let persistCursor: @Sendable (Int) async -> Void
    private var task: Task<Void, Never>?
    private var cursor: Int

    public init(
        client: APIClient,
        cursor: Int = 0,
        session: URLSession = .shared,
        persistCursor: @escaping @Sendable (Int) async -> Void = { _ in }
    ) {
        self.client = client
        self.cursor = cursor
        self.session = session
        self.persistCursor = persistCursor
    }

    deinit { task?.cancel() }

    public func start(
        onState: @escaping @Sendable (EventStreamState) async -> Void,
        onEvent: @escaping @Sendable (ServerEvent) async -> Void
    ) {
        guard task == nil else { return }
        task = Task { [weak self] in
            guard let self else { return }
            await self.run(onState: onState, onEvent: onEvent)
        }
    }

    public func stop() {
        task?.cancel()
        task = nil
    }

    private func run(
        onState: @escaping @Sendable (EventStreamState) async -> Void,
        onEvent: @escaping @Sendable (ServerEvent) async -> Void
    ) async {
        var attempt = 0
        while !Task.isCancelled {
            do {
                await onState(.catchingUp)
                try await catchUp(onEvent: onEvent)
                await onState(.connected)
                try await stream(onEvent: onEvent)
                attempt = 0
            } catch {
                if Task.isCancelled { return }
                attempt += 1
                await onState(.reconnecting(attempt))
                if case APIClientError.authenticationRequired = error {
                    _ = try? await client.refreshIfPossible()
                }
                let ceiling = min(30.0, pow(2.0, Double(min(attempt, 5))))
                let delay = ceiling + Double.random(in: 0...(ceiling * 0.25))
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    private func catchUp(onEvent: @escaping @Sendable (ServerEvent) async -> Void) async throws {
        let result = try await client.requestJSON(path: "/api/v1/events?after_id=\(cursor)&limit=1000")
        guard let items = result.arrayValue else { return }
        for item in items {
            guard let object = item.objectValue,
                  let idValue = object.number("id") else { continue }
            let id = Int(idValue)
            guard id > cursor else { continue }
            cursor = id
            await persistCursor(id)
            await onEvent(ServerEvent(id: id, name: object.string("kind") ?? "qz-event", data: item))
        }
    }

    private func stream(onEvent: @escaping @Sendable (ServerEvent) async -> Void) async throws {
        var request = try await client.authorizedRequest(
            path: "/api/v1/events/stream",
            queryItems: [URLQueryItem(name: "cursor", value: String(cursor))]
        )
        if cursor > 0 { request.setValue(String(cursor), forHTTPHeaderField: "Last-Event-ID") }
        let (bytes, rawResponse) = try await session.bytes(for: request)
        guard let response = rawResponse as? HTTPURLResponse else { throw APIClientError.invalidResponse }
        guard (200..<300).contains(response.statusCode) else {
            if response.statusCode == 401 { throw APIClientError.authenticationRequired }
            throw APIClientError.http(status: response.statusCode, code: nil, message: "Event stream failed")
        }

        var eventID: Int?
        var eventName = "qz-event"
        var dataLines: [String] = []
        for try await line in bytes.lines {
            if Task.isCancelled { return }
            if line.isEmpty {
                if let eventID, !dataLines.isEmpty, eventID > cursor {
                    let data = Data(dataLines.joined(separator: "\n").utf8)
                    let value = try JSONDecoder().decode(JSONValue.self, from: data)
                    cursor = eventID
                    await persistCursor(eventID)
                    await onEvent(ServerEvent(id: eventID, name: eventName, data: value))
                }
                eventID = nil
                eventName = "qz-event"
                dataLines.removeAll(keepingCapacity: true)
                continue
            }
            if line.hasPrefix(":") { continue }
            let field: String
            let value: String
            if let separator = line.firstIndex(of: ":") {
                field = String(line[..<separator])
                value = String(line[line.index(after: separator)...]).trimmingCharacters(in: .whitespaces)
            } else {
                field = line
                value = ""
            }
            switch field {
            case "id": eventID = Int(value)
            case "event": eventName = value
            case "data": dataLines.append(value)
            default: break
            }
        }
        throw URLError(.networkConnectionLost)
    }
}
